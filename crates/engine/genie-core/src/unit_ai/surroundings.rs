//! What the unit AI knows of its surroundings: the searches' results, its home, its targets
//! and what it has seen; and the unit's read of them as its AI runs ([`Unit::surroundings`]).

use crate::GenieError;
use crate::GenieResult;
use crate::combat::Firepower;
use crate::construction::MainBuildingId;
use crate::construction::build::CART_TASK;
use crate::groups::GroupCommandKind;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::WholeDistance;
use crate::map::spatial::TileSight;
use crate::math::fmath::BVector;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::ownership::CaptureKind;
use crate::players::Chance;
use crate::players::PlayerId;
use crate::unit_ai::world_view::OwnerView;
use crate::unit_ai::world_view::changed_view;
use crate::world::AiSwitches;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::UnitAiTuning;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use genie_diff::Diff;
use std::collections::HashSet;

/// What a [`Search`] answers: the object picked, if any, and whether the lists it read hold
/// enemies (the fifth relation list), which sets the alert mode when its query ran (`fresh`; a
/// search that read the last answer leaves the alert mode as it is).
///
/// [`Search`]: crate::unit_ai::Search
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    /// The object picked.
    pub(crate) id: Option<ObjectId>,
    /// The lists read hold enemies.
    pub(crate) enemies: bool,
    /// The object's place (x, y, z), for the caller's use.
    pub(crate) position: Option<Position>,
    /// Its query ran (else it read the last answer).
    pub(crate) fresh: bool,
    /// `ClosestObject`'s count of the players but Gaia whose objects the lists read hold
    /// (`DAT_143e6f92c`, `numberDifferentPlayerObjectsVisible`); 0 from the other picks, which
    /// do not count.
    pub(crate) players: u32,
    /// What `ClosestObject` writes to its seventh argument: the object's search distance,
    /// truncated (`CVTTSS2SI`); −1 (`DAT_14330ec20`) without one and from the other picks.
    pub(crate) distance: WholeDistance,
    /// The retarget entries `bestUnitToAttack`'s failed paths add (`FUN_140c42320`: a target and
    /// the world time until which it is ignored; at most three), the unit AI's.
    pub(crate) retries: [Option<(ObjectId, WorldTime)>; 3],
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
    mut test: impl FnMut(Tile) -> GenieResult<Option<ObjectId>>,
) -> GenieResult<Option<ObjectId>> {
    let (ux, uy) = (at.x(), at.y());
    let mut reached = HashSet::from([at]);
    if let Some(id) = test(at)? {
        return Ok(Some(id));
    }
    let mut queue = vec![at];
    let mut next = 0;
    while let Some(&next_tile) = queue.get(next) {
        next += 1;
        if (ux - next_tile.x()).abs() >= r - 1 || (uy - next_tile.y()).abs() >= r - 1 {
            continue;
        }
        for tile in next_tile.beside() {
            if !reached.insert(tile) {
                continue;
            }
            if let Some(id) = test(tile)? {
                return Ok(Some(id));
            }
            queue.push(tile);
        }
    }
    Ok(None)
}

/// A boar's part of its unit AI (`TribeElephantUnitAIModule`, factory type 4), which its idle check
/// reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Home {
    /// `+0x1f0`..`+0x1f8`, `origin`: the point it wanders around.
    pub(crate) origin: Position,
    /// `+0x1fc`, `furious`: it does not wander.
    pub(crate) furious: bool,
    /// `+0x1fd`: its next idle check sends it home; its slot 0x1f0 (`FUN_140c16d50`) sets it
    /// after a failed Attack ([`crate::combat::attack::Attack::update`]). Not saved; a loaded boar
    /// starts without it.
    #[diff(unsaved)]
    pub(crate) homeward: bool,
}

bitflags::bitflags! {
    /// What an idle check reads of its unit and the game as yes or no ([`Surroundings`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SurroundingFlags: u16 {
        /// The game's setting at `+0x680` (`FUN_140ec0cd0`) is 1: boars stay put.
        const MODE_1 = 1;
        /// The unit's action list has an action (object slot 0x430, `FUN_14106d0e0`; its slot
        /// 0x818, `FUN_14106d100`, is 0).
        const BUSY = 1 << 1;
        /// The unit's slot 0x2d0 with 0 (`FUN_14106d170`): its top action may be interrupted, or it
        /// has none.
        const INTERRUPTIBLE = 1 << 2;
        /// The master has a task of action type 0x99 (`FUN_1415c60b0`; the mule cart's): such a
        /// unit with an action or a target takes no queued order (`FUN_140c4eea0`).
        const TASK_0X99 = 1 << 3;
        /// An object of type 70 or more inside another (`+0x2f0`): it runs no queued order.
        const GARRISONED = 1 << 4;
        /// Its master has a missile (`+0x83c` 0 or more, `FUN_1410bfca0`).
        const RANGED = 1 << 5;
        /// The game mode (the save's `mGameMode`, options `+0x34`) is one of `FUN_141302540`'s:
        /// Scenario (3), Campaign (4), King of the Hill (5), Co-op Campaign (15).
        const SCENARIO = 1 << 6;
        /// The master has a task of action type 0x69 (`FUN_1415c60b0`; the healers'), whose idle
        /// soldier looks for an ally to heal (`FUN_140c16f00`).
        const TASK_0X69 = 1 << 7;
        /// The unit's `+0x685`, `underAttackValue`.
        const UNDER_ATTACK = 1 << 8;
        /// The unit's slot 0x538 with 9 (`FUN_14106d0c0`): its action list holds an Attack (type
        /// 9); read with [`Surroundings::action_state`], which tells whether the slice holds the
        /// list.
        const ATTACK_LISTED = 1 << 9;
        /// The unit's `+0x7fb`, `mAttackMoving`.
        const ATTACK_MOVING = 1 << 10;
        /// Its owner's `disableTowerPriority` (`player+0x620`, GAME 0xb's setting 0xe): a computer
        /// player's unit but a siege weapon, unpacked trebuchet or warship ignores a tower's damage
        /// ([`UnitAi::damaged`]).
        ///
        /// [`UnitAi::damaged`]: crate::unit_ai::UnitAi::damaged
        const DISABLE_TOWER_PRIORITY = 1 << 11;
        /// An object inside it (`+0x2f8`) belongs to another owner that is a computer player: a
        /// building's or tower's damage alert (0x201) reaches that owner's AI too (its slot 0x2e8,
        /// then slot 0x3c8, [`alert_owner`]).
        ///
        /// [`alert_owner`]: crate::unit_ai::alert_owner
        const GUEST_COMPUTER = 1 << 12;
    }
}

/// What an idle check reads of its unit and the game, and the build's roles and slots it reads
/// them by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surroundings {
    /// The unit's position (`+0x3c8`, `+0x4c8`, `+0x5a8`).
    pub(crate) position: Position,
    /// Its hit points (`+0x278`).
    pub(crate) hp: HitPoints,
    /// Its top speed (slot 0x208, `FUN_141202c70`; 0 for a unit that does not move), the map's
    /// size in tiles and the world's dt in seconds (`world+0x164`).
    pub(crate) speed: Speed,
    /// The map's size in tiles.
    pub(crate) map: GridSize,
    /// The world's dt in seconds (`world+0x164`).
    pub(crate) dt_seconds: Seconds,
    /// Its yes-or-no reads.
    pub(crate) flags: SurroundingFlags,
    /// The owner's id (`player+0x718`); Gaia is 0.
    pub(crate) player: PlayerId,
    /// The unit's master's class (decoded).
    pub(crate) class: Option<ClassId>,
    /// The master's `combat_level` (`+0x62c`).
    pub(crate) combat_level: CombatLevel,
    /// The master's `attack_priority` (`+0x92c`, `FUN_141202220`).
    pub(crate) attack_priority: AttackPriority,
    /// The master's radii (`+0x5d8`, `+0x5dc`), line of sight (`+0x5d0`) and search radius
    /// (`+0x794`, the unit's slot 0x2c8).
    pub(crate) size: Footprint,
    /// The master's line of sight (`+0x5d0`).
    pub(crate) los: Distance,
    /// The search radius (`+0x794`, the unit's slot 0x2c8).
    pub(crate) search_radius: Distance,
    /// Slot 0x230 (`FUN_14108d960`): its master's weapon range (`+0x828`).
    pub(crate) weapon_range: Distance,
    /// The unit's state (`+0x640`), type (`+0xf`), `+0x924` and master id.
    pub(crate) state: ObjectState,
    /// The unit's type (`+0xf`).
    pub(crate) kind: MasterKind,
    /// Its missiles per volley (`+0xa00`, `FUN_1412136d0`), 0 for a unit without volleys.
    pub(crate) firepower: Firepower,
    /// The unit's `+0x924`.
    pub(crate) capture: CaptureKind,
    /// The unit's master.
    pub(crate) master: MasterId,
    /// The unit AI's current target as the world's object table holds it (`FUN_1415417a0`),
    /// `None` when it does not exist or the unit AI has none.
    pub(crate) target: Option<Seen>,
    /// The unit's id (`+8`), which its attackers' unit AIs must target to stay attackers.
    pub(crate) id: ObjectId,
    /// The unit's slot 0x1f8 (`FUN_14106a6c0`): its animation speed (`+0x7b8`); None while the
    /// slice does not know it.
    pub(crate) anim_speed: Option<Speed>,
    /// The unit's slot 0x2b0 (`RGE_Action_Object::actionState`): its top action's state, 0
    /// without one; None for an action the slice does not hold.
    pub(crate) action_state: Option<ActionState>,
    /// The objects the unit AI names besides its target, as the world's object table holds them:
    /// its attackers ([`UnitAi::attackers`]), the callers of its queued damage notifies and
    /// missile warnings, and the units in its way. One the table does not hold is absent.
    ///
    /// [`UnitAi::attackers`]: crate::unit_ai::UnitAi::attackers
    pub(crate) units: Vec<UnitSeen>,
    /// The unit's group (`+0x6d0`, the world's `FUN_14152ff40`), when the world holds it: what
    /// the full update reads of it ([`GroupState`]).
    pub(crate) group: Option<GroupState>,
    /// Slot 400 (`FUN_14106d090`): its top action's target (`FUN_140a78450`) as the world holds
    /// it; None without one.
    pub(crate) top_target: Option<TopTarget>,
    /// Slot 0x240 (`FUN_1410863c0`): its master's least range (`+0x850`).
    pub(crate) min_range: Distance,
    /// Where the unit last moved to (slot 0x520: `+0x824`), as the game's vector (x, height,
    /// y); an object without movement state, its place.
    pub(crate) moved_at: BVector,
    /// Its owner's `chance_to_dodge_missiles` (`player+0xe10`, saved): a ship's handler takes a
    /// missile's warning (`0x20f`) as damage unless a draw below it dodges.
    pub(crate) dodge_chance: Chance,
    /// Its owner's `chance_for_archers_to_maintain_distance` (`player+0xe11`, saved): a computer
    /// player's siege unit backs up after a shot on a draw at least this.
    pub(crate) keep_distance_chance: Chance,
    /// The build's roles, through which the unit AI reads the classes and masters it meets
    /// (FD-046-D45).
    pub(crate) roles: Roles,
    /// The build's slots: the classes and masters the unit AI's code names.
    pub(crate) slots: Slots,
    /// The build's tuning values of the unit AI (FD-046-D49).
    pub(crate) unit_ai_tuning: UnitAiTuning,
}

/// The target of a unit's top action as its unit AI reads it ([`Surroundings::top_target`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopTarget {
    /// `+8`.
    pub(crate) id: ObjectId,
    /// Its type (`+0xf`).
    pub(crate) kind: MasterKind,
    /// Its place.
    pub(crate) position: Position,
    /// Its master's radii.
    pub(crate) radius: Footprint,
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
    group.is_none_or(|group_state| {
        group_state.contains(GroupState::IDLE) && !group_state.contains(GroupState::MEMBER_MOVING)
    })
}

bitflags::bitflags! {
    /// What the full update (`0x140c66b10`) reads of a unit's group.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct GroupState: u8 {
        /// Its current command (`+0x3a8`) is not -1.
        const COMMANDED = 1;
        /// `IsIdle` (`FUN_141017ad0`, [`crate::groups::UnitGroup::is_idle`]).
        const IDLE = 1 << 1;
        /// It patrols: its top path's [`crate::movement::PathFlags::LOOPS`] (`FUN_141017c40`) or
        /// its [`crate::groups::GroupFlags::PATROL`] (`FUN_141017c30`).
        const PATROL = 1 << 2;
        /// Its motion moves (`FUN_141046600`).
        const MOVING = 1 << 3;
        /// The unit itself moves (its slot 0x5c8), which the idle part reads with the group.
        const MEMBER_MOVING = 1 << 4;
    }
}

impl Surroundings {
    /// The object `id` among [`Surroundings::units`].
    #[must_use]
    pub fn unit(&self, id: impl ObjectKey) -> Option<&UnitSeen> {
        let id = id.key()?;
        self.units.iter().find(|unit_seen| unit_seen.id == id)
    }
}

/// An object a unit AI names, as its attackers' upkeep and `ClosestValidAttacker` read it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnitSeen {
    /// `+8`.
    pub(crate) id: ObjectId,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its owner's id (`+0x138`, `player+0x718`); Gaia is 0.
    pub(crate) owner: PlayerId,
    /// Its master's `combat_level` (`+0x62c`).
    pub(crate) combat_level: CombatLevel,
    /// `+0x640`.
    pub(crate) state: ObjectState,
    /// Its unit AI's current target (`+0x48`); None without a unit AI or a target.
    pub(crate) target: Option<ObjectId>,
    /// The gap between its footprint and the unit's, squared (`FUN_14118f8c0`).
    pub(crate) gap_squared: DistanceSquared,
    /// Its place.
    pub(crate) position: Position,
    /// Its hit points (`+0x278`).
    pub(crate) hp: HitPoints,
    /// Its master's id and type (`+0xf`).
    pub(crate) master: MasterId,
    /// Its type (`+0xf`).
    pub(crate) kind: MasterKind,
    /// An annex's main building (`+0xbb8`).
    pub(crate) main: Option<MainBuildingId>,
    /// The unit's owner counts its owner as its own or an ally (`FUN_141663d90` with the ally
    /// flag, [`crate::unit_ai::task_finder::allied`]).
    pub(crate) allied: bool,
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
    pub(crate) converts: bool,
    /// Its attacks' sum (slot 0x228, `FUN_141084070`: its master's attacks above 0).
    pub(crate) attacks: f32,
    /// What it deals the unit per hit (the unit's slot 0x220 with it, `FUN_141083f20`).
    pub(crate) harms: HitPoints,
    /// What the unit deals it per hit (its slot 0x220 with the unit).
    pub(crate) harmed: HitPoints,
    /// Its hit points (`+0x278`).
    pub(crate) hp: HitPoints,
}

/// An object as a unit AI's target block reads it: its state (`+0x640`) and type (`+0xf`); as
/// the follow order reads it (slot 73): its owner, master's class, place (x, y, z), its master's
/// radii on the ground (`+0x5d8`, `+0x5dc`) and whether the unit's player sees its tile now
/// (`FUN_140c68a00` gives 0xf: the tile's bit of the unified map and the player's mask `+0x2a8`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    /// The object's state.
    pub(crate) state: ObjectState,
    /// Its master's type.
    pub(crate) kind: MasterKind,
    /// Its owner.
    pub(crate) owner: PlayerId,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its place.
    pub(crate) position: Position,
    /// Its master's radii.
    pub(crate) radius: Footprint,
    /// Its tile is visible to the viewer.
    pub(crate) visible: bool,
    /// Its master's `object_flags` (`+0x654`).
    pub(crate) flags: ObjectFlags,
    /// The viewer's test of its tile (`FUN_1416635e0`, [`crate::map::spatial::Spatial::tile_sight`]):
    /// 0xf seen, 0x80 explored, else 0.
    pub(crate) sight: TileSight,
}

/// The game's `force` argument of a unit AI's order, its attack command and its move
/// ([`AiMove::force`]): forced, the order replaces what the AI or the unit does; else each
/// order's own test decides (most fail while the AI has an action value).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    pub(crate) point: Position,
    /// The range within which it arrives.
    pub(crate) range: Distance,
    /// The sixth argument: forced, the move replaces even a `MoveTo` to the same point.
    pub(crate) force: OrderForce,
    /// The seventh: whether the unit walks or runs.
    pub(crate) pace: Pace,
}

/// How an [`AiMove`]'s unit goes (its seventh argument): it walks (1, its master's walking
/// sprite, `+0x748`) or runs (0, its running sprite, `+0x750`, when it has one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pace {
    /// It walks.
    Walk,
    /// It runs.
    Run,
}

impl Unit {
    /// What the unit's AI reads of it and the world as it runs ([`Surroundings`]): the unit,
    /// its master (`master`, its class `class`), its owner `player` (a captured unit's new one,
    /// whose map sees the AI's target), `busy` (an action of its own) and `interruptible` (slot
    /// 0x2d0), through `view` (the owner's side).
    ///
    /// # Errors
    ///
    /// A busy unit of a master the slice does not have, and what the target's block and the
    /// named units do not model.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn surroundings(
        &self,
        actions: &Action,
        step: &Step<'_>,
        view: &OwnerView,
        (master, class): (Option<&Master>, Option<ClassId>),
        (player, busy, interruptible): (PlayerId, bool, bool),
    ) -> GenieResult<Surroundings> {
        Ok(Surroundings {
            position: self.position(),
            hp: self.life.hp,
            speed: step
                .w
                .parts
                .mover_of(self.id)
                .map_or(Speed::ZERO, |m| m.stats.speed),
            map: step.w.spatial.map,
            dt_seconds: step.dt.seconds(),
            flags: {
                let mut flags = SurroundingFlags::empty();
                flags.set(SurroundingFlags::MODE_1, step.w.mode_680.is_one());
                flags.set(SurroundingFlags::BUSY, busy);
                flags.set(SurroundingFlags::INTERRUPTIBLE, interruptible);
                flags.set(
                    SurroundingFlags::TASK_0X99,
                    master.is_some_and(|m| {
                        m.action_part()
                            .tasks
                            .iter()
                            .any(|t| t.action_kind == CART_TASK)
                    }),
                );
                flags.set(SurroundingFlags::GARRISONED, self.life.inside.is_some());
                flags.set(
                    SurroundingFlags::RANGED,
                    master.is_some_and(|m| m.combat_part().missile.master().is_some()),
                );
                flags.set(
                    SurroundingFlags::SCENARIO,
                    step.w.settings.game_mode.scenario(),
                );
                flags.set(
                    SurroundingFlags::TASK_0X69,
                    master.is_some_and(|m| {
                        m.action_part()
                            .tasks
                            .iter()
                            .any(|task| task.action_kind == ActionKind::HEAL)
                    }),
                );
                flags.set(
                    SurroundingFlags::UNDER_ATTACK,
                    self.flags.contains(UnitFlags::UNDER_ATTACK),
                );
                flags.set(
                    SurroundingFlags::ATTACK_LISTED,
                    actions.list.attack().is_some(),
                );
                flags.set(
                    SurroundingFlags::ATTACK_MOVING,
                    step.w
                        .parts
                        .mover_of(self.id)
                        .is_some_and(|m| m.attack_moving),
                );
                flags.set(
                    SurroundingFlags::DISABLE_TOWER_PRIORITY,
                    view.ai_settings
                        .switches
                        .contains(AiSwitches::DISABLE_TOWER_PRIORITY),
                );
                flags.set(
                    SurroundingFlags::GUEST_COMPUTER,
                    self.garrison.iter().any(|&object_id| {
                        step.w.spatial.get(object_id).is_some_and(|placed| {
                            placed.player != player
                                && step
                                    .w
                                    .players
                                    .get(placed.player.index())
                                    .is_some_and(|q| q.kind.is_computer())
                        })
                    }),
                );
                flags
            },
            player,
            class: match class {
                Some(class) => Some(class),
                None if busy => {
                    return Err(GenieError::unsupported(
                        "a busy unit of a master the slice does not have",
                    ));
                }
                None => None,
            },
            combat_level: master.map_or(CombatLevel::NONE, |m| m.combat_level),
            attack_priority: master.map_or(AttackPriority::PRIORITY_0, |m| {
                m.creatable_part().attack_priority
            }),
            size: master.map_or(Footprint::ZERO, |m| m.radius),
            los: master.map_or(Distance::ZERO, |m| m.line_of_sight),
            search_radius: master.map_or(Distance::ZERO, |m| m.action_part().search_radius),
            weapon_range: master.map_or(Distance::ZERO, |m| m.combat_part().weapon_range),
            state: self.life.state,
            kind: master.map_or(MasterKind::of_data(0), |m| m.kind),
            firepower: step.w.parts.combat_of(self.id).firepower,
            capture: self.capture_kind(step),
            master: self.life.master_id,
            // a new owner sees the target with its own map
            target: self.ai_target(step, view.player)?,
            id: self.id,
            anim_speed: step.w.parts.anim_speed_of(self.id),
            action_state: if step.w.parts.unread.contains(self.id) {
                None
            } else {
                Some(
                    actions
                        .top_action()
                        .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                )
            },
            units: self.named_units(
                step,
                changed_view(
                    OthersRef::of(&step.w.players, &step.w.index, step.player)?,
                    view.new_owner,
                ),
                view,
                master,
            )?,
            group: (self.group.id)
                .and_then(|id| step.w.groups.get(id))
                .map(|group| {
                    let mut flags = GroupState::empty();
                    flags.set(
                        GroupState::COMMANDED,
                        group.current_command != GroupCommandKind::NONE,
                    );
                    flags.set(GroupState::IDLE, group.is_idle());
                    flags.set(GroupState::PATROL, group.patrols());
                    flags.set(GroupState::MOVING, group.motion.moving());
                    flags.set(
                        GroupState::MEMBER_MOVING,
                        step.w.parts.mover_of(self.id).is_some_and(Mover::is_moving),
                    );
                    flags
                }),
            top_target: actions.top_target_seen(
                step,
                OthersRef::of(&step.w.players, &step.w.index, step.player)?,
            )?,
            min_range: master.map_or(Distance::ZERO, |m| m.combat_part().min_range),
            moved_at: step
                .w
                .parts
                .mover_of(self.id)
                .map_or_else(|| self.position(), |mover| mover.moved_at)
                .vector(),
            dodge_chance: step
                .w
                .players
                .get(player.index())
                .map_or(Chance::NONE, |q| q.dodge_chance),
            keep_distance_chance: step
                .w
                .players
                .get(player.index())
                .map_or(Chance::NONE, |q| q.keep_distance_chance),
            roles: step.w.model.roles.clone(),
            slots: step.w.model.slots,
            unit_ai_tuning: step.w.model.tuning.unit_ai,
        })
    }
}
