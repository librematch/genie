//! The damage side of a hit: what it reads of its target and its attacker and what it changes.
//! The target's slot 0xe8 takes the damage: a static object's (`FUN_14118efd0`), a combat
//! object's of type 50 or 60 (`FUN_141082e80`), of type 70 (`FUN_1412108e0`, which adds the
//! special attack, the retaliation, the least damage and the reflected damage) and a building's
//! (`FUN_1411cea90`: an annex's hit goes to its main building, its least hit, its razing's
//! counts and history event). The damage is the
//! armor's (slot 0xf0, `FUN_141082530`), less what a shield blocks (`FUN_14108d8a0`); a charge
//! (slots 0x2f0, 0x300, 0x318) adds to it, and a blast (slot 0x848, `FUN_14107f690`) spreads it.
//! When a melee hit lands is [`crate::combat::attack`]'s.
//!
//! Of these, MarvinBlue/426048731 runs only the armor and the damage of a villager's hits on
//! sheep (update 1170). The charges, shields, blasts, terrain bonuses, special attacks,
//! retaliations and reflections are ports of the code, checked by the unit tests alone.

mod allied;
mod armor;
mod armor_pair;
mod blasts;
mod hit_factors;
mod hit_points;
mod shown_values;
mod taking;

pub use self::allied::Allied;
pub use self::armor::armor_damage;
use self::armor::{master_melee_armor, melee_armor};
pub use self::armor_pair::ArmorPair;
pub use self::blasts::blast;
#[cfg(test)]
use self::blasts::blast_tiles;
use self::blasts::damage;
pub(crate) use self::hit_factors::SPECIAL_CHARGE_GAP;
pub use self::hit_factors::{
    building_elevation_factor, charge_hit, charge_works, charged, elevation_factor, facing_offset,
    range_charge_hit,
};
use self::hit_factors::{charge_damage, special_charge_ready};
pub(crate) use self::hit_points::invulnerability_floor;
use self::hit_points::{effective, hp_floor};
pub use self::hit_points::{gain_hp, heal_hp, lose, shield};
pub use self::shown_values::{
    BONUS_DAMAGE, Shown, bonus_damage, shown_attack, shown_melee_armor, shown_range,
};
use self::taking::kill_bonus;
pub use self::taking::take;
#[cfg(test)]
use self::taking::take_static;
use crate::combat::Charge;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::math::Angle;
#[cfg(test)]
use crate::math::fmath;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Relics;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;

/// Elevation: a higher attacker hits at 1.25 (`DAT_142eb56a8`), a lower one at 0.75
/// (`DAT_142e7f388`), each plus its player's and the target's player's attributes (211/272,
/// 212/273).
const HIGHER: f32 = crate::math::image::damage::HIGHER;
const LOWER: f32 = crate::math::image::damage::LOWER;
/// A building's hits with a specific relic inside (`DAT_142ed66b4`, 1.2), and with any relic
/// (`DAT_142e86c14`, 1.1), [`building_elevation_factor`].
const SPECIFIC_RELIC_BOOST: f32 = crate::math::image::damage::SPECIFIC_RELIC_BOOST;
const RELIC_BOOST: f32 = crate::math::image::damage::RELIC_BOOST;
/// The gap below which the game takes a distance as zero (`DAT_142eb0c60`), and the share of
/// reflected damage above which a target reflects.
const EPSILON: f32 = crate::math::image::damage::EPSILON;
/// A special attack and a retaliation need their attacker within 1.2 facets of where the target
/// faces (`DAT_142ed66b4`, `DAT_142eb99e4`); the special attack takes the hit's damage to 0.66
/// (`DAT_142f3855c`); a retaliation answers attackers of a weapon range below 0.2
/// (`DAT_142eae8d0`), four times over an attacker whose charge is ready (`DAT_142eb0f48`).
const FACING: f32 = crate::math::image::damage::FACING;
const MINUS_FACING: f32 = crate::math::image::damage::MINUS_FACING;
const SPECIAL: f32 = crate::math::image::damage::SPECIAL;
const RETALIATION_RANGE: f32 = crate::math::image::damage::RETALIATION_RANGE;
const RETALIATION_SCALE: f32 = crate::math::image::damage::RETALIATION_SCALE;
/// A melee attacker (weapon range below 0.99, `DAT_142eb569c`) with an area effect tramples:
/// its blast goes about itself (`FUN_1412115e0`).
pub const BLAST_RANGE: f32 = crate::math::image::damage::BLAST_RANGE;
/// What a player's attributes add to the elevation factor of a hit ([`elevation_factor`]): from
/// higher up (attribute 211 for its own hits, 272 for the hits it takes) or from lower down (212,
/// 273).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ElevationBonus {
    /// What a hit from higher up adds.
    pub higher: f32,
    /// What a hit from lower down adds.
    pub lower: f32,
}

/// A cone blast (its level's bit 7) reaches objects within half a facet of where its attacker
/// faces (`DAT_142e7f384`, `DAT_142eb6e58`), or within 1.67 facets (`DAT_142f4fea8`) for a wide
/// one (bit 5).
const CONE: f32 = crate::math::image::damage::CONE;
const MINUS_CONE: f32 = crate::math::image::damage::MINUS_CONE;
const WIDE_CONE: f32 = crate::math::image::damage::WIDE_CONE;

/// The class whose damage alerts its target's owner itself (`-0xe6c` raw): its target's unit AI
/// gets no notify 500.
const ELEPHANT_ARCHER: ClassId = class::CLASS_64;
/// The rams (masters 35, 422, 548 and 1258): their blast reaches only buildings and their
/// target.
const RAMS: [MasterId; 4] = [
    MasterId::named(35),
    MasterId::named(422),
    MasterId::named(548),
    MasterId::named(1258),
];
/// 1 (`DAT_14330eb8c`, read into `XMM9` at `0x1411cec8e`): a building's least hit, the hit
/// points below which a hit razes a building or kills a unit, and what each razing count adds.
const ONE: f32 = crate::math::image::damage::ONE;
/// The least hit of the attacker classes [`LEAST_HIT_CLASSES`] under the game's setting `+0x680`
/// at 1 (`DAT_142eb5690`, read into `XMM6` at `0x1411ceea4`).
const LEAST_CLASS_HIT: f32 = crate::math::image::damage::LEAST_CLASS_HIT;
/// The attacker classes whose least hit on a building is [`LEAST_CLASS_HIT`] (decoded `0`, `0x24`,
/// `0x16`, `0x2c`, `0x34` at `0x1411ced23`..`0x1411cee9e`).
const LEAST_HIT_CLASSES: [ClassId; 5] = [
    class::CLASS_0,
    class::CLASS_36,
    class::WARSHIP,
    class::CLASS_44,
    class::TOWER,
];

/// An object as a hit reads and changes it: the target, and the attacker as the target of a
/// retaliation or of reflected damage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Victim {
    /// The object.
    pub id: ObjectId,
    /// Its place in its tile's object list (the map tile's `+0x28`, in the order
    /// `FUN_140f5f0a0` and `FUN_140f5f560` keep), which a blast walks a tile by ([`blast`]).
    pub tile_place: usize,
    /// Its hit points (`+0x278`), what it holds (`+0x310`), its state and place.
    pub hp: HitPoints,
    /// What it holds (`+0x310`).
    pub amount: Amount,
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// Its place.
    pub position: Position,
    /// Its master's type, class, id, radii, first storage amount (`+0x610`, which a gatherer
    /// reads), `attribute_max_amount` and `area_effect_object_level` (`+0x62b`, the blast
    /// levels that reach it).
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its master.
    pub master_id: MasterId,
    /// Its master's radii.
    pub radius: Footprint,
    /// Its master's `radius_z` (`+0x5e0`): a missile meets it within that height above it
    /// (`FUN_14113f740`).
    pub height: f32,
    /// Its master's first storage amount (`+0x610`), which a gatherer reads.
    pub storage: Amount,
    /// Its master's `attribute_max_amount`.
    pub capacity: i16,
    /// `+0x644`, `worker_num`: the actions that have it as their target (a farm another farmer
    /// works).
    pub workers: i32,
    /// Its master's `area_effect_object_level` (`+0x62b`): the blast levels that reach it.
    pub blast_defense: u8,
    /// Flagged to sleep (`+0x641`, `+0x643`): a killing hit wakes it (`FUN_14119deb0` with 0).
    pub goto_sleep: bool,
    /// Its master's decay (`+0x620`), which a hunt's decay stop negates (`FUN_140ac44b0`; the
    /// write-back gives it to the object's own master).
    pub decay: AmountRate,
    /// Its master's `fog_visibility` (`+0x605`): an Attack goes on against it in the fog.
    pub fog_visibility: u8,
    /// Its player, and that player's attributes 272 and 273 (a higher or lower attacker's hits
    /// add them) and 211 and 212 (its own hits from higher or lower up add them).
    pub player: PlayerId,
    /// Its player's attributes 272 and 273: what a higher or lower attacker's hits add.
    pub elevation: ElevationBonus,
    /// Its player's attributes 211 and 212: what its own hits from higher or lower up add.
    pub attack_elevation: ElevationBonus,
    /// The attacker's player counts its player as its own or allied (`FUN_141663d90` with the
    /// ally flag), and the other way round: a blast spares the first and scales its damage on
    /// the second by `friendly_fire_damage`.
    pub allied: Allied,
    /// Its place is a guess: its player's update is not modelled.
    pub uncertain: bool,
    /// It is inside another object (`+0x2f0`): an Attack on it ends.
    pub inside: bool,
    /// Its master's hit points (`+0x530`).
    pub master_hp: i16,
    /// Its master's tasks of action type 0xa0 ([`BONUS_DAMAGE`]), in their order: the damage its
    /// hits add by hit points ([`bonus_damage`]).
    pub bonus_tasks: Vec<crate::objects::masters::Task>,
    /// Its own attack (a retaliation's): its values as a hit's source and its master's weapons
    /// (class, amount).
    pub source: Attacker,
    /// Its master's weapons (class, amount).
    pub weapons: Vec<crate::objects::masters::ClassAmount>,
    /// A combat object's armor and what its damage reads (types 50 and up); None for others.
    pub armored: Option<Armored>,
    /// The damage notifies (500, `FUN_141082e80`) its hits post to its unit AI, in order (a
    /// blast's hit on an annex and on its main building post two to the main building): the
    /// sender (a missile's shooter, none once it is gone), then the attacker, its hit points
    /// after (truncated) and its master's hit points.
    pub damaged: Vec<(Option<ObjectId>, [i32; 3])>,
    /// A hit killed it, for the caller's death handling.
    pub killed: Option<Kill>,
    /// A building's (type 80) part of a hit ([`Building`]); None for other objects.
    pub building: Option<Building>,
}

/// What a hit on a building (`TRIBE_Building_Object`, slot 0xe8 `FUN_1411cea90`) reads of it
/// besides a combat object's values, and what it changes for the write-back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Building {
    /// `+0xbb8`: an annex's main building, whose slot 0xe8 takes a hit on the annex.
    pub main: Option<ObjectId>,
    /// `+0xbc0`..`+0xbd8`: its annexes, which take its hit points after each hit.
    pub annexes: Vec<ObjectId>,
    /// `+0xb90` (not saved): the damage the blast under way has done to it ([`blast`]).
    pub blast_taken: HitPoints,
    /// `FUN_140fde380` on its master: of type 70 and up, of class 51 or 54, with a weapon
    /// (`+0x810` above 0): a trebuchet, whose kill counts as a unit's.
    pub trebuchet: bool,
    /// Its master's paid cost (`+0x880`: each entry with a resource and the paid flag, summed as
    /// floats in their order).
    pub cost: f32,
    /// `damaged_lately_timer` (`+0x30d`, saved): 60 after each hit, a second off in its update.
    pub damaged_lately: u8,
    /// A hit got past its hit point test: the write-back gives its annexes its hit points and
    /// its `damaged_lately`.
    pub struck: bool,
    /// `relic_count` and `specific_relic_count` (`+0xc20`, `+0xc24`), which its elevation factor
    /// reads as the shooter and as the target ([`building_elevation_factor`]).
    pub relics: Relics,
    /// What each hit that left it below 1 hit point counted, in order, for the write-back.
    pub razed: Vec<Razed>,
}

/// What a hit that leaves a player's building below 1 hit point counts (`FUN_1411cea90`), for
/// the write-back: the history event of the attacker's player (`FUN_141560ae0`, `data1` the
/// owner's id) and the players' attribute adds (player slot 0xf8) in the game's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Razed {
    /// The attacker's player (`param_5`), whose history takes the event.
    pub attacker_player: usize,
    /// The building's owner (`+0x718`), the event's `data1`.
    pub owner: PlayerId,
    /// A castle's (master 82) or a wonder's (276) destruction
    /// ([`crate::players::history::EventKind::CASTLE_DESTROYED`], [`crate::players::history::EventKind::WONDER_DESTROYED`]).
    pub event: Option<crate::players::history::EventKind>,
    /// The attribute adds: (player, attribute, amount).
    pub adds: Vec<(PlayerId, AttributeId, f32)>,
}

/// What a killing hit leaves for the caller after the update
/// ([`crate::world::World::kill_left`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kill {
    /// The object that killed it.
    pub attacker: ObjectId,
    /// That object's player.
    pub attacker_player: usize,
    /// That object's master's class as the hit saw it (`FUN_141082e80` reads it of the
    /// attacker it gets: an animal's 9 or 10 counts a villager's loss).
    pub attacker_class: ClassId,
    /// Gaia takes it: a player's livestock (`FUN_1412108e0`).
    pub to_gaia: bool,
}

/// Where an object faces (`FUN_14114eaa0`'s inputs): its angle (`+0x7e8`, which follows its
/// facet, `FUN_14114d780`) and its sprite's facets (`+0x140` -> `+0xa`, 0 without a sprite).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing {
    /// `+0x7e8`, where it faces.
    pub angle: Angle,
    /// Its sprite's facets, 0 without a sprite.
    pub facets: i16,
}

/// What a hit on a combat object reads of it (its master's and its own values) and changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Armored {
    /// Its master's armor classes (class, amount), base armor, `defense_terrain_bonus`,
    /// `bonus_damage_resistance`, `combat_ability`, `damage_reflection`,
    /// `invulnerability_level`, `special_attack_flag`, hit points and `friendly_fire_damage`.
    pub armors: Vec<crate::objects::masters::ClassAmount>,
    /// Its master's base armor for a class it does not list.
    pub base_armor: i16,
    /// Its master's `defense_terrain_bonus`.
    pub terrain_bonus: i16,
    /// Its master's `bonus_damage_resistance`.
    pub resistance: f32,
    /// Its master's `combat_ability`.
    pub ability: u8,
    /// Its master's `damage_reflection`.
    pub reflection: f32,
    /// Its master's `invulnerability_level`.
    pub invulnerability: f32,
    /// Its master's `special_attack_flag`.
    pub special_attack: u8,
    /// Its master's `friendly_fire_damage`.
    pub friendly_fire: f32,
    /// Its master has a special graphic (`+0x8e8`), which its special attack shows.
    pub special_graphic: bool,
    /// Its terrain defense: `defense_terrain_bonus`'s restriction's value for its tile's terrain
    /// (the terrain to restore, else the terrain; 0 counts as 1), 1 off the map; None when the
    /// slice lacks the map or the restriction.
    pub terrain: Option<f32>,
    /// The object's armor powerups (melee, pierce).
    pub armor_powerups: ArmorPair,
    /// The melee and pierce armor taken off it.
    pub armor_removed: ArmorPair,
    /// Its hit point powerup (`+0xa38`, `+0xa3c`), which absorbs damage first.
    pub hp_powerup: HpPowerup,
    /// Its master has a task of action type 0xa1 with a work value (a kill reward,
    /// `FUN_14120b5f0`).
    pub kill_tasks: bool,
    /// It has a current action (`FUN_140a98730` on its action list), a unit AI, and
    /// `FUN_140c42b30` on that AI (alert mode 1, `+0x1b8`; no attacking units, `+0xc8`; no
    /// current action): a special attack and a retaliation need it idle. The slice does not hold
    /// the alert mode and the attacking units: None.
    pub busy: bool,
    /// It has a unit AI.
    pub unit_ai: bool,
    /// Its unit AI is idle in alert mode, when the slice knows it (None: it does not).
    pub alert_idle: Option<bool>,
    /// Where it faces; None: the slice does not hold the facet.
    pub facing: Option<Facing>,
}

/// What a hit reads of its attacker's attack, and the charge it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attacker {
    /// The attacking object.
    pub id: ObjectId,
    /// Its master's type, class, id, `combat_ability` and `missile_id`.
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its master.
    pub master_id: MasterId,
    /// Its master's `combat_ability`.
    pub ability: u8,
    /// Its master's missile, −1 for none.
    pub missile: Option<MasterId>,
    /// Its master's weapon range (`+0x828`), `area_effect_range` (`+0x82c`: the blast's radius),
    /// `area_effect_level` (`+0x830`: the levels it reaches, bit 2 its friends too, bit 5 a wide
    /// cone, bit 6 a falloff, bit 7 a cone) and `area_damage` (`+0x834`: the share of the damage
    /// the objects besides its target take, a fixed damage below 0).
    pub weapon_range: Distance,
    /// Its master's `area_effect_range` (`+0x82c`): the blast's radius.
    pub area_effect: Distance,
    /// Its master's `area_effect_level` (`+0x830`).
    pub blast_level: u8,
    /// Its master's `area_damage` (`+0x834`).
    pub area_damage: f32,
    /// Its master's `charge_type`, `max_charge`, `charge_target` and `special_attack_flag`; its
    /// charge (`+0x9e0`) and `mHitSomethingValidForChargeAttack` (`+0x8a8`), which its hits set.
    pub charge_type: i16,
    /// Its master's `max_charge`.
    pub max_charge: Charge,
    /// Its master's `charge_target`.
    pub charge_target: i16,
    /// Its master's `special_attack_flag`.
    pub special_attack: u8,
    /// Its charge (`+0x9e0`).
    pub charge: Charge,
    /// `mHitSomethingValidForChargeAttack` (`+0x8a8`), which its hits set.
    pub charge_hit: bool,
    /// Its attack powerup (`+0xa24`) and the game's setting `+0x680`
    /// ([`crate::world::World::mode_680`]: at 1 a hit may do no damage).
    pub attack_powerup: f32,
    /// The game's setting `+0x680` ([`crate::world::World::mode_680`]).
    pub mode_680: i32,
    /// Its player and whether its master has a task of action type 0x9a (a kill bonus,
    /// `FUN_1415c6510`).
    pub player: PlayerId,
    /// Its master has a task of action type 0x9a (a kill bonus).
    pub kill_bonus_tasks: bool,
    /// What a missile was fired from (`+0x998`), its damage notify's sender.
    pub fired_from: Option<ObjectId>,
}

#[cfg(test)]
mod tests;
