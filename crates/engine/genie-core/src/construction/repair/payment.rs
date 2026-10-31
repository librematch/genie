//! What a repair reaches and pays: a building's and a combat unit's slot 0x8c8 by the
//! repairer's player.

use crate::GenieError;
use crate::construction::repair::ONE;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type;
use crate::players::Amount;
use crate::players::attribute::AttributeId;

/// The classes whose repair pays its unit share (`FUN_1412143b0`: the ships 20, 21, 22, 2 and
/// 53, the siege 13 and the classes 51, 54 and 55).
pub(super) const COSTED: [ClassId; 9] = [
    class::TRANSPORT,
    class::FISHING_SHIP,
    class::WARSHIP,
    class::TRADE_BOAT,
    class::CLASS_53,
    class::SIEGE,
    class::CLASS_51,
    class::CLASS_54,
    class::CLASS_55,
];
/// The trebuchets' classes (51, 54), whose building repair is the unit's.
pub(super) const TREBUCHETS: [ClassId; 2] = [class::CLASS_51, class::CLASS_54];
/// The owner's attributes of a unit's and a building's repair cost (270, 271: `player+0x100`
/// at `+0x870` and `+0x878`).
pub const UNIT_REPAIR_COST: usize = 270;
/// The building repair cost attribute (271).
pub const BUILDING_REPAIR_COST: usize = 271;

/// One object a repair reaches (the target, an annex's main building, a main building's
/// annexes) as the repair reads and changes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Repaired {
    /// The object.
    pub id: ObjectId,
    /// Its master's type.
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its master.
    pub master: MasterId,
    /// Its master's hit points (`+0x530`).
    pub master_hp: i16,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
    /// Its hit point powerup (`+0xa38`, `+0xa3c`).
    pub hp_powerup: HpPowerup,
    /// An annex's main building (`+0xbb8`).
    pub main: Option<ObjectId>,
    /// A main building's annexes (`+0xbc0`, its four slots in their order).
    pub annexes: Vec<ObjectId>,
    /// Its owner's attribute 270 ([`UNIT_REPAIR_COST`]): a unit's repair cost rate.
    pub unit_repair_cost: f32,
    /// Its owner's attribute 271 ([`BUILDING_REPAIR_COST`]): a building's repair cost rate.
    pub building_repair_cost: f32,
}

/// What a repair pays with: the repairer's player's masters and attributes (`FUN_14162fb60`
/// with the repairer's player).
pub struct Payer<'a> {
    /// The repairer's player's masters.
    pub masters: &'a crate::objects::masters::Masters,
    /// The repairer's player's attributes.
    pub attributes: &'a mut dyn crate::technology::Attributes,
}

impl std::fmt::Debug for Payer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payer").finish_non_exhaustive()
    }
}

/// The object of `set` with id `id`.
pub(super) fn find(set: &[Repaired], id: ObjectId) -> Result<usize, GenieError> {
    set.iter()
        .position(|o| o.id == id)
        .ok_or(GenieError::unreachable("a repair's object outside its set"))
}

/// A building's slot 0x8c8 (`FUN_1411d37d0`) by `amount` for the repairer (`payer`): a
/// trebuchet's class repairs as a unit ([`repair_unit`]); one below 1 hit point (`COMISS`: a NaN
/// is) takes nothing (2). From its repairer (`top`) an annex's repair is its main building's.
/// Else each of its annexes (a combat unit's or a building's type) is repaired by the amount
/// first (not `top`); then the amount, at most what its hit points lack of its master's, costs
/// its owner's building repair rate times the amount over its master's hit points of its
/// master's costs ([`crate::production::pay`]); paid, it heals as a unit's does, else 2.
///
/// # Errors
///
/// An object outside `set`, and an attribute the player does not have.
pub fn repair_building(
    set: &mut [Repaired],
    idx: usize,
    amount: f32,
    top: bool,
    payer: &mut Payer<'_>,
) -> Result<u8, GenieError> {
    if TREBUCHETS.contains(&set[idx].class) {
        return repair_unit(set, idx, amount, payer);
    }
    if crate::math::fmath::below_or_unordered(set[idx].hp.get(), ONE) {
        return Ok(2);
    }
    if top && let Some(main) = set[idx].main {
        let m = find(set, main)?;
        return repair_building(set, m, amount, true, payer);
    }
    for annex in set[idx].annexes.clone() {
        let a = find(set, annex)?;
        match set[a].kind {
            master_type::COMBAT => {
                let _ = repair_unit(set, a, amount, payer)?;
            }
            master_type::BUILDING => {
                let _ = repair_building(set, a, amount, false, payer)?;
            }
            _ => {}
        }
    }
    let o = &set[idx];
    let most = f32::from(o.master_hp);
    let mut amount = amount;
    if most < o.hp.get() + amount {
        amount = most - o.hp.get();
    }
    let share = o.building_repair_cost * amount / most;
    if crate::production::pay(payer.masters, o.master, share, &mut *payer.attributes)? {
        return repair_unit(set, idx, amount, payer);
    }
    Ok(2)
}

/// A combat unit's slot 0x8c8 (`FUN_1412143b0`, also a building's last step) by `amount`: a
/// class of [`COSTED`] first takes at most what its hit points lack of its most (slots 0xc0 and
/// 200) and pays its owner's unit repair rate times the amount over its most of its master's
/// costs (unpaid: 2); then it heals (`FUN_141193b30`, [`crate::combat::damage::heal_hp`]): 1 once full,
/// else 0.
///
/// # Errors
///
/// An attribute the player does not have.
pub fn repair_unit(
    set: &mut [Repaired],
    idx: usize,
    amount: f32,
    payer: &mut Payer<'_>,
) -> Result<u8, GenieError> {
    let o = &mut set[idx];
    let mut amount = amount;
    if COSTED.contains(&o.class) {
        let most = f32::from(o.master_hp) + o.hp_powerup.max.get();
        let now = (o.hp_powerup.room() + o.hp).get();
        if most < now + amount {
            amount = most - now;
        }
        let share = o.unit_repair_cost * amount / most;
        if !crate::production::pay(payer.masters, o.master, share, &mut *payer.attributes)? {
            return Ok(2);
        }
    }
    let full = crate::combat::damage::heal_hp(
        &mut o.hp,
        &mut o.hp_powerup,
        o.master_hp,
        HitPoints::new(amount),
    );
    Ok(u8::from(full))
}

/// A repairer's player's attributes as its repair pays from them ([`crate::construction::repair::Payer`]):
/// the attributes as the repair reads them, each
/// payment kept as an add the caller makes the owner's right after the repair's step (a
/// subtraction, so the player's value comes out as the repair saw it).
pub(crate) struct PaidAttributes {
    pub(super) values: Vec<Amount>,
    pub(crate) adds: Vec<(AttributeId, Amount)>,
}

impl PaidAttributes {
    pub(crate) fn new(attributes: &[Amount]) -> Self {
        Self {
            values: attributes.to_vec(),
            adds: Vec::new(),
        }
    }
}

impl crate::technology::Attributes for PaidAttributes {
    fn get(&self, i: AttributeId) -> Result<Amount, GenieError> {
        self.values
            .get(i.index())
            .copied()
            .ok_or(crate::world::NO_ATTRIBUTE)
    }

    fn set(&mut self, _: AttributeId, _: Amount) -> Result<(), GenieError> {
        Err(GenieError::unreachable(
            "a repair's payment that sets an attribute",
        ))
    }

    fn subtract(&mut self, i: AttributeId, amount: Amount) -> Result<(), GenieError> {
        let v = self
            .values
            .get_mut(i.index())
            .ok_or(crate::world::NO_ATTRIBUTE)?;
        *v -= amount;
        self.adds.push((i, -amount));
        Ok(())
    }
}
