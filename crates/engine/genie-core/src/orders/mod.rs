//! The orders a command gives its units: INTERACT and its splitter into loose units and groups,
//! the moves, patrols and follows, the work, gather, hunt and build orders, the drop-offs, the
//! town bell and the Empire Wars start.
//!
//! This file holds INTERACT's order (`0x140e68b90`) to its units without a group, each at once or
//! through its unit AI's queue, several of them at their offsets around the order's point, and the
//! work command it gives (object slot 37, `FUN_14106ff70`, through slot 0x128 `FUN_141211b80` and
//! slot 0x7e8 `FUN_141227190`): the unit takes the master and the task the task finder picks for
//! the target ([`crate::unit_ai::task_finder`]), and the task's action replaces its action list.

use crate::GenieError;
use crate::combat::damage::ArmorPair;
use crate::groups::GroupId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::WorkRate;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::{Master, player_master};
use crate::objects::unit::Unit;
use crate::pathing::tile_path::Found;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::tracker::ListKey;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::TaskTarget;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;

mod actions;
mod attack;
mod commands;
mod drop_site;
mod follow;
mod form;
mod gather;
mod interact;
mod moves;
mod patrol;
mod points;
mod split;
pub mod start_work;
mod tasks;
pub mod town_bell;
mod unit_order;
mod work;

#[cfg(test)]
use self::actions::{SwitchWorld, switch_master};
use self::points::{centroid, follow_axis, follow_spread, grid_point, offset_point};
#[cfg(test)]
use crate::gathering::hunt::HUNT;

/// INTERACT's fields past its units (`+0x4`..`+0x16`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interact {
    /// `t_id` (`+0x4`): the target object, none for a point.
    pub target: Option<ObjectId>,
    /// `x`, `y` (`+0x8`, `+0xc`).
    pub point: Point,
    /// `+0x12`: the work command's carried-resource test.
    pub carry_test: bool,
    /// `+0x13`: the order is queued.
    pub queued: bool,
    /// `+0x14`: a unit with a unit AI takes the order at once.
    pub direct: bool,
    /// `+0x15`: each unit's last human order time is set (`FUN_140e5e780`).
    pub human: bool,
    /// `+0x16`: the work command's task filter, -1 for any.
    pub filter: Option<ActionType>,
}

/// MOVE's fields past its units (`+0x4`..`+0x15`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveOrder {
    /// `t_id` (`+0x4`): an object to follow, none for the point.
    pub target: Option<ObjectId>,
    /// `x`, `y` (`+0x8`, `+0xc`).
    pub point: Point,
    /// `+0x13`: queued; `+0x14`: a unit with a unit AI takes it at once; `+0x15`: the last human
    /// order time is set.
    pub queued: bool,
    /// `+0x14`: a unit with a unit AI takes it at once.
    pub direct: bool,
    /// `+0x15`: the last human order time is set.
    pub human: bool,
    /// `+0x12` (`cade_api`'s `controlHeld`): the splitter makes each unit it may group a group of
    /// its own ([`World::split`]).
    pub control: bool,
}

/// What a unit's slot 0x7e8 (`FUN_141227190`) is given through slot 0x128: the target, the point
/// (x, y, z), the work command's carried-resource test and task filter, and the unit's `+0xa14`
/// while it runs (`notified`: INTERACT passes 1, the unit AI's queued order on a target 0): only
/// with it does the new action's notify clear the unit AI's notifies (`FUN_140c64e50`, for a unit
/// of a type above 69).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Work {
    pub target: Option<ObjectId>,
    pub point: Position,
    pub carry_test: bool,
    pub filter: Option<ActionType>,
    pub notified: bool,
}

/// [`World::order_work`]'s Pack or Unpack (each through slot 0x808): against a target out of
/// the unit's `reach` ([`work_reach`]) class 54 packs (`FUN_140ad4ff0`), within it class 51
/// unpacks (`FUN_140ae58a0`); without a reach, a computer's class 54 packs when the order has
/// no target the world holds (`targetless`).
fn packs(class: ClassId, reach: Option<(bool, bool)>, targetless: bool, computer: bool) -> bool {
    match reach {
        // Masters and classes the order handler and slot 0x7e8 treat apart (decoded; the game stores a
        // class as its value − 0xeac, a master id as its id + 0x609c).
        None => targetless && class == class::CLASS_54 && computer,
        Some((within, _)) => {
            (class == class::CLASS_54 && !within) || (class == class::CLASS_51 && within)
        }
    }
}
/// A hunt sends at most this many animals of a herd to its drop site (`FUN_140ac4c60`'s list,
/// `DAT_143e55e20`).
const HERD_MOST: usize = 16;
/// The groups one command lists (`DAT_143f152b8`, below `0x3c`).
const LISTED: usize = 60;
/// The range of GUARD's defend order (`DAT_142eb3634`, [`World::guard`]).
const GUARD_RANGE: f32 = crate::math::image::order::GUARD_RANGE;
/// The task filter of garrisoning (3).
pub(crate) const GARRISON: ActionType = ActionType::named(3);
/// A task's action type of attacking (7), whose factory makes an Attack (type 9).
const ATTACK_TASK: ActionType = ActionType::named(7);

/// Where an object is: its player and its index in the live or the sleeping list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    Live(PlayerId, ListIndex),
    Sleeping(PlayerId, ListIndex),
}

impl Place {
    /// The object's player (`+0x138`'s number).
    pub(crate) const fn player(self) -> PlayerId {
        match self {
            Self::Live(p, _) | Self::Sleeping(p, _) => p,
        }
    }
}

/// What the order splitter reads of a unit: its master's type, formation category (not 0),
/// object flags' bit 1, its top speed (slot 0x208), its place and where it last moved to (slot
/// 0x520's ground part), its state, and for the path between two units its master's radius on
/// x (`+0x5d8`) and its terrain restriction.
#[derive(Clone, Copy, Debug)]
struct Splittable {
    id: ObjectId,
    group: Option<GroupId>,
    kind: MasterKind,
    grouped: bool,
    turns: bool,
    speed: Speed,
    position: Position,
    moved_at: Point,
    state: ObjectState,
    radius: Distance,
    restriction: Option<crate::map::RestrictionId>,
}

/// What the order splitter makes of an order's units (`FUN_140fd76d0`'s result at
/// `DAT_143f11960`): the units that move alone, in the order the splitter leaves them, its
/// groups, each its first unit first, and the units a path did not join.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Split {
    /// The owner of the list's first unit (`DAT_143f11960`), whose groups a new group of the
    /// split joins; None for a list whose first unit is gone (the game's −1).
    pub(crate) owner: Option<PlayerId>,
    pub(crate) loose: Vec<ObjectId>,
    pub(crate) groups: Vec<Vec<ObjectId>>,
    /// The units a path from them did not join (`+0x3a48`, a `std::unordered_set` the splitter
    /// fills through `FUN_140a3e380` and the kept-group order `FUN_140fd9f40` reads), as a set:
    /// its bucket order is not held.
    pub(crate) far: std::collections::BTreeSet<ObjectId>,
}

impl Split {
    /// The splitter keeps a group while it holds fewer than 60 and the group has at most 60
    /// units (`+0x20` below `0x3c`, the count below `0x3d`); else the group and its units are
    /// dropped.
    fn keep(&mut self, units: Vec<ObjectId>) {
        if self.groups.len() < 60 && units.len() <= 60 {
            self.groups.push(units);
        }
    }
}

/// What FOLLOW reads of its target (`FUN_14161c920`'s `param_3`): its id, its place on the
/// ground and, for a master with a footprint (`+0x5d8` or `+0x5dc` above 0), its class.
#[derive(Clone, Copy, Debug)]
struct Followed {
    id: ObjectId,
    place: Point,
    class: Option<ClassId>,
}

/// The splitter's path from one unit to another (`FUN_140f7cb30` -> `FUN_140f7c530`): the
/// planner's result and the path's length; None without a planner for the restriction.
type SplitPath<'a> =
    dyn FnMut(&Splittable, &Splittable) -> Result<Option<(Found, Distance)>, GenieError> + 'a;

/// The drop-site table's search starts beyond this squared distance (`FUN_141658e20`,
/// `DAT_142f89ac4`).
const TABLE_DROP_FAR: f32 = crate::math::image::order::TABLE_DROP_FAR;
/// The loads whose drop-off works with the hunt's task filter (`FUN_14161e370`'s list: meat 15,
/// wood 1, food 0).
const HUNTED_LOADS: [AttributeId; 3] = [attribute::MEAT, attribute::WOOD, attribute::FOOD];
/// The action's point on neither axis (`DAT_14330ec20`).
const NO_POINT: f32 = crate::math::image::garrison::NO_POINT;

/// A hunt command on an object the world does not hold.
pub(crate) const HUNT_TARGET_GONE: GenieError =
    GenieError::unsupported("a hunt command on an object the world does not hold");
/// A hunt command whose unit has no task for its target.
pub(crate) const NO_HUNT_TASK: GenieError =
    GenieError::unsupported("a hunt command without a task");

/// The splitter groups units of a type above this (`0x45`).
const MAX_UNGROUPED: MasterKind = MasterKind::of_data(69);
/// Units whose speeds differ by more than this do not join where the player breaks formations by
/// speed (`FUN_140fd76d0`, `DAT_143cc2918`).
const SPLIT_SPEED_GAP: f32 = crate::math::image::order::SPLIT_SPEED_GAP;
/// Units nearer than this are measured by a path (`FUN_140fd76d0`, `DAT_142e7f38c`) while
/// `DAT_1439e60e3` (1 at run time) holds.
const SPLIT_PATH_REACH: f32 = crate::math::image::order::SPLIT_PATH_REACH;
const SPLIT_BY_PATH: bool = true;

/// `FUN_140fd76d0`'s test whether `b` joins `a`: both above type 69, both of formation category
/// 0 or neither, the same object flags' bit 1, speeds within [`SPLIT_SPEED_GAP`] where the player
/// breaks formations by speed (`COMISS`, `JA`: a NaN gap is within), then (`member`: for the
/// chosen unit's group, also `b` in a state of 2 or more) within the player's influence distance
/// on the ground (between where each last moved to, slot 0x520: `FUN_14087e560` with the height
/// zeroed; `COMISS`, `JA`: a NaN distance is within). Below [`SPLIT_PATH_REACH`] (`COMISS`, `JBE`:
/// a NaN distance is not) the game measures a path instead, when the planner of `b`'s
/// restriction exists: from `b`'s place to `a`'s (`FUN_140f7c530` with `a`'s radius on x as the
/// radii and `b`'s as the range, no tile search), which joins `b` when it reaches the goal
/// (result 1) no longer than the influence distance (a NaN length is not longer); else `b` joins
/// the splitter's far units (`FUN_140a3e380`).
fn splits_together(
    a: &Splittable,
    b: &Splittable,
    f: crate::world::Formations,
    member: bool,
    path: &mut SplitPath<'_>,
    far: &mut std::collections::BTreeSet<ObjectId>,
) -> Result<bool, GenieError> {
    if a.kind <= MAX_UNGROUPED || b.kind <= MAX_UNGROUPED {
        return Ok(false);
    }
    if a.grouped != b.grouped || a.turns != b.turns {
        return Ok(false);
    }
    if f.by_speed && SPLIT_SPEED_GAP < (a.speed - b.speed).get().abs() {
        return Ok(false);
    }
    if member && b.state < ObjectState::Alive {
        return Ok(false);
    }
    let (dx, dy) = (
        a.moved_at.x() - b.moved_at.x(),
        a.moved_at.y() - b.moved_at.y(),
    );
    let d = crate::math::fmath::sqrt(dx * dx + 0.0 + dy * dy);
    if SPLIT_BY_PATH
        && d < SPLIT_PATH_REACH
        && let Some((found, length)) = path(a, b)?
    {
        let joins = found == Found::Goal
            && crate::math::fmath::at_most_or_unordered(length.get(), f.influence.get());
        if !joins {
            let _ = far.insert(b.id);
        }
        return Ok(joins);
    }
    Ok(crate::math::fmath::at_most_or_unordered(
        d,
        f.influence.get(),
    ))
}

/// The trophy master `TRANSFORM_OBJECT` changes ("Trophy None") and the trophies it may take.
const TROPHY: MasterId = MasterId::named(1649);
const TROPHIES: std::ops::RangeInclusive<i16> = 1650..=1653;
/// A patrol's waypoint counts when it is this far from the last one on x or y
/// (`FUN_14161b970`, `DAT_14330eb8c`), or this far the other way (`DAT_14330ec20`).
const WAYPOINT_APART: f32 = crate::math::image::order::WAYPOINT_APART;
const WAYPOINT_APART_LOW: f32 = crate::math::image::order::WAYPOINT_APART_LOW;

/// An object that a caller found or was given, which the world does not hold.
pub(crate) fn object_gone(id: impl ObjectKey) -> GenieError {
    let gone = GenieError::unreachable("an object the caller holds that the world does not hold");
    match id.key() {
        Some(id) => gone.with("object", id),
        None => gone.with("object", "none"),
    }
}

/// A building the caller found or was given, which holds no production queue.
pub(crate) const NO_QUEUE: GenieError =
    GenieError::unreachable("a building without a production queue where one is read");

/// A technology index outside the tree where RESEARCH has queued one.
pub(crate) fn tech_outside(tech: i16) -> GenieError {
    GenieError::invalid_input("a technology outside the tree").with("tech", tech)
}

impl World {
    pub(crate) fn place(&self, id: impl ObjectKey) -> Option<Place> {
        let id = id.key()?;
        PlayerId::each(self.players.len())
            .zip(&self.players)
            .find_map(|(p, player)| {
                if let Some(i) = player.objects.position_of(id) {
                    return Some(Place::Live(p, i));
                }
                let i = player.sleeping.position_of(id)?;
                Some(Place::Sleeping(p, i))
            })
    }

    /// The live slot of object `id` when its own update knows it, `(p, i)`: that slot while it
    /// still holds the object (the game's update works on the object itself, not its id), else
    /// the live slot [`World::place`] finds.
    pub(crate) fn live_slot(
        &self,
        (p, i, id): (PlayerId, ListIndex, ObjectId),
    ) -> Option<(PlayerId, ListIndex)> {
        let held = self
            .players
            .get(p.index())
            .and_then(|player| player.objects.get(i));
        if held.is_some_and(|u| u.id == id) {
            return Some((p, i));
        }
        match self.place(id)? {
            Place::Live(p, i) => Some((p, i)),
            Place::Sleeping(..) => None,
        }
    }

    pub(crate) fn object(&self, id: impl ObjectKey) -> Option<(PlayerId, &Unit)> {
        let id = id.key()?;
        PlayerId::each(self.players.len())
            .zip(&self.players)
            .find_map(|(p, player)| {
                let u = player
                    .objects
                    .by_id(id)
                    .or_else(|| player.sleeping.by_id(id))?;
                Some((p, u))
            })
    }

    /// The object state (`+0x640`) of the object `id`, live or sleeping, None for −1 or an id the
    /// world no longer holds: what a `MoveTo`'s first check reads of its target
    /// ([`crate::movement::move_to::MoveToAction::check_held`]).
    pub(crate) fn object_state(&self, id: impl ObjectKey) -> Option<ObjectState> {
        self.object(id).map(|(_, u)| u.life.state)
    }

    /// The object `id`, live or sleeping, to change (`FUN_1415417a0`'s object).
    pub(crate) fn object_mut(&mut self, id: impl ObjectKey) -> Option<(PlayerId, &mut Unit)> {
        let id = id.key()?;
        PlayerId::each(self.players.len())
            .zip(&mut self.players)
            .find_map(|(p, player)| {
                let crate::world::player::Player {
                    objects, sleeping, ..
                } = player;
                let u = objects.by_id_mut(id).or_else(|| sleeping.by_id_mut(id))?;
                Some((p, u))
            })
    }

    /// Player `player`'s master `master` ([`player_master`]), which the caller's object or
    /// command has.
    ///
    /// # Errors
    ///
    /// A player without a table or a table without the master ([`crate::world::OBJECT_MASTER`],
    /// with the player and the master).
    pub(crate) fn master(&self, player: PlayerId, master: MasterId) -> Result<&Master, GenieError> {
        player_master(&self.masters, player, master).ok_or_else(|| {
            crate::world::OBJECT_MASTER
                .with("player", player)
                .with("master", master)
        })
    }

    /// What the task finder reads of object `id`; an annex stands for its main building (`+0xbb8`),
    /// `steps` times.
    pub(crate) fn task_target(
        &self,
        id: ObjectId,
        steps: usize,
    ) -> Result<TaskTarget<'_>, GenieError> {
        let mut id = id;
        for _ in 0..steps {
            match self.object(id).and_then(|(_, u)| u.annex_of) {
                Some(main) => id = main,
                None => break,
            }
        }
        let (player, u) = self
            .object(id)
            .ok_or(GenieError::unsupported("a target the slice does not hold"))?;
        let master = u.master(&self.masters, player)?;
        Ok(TaskTarget {
            id: u.id,
            master,
            player,
            state: u.life.state,
            held: u.life.held(),
            hp: u.life.hp + u.hp_powerup.room(),
            max_hp: HitPoints::of_master(master.hit_points) + u.hp_powerup.max,
            attackable: u.attackable,
            pasture: u.pasture,
            top_action: u.top_action_kind(),
        })
    }
}

/// `FUN_140c42b60`, the readiness pass of a unit AI's queue: from the last queued order to the
/// first, one not ready becomes ready, and with `drop_ready` one already ready leaves the queue
/// (the orders after it move up). With `drop_ready` (1: after MOVE's and INTERACT's order that
/// is neither queued nor direct) the pass ends in the unit's stop (its slot 0x148, a tail jump at
/// 0x140c42cd4), which the caller runs ([`World::object_stop`]); with 0 (BUILD's and
/// `BUILD_WALL`'s) every order is ready after it.
pub(crate) fn readiness(orders: &mut Vec<crate::unit_ai::QueuedOrder>, drop_ready: bool) {
    // each order is judged alone, so the walk's direction does not matter
    orders.retain_mut(|order| {
        if order.ready {
            !drop_ready
        } else {
            order.ready = true;
            true
        }
    });
}

/// `FUN_1412066b0`, a master change's powerups: the attack (`+0xa24`), reload (`+0xa28`), work
/// rate (`+0xa30`), most hit points (`+0xa3c`), max range (`+0xa40`) and melee and pierce armor
/// (`+0xa48`, `+0xa4c`) powerups go; the hit points powerup (`+0xa38`) stays. The routine also
/// zeroes `+0xa34`, `+0xa50` and `+0x9f8`, empties the powerup map (`+0xa98`, count `+0xaa0`),
/// sets `+0xa2c` to slot 0x1b8 × 0.05 and takes a sight powerup (`+0xad4`) off (slots 0x1d8,
/// 0x1d0): none of them is held.
const fn clear_powerups(unit: &mut Unit) {
    unit.attack_powerup = crate::combat::AttackPowerup::ZERO;
    unit.range_powerup = Distance::ZERO;
    unit.reload_powerup = Seconds::ZERO;
    unit.work_rate_powerup = WorkRate::ZERO;
    unit.hp_powerup.max = HitPoints::ZERO;
    unit.armor_powerups = ArmorPair::ZERO;
}

/// `FUN_140c4da50`: the target of the first order in unit AI `ai`'s queue (`+0x20`, count
/// `+0x18`) that has one (not negative) and that its own unit `id` issued; −1 for none.
/// The adjusted idle timeout and secondary timer `FUN_14161b970` gives the unit AI of the
/// order's `i`th entry: 750 ms and the world's dt (`world+0x7c`) more for each six before it
/// (`(i / 6) as f64 * dt as f64`, `CVTTSD2SI` to 64 bits, the low 32 plus 750; the product
/// stays below 2^42, so the conversion never sees a value it cannot hold).
fn patrol_timeout(i: usize, dt: Millis) -> Millis {
    let stagger = ((i / 6) as f64 * f64::from(dt.get())) as i64;
    Millis::new((stagger as u32).wrapping_add(750))
}

/// What `FUN_14161b970` is told besides its units, its waypoints and its target (none here):
/// its patrol flag (`param_2`), `param_6` (the command's `+0x8`: the unit AIs' saved patrols
/// go), `param_7` (`+0x9`) and `param_8` (`+0xa`). PATROL passes 0 and 1 for the last two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sweep {
    pub(crate) patrol: bool,
    pub(crate) reset: bool,
    pub(crate) queued: bool,
    pub(crate) ignore_pending: bool,
}

/// An attack move through a unit AI's queue (`FUN_14161b970`'s slot 0x30): the call writes only
/// the low byte of the record's shown x and leaves its shown y and ready flag to the stack, which
/// is not modelled.
const ATTACK_MOVE_QUEUED: GenieError =
    GenieError::unsupported("an attack move through a unit AI's queue (slot 0x30)");

/// The waypoints a walker from `from` passes on its way (`FUN_14161b970`'s two filters): each
/// one at least [`WAYPOINT_APART`] from the last one kept on x or on y, either way
/// (`COMISS`/`JNC` on each of the four: a NaN difference is not), the first measured from
/// `from`.
fn waypoints_from(from: Point, waypoints: &[Point]) -> Vec<Point> {
    let mut last = from;
    let mut kept = Vec::with_capacity(waypoints.len());
    for &w in waypoints {
        let (dx, dy) = (w.x() - last.x(), w.y() - last.y());
        if WAYPOINT_APART <= dx
            || dx <= WAYPOINT_APART_LOW
            || WAYPOINT_APART <= dy
            || dy <= WAYPOINT_APART_LOW
        {
            kept.push(w);
            last = w;
        }
    }
    kept
}

fn own_order_target(ai: &crate::unit_ai::UnitAi, id: ObjectId) -> Option<ObjectId> {
    // the issuer compares as the game's integer (a player's number equals that object's id)
    ai.orders
        .iter()
        .filter(|o| o.issuer.is_some_and(|i| i == id))
        .find_map(|o| o.target.filter(|t| !t.is_recyclable()))
}

/// A unit AI's slot 0x1c0 (`FUN_140c64d50`): the stance (`+0x1a9`) takes `state` and the stance's
/// place (`+0x1ac`, `+0x1b0`) the unit's (`place`, with its master's class); `NoAttack` (3) on a
/// unit AI that attacks (its order 700 or its action 600) stops the action (slot 0xb0 with 1,
/// [`crate::unit_ai::UnitAi::stop_with`], whose stopAction goes to the unit's `side`, with the AI
/// stream `rng`) and forgets the target (`FUN_140c5e1e0`).
pub(crate) fn set_stance(
    ai: &mut crate::unit_ai::UnitAi,
    state: crate::unit_ai::UnitStance,
    place: (Point, Option<ClassId>),
    side: &mut dyn crate::unit_ai::UnitSide,
    rng: &mut crate::math::rng::Well512,
) -> Result<(), GenieError> {
    ai.stance = state;
    ai.state_position = place.0;
    if state == crate::unit_ai::UnitStance::NO_ATTACK
        && (ai.current_order == Some(OrderKind::Attack)
            || ai.current_action == Some(ActionKind::Attack))
    {
        ai.stop_with(true, Some(place), side, rng)?;
        ai.remove_target();
    }
    Ok(())
}

/// A building's slot 0x670 (`FUN_1411dba30`): its gate lock (`mGateLocked`, `+0xc7c`, an int its
/// callers set to 0 or 1) takes `locked`; a lock also sets its last move time (`+0x870`,
/// [`crate::movement::Mover::last_move`]) to the world time (`FUN_1415300d0`) plus 1.
pub(crate) fn lock_gate(unit: &mut Unit, locked: bool, now: WorldTime) -> Result<(), GenieError> {
    let gate = unit.gate.as_mut().ok_or(GenieError::unsupported(
        "a GATE of a building without its gate state",
    ))?;
    gate.locked = locked;
    if locked {
        unit.body
            .mover_mut()
            .ok_or(GenieError::unsupported(
                "a locked gate without its moving part",
            ))?
            .last_move = now.after(Millis::new(1));
    }
    Ok(())
}

/// `setDifferentiatorGroups` (`FUN_1410395a0`): group `g`'s differentiator groups (`+0x108`,
/// count `+0x110`) are `list` instead. The routine's false for an allocation that failed cannot
/// come: `operator new` (`FUN_142a8bd2c`) throws rather than return none.
fn set_differentiator_groups(g: &mut crate::groups::UnitGroup, list: &[GroupId]) {
    g.differentiator_groups.clear();
    g.differentiator_groups.extend_from_slice(list);
}

/// `FUN_14118c790`: unit `id` joins the end of a unit's ignore list (`+0x670`, count `+0x678`);
/// the list's capacity (`+0x67c`: 24 at first, then twice the count, the new slots −1) is the
/// `Vec`'s, which no reader sees.
fn ignore_unit(ignore: &mut Vec<ObjectId>, id: ObjectId) {
    ignore.push(id);
}

/// `FUN_1410862c0`: the footprint gap (`FUN_14118f680`) against the unit's ranges: whether it
/// is within the weapon range (slot 0x230; `COMISS`, `JC`: NaN out) and not too close, and
/// whether it is too close, below the least range (slot 0x240; `COMISS`, `SETA`: NaN not).
fn work_reach(gap: Distance, weapon_range: Distance, min_range: Distance) -> (bool, bool) {
    let too_close = gap < min_range;
    let within =
        !crate::math::fmath::below_or_unordered(weapon_range.get(), gap.get()) && !too_close;
    (within, too_close)
}

/// A whole share (`DAT_14330ebcc`), which `FUN_1411859a0` takes hit points against.
const DAMAGE_WHOLE: f32 = crate::math::image::order::DAMAGE_WHOLE;

/// The damage share `FUN_1411859a0` picks damage sprites by: `100 − hp × 100 / hit points`,
/// whole, of hit points at most the master's. An object whose share is its resource's (the
/// classes `FUN_141193590` names; its master's `+0x681` would add any) is not modelled.
fn damage_share(
    hp: HitPoints,
    hit_points: MasterHitPoints,
    class: ClassId,
) -> Result<u8, GenieError> {
    if matches!(
        class,
        class::CLASS_5
            | class::CLASS_7
            | class::CLASS_8
            | class::PREY
            | class::PREDATOR
            | class::TREE
            | class::CLASS_29
            | class::CLASS_31
            | class::CLASS_32
            | class::CLASS_33
            | class::CLASS_38
            | class::CLASS_40
            | class::CLASS_41
            | class::CLASS_48
            | class::LIVESTOCK
            | class::CLASS_61
            | class::CLASS_63
    ) {
        return Err(GenieError::unsupported(
            "the damage share of a resource object (FUN_141193590)",
        ));
    }
    let most = HitPoints::of_master(hit_points).get();
    let hp = hp.get().min(most);
    Ok((DAMAGE_WHOLE - hp * DAMAGE_WHOLE / most) as i32 as u8)
}

/// `FUN_141658e20` over a player's drop-site table (`table`), civ and object tracker at `at` for
/// a load of `held` from a worker of `class`: of the buildings the table gives
/// ([`crate::unit_ai::task_finder::table_drop_buildings`]), the objects of each master's done list in the
/// tracker in turn (`FUN_14104f600`, the master itself the key) that `look` holds (their state
/// and place) in state 2, the nearest by the squared distance of the centres (the first of
/// equals; none from [`TABLE_DROP_FAR`] on).
pub(crate) fn nearest_table_site(
    (table, civ, tracker): (
        &[crate::unit_ai::task_finder::DropSite],
        crate::players::CivId,
        &crate::players::tracker::Tracker,
    ),
    (at, held, class): (Point, Option<AttributeId>, ClassId),
    look: impl Fn(ObjectId) -> Option<(ObjectState, Position)>,
) -> Option<ObjectId> {
    let buildings =
        crate::unit_ai::task_finder::table_drop_buildings(table, held, Some(class), civ);
    let mut best = (None, TABLE_DROP_FAR);
    for m in buildings {
        // a queued unit's place (−1) names no object
        for &id in tracker.list(ListKey::master(m)).iter().flatten() {
            let Some((state, position)) = look(id) else {
                continue;
            };
            if state != ObjectState::Alive {
                continue;
            }
            let (dx, dy) = (at.x() - position.x(), at.y() - position.y());
            let d = dy * dy + dx * dx;
            if d < best.1 {
                best = (Some(id), d);
            }
        }
    }
    best.0
}

#[cfg(test)]
mod tests;
