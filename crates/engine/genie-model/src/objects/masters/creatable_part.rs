//! A master's creatable layer: the TRIBE combat master's values (types 70 and 80, the `.dat`'s
//! creatable section; FD-046-D05).

use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::combat::{Firepower, MissileCount};
use crate::objects::Regeneration;
use crate::objects::SpriteId;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::COST_SLOTS;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::CombatPart;
use crate::objects::masters::ConversionAdjust;
use crate::objects::masters::CostSlot;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::InvulnerabilityLevel;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MissilePart;
use crate::objects::masters::MovingPart;
use crate::objects::masters::SpacingModifier;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::TrainLocation;
use crate::objects::masters::TrainLocationEntry;
use crate::objects::masters::VolleySpread;
use crate::world::WholeSeconds;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;
use std::sync::LazyLock;

/// The creatable layer of a master of type 70 or 80 (`TRIBE_Master_Combat_Object`,
/// `+0x880`..`+0x94f`; the `.dat`'s creatable section): its costs, train locations, volleys, charge
/// and the values its interface shows.
#[derive(Clone, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreatablePart {
    /// `+0x880`, the cost slots in their places (an empty one among them).
    pub cost: [CostSlot; COST_SLOTS],
    /// The build time: the first train location's train time, 0 without one.
    pub build_time: WholeSeconds,
    /// The train locations' ids (`+0x898`, each entry's first short): the units that build a
    /// building, the buildings that train a unit (`FUN_141263610`).
    pub trained_at: Vec<MasterId>,
    /// `charge_type` (`+0x922`): 6 and 7 add `charge_event` (`+0x920`) to the range against some
    /// targets (slot 0x2f8).
    pub charge_kind: ChargeKind,
    /// `+0x8bc`, `mFormationCategory`: 0 (villagers) keeps a unit out of the groups an order's
    /// splitter forms (`FUN_140fd76d0`).
    pub formation_category: FormationClass,
    /// `+0x8d9`, `hero_flag`: a combat master it grants
    /// [`HeroFlags::PROTECTED_FORMATION`] joins a group's fifth sub-formation (`FUN_141011300`).
    pub hero_flag: HeroFlags,
    /// `+0x8b8`, `mSpacingModifier`: with the radius on x the unit's radius in a group's
    /// formation (`FUN_141025920`). The player's masters copy it from the data file's.
    pub spacing_modifier: SpacingModifier,
    /// `invulnerability_level` (`+0x930`): a hit point floor.
    pub invulnerability: InvulnerabilityLevel,
    /// `attack_priority` (`+0x92c`, `FUN_141202220`): 3 attacks only the classes of
    /// `FUN_140fddf10`.
    pub attack_priority: AttackPriority,
    /// `special_attack_flag` (`+0x8d8`).
    pub special_attack: SpecialAttack,
    /// The special graphic's sprite (the `.dat`'s `special_graphic`), −1 for none: a herder
    /// leading livestock shows it. Whether the master has one (`+0x8e8`, the sprite the loader
    /// looks up for it) is whether it is set.
    pub special_sprite: Option<SpriteId>,
    /// `max_charge` (`+0x918`).
    pub max_charge: Charge,
    /// `charge_target` (`+0x924`).
    pub charge_target: ChargeTarget,
    /// The sprite it waits with between swings (`+0x910`, −1 for none).
    pub idle_attack_sprite: Option<SpriteId>,
    /// `volley_fire_amount` (`+0x8c0`, the `.dat`'s total projectiles): a new object's missiles
    /// per volley (`+0xa00`).
    pub volley_fire: Firepower,
    /// `max_attacks_in_volley` (`+0x8c4`, the `.dat`'s max total projectiles).
    pub max_volley: MissileCount,
    /// `volley_x_spread`, `volley_y_spread` and `volley_start_spread_adjustment` (`+0x8c8`..
    /// `+0x8d0`, the `.dat`'s projectile spawning area): how far a volley's missiles scatter.
    pub volley_spread: VolleySpread,
    /// `volley_missile_id` (`+0x8d4`, the `.dat`'s secondary projectile), None for −1.
    pub volley_missile: Option<MasterId>,
    /// `mChargeProjectileUnit` (`+0x928`), None for −1.
    pub charge_projectile: Option<MasterId>,
    /// The train locations (`+0x898`, 12 bytes each; the save's `train_location`), which
    /// [`Self::trained_at`] and [`Self::build_time`] follow.
    pub train_locations: Vec<TrainLocation>,
    /// `+0x940`, `mTrainLocationEntryMod`: the train location the attributes 42, 43, 58 and 101
    /// change.
    pub train_location_entry: TrainLocationEntry,
    /// `+0x8b4`, `regeneration_rate`.
    pub regeneration: Regeneration,
    /// `+0x91c`, `mRechargeRate`.
    pub recharge_rate: ChargeRate,
    /// `+0x920`, `mChargeEvent`.
    pub charge_event: ChargeEvent,
    /// `+0x934`, `+0x938`, `+0x93c`: `mMinConversionTimeAdj`, `mMaxConversionTimeAdj`,
    /// `mConversionChanceAdj`.
    pub conversion: ConversionAdjust,
    /// `+0x908`, the hero glow sprite (−1 for none).
    pub hero_glow_sprite: Option<SpriteId>,
    /// `+0x8f0`, the garrison sprite (−1 for none).
    pub garrison_sprite: Option<SpriteId>,
    /// `+0x8b0`, `orig_pierce_armor`: the pierce armor the master shows.
    pub pierce_armor_shown: DamageAmount,
}

/// What a master without the layer reads ([`Master::creatable_part`]): the part's default.
static ABSENT: LazyLock<CreatablePart> = LazyLock::new(CreatablePart::default);

impl Master {
    /// A master of type `kind` with the defaults of the layers its type holds (the animated layer
    /// for [`MasterKind::has_animated_layer`], the moving one for
    /// [`MasterKind::has_moving_layer`], the action one for [`MasterKind::has_action_layer`], the
    /// combat one for [`MasterKind::has_combat_layer`], the missile one for
    /// [`MasterKind::has_missile_layer`], the creatable one for
    /// [`MasterKind::has_creatable_layer`], the building one for
    /// [`MasterKind::has_building_layer`]).
    #[must_use]
    pub fn of_kind(kind: MasterKind) -> Self {
        Self {
            kind,
            animated: kind.has_animated_layer().then(AnimatedPart::default),
            moving: kind.has_moving_layer().then(MovingPart::default),
            action: kind.has_action_layer().then(ActionPart::default),
            combat: kind.has_combat_layer().then(CombatPart::default),
            missile: kind.has_missile_layer().then(MissilePart::default),
            creatable: kind.has_creatable_layer().then(CreatablePart::default),
            building: kind.has_building_layer().then(BuildingMaster::default),
            ..Self::default()
        }
    }

    /// Its creatable layer, or for a master of another type the part's defaults (what the
    /// game's static class answers for a value it does not have).
    #[must_use]
    pub fn creatable_part(&self) -> &CreatablePart {
        self.creatable.as_ref().unwrap_or(&ABSENT)
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::MissileCount;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;

    /// A master of type 70 or 80 holds the creatable layer; a moving one (30), a missile (60)
    /// and one of the data's own type 90 do not (the `.dat`'s creatable section).
    #[test]
    fn masters_hold_the_creatable_layer_of_their_kind() {
        let layered = |kind| {
            Master::of_kind(MasterKind::of_data(kind))
                .creatable
                .is_some()
        };
        assert_eq!(
            [30, 60, 70, 80, 90].map(layered),
            [false, false, true, true, false]
        );
    }

    /// A master without the layer reads the part's defaults, one with it its own values.
    #[test]
    fn masters_read_their_creatable_layer_or_the_defaults() {
        let moving = Master::of_kind(MasterKind::MOVING);
        assert_eq!(moving.creatable_part(), &CreatablePart::default());
        let mut combat = Master::of_kind(MasterKind::COMBAT);
        if let Some(p) = combat.creatable.as_mut() {
            p.max_volley = MissileCount::of_data(3);
        }
        assert_eq!(combat.creatable_part().max_volley.get(), 3);
    }
}
