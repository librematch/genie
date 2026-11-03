//! What the unit AI knows of its surroundings: the searches' results, its home, its targets
//! and what it has seen.

use crate::GenieError;
use crate::combat::Firepower;
use crate::construction::MainBuildingId;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::WholeDistance;
use crate::map::spatial::TileSight;
use crate::math::fmath::BVector;
use crate::movement::Speed;
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::{ObjectId, ObjectKey};
use crate::ownership::CaptureKind;
use crate::players::Chance;
use crate::players::PlayerId;
use crate::unit_ai::Search;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::UnitAiTuning;
use genie_diff::Diff;
use std::collections::HashSet;

/// What a [`Search`] answers: the object picked, if any, and whether the lists it read hold
/// enemies (the fifth relation list), which sets the alert mode when its query ran (`fresh`; a
/// search that read the last answer leaves the alert mode as it is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    /// The object picked.
    pub id: Option<ObjectId>,
    /// The lists read hold enemies.
    pub enemies: bool,
    /// The object's place (x, y, z), for the caller's use.
    pub position: Option<Position>,
    /// Its query ran (else it read the last answer).
    pub fresh: bool,
    /// `ClosestObject`'s count of the players but Gaia whose objects the lists read hold
    /// (`DAT_143e6f92c`, `numberDifferentPlayerObjectsVisible`); 0 from the other picks, which
    /// do not count.
    pub players: u32,
    /// What `ClosestObject` writes to its seventh argument: the object's search distance,
    /// truncated (`CVTTSS2SI`); −1 (`DAT_14330ec20`) without one and from the other picks.
    pub distance: WholeDistance,
    /// The retarget entries `bestUnitToAttack`'s failed paths add (`FUN_140c42320`: a target and
    /// the world time until which it is ignored; at most three), the unit AI's.
    pub retries: [Option<(ObjectId, WorldTime)>; 3],
}

impl Found {
    /// Nothing found, no enemies.
    pub const NONE: Self = Self {
        id: None,
        enemies: false,
        position: None,
        fresh: true,
        players: 0,
        distance: WholeDistance::NONE,
        retries: [None; 3],
    };
}

/// The builder's flood of `FUN_140c1c7c0` from tile `at` with reach `r`: the start is tested
/// first; then each tile it reached, the first queued first, tries its neighbours left, right,
/// below and above (x − 1, x + 1, y − 1, y + 1) while it lies less than `r − 1` from the start on
/// both axes, and a neighbour not yet reached is tested and queued. The first tile `test` names
/// an object on answers it.
///
/// # Errors
///
/// The first error `test` returns.
pub fn flood_search(
    at: Tile,
    r: i32,
    mut test: impl FnMut(Tile) -> Result<Option<ObjectId>, GenieError>,
) -> Result<Option<ObjectId>, GenieError> {
    let (ux, uy) = (at.x(), at.y());
    let mut reached = HashSet::from([at]);
    if let Some(id) = test(at)? {
        return Ok(Some(id));
    }
    let mut queue = vec![at];
    let mut next = 0;
    while let Some(&q) = queue.get(next) {
        next += 1;
        if (ux - q.x()).abs() >= r - 1 || (uy - q.y()).abs() >= r - 1 {
            continue;
        }
        for n in q.beside() {
            if !reached.insert(n) {
                continue;
            }
            if let Some(id) = test(n)? {
                return Ok(Some(id));
            }
            queue.push(n);
        }
    }
    Ok(None)
}

/// A search that never runs (units without a search).
pub const fn no_search(_: &Search<'_>) -> Result<Found, GenieError> {
    Err(GenieError::unsupported(
        "a unit AI's search the caller does not answer",
    ))
}

/// A boar's part of its unit AI (`TribeElephantUnitAIModule`, factory type 4), which its idle check
/// reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Home {
    /// `+0x1f0`..`+0x1f8`, `origin`: the point it wanders around.
    pub origin: Position,
    /// `+0x1fc`, `furious`: it does not wander.
    pub furious: bool,
    /// `+0x1fd`: its next idle check sends it home; its slot 0x1f0 (`FUN_140c16d50`) sets it
    /// after a failed Attack ([`crate::combat::attack::Attack::update`]). Not saved; a loaded boar
    /// starts without it.
    #[diff(unsaved)]
    pub go_home: bool,
}

/// What an idle check reads of its unit and the game, and the build's roles and slots it reads
/// them by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surroundings {
    /// The unit's position (`+0x3c8`, `+0x4c8`, `+0x5a8`).
    pub position: Position,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
    /// Its top speed (slot 0x208, `FUN_141202c70`; 0 for a unit that does not move), the map's
    /// size in tiles and the world's dt in seconds (`world+0x164`).
    pub speed: Speed,
    /// The map's size in tiles.
    pub map: GridSize,
    /// The world's dt in seconds (`world+0x164`).
    pub dt_seconds: Seconds,
    /// The game's setting at `+0x680` (`FUN_140ec0cd0`) is 1: boars stay put.
    pub mode_1: bool,
    /// The unit's action list has an action (object slot 0x430, `FUN_14106d0e0`; its slot 0x818,
    /// `FUN_14106d100`, is 0).
    pub busy: bool,
    /// The owner's id (`player+0x718`); Gaia is 0.
    pub player: PlayerId,
    /// The unit's master's class (decoded).
    pub class: Option<ClassId>,
    /// The master's `combat_level` (`+0x62c`).
    pub combat_level: CombatLevel,
    /// The master's `attack_priority` (`+0x92c`, `FUN_141202220`).
    pub attack_priority: AttackPriority,
    /// The master's radii (`+0x5d8`, `+0x5dc`), line of sight (`+0x5d0`) and search radius
    /// (`+0x794`, the unit's slot 0x2c8).
    pub size: Footprint,
    /// The master's line of sight (`+0x5d0`).
    pub los: Distance,
    /// The search radius (`+0x794`, the unit's slot 0x2c8).
    pub search_radius: Distance,
    /// Slot 0x230 (`FUN_14108d960`): its master's weapon range (`+0x828`).
    pub weapon_range: Distance,
    /// The unit's slot 0x2d0 with 0 (`FUN_14106d170`): its top action may be interrupted, or it
    /// has none.
    pub interruptible: bool,
    /// The master has a task of action type 0x99 (`FUN_1415c60b0`; the mule cart's): such a
    /// unit with an action or a target takes no queued order (`FUN_140c4eea0`).
    pub task_0x99: bool,
    /// An object of type 70 or more inside another (`+0x2f0`): it runs no queued order.
    pub garrisoned: bool,
    /// The unit's state (`+0x640`), type (`+0xf`), `+0x924` and master id.
    pub state: ObjectState,
    /// The unit's type (`+0xf`).
    pub kind: MasterKind,
    /// Its master has a missile (`+0x83c` 0 or more, `FUN_1410bfca0`).
    pub ranged: bool,
    /// Its missiles per volley (`+0xa00`, `FUN_1412136d0`), 0 for a unit without volleys.
    pub firepower: Firepower,
    /// The unit's `+0x924`.
    pub capture: CaptureKind,
    /// The unit's master.
    pub master: MasterId,
    /// The game mode (the save's `mGameMode`, options `+0x34`) is one of `FUN_141302540`'s:
    /// Scenario (3), Campaign (4), King of the Hill (5), Co-op Campaign (15).
    pub scenario: bool,
    /// The master has a task of action type 0x69 (`FUN_1415c60b0`; the healers'), whose idle
    /// soldier looks for an ally to heal (`FUN_140c16f00`).
    pub task_0x69: bool,
    /// The unit AI's current target as the world's object table holds it (`FUN_1415417a0`),
    /// `None` when it does not exist or the unit AI has none.
    pub target: Option<Seen>,
    /// The unit's `+0x685`, `underAttackValue`.
    pub under_attack: bool,
    /// The unit's id (`+8`), which its attackers' unit AIs must target to stay attackers.
    pub id: ObjectId,
    /// The unit's slot 0x1f8 (`FUN_14106a6c0`): its animation speed (`+0x7b8`); None while the
    /// slice does not know it.
    pub anim_speed: Option<Speed>,
    /// The unit's slot 0x2b0 (`RGE_Action_Object::actionState`): its top action's state, 0
    /// without one; None for an action the slice does not hold.
    pub action_state: Option<ActionState>,
    /// The unit's slot 0x538 with 9 (`FUN_14106d0c0`): its action list holds an Attack (type
    /// 9); read with [`Surroundings::action_state`], which tells whether the slice holds the
    /// list.
    pub attack_listed: bool,
    /// The objects the unit AI names besides its target, as the world's object table holds them:
    /// its attackers ([`UnitAi::attackers`]) and the callers of its queued damage notifies. One
    /// the table does not hold is absent.
    ///
    /// [`UnitAi::attackers`]: crate::unit_ai::UnitAi::attackers
    pub units: Vec<UnitSeen>,
    /// The unit's group (`+0x6d0`, the world's `FUN_14152ff40`), when the world holds it: what
    /// the full update reads of it ([`GroupState`]).
    pub group: Option<GroupState>,
    /// Slot 400 (`FUN_14106d090`): its top action's target (`FUN_140a78450`) as the world holds
    /// it; None without one.
    pub top_target: Option<TopTarget>,
    /// Slot 0x240 (`FUN_1410863c0`): its master's least range (`+0x850`).
    pub min_range: Distance,
    /// The unit's `+0x7fb`, `mAttackMoving`.
    pub attack_moving: bool,
    /// Where the unit last moved to (slot 0x520: `+0x824`), as the game's vector (x, height,
    /// y); an object without movement state, its place.
    pub moved_at: BVector,
    /// Its owner's `chance_to_dodge_missiles` (`player+0xe10`, saved): a ship's handler takes a
    /// missile's warning (`0x20f`) as damage unless a draw below it dodges.
    pub dodge_chance: Chance,
    /// Its owner's `chance_for_archers_to_maintain_distance` (`player+0xe11`, saved): a computer
    /// player's siege unit backs up after a shot on a draw at least this.
    pub keep_distance_chance: Chance,
    /// Its owner's `disableTowerPriority` (`player+0x620`, GAME 0xb's setting 0xe): a computer
    /// player's unit but a siege weapon, unpacked trebuchet or warship ignores a tower's damage
    /// ([`UnitAi::damaged`]).
    ///
    /// [`UnitAi::damaged`]: crate::unit_ai::UnitAi::damaged
    pub disable_tower_priority: bool,
    /// An object inside it (`+0x2f8`) belongs to another owner that is a computer player: a
    /// building's or tower's damage alert (0x201) reaches that owner's AI too (its slot 0x2e8,
    /// then slot 0x3c8, [`alert_owner`]).
    ///
    /// [`alert_owner`]: crate::unit_ai::alert_owner
    pub guest_computer: bool,
    /// The build's roles, through which the unit AI reads the classes and masters it meets
    /// (FD-046-D45).
    pub roles: Roles,
    /// The build's slots: the classes and masters the unit AI's code names.
    pub slots: Slots,
    /// The build's tuning values of the unit AI (FD-046-D49).
    pub unit_ai_tuning: UnitAiTuning,
}

/// The target of a unit's top action as its unit AI reads it ([`Surroundings::top_target`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopTarget {
    /// `+8`.
    pub id: ObjectId,
    /// Its type (`+0xf`).
    pub kind: MasterKind,
    /// Its place.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
}

/// The target of the unit's top action as the full update stands ([`UnitAi::top_target_live`]).
///
/// [`UnitAi::top_target_live`]: crate::unit_ai::UnitAi::top_target_live
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LiveTop {
    /// `+8`.
    pub(super) id: ObjectId,
    /// Its type (`+0xf`).
    pub(super) kind: MasterKind,
    /// The gap between its footprint and the unit's, squared (`FUN_14118f8c0`).
    pub(super) gap_squared: DistanceSquared,
}

/// The idle part's group test (`0x140c67e8f`): no group, or one that is idle (`FUN_141017ad0`) of
/// a unit that does not move (slot 0x5c8).
pub(super) fn group_lets_idle(group: Option<GroupState>) -> bool {
    group.is_none_or(|g| g.idle && !g.member_moving)
}

/// What the full update (`0x140c66b10`) reads of a unit's group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroupState {
    /// Its current command (`+0x3a8`) is not -1.
    pub commanded: bool,
    /// `IsIdle` (`FUN_141017ad0`, [`crate::groups::UnitGroup::is_idle`]).
    pub idle: bool,
    /// It patrols: its top path's [`crate::movement::PathFlags::LOOPS`] (`FUN_141017c40`) or its
    /// [`crate::groups::GroupFlags::PATROL`] (`FUN_141017c30`).
    pub patrol: bool,
    /// Its motion moves (`FUN_141046600`).
    pub moving: bool,
    /// The unit itself moves (its slot 0x5c8), which the idle part reads with the group.
    pub member_moving: bool,
}

impl Surroundings {
    /// The object `id` among [`Surroundings::units`].
    #[must_use]
    pub fn unit(&self, id: impl ObjectKey) -> Option<&UnitSeen> {
        let id = id.key()?;
        self.units.iter().find(|u| u.id == id)
    }
}

/// An object a unit AI names, as its attackers' upkeep and `ClosestValidAttacker` read it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnitSeen {
    /// `+8`.
    pub id: ObjectId,
    /// Its master's class.
    pub class: ClassId,
    /// Its owner's id (`+0x138`, `player+0x718`); Gaia is 0.
    pub owner: PlayerId,
    /// Its master's `combat_level` (`+0x62c`).
    pub combat_level: CombatLevel,
    /// `+0x640`.
    pub state: ObjectState,
    /// Its unit AI's current target (`+0x48`); None without a unit AI or a target.
    pub target: Option<ObjectId>,
    /// The gap between its footprint and the unit's, squared (`FUN_14118f8c0`).
    pub gap_squared: DistanceSquared,
    /// Its place.
    pub position: Position,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
    /// Its master's id and type (`+0xf`).
    pub master: MasterId,
    /// Its type (`+0xf`).
    pub kind: MasterKind,
    /// An annex's main building (`+0xbb8`).
    pub main: Option<MainBuildingId>,
    /// The unit's owner counts its owner as its own or an ally (`FUN_141663d90` with the ally
    /// flag, [`crate::unit_ai::task_finder::allied`]).
    pub allied: bool,
}

/// What a computer player's unit weighs of an object on a damage notify ([`UnitAi::damaged`]):
/// its current target's or its attacker's threat and how long it would take to kill.
///
/// [`UnitAi::damaged`]: crate::unit_ai::UnitAi::damaged
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duel {
    /// The object may convert the unit (its unit AI's slot 0x1b8 with the unit and 1: a priest's
    /// [`can_convert`], any other's 0).
    ///
    /// [`can_convert`]: crate::unit_ai::can_convert
    pub converts: bool,
    /// Its attacks' sum (slot 0x228, `FUN_141084070`: its master's attacks above 0).
    pub attacks: f32,
    /// What it deals the unit per hit (the unit's slot 0x220 with it, `FUN_141083f20`).
    pub harms: HitPoints,
    /// What the unit deals it per hit (its slot 0x220 with the unit).
    pub harmed: HitPoints,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
}

/// An object as a unit AI's target block reads it: its state (`+0x640`) and type (`+0xf`); as
/// the follow order reads it (slot 73): its owner, master's class, place (x, y, z), its master's
/// radii on the ground (`+0x5d8`, `+0x5dc`) and whether the unit's player sees its tile now
/// (`FUN_140c68a00` gives 0xf: the tile's bit of the unified map and the player's mask `+0x2a8`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    /// The object's state.
    pub state: ObjectState,
    /// Its master's type.
    pub kind: MasterKind,
    /// Its owner.
    pub owner: PlayerId,
    /// Its master's class.
    pub class: ClassId,
    /// Its place.
    pub position: Position,
    /// Its master's radii.
    pub radius: Footprint,
    /// Its tile is visible to the viewer.
    pub visible: bool,
    /// Its master's `object_flags` (`+0x654`).
    pub flags: ObjectFlags,
    /// The viewer's test of its tile (`FUN_1416635e0`, [`crate::map::spatial::Spatial::tile_sight`]):
    /// 0xf seen, 0x80 explored, else 0.
    pub sight: TileSight,
}

/// The game's `force` argument of a unit AI's order, its attack command and its move
/// ([`AiMove::force`]): forced, the order replaces what the AI or the unit does; else each
/// order's own test decides (most fail while the AI has an action value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderForce {
    /// Forced (1).
    Forced,
    /// Not forced (0).
    Unforced,
}

/// A move the unit AI gives its unit (unit-AI slot 0x130, `FUN_140c55620`, to the unit's slot
/// 0x340): a `MoveTo` to `point` within `range`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiMove {
    /// The point.
    pub point: Position,
    /// The range within which it arrives.
    pub range: Distance,
    /// The sixth argument: forced, the move replaces even a `MoveTo` to the same point.
    pub force: OrderForce,
    /// The seventh: the unit walks (its master's walking sprite, `+0x748`), not runs.
    pub walk: bool,
}
