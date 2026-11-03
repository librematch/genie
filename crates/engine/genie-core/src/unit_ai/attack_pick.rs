//! Which object a unit may attack: the attack search, its picks and each type's rules.

use crate::GenieError;
use crate::combat::Firepower;
use crate::combat::attack::ATTACK;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::maxss;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ActionKind;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::QueryRelation;
use crate::players::attribute::AttributeId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::best_target::Ask;
use crate::unit_ai::{
    ARTIFACT_AI, BOAR_AI, BUILDING_AI, CIVILIAN, FISHING_AI, OrderRecord, SIEGE_AI, SIEGE_TOWER_AI,
    TRADE_AI, TRANSPORT_AI, civilian_may_attack, defend_patrol_range, lion_may_attack,
    siege_can_attack,
};
use crate::world::WorldTime;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::UnitAiTuning;

/// A unit AI's search (`FUN_140c63e20`) as the idle checks run it: it queries its player's
/// objects in sight of `classes` within `radius` of `at` for a searcher of radii `size`
/// (`GetVisibleUnits`, mode 0, every relation), and `pick` chooses among them. The caller answers
/// with the object's id, −1 for none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Search<'a> {
    /// Where it searches from.
    pub at: Point,
    /// How far.
    pub radius: Distance,
    /// The object classes it cares about.
    pub classes: &'a [ClassId],
    /// The searcher's radii.
    pub size: Footprint,
    /// What it picks of what it finds.
    pub pick: Pick,
    /// What an attack pick reads of the unit AI's memory.
    pub memory: Memory<'a>,
}

/// `bestUnitToAttack`'s arguments past its point ([`crate::unit_ai::best_target::Ask`] without its
/// list: at most one extra object).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackAsk {
    /// The reference distance (0: none).
    pub distance: Distance,
    /// An idle check.
    pub idle: bool,
    /// The path checks.
    pub paths: bool,
    /// An extra object for the statistics.
    pub extra: Option<ObjectId>,
    /// Any class.
    pub any_class: bool,
    /// The unit is in an attack work state.
    pub work_state: bool,
}

impl AttackAsk {
    /// The idle checks' call: no reference distance, an idle check, path checks on, no extra
    /// object, any class, not in an attack work state.
    pub const IDLE: Self = Self {
        distance: Distance::ZERO,
        idle: true,
        paths: true,
        extra: None,
        any_class: true,
        work_state: false,
    };

    /// The full arguments ([`crate::unit_ai::best_target::Ask`]).
    #[must_use]
    pub fn ask(&self) -> Ask {
        Ask {
            distance: self.distance,
            idle: self.idle,
            paths: u8::from(self.paths),
            extra: self.extra.into_iter().collect(),
            any_class: self.any_class,
            work_state: self.work_state,
        }
    }
}

/// What `bestUnitToAttack` reads of the searching unit AI's memory: its retarget entries
/// (`+0x1e0`) and its order history (`+0x78`'s list).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Memory<'a> {
    /// The retarget entries.
    pub retries: &'a [(ObjectId, WorldTime)],
    /// The order history, the newest first.
    pub history: &'a [OrderRecord],
}

/// What a [`Search`] picks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    /// `ClosestObject` (`FUN_140c488b0`) with its reachability check: the closest object of
    /// master class `group`, of relation `relation` (0 itself or friends, 1 neutrals, 2 enemies,
    /// 3 itself), in state `state`, of master `master` and below `workers` workers (each −1 for
    /// any), reachable ones only.
    Closest {
        /// The master class (−1 any).
        group: Option<ClassId>,
        /// The relation (−1 any).
        relation: Option<QueryRelation>,
        /// The object state (−1 any).
        state: Option<ObjectState>,
        /// The master (−1 any).
        master: Option<MasterId>,
        /// The workers cap (−1 none).
        workers: WorkerCount,
        /// The relation passed over (argument 9; −1 none).
        spared: Option<QueryRelation>,
        /// Argument 8: only objects the unit reaches count (slot 0x3d0); every caller but the
        /// artifact's passes 1.
        reach: bool,
    },
    /// `bestUnitToAttack` (`FUN_140c44680`) with no current target and no extra objects, as the
    /// idle checks call it: of the relations from enemies down to `lowest` (2 in the scenario
    /// modes for a human player, else 0, Gaia), the object to attack, by the unit AI's attack
    /// check ([`can_attack`]). The search sets the alert mode
    /// (`+0x1b8`) when it finds enemies, and holds a second search from within 5 tiles in the same
    /// update to the first's answer, which the idle checks never ask.
    Attack {
        /// The lowest relation it looks at (0 Gaia, 2 in a scenario).
        lowest: usize,
        /// What the pick reads of the searching unit AI.
        seeker: Seeker,
        /// `bestUnitToAttack`'s arguments past its point.
        ask: AttackAsk,
    },
    /// The search alone (slot 0x50 as a `NoAttack` soldier's idle check calls it): no object, and
    /// whether it found enemies (the alert mode).
    Look,
}

/// What a unit AI's attack check (its slot 0xa0) and `bestUnitToAttack` read of the unit: its
/// owner's id, its state (`+0x640`), what it holds (`+0x308`) and its top action, and its
/// master's id, type, class, `attack_priority` (`+0x92c`), `missile_id` (`+0x83c`), weapon and
/// least ranges (slots 0x230, 0x240), charge type, blast (`+0x82c`, `+0x830`) and line of sight;
/// its place and top speed (slot 0x208).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attacker {
    /// The unit AI's type.
    pub ai_kind: UnitAiKind,
    /// The unit's player.
    pub player: PlayerId,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub player_kind: PlayerKind,
    /// The unit's state.
    pub state: ObjectState,
    /// The kind it holds (`attribute_type_held`, `+0x308`), none for −1.
    pub held: Option<AttributeId>,
    /// Its top action's type, if any.
    pub top_action: Option<ActionKind>,
    /// Its master's id.
    pub master: MasterId,
    /// Its master's type.
    pub kind: MasterKind,
    /// Its master's class.
    pub class: Option<ClassId>,
    /// Its master's `attack_priority`.
    pub attack_priority: AttackPriority,
    /// Its master's missile, −1 for none.
    pub missile: Option<MasterId>,
    /// Its master's weapon range (slot 0x230, `+0x828`).
    pub range: Distance,
    /// Its master's least range (slot 0x240, `+0x850`).
    pub min_range: Distance,
    /// Its master's charge type: 6 and 7 add to the range against a target (slot 0x2f8).
    pub charge_kind: ChargeKind,
    /// Its master's `area_effect_range` (`+0x82c`).
    pub area_effect: Distance,
    /// Its master's `area_effect_level` (`+0x830`).
    pub blast_level: BlastLevel,
    /// Its master's line of sight (`+0x5d0`).
    pub los: Distance,
    /// Its place.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
    /// Its top speed (slot 0x208).
    pub speed: Speed,
    /// The game mode is a scenario's.
    pub scenario: bool,
    /// Its firepower (`+0xa00`): the units inside a building add theirs.
    pub firepower: Firepower,
    /// Its `+0x924` (`FUN_14107cc90`): a Gaia siege unit attacks only with 0.
    pub capture: CaptureKind,
}

/// What a unit AI's attack check and `bestUnitToAttack` read of an object it found: the owner's
/// id, the master's class, type, id and `combat_level` (`+0x62c`), and the object's `+0x924`,
/// unit AI (`+0x6b0`), container (`+0x2f0`), targeting and attackable flags (`+0x689`,
/// `+0x68a`), `desolid_flag` (`+0xc60`), place and master's radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// The object's player.
    pub owner: PlayerId,
    /// Its master's class.
    pub class: ClassId,
    /// Its master's type.
    pub kind: MasterKind,
    /// Its master.
    pub master: MasterId,
    /// Its master's `combat_level`.
    pub combat_level: CombatLevel,
    /// The object's `+0x924`.
    pub capture: CaptureKind,
    /// It has a unit AI (`+0x6b0`).
    pub has_ai: bool,
    /// It is inside another (`+0x2f0`).
    pub inside: bool,
    /// `+0x689`, `unit_targeting_flag`.
    pub targeting: bool,
    /// `+0x68a`, `unit_attackable_flag`.
    pub attackable: bool,
    /// `+0xc60`, `desolid_flag`: a foundation no builder has started.
    pub desolid: bool,
    /// Its place.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
}

/// What `bestUnitToAttack` reads of the unit AI that searches: its stance (`+0x1a9`), state
/// position (`+0x1ac`), order (`+0x3c`), action (`+0x44`), target (`+0x48`), the current and last
/// target's classes (`+0x4c`, `+0x70`) and whether it holds a patrol path (`+0x1c0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seeker {
    /// `+0x1a9`.
    pub stance: UnitStance,
    /// `+0x1ac`, `+0x1b0`.
    pub state_position: Point,
    /// `+0x3c`.
    pub order: Option<OrderKind>,
    /// `+0x44`.
    pub action: Option<AiAction>,
    /// `+0x48`.
    pub target: Option<ObjectId>,
    /// `+0x4c`: the target's class.
    pub target_class: Option<ClassId>,
    /// `+0x70`: the last target's class.
    pub last_target_class: Option<ClassId>,
    /// `+0x1c0` holds a patrol path.
    pub patrol_path: bool,
    /// The master type of the current target as the world holds it ([`Surroundings::target`]),
    /// which the siege unit's attack check reads.
    ///
    /// [`Surroundings::target`]: crate::unit_ai::Surroundings::target
    pub target_master_kind: Option<MasterKind>,
}

impl Seeker {
    /// `canMoveToAttack` with no target (slot 0x1d0 with −1, `FUN_140c48420`; the slice holds no
    /// unit groups): with an order, or without one in a stance below 2.
    #[must_use]
    pub const fn can_move(&self) -> bool {
        self.order.is_some() || self.stance.below(UnitStance::StandGround)
    }
}

/// Slot 0xa0 of `me`'s unit AI with 0, `CanAttack`: whether it may attack `t`, by type: the
/// hunted animal's (`FUN_140c18d00`) while its action list holds an Attack (type 9) or a type 7
/// (slot 0x538); the predator's ([`predator_may_attack`]), the lion's ([`lion_may_attack`]) and
/// the civilian's ([`civilian_may_attack`]) by attack priority and class; the base's
/// (`FUN_140c48400`: the boar, the ranged units, the ships, the artifact, the siege tower,
/// livestock) always; the building's (`FUN_140c18b60`) never; the tower's
/// ([`tower_can_attack`]), the soldier's ([`soldier_can_attack`]) and the siege unit's. The
/// priest's (`FUN_140c19020`: its conversion test, slot 0x1b8) is not modelled here.
pub fn can_attack(
    me: &Attacker,
    seeker: &Seeker,
    t: &Target,
    roles: &Roles,
    unit_ai_tuning: &UnitAiTuning,
) -> Result<bool, GenieError> {
    match me.ai_kind {
        UnitAiKind::HUNTED => Ok(matches!(me.top_action, Some(ATTACK | ActionKind::COMBAT))),
        UnitAiKind::PREDATOR => Ok(predator_may_attack(me.attack_priority, t.class, roles)),
        BOAR_AI
        | UnitAiKind::RANGED_6
        | UnitAiKind::RANGED_7
        | TRADE_AI
        | TRANSPORT_AI
        | FISHING_AI
        | ARTIFACT_AI
        | SIEGE_TOWER_AI
        | UnitAiKind::LIVESTOCK => Ok(true),
        UnitAiKind::LION => Ok(lion_may_attack(me.attack_priority, t.class, roles)),
        CIVILIAN => Ok(civilian_may_attack(me.attack_priority, t.class, roles)),
        BUILDING_AI => Ok(false),
        UnitAiKind::TOWER => tower_can_attack(me, t, roles, unit_ai_tuning),
        UnitAiKind::SOLDIER => soldier_can_attack(me, seeker, t, roles, unit_ai_tuning),
        SIEGE_AI => siege_can_attack(me, seeker, t, false, roles, unit_ai_tuning),
        _ => Err(GenieError::unsupported(
            "an attack check (slot 0xa0) of a unit-AI type not modelled",
        )),
    }
}

/// The predator's slot 0xa0 (`FUN_140c18ed0`) without its `any` flag: by the master's attack
/// priority 1 and 2 anything, 3 the classes of
/// [`ClassRoles::PRIORITY_3_TARGET`](crate::world::layer::ClassRoles::PRIORITY_3_TARGET), any other
/// the classes of
/// [`ClassRoles::PREDATOR_TARGET`](crate::world::layer::ClassRoles::PREDATOR_TARGET).
pub(super) fn predator_may_attack(
    attack_priority: AttackPriority,
    class: ClassId,
    roles: &Roles,
) -> bool {
    match attack_priority {
        AttackPriority::PRIORITY_1 | AttackPriority::PRIORITY_2 => true,
        AttackPriority::PRIORITY_3 => roles
            .of_class(class)
            .contains(ClassRoles::PRIORITY_3_TARGET),
        _ => roles.of_class(class).contains(ClassRoles::PREDATOR_TARGET),
    }
}

/// The tower's attack check (`FUN_140c1a760`): never for Gaia's own tower nor the classes 1 and 42;
/// Gaia's objects only of the classes 9 and 10, unless they are of the combat type with `+0x924` 0
/// outside the scenario modes; never a building (master type `'P'`) no builder has started
/// (`desolid_flag`, `+0xc60`), masters 50 and 1187, nor from a tower in a state below 2. Class 10
/// with an attack priority below 3 only of combat level 4 (`FUN_140fde120`); else when the tower is
/// armed (`FUN_14120ea30`: classes 51 and 54, or a missile, or else a firepower, `+0xa00`, above 1)
/// and its attack priority is not 3 or the class one of
/// [`ClassRoles::PRIORITY_3_TARGET`](crate::world::layer::ClassRoles::PRIORITY_3_TARGET).
pub fn tower_can_attack(
    me: &Attacker,
    t: &Target,
    roles: &Roles,
    unit_ai_tuning: &UnitAiTuning,
) -> Result<bool, GenieError> {
    if me.player == PlayerId::GAIA
        || roles
            .of_class(t.class)
            .intersects(ClassRoles::ARTIFACT | ClassRoles::RELIC)
    {
        return Ok(false);
    }
    let gaia_combat = !me.scenario && MasterKind::COMBAT <= t.kind && !t.capture.any();
    if !gaia_combat
        && t.owner == PlayerId::GAIA
        && !roles
            .of_class(t.class)
            .intersects(ClassRoles::PREY | ClassRoles::PREDATOR)
    {
        return Ok(false);
    }
    if (t.kind == MasterKind::BUILDING && t.desolid)
        || roles
            .of_master(t.master)
            .contains(MasterRoles::TOWER_SPARED)
        || me.state <= ObjectState::Unfinished
    {
        return Ok(false);
    }
    if roles.of_class(t.class).contains(ClassRoles::PREDATOR)
        && me.attack_priority < AttackPriority::PRIORITY_3
    {
        return Ok(t.combat_level == CombatLevel::MILITARY);
    }
    if !roles
        .of_some_class(me.class)
        .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
        && me.missile.is_none()
        && me.firepower <= Firepower::new(unit_ai_tuning.armed_firepower)
    {
        return Ok(false);
    }
    Ok(!me.attack_priority.limited()
        || roles
            .of_class(t.class)
            .contains(ClassRoles::PRIORITY_3_TARGET))
}

/// The squared gap between a point and an object's footprint (`FUN_140a847c0`: per axis the
/// distance less the radius, `MAXSS 0` of it, so a gap that is not a number stays one).
pub(super) fn point_gap_squared(point: Point, at: Position, radius: Footprint) -> DistanceSquared {
    let gap = |d: f32, r: f32| maxss(0.0, d.abs() - r);
    let x = gap(point.x() - at.x(), radius.x());
    let y = gap(point.y() - at.y(), radius.y());
    DistanceSquared::new(y * y + x * x)
}

impl Attacker {
    /// The weapon range against a target (slot 0x238, `FUN_14108d9b0`): the master's (slot
    /// 0x230); a charge of type 6 or 7 adds to it (slot 0x2f8), which is not modelled.
    ///
    /// # Errors
    ///
    /// A charge of type 6 or 7.
    pub const fn range_against(&self) -> Result<Distance, GenieError> {
        if self.charge_kind.ranged() {
            return Err(GenieError::unsupported(
                "an attacker whose charge adds to its range (slot 0x2f8)",
            ));
        }
        Ok(self.range)
    }
}

/// The soldier's attack check (`FUN_140c19a20`): never class 64; a Gaia soldier only objects of
/// the combat type with `+0x924` 0 outside the scenario modes (`FUN_141302540`, those it takes on
/// by sight), and of Gaia's objects only classes 9 and 10 or those; never an object whose
/// targeting flag (`+0x689`) is off. A computer player's tests (its priorities, ranges) are not
/// modelled. Never from class 51, master 444, a packed class 54 or master 109 (top action 124 or
/// 125), or a unit holding kind 57; never the classes 1, 42 and 60, nor a building no builder has
/// started (`+0xc60`); never in `NoAttack`. In `StandGround` (stance 2) only within its range
/// against the target plus 0.5 of its state position (by the squared footprint gap,
/// `FUN_140a847c0`), but on order `0x2bd`; in Defend (1) within 5 more, the patrol orders' and a
/// patrol path's tests not modelled. Then an attack priority of 3 takes only
/// [`ClassRoles::PRIORITY_3_TARGET`](crate::world::layer::ClassRoles::PRIORITY_3_TARGET).
pub fn soldier_can_attack(
    me: &Attacker,
    s: &Seeker,
    t: &Target,
    roles: &Roles,
    unit_ai_tuning: &UnitAiTuning,
) -> Result<bool, GenieError> {
    if roles.of_class(t.class).contains(ClassRoles::LAND_MINE) {
        return Ok(false);
    }
    let gaia_combat = !me.scenario && MasterKind::COMBAT <= t.kind && !t.capture.any();
    if me.player == PlayerId::GAIA && !gaia_combat {
        return Ok(false);
    }
    if t.owner == PlayerId::GAIA
        && !roles
            .of_class(t.class)
            .intersects(ClassRoles::PREY | ClassRoles::PREDATOR)
        && !gaia_combat
    {
        return Ok(false);
    }
    if !t.targeting {
        return Ok(false);
    }
    if me.player_kind.is_computer() {
        return Err(GenieError::unsupported(
            "a computer player's soldier's attack check",
        ));
    }
    if never_attacks(me, roles) {
        return Ok(false);
    }
    if roles
        .of_some_attribute(me.held)
        .contains(AttributeRoles::RELIC)
        || roles
            .of_class(t.class)
            .intersects(ClassRoles::ARTIFACT | ClassRoles::RELIC | ClassRoles::MISC_BUILDING)
    {
        return Ok(false);
    }
    if t.kind == MasterKind::BUILDING && t.desolid {
        return Ok(false);
    }
    if s.stance == UnitStance::NoAttack {
        return Ok(false);
    }
    if !stance_reaches(me, s, t, unit_ai_tuning)? {
        return Ok(false);
    }
    Ok(!me.attack_priority.limited()
        || roles
            .of_class(t.class)
            .contains(ClassRoles::PRIORITY_3_TARGET))
}

/// [`soldier_can_attack`]'s soldiers that never attack: class 51, master 444, and a packed class
/// 54 or master 109 under its Pack or Unpack (top action 124 or 125).
pub(super) fn never_attacks(me: &Attacker, roles: &Roles) -> bool {
    if roles
        .of_some_class(me.class)
        .contains(ClassRoles::PACKED_TREBUCHET)
        || roles
            .of_master(me.master)
            .contains(MasterRoles::PEACEFUL_SOLDIER)
    {
        return true;
    }
    (roles
        .of_some_class(me.class)
        .contains(ClassRoles::PACKED_ATTACKS_NOTHING)
        || roles
            .of_master(me.master)
            .contains(MasterRoles::PACKED_PEACEFUL))
        && matches!(me.top_action, Some(ActionKind::PACK | ActionKind::UNPACK))
}

/// [`soldier_can_attack`]'s stance: in `StandGround` (stance 2) only within its range against
/// the target plus 0.5 of its state position (by the squared footprint gap, `FUN_140a847c0`),
/// but on order `0x2bd`; in Defend (1) within 5 more, the patrol orders' and a patrol path's
/// tests not modelled; any other stance reaches.
///
/// # Errors
///
/// A range the slice does not model, and a defending patrol's range.
pub(super) fn stance_reaches(
    me: &Attacker,
    s: &Seeker,
    t: &Target,
    unit_ai_tuning: &UnitAiTuning,
) -> Result<bool, GenieError> {
    let d2 = point_gap_squared(s.state_position, t.position, t.radius);
    let reach = me.range_against()? + Distance::new(unit_ai_tuning.stance_range_growth);
    match s.stance {
        UnitStance::StandGround
            if !matches!(s.order, Some(OrderKind::Defend)) && reach.squared() < d2 =>
        {
            return Ok(false);
        }
        UnitStance::Defensive => {
            if matches!(s.order, Some(OrderKind::Patrol | OrderKind::AttackMove)) || s.patrol_path {
                defend_patrol_range()?;
            }
            let reach = reach + Distance::new(unit_ai_tuning.defend_growth);
            if reach.squared() < d2 {
                return Ok(false);
            }
        }
        _ => {}
    }
    Ok(true)
}

/// `FUN_14120e4e0` for an attacker of type 70: one of attack priority 3 attacks only
/// [`ClassRoles::PRIORITY_3_TARGET`](crate::world::layer::ClassRoles::PRIORITY_3_TARGET); a ranged
/// one (range above 1) with a blast (`+0x82c` above 0) whose `area_effect_level` has neither
/// [`BlastLevel::SPARES_FRIENDS`] nor [`BlastLevel::SKIPS_FRIEND_CHECK`] (`FUN_1410bf160`,
/// `FUN_1410bfcb0`) scans the tiles of the blast around the target for its own or allied objects
/// (not modelled).
pub(crate) fn spares_friends(me: &Attacker, t: &Target, roles: &Roles) -> Result<bool, GenieError> {
    if me.attack_priority == AttackPriority::PRIORITY_3
        && !roles
            .of_class(t.class)
            .contains(ClassRoles::PRIORITY_3_TARGET)
    {
        return Ok(false);
    }
    let ranged = Distance::new(1.0) < me.range;
    if ranged
        && !me.blast_level.contains(BlastLevel::SKIPS_FRIEND_CHECK)
        && Distance::ZERO < me.area_effect
        && !me.blast_level.contains(BlastLevel::SPARES_FRIENDS)
    {
        return Err(GenieError::unsupported(
            "a blast attacker's scan for friends near its target (FUN_14120e4e0)",
        ));
    }
    Ok(true)
}
