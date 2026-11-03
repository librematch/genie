//! `bestUnitToAttack` itself (`FUN_140c44680`): the object tests, the ranges and the choice.

use crate::map::Distance;
use crate::math::image::best_target::ONE as BEST_TARGET_ONE;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::MasterKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::{ByRelation, UnitRelation};
use crate::unit_ai::best_target::ThingFlags;
use crate::unit_ai::best_target::{
    Ask, AttackCategory, Battlefield, EPSILON, EntryCount, STATISTICS, Searcher, Statistics, Thing,
    category,
};
use crate::unit_ai::{AiAction, OrderKind};
use crate::world::Millis;
use crate::world::WorldTime;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::BestTargetTuning;
use crate::world::slots::CheckedIndex;
use std::mem::take;

/// The gap of a closest gate or wall not found yet.
const NO_GAP: Distance = Distance::new(-1.0);
/// The moving targets past which the current target and the closest moving one answer.
pub(super) const FEW_MOVING: EntryCount = EntryCount::new(5);
/// The retarget entries' times: a moving object's 1 s, another's 2 s.
pub(super) const RETRY_MOVING: Millis = Millis::new(1000);
pub(super) const RETRY_OTHER: Millis = Millis::new(2000);

/// What `bestUnitToAttack` found: the object to attack (none for none) and the retarget entries
/// its failed paths add (`FUN_140c42320`: each target and the world time until which it is
/// ignored), the caller's for the unit AI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    /// The object, if any.
    pub target: Option<ObjectId>,
    /// The retarget entries to add, in order.
    pub retries: Vec<(ObjectId, WorldTime)>,
}

/// The object tests the statistics' main and retry walks share: the world's attack checks
/// ([`may_attack`]); a Gaia unit others take by sight is passed outside the scenarios; its
/// flags and whether the unit shuns it; then the owner and class rules
/// ([`kept_by_owner_and_class`]). The object and its category, or None.
pub(super) fn tested(
    me: &Searcher,
    ask: &Ask,
    id: ObjectId,
    relation: UnitRelation,
    world: &mut dyn Battlefield,
    roles: &Roles,
) -> Result<Option<Thing>, crate::GenieError> {
    let Some(t) = world.thing(id)? else {
        return Ok(None);
    };
    if !may_attack(me, (id, &t), relation, world)? {
        return Ok(None);
    }
    let gaia_taken = !me.scenario && t.owner == PlayerId::GAIA && MasterKind::COMBAT <= t.kind;
    if gaia_taken && t.capture.any() {
        return Ok(None);
    }
    if !t.flags.contains(ThingFlags::TARGETABLE)
        || (!t.flags.contains(ThingFlags::ATTACKABLE) && me.action == Some(AiAction::Attack))
        || world.shuns(id)?
    {
        return Ok(None);
    }
    Ok(kept_by_owner_and_class(me, ask, &t, roles).then_some(t))
}

/// The world's checks of object `id` (`t`, of relation list `relation`), in the game's order: it
/// has a unit AI, is alive, not allied and not inside another; the unit AI's attack check; a
/// combat unit's friends' scan; a neutral object's (list 3) test.
pub(super) fn may_attack(
    me: &Searcher,
    (id, t): (ObjectId, &Thing),
    relation: UnitRelation,
    world: &mut dyn Battlefield,
) -> Result<bool, crate::GenieError> {
    if !t.flags.contains(ThingFlags::HAS_AI)
        || ObjectState::Dying <= t.state
        || t.flags.contains(ThingFlags::ALLIED)
        || t.flags.contains(ThingFlags::INSIDE)
        || !world.can_attack(id)?
    {
        return Ok(false);
    }
    if me.kind == MasterKind::COMBAT && !world.spares_friends(id)? {
        return Ok(false);
    }
    if relation == UnitRelation::Neutral && !world.attacks_neutral(t.class)? {
        return Ok(false);
    }
    Ok(true)
}

/// The owner and class rules of an object `t`: without `any_class` the buildings, walls,
/// towers, gates and packed units are passed over; a computer player's unit passes over Gaia's
/// objects that hold something or that others take by sight, and boats unless it is a boat, of
/// class 52 or of [`MasterRoles::BOAT_HUNTER`].
pub(super) fn kept_by_owner_and_class(me: &Searcher, ask: &Ask, t: &Thing, roles: &Roles) -> bool {
    if !ask.any_class && roles.of_class(t.class).contains(ClassRoles::NOT_ANY) {
        return false;
    }
    let captured = MasterKind::COMBAT <= t.kind && t.capture.any();
    // nothing held (`+0x310` at most 0) and not taken by sight
    let free = t.held <= Amount::ZERO && !captured;
    if me.player_kind.is_computer() && t.owner == PlayerId::GAIA && !free {
        return false;
    }
    let boat_free = roles.of_some_class(me.class).contains(ClassRoles::TOWER)
        || roles
            .of_master(me.master)
            .contains(MasterRoles::BOAT_HUNTER)
        || !roles.of_class(t.class).contains(ClassRoles::BOAT);
    let is_boat = me
        .class
        .is_some_and(|c| roles.of_class(c).contains(ClassRoles::BOAT));
    !(me.player_kind.is_computer() && !boat_free && !is_boat)
}

/// The ranges the statistics' walks test an object at `gap` by: at least the least range,
/// within the weapon range unless the unit moves, within `near` (the main walk's sight, the
/// retry walk's range) unless it may move, within the reference distance when there is one, and
/// within the sight.
pub(super) fn within(me: &Searcher, ask: &Ask, gap: Distance, near: Distance) -> bool {
    me.min_range <= gap
        && (gap <= me.weapon_range || me.speed != Speed::ZERO)
        && (me.can_move || gap <= near)
        && (ask.distance <= Distance::ZERO || gap <= ask.distance)
        && gap <= me.sight
}

/// `UnitAIModule::bestUnitToAttack` (`FUN_140c44680`) for the unit `me`, over what its search
/// (slot 0x50, the caller's) left in the relation lists `lists` (0 Gaia .. 4 enemies). The
/// statistics ([`Statistics`]) take:
///
/// - the current target (the world holds it alive, not allied) when within the unit's range
///   against it or the unit moves; outside an idle check, one within the reference distance
///   that is no building (3) nor farm (49) narrows the search to its category;
/// - the extra objects but the current target, by the attack check, one under the least range
///   at its gap plus the weapon range plus 1 (none for a unit that does not move);
/// - each list from enemies down to Gaia's (a human player's down to 2 in a scenario), each
///   record but the current target and the extra objects: a retarget entry's target waits for
///   the retry walk; the others pass the shared tests ([`tested`]), then the ranges (the least
///   range, the weapon range unless the unit moves, the sight unless it may move, the reference
///   distance, the sight), the category; the closest gate and the closest wall are noted, and
///   count only for a unit that attacks walls (its targets' classes, or its owner's flag
///   outside the defend patrol of a unit that is no siege).
///
/// With walls to attack and nothing but walls, the closest gate answers. Without statistics the
/// retry walk adds its targets the same way (by the unit's range, not the sight, unless it may
/// move); still none, the closest wall answers (a unit that attacks walls) or none. Then the scores
/// ([`Statistics::score`]), the ranks (a fixed entry but category 6 by its weapon range: 3 above
/// the epsilon, else 2, a farm 1; the others 4 for a unit of priority 1 or of 0 but
/// [`MasterRoles::MOVING_RULE_EXEMPT`], else 0) and the sort. Fewer than 5 moving entries and a
/// current target that moves (or nothing moves at all), outside an idle check, when it engages or
/// nothing is armed: the current target, by its path (without path checks at once). Then the first
/// three entries by the quick path (a moving one, or with nothing moving or a unit of another rank
/// rule); more than 5 moving entries outside an idle check: the closest moving one by its path; in
/// an attack work state a current target on top; without path checks the top; else the first three
/// by their paths, each one that fails ignored for 1 s (moving) or 2 s. Last the closest wall (a
/// unit that attacks walls) or none.
///
/// # Errors
///
/// What the world's reads and tests do not model.
pub fn best_unit_to_attack(
    me: &Searcher,
    ask: &Ask,
    lists: &ByRelation<Vec<ObjectId>>,
    world: &mut dyn Battlefield,
    roles: &Roles,
    best_target_tuning: &BestTargetTuning,
) -> Result<Choice, crate::GenieError> {
    let current = current_target(me, world)?;
    let walls = attacks_walls(me, roles);
    let mut st = Statistics::default();
    let narrowed = add_current(me, ask, current, &mut st, world, roles, best_target_tuning)?;
    add_extra(me, ask, &mut st, world, roles, best_target_tuning)?;
    let mut walked = Walked::new(narrowed, walls);
    let layer = WalkLayer {
        roles,
        best_target_tuning,
    };
    walked.walk_lists(me, ask, lists, &mut st, world, layer)?;
    if walls && !walked.beyond_walls && walked.gate.0.is_some() {
        return Ok(Choice {
            target: walked.gate.0,
            retries: Vec::new(),
        });
    }
    if st.entries.is_empty() {
        walked.walk_later(me, ask, &mut st, world, layer)?;
        if st.entries.is_empty() {
            return Ok(Choice {
                target: walked.last_resort(),
                retries: Vec::new(),
            });
        }
    }
    let ranked = rank_entries(me, &mut st, world, roles, best_target_tuning)?;
    choose(me, ask, current, &st, ranked, world, walked.last_resort())
}

/// The current target and what the world holds of it, when it holds it alive (state 2 at
/// most) and not allied.
fn current_target(
    me: &Searcher,
    world: &mut dyn Battlefield,
) -> Result<Option<(ObjectId, Thing)>, crate::GenieError> {
    let Some(target) = me.target else {
        return Ok(None);
    };
    Ok(world
        .thing(target)?
        .filter(|t| t.state <= ObjectState::Alive && !t.flags.contains(ThingFlags::ALLIED))
        .map(|t| (target, t)))
}

/// Whether the unit attacks walls: its targets' classes name a wall or a gate, or its owner's
/// flag allows them outside the defend patrol of a unit that is no siege.
fn attacks_walls(me: &Searcher, roles: &Roles) -> bool {
    let defending =
        // Class 13 (siege), whose units keep the walls' owner flag on the attack move order (`0x2dd`).
        matches!(me.order, Some(OrderKind::AttackMove)) && !roles.of_some_class(me.class).contains(ClassRoles::SIEGE);
    me.target_classes.iter().any(|c| {
        roles
            .of_some_class(*c)
            .intersects(ClassRoles::WALL | ClassRoles::GATE)
    }) || (me.walls_allowed && !defending)
}

/// The current target's entry, when within the unit's range or the unit moves; outside an idle
/// check, one within the reference distance that is no building (3) nor farm (49) narrows the
/// search to its category. The category the search keeps to, none (−1) for any.
fn add_current(
    me: &Searcher,
    ask: &Ask,
    current: Option<(ObjectId, Thing)>,
    st: &mut Statistics,
    world: &mut dyn Battlefield,
    roles: &Roles,
    best_target_tuning: &BestTargetTuning,
) -> Result<Option<AttackCategory>, crate::GenieError> {
    let Some((target, t)) =
        current.filter(|(_, t)| t.distance <= me.range || Speed::ZERO < me.speed)
    else {
        return Ok(None);
    };
    let k = category(t.master, t.class, roles);
    st.add(&world.candidate(target, t.distance, k)?, best_target_tuning);
    let narrows = !ask.idle
        && t.distance <= ask.distance
        && !roles
            .of_class(t.class)
            .intersects(ClassRoles::BUILDING | ClassRoles::FARM);
    Ok(if narrows { k } else { None })
}

/// The extra objects' entries but the current target's, by the attack check: one under the
/// least range counts at its gap plus the weapon range plus [`ONE`] (none for a unit that does
/// not move).
pub(super) fn add_extra(
    me: &Searcher,
    ask: &Ask,
    st: &mut Statistics,
    world: &mut dyn Battlefield,
    roles: &Roles,
    best_target_tuning: &BestTargetTuning,
) -> Result<(), crate::GenieError> {
    for &id in &ask.extra {
        if Some(id) == me.target {
            continue;
        }
        let Some(t) = world.thing(id)? else {
            continue;
        };
        if !world.can_attack(id)? {
            continue;
        }
        let mut gap = t.distance;
        if gap < me.min_range {
            if me.speed == Speed::ZERO {
                continue;
            }
            gap = gap + me.weapon_range + Distance::new(ONE);
        }
        st.add(
            &world.candidate(id, gap, category(t.master, t.class, roles))?,
            best_target_tuning,
        );
    }
    Ok(())
}

/// An object of the unit's relation lists: its id and its list.
#[derive(Clone, Copy, Debug)]
struct Listed {
    /// The object.
    id: ObjectId,
    /// Its list.
    relation: UnitRelation,
}

/// What the walks read of the build layer (FD-046-D45, D49): the roles of the classes they
/// meet and the target search's tuning values, one argument of [`Walked::walk_lists`] and
/// [`Walked::walk_later`].
#[derive(Clone, Copy)]
struct WalkLayer<'a> {
    roles: &'a Roles,
    best_target_tuning: &'a BestTargetTuning,
}

/// The relation lists' walks (the main one and the retry walk): the category the search keeps
/// to, whether the unit attacks walls, the retarget entries' targets the main walk leaves for
/// the retry walk, the closest gate and wall (id, gap; none at −1) and whether anything but
/// walls counted.
struct Walked {
    narrowed: Option<AttackCategory>,
    walls: bool,
    later: Vec<(ObjectId, UnitRelation)>,
    gate: (Option<ObjectId>, Distance),
    wall: (Option<ObjectId>, Distance),
    beyond_walls: bool,
}

impl Walked {
    /// Before the walks: no gate, no wall, nothing left for later.
    const fn new(narrowed: Option<AttackCategory>, walls: bool) -> Self {
        Self {
            narrowed,
            walls,
            later: Vec::new(),
            gate: (None, NO_GAP),
            wall: (None, NO_GAP),
            beyond_walls: false,
        }
    }

    /// The main walk: each list from enemies down to Gaia's (a human player's down to 2 in a
    /// scenario) until the statistics are full, each record but the current target and the
    /// extra objects; a retarget entry's target waits for the retry walk, the others count by
    /// their tests within the sight ([`Walked::counted`]).
    fn walk_lists(
        &mut self,
        me: &Searcher,
        ask: &Ask,
        lists: &ByRelation<Vec<ObjectId>>,
        st: &mut Statistics,
        world: &mut dyn Battlefield,
        layer: WalkLayer<'_>,
    ) -> Result<(), crate::GenieError> {
        let WalkLayer {
            roles,
            best_target_tuning,
        } = layer;
        let lowest = if me.scenario && !me.player_kind.is_computer() {
            UnitRelation::Ally
        } else {
            UnitRelation::Gaia
        };
        for (relation, list) in lists.iter().rev() {
            if relation < lowest {
                break;
            }
            for &id in list {
                if STATISTICS <= st.entries.len() {
                    return Ok(());
                }
                if Some(id) == me.target || ask.extra.contains(&id) {
                    continue;
                }
                if me.retries.contains(&id) {
                    self.later.push((id, relation));
                    continue;
                }
                let listed = Listed { id, relation };
                let Some((t, gap, k)) = self.counted(me, ask, listed, me.sight, world, roles)?
                else {
                    continue;
                };
                {
                    let held = roles.of_class(t.class);
                    if held.contains(ClassRoles::GATE) {
                        if self.gate.1 < Distance::ZERO || gap < self.gate.1 {
                            self.gate = (Some(id), gap);
                        }
                        if !self.walls {
                            continue;
                        }
                    } else if held.contains(ClassRoles::WALL) {
                        if !self.note_wall(id, gap) {
                            continue;
                        }
                    } else {
                        self.beyond_walls = true;
                    }
                }
                st.add(&world.candidate(id, gap, k)?, best_target_tuning);
            }
        }
        Ok(())
    }

    /// The retry walk over the retarget entries' targets the main walk left: the same tests
    /// within the unit's range instead of the sight; a gate counts only for a unit that attacks
    /// walls, and notes nothing.
    fn walk_later(
        &mut self,
        me: &Searcher,
        ask: &Ask,
        st: &mut Statistics,
        world: &mut dyn Battlefield,
        layer: WalkLayer<'_>,
    ) -> Result<(), crate::GenieError> {
        let WalkLayer {
            roles,
            best_target_tuning,
        } = layer;
        for (id, relation) in take(&mut self.later) {
            let listed = Listed { id, relation };
            let Some((t, gap, k)) = self.counted(me, ask, listed, me.range, world, roles)? else {
                continue;
            };
            let counts = {
                let held = roles.of_class(t.class);
                if held.contains(ClassRoles::GATE) {
                    self.walls
                } else if held.contains(ClassRoles::WALL) {
                    self.note_wall(id, gap)
                } else {
                    true
                }
            };
            if counts {
                st.add(&world.candidate(id, gap, k)?, best_target_tuning);
            }
        }
        Ok(())
    }

    /// The object `id` of list `relation`, its gap and its category when it passes the shared tests
    /// ([`tested`]), the ranges ([`within`], `near` the walk's) and the narrowed category.
    fn counted(
        &self,
        me: &Searcher,
        ask: &Ask,
        Listed { id, relation }: Listed,
        near: Distance,
        world: &mut dyn Battlefield,
        roles: &Roles,
    ) -> Result<Option<(Thing, Distance, Option<AttackCategory>)>, crate::GenieError> {
        let Some(t) = tested(me, ask, id, relation, world, roles)? else {
            return Ok(None);
        };
        let gap = t.distance;
        if !within(me, ask, gap, near) {
            return Ok(None);
        }
        let k = category(t.master, t.class, roles);
        if self.narrowed.is_some_and(|n| k != Some(n)) {
            return Ok(None);
        }
        Ok(Some((t, gap, k)))
    }

    /// A wall `id` at `gap`: the closest wall when it is the first or nearer; whether it counts
    /// (only for a unit that attacks walls).
    fn note_wall(&mut self, id: ObjectId, gap: Distance) -> bool {
        if self.wall.1 < Distance::ZERO || gap < self.wall.1 {
            self.wall = (Some(id), gap);
        }
        self.walls
    }

    /// The answer without a target: the closest wall for a unit that attacks walls, else none.
    const fn last_resort(&self) -> Option<ObjectId> {
        if self.walls { self.wall.0 } else { None }
    }
}

/// The scores ([`Statistics::score`]), the ranks and the sort: a fixed entry but category 6 by
/// its weapon range (3 above the epsilon, else 2, a farm 1); the others 4 for a unit of
/// priority 1 or of 0 but [`MasterRoles::MOVING_RULE_EXEMPT`], else 0. Whether the unit ranks so.
fn rank_entries(
    me: &Searcher,
    st: &mut Statistics,
    world: &mut dyn Battlefield,
    roles: &Roles,
    best_target_tuning: &BestTargetTuning,
) -> Result<bool, crate::GenieError> {
    st.score(
        &me.weights,
        me.targeting,
        &mut |id| world.armored(id),
        best_target_tuning,
    )?;
    let ranked = me.attack_priority == AttackPriority::PRIORITY_1
        || (me.attack_priority == AttackPriority::PRIORITY_0
            && !roles
                .of_master(me.master)
                .contains(MasterRoles::MOVING_RULE_EXEMPT));
    st.rank(
        &mut |e| {
            if !e.fixed || e.category == Some(AttackCategory::SIEGE) {
                return Ok(i32::from(ranked) << 2);
            }
            if EPSILON < (world.weapon_range(e.id)?).get() {
                return Ok(3);
            }
            let farm = world
                .thing(e.id)?
                .is_some_and(|t| roles.of_class(t.class).contains(ClassRoles::FARM));
            Ok(if farm { 1 } else { 2 })
        },
        best_target_tuning,
    )?;
    st.sort()?;
    Ok(ranked)
}

/// The choice over the sorted entries: the current target when it holds
/// ([`current_holds`]); the first three by the quick path (a moving one, or with nothing
/// moving or a unit of another rank rule); more than 5 moving entries outside an idle check:
/// the closest moving one by its path; in an attack work state a current target on top;
/// without path checks the top; else the first three by their paths, each one that fails
/// ignored for 1 s (moving) or 2 s; last `last_resort`.
fn choose(
    me: &Searcher,
    ask: &Ask,
    current: Option<(ObjectId, Thing)>,
    st: &Statistics,
    ranked: bool,
    world: &mut dyn Battlefield,
    last_resort: Option<ObjectId>,
) -> Result<Choice, crate::GenieError> {
    let range = me.range;
    let mut choice = Choice::default();
    choice.target = if let Some(target) = current_holds(me, ask, current, st, world)? {
        Some(target)
    } else if let Some(id) = quick_pick(st, ranked, range, world)? {
        Some(id)
    } else {
        let mut tried = None;
        if !ask.idle && FEW_MOVING < st.moving {
            // none at the largest float: a gone target's path fails (slot 0x3d0)
            let closest = st.closest_moving.1;
            if ask.paths == 0 || closest.map_or(Ok(false), |c| world.path(c, range))? {
                choice.target = closest;
                return Ok(choice);
            }
            tried = closest;
        }
        let top = st.entries.checked(*st.order.checked(0)?)?.id;
        // the game's two branches: the unit's own target when it is the top, else the top
        // without paths; either is the top
        if (ask.work_state && Some(top) == me.target) || ask.paths == 0 {
            Some(top)
        } else {
            path_pick(me, st, tried, world, &mut choice.retries)?.or(last_resort)
        }
    };
    Ok(choice)
}

/// The current target when it answers: fewer than 5 moving entries and a current target that
/// moves (or nothing moves at all), outside an idle check, when it engages or nothing is armed,
/// by its path (without path checks at once).
fn current_holds(
    me: &Searcher,
    ask: &Ask,
    current: Option<(ObjectId, Thing)>,
    st: &Statistics,
    world: &mut dyn Battlefield,
) -> Result<Option<ObjectId>, crate::GenieError> {
    let Some((target, t)) = current else {
        return Ok(None);
    };
    let holds = st.moving < FEW_MOVING
        && ((t.speed < Speed::new(EPSILON) && st.moving == EntryCount::ZERO)
            || Speed::ZERO < t.speed)
        && !ask.idle
        && (world.engages(target)? || st.armed < EntryCount::new(1))
        && (ask.paths == 0 || world.path(target, me.range)?);
    Ok(holds.then_some(target))
}

/// The first of the first three entries with a quick path: a moving one, or any with nothing
/// moving or for a unit of another rank rule.
fn quick_pick(
    st: &Statistics,
    ranked: bool,
    range: Distance,
    world: &mut dyn Battlefield,
) -> Result<Option<ObjectId>, crate::GenieError> {
    for &i in st.order.iter().take(3) {
        let e = *st.entries.checked(i)?;
        if (e.moving || st.moving < EntryCount::new(1) || !ranked)
            && world.quick_path(e.id, range)?
        {
            return Ok(Some(e.id));
        }
    }
    Ok(None)
}

/// The first of the first three entries but `tried` with a path; each one before it that
/// fails is ignored for 1 s (moving) or 2 s from now (`retries`, in order).
fn path_pick(
    me: &Searcher,
    st: &Statistics,
    tried: Option<ObjectId>,
    world: &mut dyn Battlefield,
    retries: &mut Vec<(ObjectId, WorldTime)>,
) -> Result<Option<ObjectId>, crate::GenieError> {
    for &i in st.order.iter().take(3) {
        let e = *st.entries.checked(i)?;
        if Some(e.id) != tried && world.path(e.id, me.range)? {
            return Ok(Some(e.id));
        }
        let wait = if e.moving { RETRY_MOVING } else { RETRY_OTHER };
        retries.push((e.id, me.now.after(wait)));
    }
    Ok(None)
}

/// 1 (`DAT_14330eb8c`): an extra object under the least range counts this much beyond the
/// weapon range.
pub(super) const ONE: f32 = BEST_TARGET_ONE;
