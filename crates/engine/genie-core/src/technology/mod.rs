//! Technologies and upgrades: the players' technologies, a research's effects and an object's
//! upgrade to another master.
//!
//! This file holds the players' technologies (`TRIBE_Player_Tech`, `player+0x760`): the `.dat`'s
//! technology records ([`TechDef`], the tree's `+0x28`, 0x88 bytes each) and each player's state of
//! each ([`TechState`], `Tech_Player_Tree`, 0x80 bytes, saved). 158041: the cost list
//! `FUN_1413d4a00`, the research test `FUN_1413cd600`, the payment `FUN_1413d4370` (from
//! `FUN_14162fd90`), the repeat test `FUN_1413d1050`.

use crate::GenieError;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::slots::CheckedIndex;

mod command_args;
mod cost_override;
mod cost_paid;
pub mod effect;
mod effect_command;
mod effect_id;
mod research_count;
mod research_state;
mod tech_id;
mod tech_type;
pub mod upgrade;

pub use self::command_args::{ChangeMode, DisabledTech, ModifyLocation, ModifyMode};
pub use self::cost_override::{CostOverride, OverrideUse};
pub use self::cost_paid::CostPaid;
pub use self::effect_command::{
    CostEdit, CostUnits, EffectCommand, ObjectsKind, Party, ResourceChange,
};
pub use self::effect_id::EffectId;
pub use self::research_count::{ResearchCap, ResearchCount};
pub use self::research_state::ResearchState;
pub use self::tech_id::TechId;
pub use self::tech_type::TechType;

/// The age technologies (`FUN_1413cc3c0`: 101 to 103), whose costs the research cost modifier
/// leaves alone.
#[must_use]
pub fn is_age(tech: TechId) -> bool {
    (TechId::FIRST_AGE..=TechId::LAST_AGE).contains(&tech)
}

/// Spies (0x198), whose cost grows with the enemies' villagers (`FUN_1413cbd80`).
const SPIES: TechId = TechId::SPIES;
/// The technology whose cost adjustments scale by attribute 38 (0x2f3) instead of 1.
const SCALED_BY_38: TechId = TechId::SCALED_BY_38;
/// The attribute 0x2f3's cost adjustments scale by (`+0x130`).
const ATTRIBUTE_38: AttributeId = AttributeId::numbered(38);
/// The resource a cost the player cannot meet takes the rest from (`+0x18`: gold).
const GOLD: AttributeId = attribute::GOLD;

/// How a group of the game's `linkedTechs.json` links its technologies: the `+4` byte of the
/// record the game settings' table (`FUN_140f39f40`) holds for each of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linked {
    /// `MutuallyExclusive` (0).
    MutuallyExclusive,
    /// `Toggleable` (1): `FUN_140f39e80` asks the tech tree (`FUN_1413cc3d0`).
    Toggleable,
    /// `Cycle` (2): `FUN_140f39e80` holds it researchable in any state.
    Cycle,
}

impl Linked {
    /// The kind of the file's `Type`; None for one the game does not know.
    #[must_use]
    pub fn of(kind: &str) -> Option<Self> {
        match kind {
            "MutuallyExclusive" => Some(Self::MutuallyExclusive),
            "Toggleable" => Some(Self::Toggleable),
            "Cycle" => Some(Self::Cycle),
            _ => None,
        }
    }
}

/// One cost entry of a technology in the `.dat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// The attribute it takes, none for −1.
    pub attribute: Option<AttributeId>,
    /// How much.
    pub amount: crate::objects::masters::CostAmount,
    /// 0 not paid, 1 paid from the player's attribute, 2 from the building (`FUN_141204310`).
    pub paid: CostPaid,
}

/// A technology of the `.dat` as the tech tree holds it (its 0x88-byte record).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TechDef {
    /// `+0x2a`, `+0x30`, `+0x36`: the cost entries' attributes, amounts and paid flags.
    pub costs: [Cost; 3],
    /// `+0x3c`, the technology's type.
    pub kind: TechType,
    /// Its effect (`+0x3a`) and the effect's commands (the world's effect table `+0x128`).
    pub effect_id: EffectId,
    /// The effect's commands.
    pub effect: Vec<EffectCommand>,
    /// `+0x18`, `+0x24`: the technologies it requires and how many of them.
    pub required: [Option<TechId>; 6],
    /// How many of them it requires.
    pub required_count: i16,
    /// `+0x26`: the civ it belongs to (below 1 every civ's); `+0x28`: the game modes it is for
    /// (0 every one, `FUN_1413cf2f0`).
    pub civ: Option<crate::players::CivId>,
    /// `+0x28`: the game modes it is for (0 and 1 every one, `FUN_1413cf2f0`).
    pub mode: i16,
    /// `+0x70`: where it is researched, the building's master and the seconds it takes.
    pub locations: Vec<(Option<MasterId>, crate::world::WholeSeconds)>,
}

impl TechDef {
    /// Its effect upgrades a unit (a command of kind 3, the player's own), which keeps
    /// RESEARCH's queue to one at a time (`FUN_1411d70f0`).
    #[must_use]
    pub fn upgrades(&self) -> bool {
        self.effect.iter().any(|c| {
            matches!(
                c,
                EffectCommand::Upgrade {
                    party: Party::Own,
                    ..
                }
            )
        })
    }

    /// Its effect marks a master with an upgrade when its research starts: an upgrade of any
    /// block (kinds 3, 13, 23, 33, 43; `FUN_140ec4540` from the research's start,
    /// `FUN_1413d1530`, marks the master with the unit it turns into, `+0x330`).
    #[must_use]
    pub fn marks_upgrade(&self) -> bool {
        self.effect
            .iter()
            .any(|c| matches!(c, EffectCommand::Upgrade { .. }))
    }
}

/// Where a technology is researched (an entry of `+0x68`, 16 bytes): the building's master,
/// the seconds it takes there, its button and the seconds added (`+0xc`, `FUN_1413cec90`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Location {
    /// The building's master, none for an entry the data leaves open (−1).
    pub building: Option<MasterId>,
    /// The seconds it takes there (the save's `u16`, its bits kept).
    pub time: crate::world::WholeSeconds,
    /// Its button.
    pub button: crate::production::LocationButton,
    /// The seconds added (`+0xc`, `FUN_1413cec90`).
    pub adjustment: Seconds,
}

/// A player's state of one technology (`Tech_Player_Tree`, 0x80 bytes, saved).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TechState {
    /// `+0x0`: the research's seconds of work done.
    pub research_done: Seconds,
    /// `+0x4`: its state.
    pub state: ResearchState,
    /// `+0x6`: added to the `.dat`'s amounts.
    pub cost_adj: [crate::objects::masters::CostAmount; 3],
    /// `+0xc`.
    pub times_researched: ResearchCount,
    /// `+0xe`.
    pub allow_multiple: bool,
    /// `+0x40`: its researches queued and done are held to its cap.
    pub stack: bool,
    /// `+0x42`, `+0x48`, `+0x4e`: the cost overrides' amounts, attributes and use (−2 the `.dat`'s
    /// entry where it is paid, 0 none, else the override).
    pub overrides: [CostOverride; 3],
    /// `+0x56`: the cap of its researches queued and done (below 0: 0x100).
    pub research_cap: ResearchCap,
    /// `+0x58`: its researches queued.
    pub pending: ResearchCount,
    /// `+0x60` (`mEnabledTech`): the dependents' update leaves it alone.
    pub enabled: bool,
    /// `+0x62`: the effect it gives instead of its own (−1 none).
    pub effect_override: EffectId,
    /// `+0x64`: the master of the building it was last queued at.
    pub building_master: Option<MasterId>,
    /// `+0x68`.
    pub locations: Vec<Location>,
}

/// The two factors of a technology's costs: the research cost modifier (attribute 85, none
/// for the age technologies), which scales the costs above 0, and the scale of the cost
/// adjustments (1; attribute 38 for 0x2f3). Spies is not modelled.
pub fn factors(
    tech: TechId,
    attribute: &dyn Fn(AttributeId) -> Result<Amount, GenieError>,
) -> Result<(f32, i16), GenieError> {
    if tech == SPIES {
        return Err(GenieError::unsupported("the cost of Spies (FUN_1413cbd80)"));
    }
    let modifier = if is_age(tech) {
        0.0
    } else {
        // The attribute that scales research costs above 0 (`+0x2a8`: 85).
        attribute(attribute::RESEARCH_COST_MODIFIER)?.get()
    };
    let scale = if tech == SCALED_BY_38 {
        attribute(ATTRIBUTE_38)?.get() as i32 as i16
    } else {
        1
    };
    Ok((modifier, scale))
}

impl TechState {
    /// `FUN_1413cbd20`: the location at the building's master `building`, else the first.
    #[must_use]
    pub fn location(&self, building: MasterId) -> Option<&Location> {
        self.locations
            .iter()
            .find(|l| l.building == Some(building))
            .or_else(|| self.locations.first())
    }

    /// `FUN_1411c3190` for a research in state 2 or 4 (else none): the costs its queue entry
    /// records, attribute and amount: an override in use (or a `.dat` entry without an attribute,
    /// under −2) gives its attribute and amount, else the `.dat`'s entry; the adjustment times
    /// the scale adds to the amount (in 32 bits), which the modifier above 0 then scales (not
    /// truncated). An entry without either is (−1, 0).
    pub fn entry_costs(
        &self,
        def: &TechDef,
        (modifier, scale): (f32, i16),
    ) -> Result<[(Option<AttributeId>, f32); 3], GenieError> {
        let mut out = [(None, 0.0); 3];
        if !matches!(
            self.state,
            ResearchState::Researching | ResearchState::Queued
        ) {
            return Ok(out);
        }
        for (j, slot) in out.iter_mut().enumerate() {
            let o = *self.overrides.checked(j)?;
            let c = *def.costs.checked(j)?;
            let (attribute, base) = match o.used {
                OverrideUse::NONE => continue,
                OverrideUse::DAT if c.attribute.is_some() => (c.attribute, c.amount),
                _ => (o.attribute, o.amount),
            };
            let Some(attribute) = attribute else {
                continue;
            };
            let adj = self.cost_adj.checked(j)?.get();
            let mut v = (i32::from(adj) * i32::from(scale) + i32::from(base.get())) as f32;
            if 0.0 < modifier {
                v *= modifier;
            }
            *slot = (Some(attribute), v);
        }
        Ok(out)
    }

    /// The research's start test (`FUN_1413cbef0`) against what its queue entry `paid`: each
    /// cost with a paid `.dat` entry needs the player's attribute and what the entry paid of that
    /// attribute to cover its amount; false when one does not. A difference left to pay
    /// (`FUN_1411c68b0`) and a cost the building holds are not modelled.
    pub fn start_test(
        &self,
        def: &TechDef,
        factors: (f32, i16),
        paid: &[(Option<AttributeId>, f32); 3],
        attribute: &dyn Fn(AttributeId) -> Result<Amount, GenieError>,
    ) -> Result<bool, GenieError> {
        let (modifier, scale) = factors;
        for j in 0..3 {
            let o = *self.overrides.checked(j)?;
            let c = *def.costs.checked(j)?;
            let attr = if o.used == OverrideUse::OVERRIDE {
                o.attribute
            } else {
                c.attribute
            };
            let Some(attr) = attr.filter(|_| c.paid.is_paid()) else {
                continue;
            };
            let mut v = if o.used == OverrideUse::OVERRIDE {
                o.amount.as_f32()
            } else {
                (i32::from(self.cost_adj.checked(j)?.get()) * i32::from(scale)) as f32
                    + c.amount.as_f32()
            };
            if 0.0 < modifier {
                v = f32::from((v * modifier) as i32 as i16);
            }
            let before: f32 = paid
                .iter()
                .filter(|&&(k, _)| k == Some(attr))
                .map(|&(_, a)| a)
                .fold(0.0, |s, a| s + a);
            if c.paid == CostPaid::BUILDING {
                return Err(GenieError::unsupported(
                    "a research cost the building holds (FUN_141204310)",
                ));
            }
            if attribute(attr)?.get() + before < v {
                return Ok(false);
            }
            if 0.0 < v - before {
                return Err(GenieError::unsupported(
                    "a research's start that pays a difference (FUN_1411c68b0)",
                ));
            }
        }
        Ok(true)
    }

    /// `FUN_1413d4a00`: the three costs, attribute and amount, as the pending debit and the
    /// building's queue read them. An override in use (or a `.dat` entry not paid, under −2)
    /// gives its attribute and amount, else the `.dat`'s entry does; the adjustment times the
    /// scale adds to the amount, in 16 bits, which the modifier then scales (truncated). An
    /// override of 0 gives none; an amount below 1 leaves no attribute.
    pub fn listed_costs(
        &self,
        def: &TechDef,
        (modifier, scale): (f32, i16),
    ) -> Result<[(Option<AttributeId>, i16); 3], GenieError> {
        let mut out = [(None, 0); 3];
        for (j, slot) in out.iter_mut().enumerate() {
            let o = *self.overrides.checked(j)?;
            let c = *def.costs.checked(j)?;
            let (attribute, base) = match o.used {
                OverrideUse::NONE => continue,
                OverrideUse::DAT if c.paid.is_paid() => (c.attribute, c.amount),
                _ => (o.attribute, o.amount),
            };
            slot.0 = attribute;
            if attribute.is_none() {
                continue;
            }
            let mut v = self
                .cost_adj
                .checked(j)?
                .get()
                .wrapping_mul(scale)
                .wrapping_add(base.get());
            if 0.0 < modifier {
                v = (f32::from(v) * modifier) as i32 as i16;
            }
            slot.1 = v;
        }
        for slot in &mut out {
            if slot.1 < 1 {
                slot.0 = None;
            }
        }
        Ok(out)
    }

    /// A cost entry as the research test and the payment read it: the attribute (the
    /// override's where it is in use, 1), the amount and how it is paid, when the entry is paid
    /// at all (its attribute at least 0, its `.dat` entry paid or an override other than 0); the
    /// modifier above 0 scales the amount (truncated).
    fn payment(
        &self,
        def: &TechDef,
        j: usize,
        (modifier, scale): (f32, i16),
    ) -> Option<(AttributeId, f32, CostPaid)> {
        let o = *self.overrides.get(j)?;
        let c = *def.costs.get(j)?;
        let attribute = if o.used == OverrideUse::OVERRIDE {
            o.attribute
        } else {
            c.attribute
        };
        let attribute = attribute.filter(|_| c.paid.is_paid() || o.used != OverrideUse::NONE)?;
        let mut v = if o.used == OverrideUse::OVERRIDE {
            o.amount.as_f32()
        } else {
            (i32::from(self.cost_adj.get(j)?.get()) * i32::from(scale)) as f32 + c.amount.as_f32()
        };
        if 0.0 < modifier {
            v = f32::from((v * modifier) as i32 as i16);
        }
        Some((attribute, v, c.paid))
    }

    /// The research test (`FUN_1413cd600` without a building's holdings, `param_4` and
    /// `param_5` 0): a technology not in state 1 passes only with `repeat` (`param_6`), a state
    /// other than −1 and 0 and multiple researches allowed, or of the shared kind; then each
    /// paid cost needs the player's attribute to hold at least its amount. The game's
    /// per-technology overrides (`FUN_140f39f40` on the game's `+0x18`) are not held: none
    /// (probed for Loom, 22).
    pub fn can_research(
        &self,
        def: &TechDef,
        factors: (f32, i16),
        repeat: bool,
        attribute: &dyn Fn(AttributeId) -> Result<Amount, GenieError>,
    ) -> Result<bool, GenieError> {
        let shared = def.kind == TechType::SHARED;
        if self.state != ResearchState::Available
            && (!repeat
                || self.state == ResearchState::Disabled
                || self.state == ResearchState::Unavailable
                || !self.allow_multiple)
            && !shared
        {
            return Ok(false);
        }
        for j in 0..3 {
            let Some((attr, amount, paid)) = self.payment(def, j, factors) else {
                continue;
            };
            if paid == CostPaid::BUILDING {
                return Err(GenieError::unsupported(
                    "a research cost the building holds (FUN_141204310)",
                ));
            }
            if attribute(attr)?.get() < amount {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The payment (`FUN_1413d4370` with `param_3` 1 and no location test, `param_5` −1): the
    /// research test first (false: nothing paid); then each paid cost comes off its attribute,
    /// or, beyond what the attribute holds, the rest off gold and the attribute to 0. The
    /// technology takes state 4 and the building's master `building`. The game's statistics
    /// (`FUN_14097daf0`) are not held; a cost the building pays (`FUN_1411c7470`) and a
    /// technology of the shared kind (`FUN_1411c49a0`) are not modelled.
    pub fn pay(
        &mut self,
        def: &TechDef,
        factors: (f32, i16),
        repeat: bool,
        building: MasterId,
        attributes: &mut dyn Attributes,
    ) -> Result<bool, GenieError> {
        if !self.can_research(def, factors, repeat, &|i| attributes.get(i))? {
            return Ok(false);
        }
        for j in 0..3 {
            let Some((attr, amount, paid)) = self.payment(def, j, factors) else {
                continue;
            };
            if paid == CostPaid::BUILDING {
                return Err(GenieError::unsupported(
                    "a research cost the building pays (FUN_1411c7470)",
                ));
            }
            let have = attributes.get(attr)?;
            if amount <= have.get() {
                attributes.set(attr, have - Amount::new(amount))?;
            } else {
                let gold = attributes.get(GOLD)?;
                attributes.set(GOLD, gold - Amount::new(amount - have.get()))?;
                attributes.set(attr, Amount::ZERO)?;
            }
        }
        if def.kind == TechType::SHARED {
            return Err(GenieError::unsupported(
                "a research of the shared kind (FUN_1411c49a0)",
            ));
        }
        self.state = ResearchState::Queued;
        self.building_master = Some(building);
        Ok(true)
    }

    /// `FUN_1413d1050`: the technology may be queued again (not of the shared kind, multiple
    /// researches allowed, and its researches queued and done below its cap with `stack`, or
    /// below 0x100 under the player's `+0x648`, `many`).
    #[must_use]
    pub fn repeatable(&self, def: &TechDef, many: bool) -> bool {
        if def.kind == TechType::SHARED || !self.allow_multiple {
            return false;
        }
        let done = i32::from(self.pending.get()) + i32::from(self.times_researched.get());
        let capped = done < i32::from(self.research_cap.limit());
        (self.stack && capped) || (many && done < 0x100)
    }
}

/// The player's attributes as a technology's costs read and write them.
pub trait Attributes {
    /// Attribute `i`.
    fn get(&self, i: AttributeId) -> Result<Amount, GenieError>;
    /// Attribute `i` set to `v`.
    fn set(&mut self, i: AttributeId, v: Amount) -> Result<(), GenieError>;
    /// Attribute `i` less `amount` (a payment's `SUBSS`), by default through [`Attributes::get`]
    /// and [`Attributes::set`].
    ///
    /// # Errors
    ///
    /// An attribute the player does not have.
    fn subtract(&mut self, i: AttributeId, amount: Amount) -> Result<(), GenieError> {
        let v = self.get(i)?;
        self.set(i, v - amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::masters::ids;
    use crate::players::attribute;
    use crate::production::LocationButton;
    use crate::test_ids::master_id;
    use crate::world::WholeSeconds;

    /// Loom (22) as player 2 of MarvinBlue/426048731 holds it before its RESEARCH in 3842.
    fn loom() -> (TechState, TechDef) {
        let state = TechState {
            state: ResearchState::Available,
            allow_multiple: true,
            overrides: [CostOverride::UNSET; 3],
            research_cap: ResearchCap::of_data(-1),
            building_master: None,
            locations: vec![Location {
                building: Some(ids::TOWN_CENTRE),
                time: WholeSeconds::of_data(25),
                button: LocationButton::of_data(6),
                adjustment: Seconds::ZERO,
            }],
            ..TechState::default()
        };
        let none = Cost {
            attribute: None,
            amount: crate::objects::masters::CostAmount::of_data(0),
            paid: CostPaid::UNPAID,
        };
        let gold = Cost {
            attribute: Some(attribute::GOLD),
            amount: crate::objects::masters::CostAmount::of_data(50),
            paid: CostPaid::PLAYER,
        };
        let def = TechDef {
            costs: [gold, none, none],
            ..TechDef::default()
        };
        (state, def)
    }

    struct Held([Amount; 4]);

    impl Attributes for Held {
        fn get(&self, i: AttributeId) -> Result<Amount, GenieError> {
            Ok(self.0[i.index()])
        }
        fn set(&mut self, i: AttributeId, v: Amount) -> Result<(), GenieError> {
            self.0[i.index()] = v;
            Ok(())
        }
    }

    #[test]
    fn loom_lists_its_gold_and_pays_it_once_queued() {
        let (mut s, d) = loom();
        assert_eq!(
            s.listed_costs(&d, (0.0, 1)).unwrap(),
            [(Some(attribute::GOLD), 50), (None, 0), (None, 0)]
        );
        assert!(!s.repeatable(&d, false));
        // the game's attributes in 3842 (80 food, 157 wood, 200 stone, 100 gold) and after it
        let mut held = Held([
            Amount::new(79.956_116),
            Amount::new(157.05705),
            Amount::new(200.0),
            Amount::new(100.0),
        ]);
        assert_eq!(
            s.pay(&d, (0.0, 1), false, ids::TOWN_CENTRE, &mut held),
            Ok(true)
        );
        assert_eq!(
            held.0,
            crate::test_ids::amounts([79.956_116, 157.05705, 200.0, 50.0])
        );
        assert_eq!(
            (s.state, s.building_master),
            (ResearchState::Queued, Some(master_id(109)))
        );
        // queued, it may not be researched again
        assert_eq!(
            s.pay(&d, (0.0, 1), false, ids::TOWN_CENTRE, &mut held),
            Ok(false)
        );
        // its queue entry records the 50 gold, which its start finds paid
        let paid = s.entry_costs(&d, (0.0, 1)).unwrap();
        assert_eq!(
            paid,
            [(Some(attribute::GOLD), 50.0), (None, 0.0), (None, 0.0)]
        );
        let get = |i: AttributeId| held.get(i);
        assert_eq!(s.start_test(&d, (0.0, 1), &paid, &get), Ok(true));
        // without the entry's payment the 50 gold left would do too, but that difference is
        // the game's FUN_1411c68b0
        assert!(s.start_test(&d, (0.0, 1), &[(None, 0.0); 3], &get).is_err());
        assert_eq!(
            s.location(ids::TOWN_CENTRE).map(|l| l.time),
            Some(WholeSeconds::of_data(25))
        );
    }

    #[test]
    fn a_cost_beyond_the_attribute_takes_the_rest_from_gold() {
        let (mut s, mut d) = loom();
        d.costs[0].attribute = Some(attribute::WOOD);
        let mut held = Held([
            Amount::ZERO,
            Amount::new(30.0),
            Amount::ZERO,
            Amount::new(100.0),
        ]);
        // the test asks for 50 wood: 30 is too little
        assert_eq!(
            s.pay(&d, (0.0, 1), false, ids::TOWN_CENTRE, &mut held),
            Ok(false)
        );
        // the modifier scales the amount (truncated) before the test
        assert_eq!(
            s.pay(&d, (0.5, 1), false, ids::TOWN_CENTRE, &mut held),
            Ok(true)
        );
        assert_eq!(held.0, crate::test_ids::amounts([0.0, 5.0, 0.0, 100.0]));
        assert_eq!(
            s.listed_costs(&d, (0.5, 1)).unwrap(),
            [(Some(attribute::WOOD), 25), (None, 0), (None, 0)]
        );
    }
}
