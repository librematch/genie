//! Conversion, neutral targets, patrol reaches and siege: the attack rules of the unit-AI
//! types.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::math::fmath::{at_most_or_unordered, maxss, powf, sqrt};
use crate::math::image::unit_ai::{
    DEFEND_PATROL_LEASH as UNIT_AI_DEFEND_PATROL_LEASH, FULL_FAITH as UNIT_AI_FULL_FAITH,
    GAIA_TARGET_REACH as UNIT_AI_GAIA_TARGET_REACH, PATROL_FARTHEST as UNIT_AI_PATROL_FARTHEST,
    STAND_GROUND_PATROL_LEASH as UNIT_AI_STAND_GROUND_PATROL_LEASH,
};
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::ClassMask;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::class;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    Attacker, BUILDING_AI, CIVILIAN, DEFEND_GROWTH, LION_AI, PREDATOR_AI, PRIEST_AI,
    PRIORITY_3_CLASSES, SIEGE_AI, SOLDIER_AI, Seeker, TOWER_AI, Target,
};
use crate::world::slots::CheckedIndex;

/// What a priest's conversion test (`FUN_140c1af00`, the priest's slot 0x1b8) reads of the
/// priest: its master's class and id, its faith (`+0x310`), its owner's attributes and its
/// master's tasks.
#[derive(Clone, Copy, Debug)]
pub struct Converter<'a> {
    /// Its master's class.
    pub class: ClassId,
    /// Its master's id.
    pub master: MasterId,
    /// Its faith (`+0x310`, what it holds).
    pub faith: Amount,
    /// Its owner's attributes (`player+0x100`, count `+0x90`).
    pub attributes: &'a [Amount],
    /// Its master's tasks (`+0x788`).
    pub tasks: &'a [Task],
}

/// What the conversion test reads of the object: its owner, its master's class, id and hero
/// flag (`+0x8d9`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Convertee {
    /// Its owner's id.
    pub owner: PlayerId,
    /// Its master's class.
    pub class: ClassId,
    /// Its master's id.
    pub master: MasterId,
    /// Its master's hero flag (`hero_flag`, `+0x8d9`).
    pub hero: HeroFlags,
}

/// The faith a priest's conversion asks unless `any` (`DAT_14330ebcc`).
pub(super) const FULL_FAITH: Amount = Amount::new(UNIT_AI_FULL_FAITH);
/// The town centres a priest never converts (109, 618 to 621), the relic cart's master 159, the
/// farms and fish trap (50, 1187, 199).
pub(super) const NEVER_CONVERTED: [MasterId; 9] = [
    MasterId::named(109),
    MasterId::named(618),
    MasterId::named(619),
    MasterId::named(620),
    MasterId::named(621),
    MasterId::named(159),
    MasterId::named(50),
    MasterId::named(1187),
    MasterId::named(199),
];
/// The priest master that converts only boats (53).
pub(super) const BOAT_CONVERTER: MasterId = MasterId::named(53);

/// The priest's slot 0x1b8 (`FUN_140c1af00`, "CanConvert"): whether priest `p` may convert object
/// `t` (None: one the world does not hold), with `any` its faith not asked. Never for class 43, nor
/// without full faith (100, `COMISS`: a NaN fails) unless `any`; never a Gaia object, nor with the
/// owner's attribute 67 at most 0, nor livestock (58); master 53 converts only the boats (2, 20,
/// 21, 22, 53); never the [`NEVER_CONVERTED`] masters, walls (27), gates (39) or flags (30); never
/// a master whose hero flag grants [`HeroFlags::NO_CONVERSION`]. Then the priest's convert task
/// (action type 104): its last for any class and master, or the first for the object's master or
/// class; one that names an attribute (`+0x26`) the owner holds at most 0 (or not a number) fails.
#[must_use]
pub fn can_convert(p: &Converter<'_>, t: Option<&Convertee>, any: bool) -> bool {
    if p.class == class::CLASS_43 {
        return false;
    }
    // full faith (`COMISS`: a NaN has none)
    let faithful = FULL_FAITH <= p.faith;
    if !any && !faithful {
        return false;
    }
    let Some(t) = t else {
        return false;
    };
    if t.owner == PlayerId::GAIA
        || p.attributes.get(67).is_some_and(|&a| a <= Amount::ZERO)
        || t.class == class::LIVESTOCK
    {
        return false;
    }
    if p.master == BOAT_CONVERTER {
        return matches!(
            t.class,
            class::TRADE_BOAT
                | class::TRANSPORT
                | class::FISHING_SHIP
                | class::WARSHIP
                | class::CLASS_53
        );
    }
    if NEVER_CONVERTED.contains(&t.master)
        || matches!(t.class, class::WALL | class::GATE | class::CLASS_30)
    {
        return false;
    }
    if t.hero.grants(HeroFlags::NO_CONVERSION) {
        return false;
    }
    let mut task = None;
    for k in p
        .tasks
        .iter()
        .filter(|k| k.action_kind == ActionKind::CONVERT)
    {
        if k.object_group.is_none() && k.object_id.is_none() {
            task = Some(k);
        }
        if k.object_id == Some(t.master) || k.for_class(t.class) {
            task = Some(k);
            break;
        }
    }
    if let Some(k) = task
        && let Some(i) = k.attribute_type4
        && let Some(&a) = p.attributes.get(i.index())
        && at_most_or_unordered(a.get(), 0.0)
    {
        return false;
    }
    true
}

/// `FUN_140c47f90` for a human player's unit (`me`): the farthest `bestUnitToAttack` looks, twice
/// its master's line of sight. A computer player's (by the game's difficulty and the player's
/// `+0x608`/`+0x60c` in a scenario) is not modelled.
///
/// # Errors
///
/// A computer player's unit.
pub fn attack_sight(me: &Attacker) -> Result<Distance, GenieError> {
    if me.player_type.is_computer() {
        return Err(GenieError::unsupported(
            "a computer player's bestUnitToAttack",
        ));
    }
    Ok(me.los + me.los)
}

/// The unit AI's slot 0xa8 (`me`'s type, [`type_attacks_neutral`]) on a neutral object of
/// `class`.
#[must_use]
pub fn attacks_neutral(me: &Attacker, class: ClassId) -> bool {
    type_attacks_neutral(me.ai_type, me.attack_priority, class)
}

/// Slot 0xa8 of the unit AI of factory type `ai_type` (`FUN_141215560`'s modules), whether it
/// attacks a neutral object of `class`, its unit's master of `attack_priority`: the civilian's
/// (`FUN_140c1acf0`) and the building's (`FUN_140c1ace0`) never; the predator's
/// (`FUN_140c1ad60`) and the lion's (`FUN_140c1ad00`) not classes 13, 22, 52, 54 and 55; the
/// priest's ([`priest_attacks_neutral`]), the soldier's (`FUN_140c1ae60`) and the tower's
/// (`FUN_140c1aeb0`, the same body: [`soldier_attacks_neutral`]) and the siege unit's
/// ([`siege_attacks_neutral`]);
/// the others' the base's (`FUN_140c48410`): always. The predator's and the lion's also take
/// attack priority 3 to [`PRIORITY_3_CLASSES`].
#[must_use]
pub fn type_attacks_neutral(
    ai_type: UnitAiType,
    attack_priority: AttackPriority,
    class: ClassId,
) -> bool {
    match ai_type {
        CIVILIAN | BUILDING_AI => false,
        PREDATOR_AI | LION_AI => {
            !matches!(
                class,
                class::SIEGE | class::WARSHIP | class::TOWER | class::CLASS_54 | class::CLASS_55
            ) && (!attack_priority.limited() || PRIORITY_3_CLASSES.contains(&class))
        }
        PRIEST_AI => priest_attacks_neutral(attack_priority, class),
        SOLDIER_AI | TOWER_AI => soldier_attacks_neutral(attack_priority, class),
        SIEGE_AI => siege_attacks_neutral(attack_priority, class),
        _ => true,
    }
}

/// The classes the priest never attacks as neutral objects (`FUN_140c1adc0`'s mask
/// `0x200800838001c` below class 50: 2, 3, 4, 19, 20, 21, 27, 39, 49).
pub(super) const PRIEST_NEUTRAL_SPARED: ClassMask = ClassMask::new(0x2_0080_0838_001c, 0x32);

/// Slot 0xa8 of the priest's unit AI (`FUN_140c1adc0`): never a class below 50 of
/// [`PRIEST_NEUTRAL_SPARED`] (`class` taken as unsigned), and with attack priority 3 only
/// [`PRIORITY_3_CLASSES`].
#[must_use]
pub fn priest_attacks_neutral(attack_priority: AttackPriority, class: ClassId) -> bool {
    if PRIEST_NEUTRAL_SPARED.contains(class) {
        return false;
    }
    !attack_priority.limited() || PRIORITY_3_CLASSES.contains(&class)
}

/// The classes slot 0xa8 of the soldier's unit AI (`FUN_140c1ae60`) passes among neutral objects:
/// those of the mask `0x200200800838001c` (2, 3, 4, 19, 20, 21, 27, 39, 49, 61) never.
pub(super) const NEUTRAL_SPARED: ClassMask = ClassMask::new(0x2002_0080_0838_001c, 0x3e);

/// Slot 0xa8 of the soldier's unit AI (`FUN_140c1ae60`), whether it attacks a neutral object of
/// `class` (taken as unsigned, so a class outside `0..0x3e` passes the mask): never one of
/// [`NEUTRAL_SPARED`]'s classes, and with the unit's master's `attack_priority` 3
/// (`FUN_141202220`) only [`PRIORITY_3_CLASSES`] (`FUN_140fddf10`).
#[must_use]
pub fn soldier_attacks_neutral(attack_priority: AttackPriority, class: ClassId) -> bool {
    if NEUTRAL_SPARED.contains(class) {
        return false;
    }
    !attack_priority.limited() || PRIORITY_3_CLASSES.contains(&class)
}

/// A computer player's soldier's or siege unit's idle check past its target: its target test
/// (`FUN_140c41690`) and its saved patrol's leash (`player+0x624`); not modelled.
pub(super) const fn computer_idle_check() -> Result<(), GenieError> {
    Err(GenieError::unsupported(
        "a computer player's soldier's idle check",
    ))
}

/// A unit's back-up from a target under its least range (`FUN_140c43de0`, the soldier's and the
/// siege unit's case 0x1fe); not modelled.
pub(super) const fn back_up() -> Result<u32, GenieError> {
    Err(GenieError::unsupported(
        "a soldier's target under its least range",
    ))
}

/// A defending unit's range on a patrol order or with a patrol path (slot 0x470, `FUN_140f95e10`),
/// which the soldier's and the siege unit's attack checks read; not modelled.
pub(super) const fn defend_patrol_range() -> Result<(), GenieError> {
    Err(GenieError::unsupported(
        "a defending soldier's patrol range (slot 0x470, FUN_140f95e10)",
    ))
}

/// The classes the siege unit never attacks as neutral objects (`FUN_140c1ae10`'s mask
/// `0x200200800830001c`: 2, 3, 4, 20, 21, 27, 39, 49, 61; the soldier's but class 19).
pub(super) const SIEGE_NEUTRAL_SPARED: ClassMask = ClassMask::new(0x2002_0080_0830_001c, 0x3e);

/// Slot 0xa8 of the siege unit's AI (`FUN_140c1ae10`): never a class of
/// [`SIEGE_NEUTRAL_SPARED`] (`class` taken as unsigned), and with attack priority 3 only
/// [`PRIORITY_3_CLASSES`].
#[must_use]
pub fn siege_attacks_neutral(attack_priority: AttackPriority, class: ClassId) -> bool {
    if SIEGE_NEUTRAL_SPARED.contains(class) {
        return false;
    }
    !attack_priority.limited() || PRIORITY_3_CLASSES.contains(&class)
}

/// The rams (masters 35, 422, 548 and 1258, raw `0x60bf`, `0x6242`, `0x62c0`, `0x6586`), which
/// by attack priority 0 attack only [`RAM_TARGETS`].
pub(super) const RAM_MASTERS: [MasterId; 4] = [
    MasterId::named(35),
    MasterId::named(422),
    MasterId::named(548),
    MasterId::named(1258),
];
/// What a ram attacks by its own choice: buildings, siege, walls, gates, farms, the packed
/// units, towers, the unpacked siege and the scorpions.
pub(super) const RAM_TARGETS: [ClassId; 9] = [
    class::BUILDING,
    class::SIEGE,
    class::WALL,
    class::GATE,
    class::FARM,
    class::CLASS_51,
    class::TOWER,
    class::CLASS_54,
    class::CLASS_55,
];
/// What a trebuchet (class 51 or 54) attacks by its own choice: the rams' but siege and
/// scorpions.
pub(super) const TREBUCHET_TARGETS: [ClassId; 7] = [
    class::BUILDING,
    class::WALL,
    class::GATE,
    class::FARM,
    class::CLASS_51,
    class::TOWER,
    class::CLASS_54,
];
/// The siege masters whose current building target decides by attack priority alone (1744,
/// 1746, raw `0x676c`, `0x676e`).
pub(super) const SIEGE_BUILDING_MASTERS: [MasterId; 2] =
    [MasterId::named(1744), MasterId::named(1746)];
/// A Gaia target lets the siege unit's range grow by this (`DAT_142eb4838`).
pub(super) const GAIA_TARGET_REACH: Distance = Distance::new(UNIT_AI_GAIA_TARGET_REACH);
/// The packed unit's class (51), which a human player's siege attack check passes over unless
/// asked for any target.
pub(super) const PACKED_UNIT_CLASS: ClassId = class::CLASS_51;

/// The defend stance's leash from a saved patrol's path (`DAT_14330ebb8`, 5).
pub(super) const DEFEND_PATROL_LEASH: Distance = Distance::new(UNIT_AI_DEFEND_PATROL_LEASH);
/// Stand ground's leash from a saved patrol's path (`DAT_142e7f384`, 0.5).
pub(super) const STAND_GROUND_PATROL_LEASH: Distance =
    Distance::new(UNIT_AI_STAND_GROUND_PATROL_LEASH);
/// `FUN_140f95e10`'s first least distance (`DAT_14330ebf4`, the largest f32).
pub(super) const PATROL_FARTHEST: f32 = UNIT_AI_PATROL_FARTHEST;

/// `FUN_14087e7c0`: the distance from `p` to the line through `a` and `b` (the cross product's
/// length over the direction's, its squares divided before `sqrtf`, in the routine's order).
/// Its flag whether `p` lies between the two (the dot products' signs) is not read here.
pub(super) fn line_distance(p: BVector, a: BVector, b: BVector) -> Distance {
    let [px, py, pz] = [p.x() - a.x(), p.height() - a.height(), p.y() - a.y()];
    let [dx, dy, dz] = [b.x() - a.x(), b.height() - a.height(), b.y() - a.y()];
    let c1 = py * dz - pz * dy;
    let c3 = px * dy - py * dx;
    let c2 = pz * dx - px * dz;
    let cross = c2 * c2 + c1 * c1 + c3 * c3;
    let length = dy * dy + dx * dx + dz * dz;
    Distance::new(sqrt(cross / length))
}

/// `FUN_140f95e10`: the least distance from `point` to the lines through a path's consecutive
/// `points` ([`line_distance`]), from [`PATROL_FARTHEST`], each new one kept unless the least so
/// far is at most it (a NaN distance is kept, and the next one replaces it); a path of one point
/// answers [`PATROL_FARTHEST`].
///
/// # Errors
///
/// An empty path, whose loop the game runs past its end.
pub(crate) fn patrol_distance(points: &[BVector], point: BVector) -> Result<Distance, GenieError> {
    if points.is_empty() {
        return Err(GenieError::unreachable("a saved patrol without points"));
    }
    let mut least = PATROL_FARTHEST;
    for pair in points.windows(2) {
        let d = line_distance(point, *pair.checked(0)?, *pair.checked(1)?);
        least = if least <= d.get() { least } else { d.get() };
    }
    Ok(Distance::new(least))
}

/// `FUN_14118faf0`: the gap from an object at `at` with radii `radius` to `point`: each axis's
/// distance less the radius, `MAXSS` 0 (an unordered one is 0), then the length (`sqrtf`).
pub(super) fn footprint_point_gap(at: Position, radius: Footprint, point: Point) -> Distance {
    let gx = maxss((point.x() - at.x()).abs() - radius.x(), 0.0);
    let gy = maxss((point.y() - at.y()).abs() - radius.y(), 0.0);
    Distance::new(sqrt(gy * gy + gx * gx))
}

/// The siege unit's attack check (slot 0xa0, `FUN_140c19190`) with `any` (its third argument):
/// never the classes 1 and 42, nor in `NoAttack`. Its reach is the target's footprint gap to the
/// unit's state position (`FUN_14118faf0`) less the unit's own diagonal (`sqrtf` of its radii
/// squared by `powf`), against its range (slot 0x230), grown by [`GAIA_TARGET_REACH`] for a Gaia
/// target: in `StandGround` (stance 2) but on order `0x2bd` within the range (`COMISS`, `JBE`:
/// an unordered one passes); in Defend (1) within the range (ungrown) plus 5
/// (`DAT_14330ebb8`; `JA` rejects); the patrol orders' and a patrol path's ranges (slot 0x470,
/// `FUN_140f95e10`) are not modelled. Then a Gaia unit only of the combat type with `+0x924` 0
/// outside the scenario modes; never a building no builder has started (`+0xc60`); never from a
/// unit below state 2; not `any`, never from a human player's packed unit (51); never while its
/// top action packs or unpacks (124, 125). Not `any`, by attack priority 0 a ram attacks only
/// [`RAM_TARGETS`] and a trebuchet (classes 51, 54) [`TREBUCHET_TARGETS`]; masters 1744 and 1746
/// with a building as their current target ([`Seeker::target_kind`]) by their priority alone (1
/// or 4 and up); attack priority 3 only [`PRIORITY_3_CLASSES`].
///
/// # Errors
///
/// A defending unit's patrol range.
pub fn siege_can_attack(
    me: &Attacker,
    s: &Seeker,
    t: &Target,
    any: bool,
) -> Result<bool, GenieError> {
    if matches!(t.class, class::ARTIFACT | class::RELIC)
        || s.stance == UnitStance::NO_ATTACK
        || !siege_in_reach(me, s, t)?
    {
        return Ok(false);
    }
    let takes_by_sight = !me.scenario && MasterKind::COMBAT <= me.kind && !me.capture.any();
    if me.player == PlayerId::GAIA && !takes_by_sight {
        return Ok(false);
    }
    if (t.kind == MasterKind::BUILDING && t.desolid) || me.state < ObjectState::Alive {
        return Ok(false);
    }
    if !me.player_type.is_computer() && !any && me.class == Some(PACKED_UNIT_CLASS) {
        return Ok(false);
    }
    if matches!(me.top_action, Some(ActionKind::PACK | ActionKind::UNPACK)) {
        return Ok(false);
    }
    Ok(any || siege_chooses(me, s, t))
}

/// [`siege_can_attack`]'s reach by the stance: the target's footprint gap to the unit's state
/// position less the unit's own diagonal, against its range grown for a Gaia target in
/// `StandGround` (but on the defend order), against the ungrown range plus 5 in Defend.
///
/// # Errors
///
/// A defending unit's patrol range.
pub(super) fn siege_in_reach(me: &Attacker, s: &Seeker, t: &Target) -> Result<bool, GenieError> {
    // `powf` with 2 (`DAT_142e8c150`), y's first
    let diagonal = sqrt(powf(me.radius.y(), 2.0) + powf(me.radius.x(), 2.0));
    let gap = footprint_point_gap(t.position, t.radius, s.state_position) - Distance::new(diagonal);
    let mut range = me.range;
    if t.owner == PlayerId::GAIA {
        range += GAIA_TARGET_REACH;
    }
    match s.stance {
        UnitStance::STAND_GROUND
            if s.order != Some(OrderKind::Defend)
                && !at_most_or_unordered(gap.get(), range.get()) =>
        {
            Ok(false)
        }
        UnitStance::DEFENSIVE => {
            if matches!(s.order, Some(OrderKind::Patrol | OrderKind::AttackMove)) || s.patrol_path {
                defend_patrol_range()?;
            }
            // `JA` rejects: an unordered gap passes
            Ok(at_most_or_unordered(
                gap.get(),
                me.range.get() + DEFEND_GROWTH,
            ))
        }
        _ => Ok(true),
    }
}

/// [`siege_can_attack`]'s own choice (not `any`): by attack priority 0 a ram only
/// [`RAM_TARGETS`] and a trebuchet [`TREBUCHET_TARGETS`]; masters 1744 and 1746 with a building
/// as their current target by their priority alone; attack priority 3 only
/// [`PRIORITY_3_CLASSES`].
pub(super) fn siege_chooses(me: &Attacker, s: &Seeker, t: &Target) -> bool {
    let priority = me.attack_priority;
    if priority == AttackPriority::PRIORITY_0 {
        if RAM_MASTERS.contains(&me.master) {
            return RAM_TARGETS.contains(&t.class);
        }
        if matches!(me.class, Some(class::CLASS_51 | class::CLASS_54)) {
            return TREBUCHET_TARGETS.contains(&t.class);
        }
    }
    if SIEGE_BUILDING_MASTERS.contains(&me.master)
        && s.target.is_some()
        && s.target_kind == Some(MasterKind::BUILDING)
    {
        return priority == AttackPriority::PRIORITY_1 || priority.past_3();
    }
    !priority.limited() || PRIORITY_3_CLASSES.contains(&t.class)
}

/// The classes a lion attacks by its own choice (its slot 0xa0's list, `FUN_140c18d50`; the
/// game's `+0x6b4` of 1, not met, adds 13, 18 and 55).
pub(super) const LION_ATTACKS: [ClassId; 19] = [
    class::CLASS_0,
    class::VILLAGER,
    class::CLASS_6,
    class::PREY,
    class::CLASS_12,
    class::CLASS_23,
    class::CLASS_24,
    class::CLASS_25,
    class::CLASS_26,
    class::CLASS_28,
    class::CLASS_35,
    class::CLASS_36,
    class::CLASS_44,
    class::CLASS_45,
    class::CLASS_46,
    class::CLASS_47,
    class::CLASS_50,
    class::CLASS_56,
    class::CLASS_57,
];

/// The lion's slot 0xa0 (`FUN_140c18d50`) without its `any` flag: by the master's attack
/// priority 1 and 2 anything, 3 the classes of [`PRIORITY_3_CLASSES`], any other the classes
/// of [`LION_ATTACKS`].
pub(super) fn lion_may_attack(attack_priority: AttackPriority, class: ClassId) -> bool {
    match attack_priority {
        AttackPriority::PRIORITY_1 | AttackPriority::PRIORITY_2 => true,
        AttackPriority::PRIORITY_3 => PRIORITY_3_CLASSES.contains(&class),
        _ => LION_ATTACKS.contains(&class),
    }
}

/// The civilian's slot 0xa0 (`FUN_140c18b70`) without its `any` flag: never classes 1 and 42;
/// with the unit's master's `attack_priority` 3 (`FUN_141202220`) only [`PRIORITY_3_CLASSES`]
/// (`FUN_140fddf10`).
pub(super) fn civilian_may_attack(attack_priority: AttackPriority, class: ClassId) -> bool {
    !matches!(class, class::ARTIFACT | class::RELIC)
        && (!attack_priority.limited() || PRIORITY_3_CLASSES.contains(&class))
}

/// What [`UnitSide::attack`] answers: the unit attacks the target, and the notifies the command
/// posted to its unit AI (slot 0x328 with the kind and three parameters).
///
/// [`UnitSide::attack`]: crate::unit_ai::UnitSide::attack
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attacked {
    /// The unit attacks the target.
    pub attacks: bool,
    /// The notifies posted, in order.
    pub posted: Vec<(NotifyKind, [i32; 3])>,
}
