//! A master's combat layer: the RGE combat master's values (types 50 and up but 90, the `.dat`'s
//! type-50 section; FD-046-D05).

use crate::combat::Firepower;
use crate::combat::missile::MissSpread;
use crate::map::Distance;
use crate::map::Offset;
use crate::objects::Frame;
use crate::objects::SpriteId;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageShare;
use crate::objects::masters::HitChance;
use crate::objects::masters::Master;
use crate::objects::masters::MasterField;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::Shown;
use crate::world::Seconds;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// The combat layer of a master of type 50 and up but 90 (`RGE_Master_Combat_Object`,
/// `+0x7f8`..`+0x87f`; the `.dat`'s type-50 section): its attack, its armor, its blast and the
/// values its interface shows. A missile (60), a combat unit (70) and a building (80) hold it.
///
/// A combat master's attack reads its standing sprite (`+0x470`, the base's), its attack sprite
/// (`+0x7f8`, `fight_sprite_id`), a second attack sprite (`+0x878`, the `.dat`'s
/// `attack_graphic2`), `missile_id`, `fire_missile_at_frame`, `interrupt_frame`,
/// `base_hit_chance`, `speed_of_attack` (the reload time) and its weapons (class, amount); and
/// the sprite it waits with between swings (`+0x910`, the creatable layer's). A hit on an object
/// of this master reads (`FUN_141082530`) its armor classes (class, amount; `+0x808`, count
/// `+0x802`), its base armor for a class it does not list (`+0x800`), `bonus_damage_resistance`
/// (`+0x824`), `combat_ability` (`+0x840`), `damage_reflection` (`+0x864`),
/// `friendly_fire_damage` (`+0x868`) and the base's `area_effect_object_level` (`+0x62b`, the
/// blasts that reach it), with the creatable layer's `invulnerability_level` (`+0x930`: a hit
/// point floor, `FUN_141202a80`), `special_attack_flag` (`+0x8d8`) and special graphic
/// (`+0x8e8`); an attacker of it reads `area_effect_range` (`+0x82c`), `area_effect_level`
/// (`+0x830`), `area_damage` (`+0x834`) and the creatable layer's `max_charge` (`+0x918`) and
/// `charge_target` (`+0x924`).
#[derive(Clone, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CombatPart {
    /// `weapon_range` (`+0x828`, object slot 0x230).
    pub weapon_range: Distance,
    /// `minimum_weapon_range` (`+0x850`, slot 0x240).
    pub min_range: Distance,
    /// `+0x820`, the `.dat`'s `defense_terrain_bonus` (a terrain restriction): a completed
    /// building keeps to restore a tile's terrain whose value in it is neither 0 nor 1.
    pub terrain_bonus: RestrictionField,
    /// `+0x800`: its base armor, for a class its armor classes do not list.
    pub base_armor: DamageAmount,
    /// Its armor classes (class, amount; `+0x808`, count `+0x802`).
    pub armors: Vec<ClassAmount>,
    /// `bonus_damage_resistance` (`+0x824`).
    pub resistance: DamageShare,
    /// `combat_ability` (`+0x840`).
    pub combat_ability: CombatAbility,
    /// `damage_reflection` (`+0x864`).
    pub reflection: DamageShare,
    /// `friendly_fire_damage` (`+0x868`).
    pub friendly_fire: DamageShare,
    /// `area_effect_range` (`+0x82c`): its blast's radius.
    pub area_effect: Distance,
    /// `area_effect_level` (`+0x830`).
    pub blast_level: BlastLevel,
    /// `area_damage` (`+0x834`).
    pub area_damage: DamageShare,
    /// Its attack sprite (`+0x7f8`, `fight_sprite_id`).
    pub attack_sprite: Option<SpriteId>,
    /// A second attack sprite (`+0x878`).
    pub attack_sprite2: Option<SpriteId>,
    /// `missile_id`, −1 for none: every short the effects leave in it is kept.
    pub missile: MasterField,
    /// `fire_missile_at_frame`.
    pub fire_frame: Frame,
    /// `interrupt_frame`.
    pub interrupt_frame: Frame,
    /// `base_hit_chance`.
    pub hit_chance: HitChance,
    /// `speed_of_attack`: the reload time.
    pub reload: Seconds,
    /// Its weapons (class, amount).
    pub weapons: Vec<ClassAmount>,
    /// Where its missile starts from its place, turned by its facing (`+0x844`..`+0x84c`,
    /// `weapon_offset`), and how far a missed missile strays per tile of the way
    /// (`missed_missile_spread`, `+0x860`).
    pub weapon_offset: Offset,
    /// How far a missed missile strays per tile of the way (`missed_missile_spread`, `+0x860`).
    pub missed_spread: MissSpread,
    /// `garrison_firepower` (`+0x870`): what a unit adds to the volleys of a building it is
    /// inside; one at most 0 adds its magnitude.
    pub garrison_firepower: Firepower,
    /// `+0x856`, `+0x858` and `+0x854`: the attack, range and melee armor a combat master shows
    /// (`orig_weapon`, `orig_weapon_range`, `orig_armor`).
    pub shown: Shown,
}

impl CombatPart {
    /// The layer of a master without the `.dat`'s type-50 section: the defaults but no terrain
    /// bonus (−1), what the import gave such a master and what a master without the layer reads
    /// ([`Master::combat_part`]); the save import starts a layer from it.
    pub const ABSENT: Self = Self {
        weapon_range: Distance::ZERO,
        min_range: Distance::ZERO,
        terrain_bonus: RestrictionField::NONE,
        base_armor: DamageAmount::ZERO,
        armors: Vec::new(),
        resistance: DamageShare::ZERO,
        combat_ability: CombatAbility::NONE,
        reflection: DamageShare::ZERO,
        friendly_fire: DamageShare::ZERO,
        area_effect: Distance::ZERO,
        blast_level: BlastLevel::NONE,
        area_damage: DamageShare::ZERO,
        attack_sprite: None,
        attack_sprite2: None,
        missile: MasterField::NONE,
        fire_frame: Frame::ZERO,
        interrupt_frame: Frame::ZERO,
        hit_chance: HitChance::of_data(0),
        reload: Seconds::ZERO,
        weapons: Vec::new(),
        weapon_offset: Offset::ZERO,
        missed_spread: MissSpread::ZERO,
        garrison_firepower: Firepower::ZERO,
        shown: Shown::ZERO,
    };
}

impl Master {
    /// Its combat layer, or for a master of another type the part's defaults with no terrain
    /// bonus (what the game's lower class answers for a value it does not have,
    /// [`CombatPart::ABSENT`]).
    #[must_use]
    pub fn combat_part(&self) -> &CombatPart {
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        self.combat
            .as_ref()
            .unwrap_or(const { &CombatPart::ABSENT })
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CombatPart;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::RestrictionField;

    /// A master of type 50 and up but 90 holds the combat layer, a missile (60) and a building
    /// (80) too; a moving one (30), an action one (40) and one of the data's own type 90 do not.
    #[test]
    fn masters_hold_the_combat_layer_of_their_kind() {
        let layered = |kind| Master::of_kind(MasterKind::of_data(kind)).combat.is_some();
        assert_eq!(
            [30, 40, 50, 60, 70, 80, 90].map(layered),
            [false, false, true, true, true, true, false]
        );
    }

    /// A master without the layer reads no terrain bonus (−1) and the rest's defaults, as the
    /// import gave a unit without the type-50 section.
    #[test]
    fn masters_without_the_combat_layer_read_no_terrain_bonus() {
        let moving = Master::of_kind(MasterKind::MOVING);
        assert_eq!(moving.combat_part().terrain_bonus, RestrictionField::NONE);
        assert_eq!(
            moving.combat_part(),
            &CombatPart {
                terrain_bonus: RestrictionField::NONE,
                ..CombatPart::default()
            }
        );
    }
}
