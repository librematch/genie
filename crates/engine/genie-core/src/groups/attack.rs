//! A group's command on an attack task (FD-036): `FUN_14100f7f0`, which `BUnitGroup::command`
//! runs for the tasks 1, 7, 9, `0x68` and `0x7d` on a target, with the member passes it makes
//! (`FUN_14100c6c0`, `FUN_141007ba0`, `FUN_14102bbd0`).

use crate::GenieError;
use crate::combat::Charge;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::command::MoveTo;
use crate::groups::group_list::member_gone;
use crate::groups::stats::weapon_range;
use crate::groups::{GroupTarget, TargetData, UnitGroup, WorldObject};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::math::fmath;
use crate::math::fmath::{BVector, below_or_unordered};
use crate::objects::ObjectId;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The formation categories a group keeps its units by (`FUN_141007ba0`, `FUN_14100c6c0`).
const CATEGORIES: std::ops::RangeInclusive<i32> = 0..=5;
/// What the special units list holds at most for a group of five members or more
/// (`FUN_14100c6c0`); a smaller group's holds one.
const SPECIAL_MOST: usize = 4;
/// What each of the refill's two first-pass lists holds (`auStack_68`, `auStack_58`).
const FIRST_PASS_MOST: usize = 3;
/// The masters of the refill's second first-pass list (`FUN_14100c6c0`).
const LISTED_MASTERS: [MasterId; 4] = [ids::ID_329, ids::ID_330, ids::ID_282, ids::ID_556];
/// A target of one of these classes takes both first-pass lists, the masters' first
/// (`FUN_14100c6c0`).
const BOTH_LISTS_CLASSES: [ClassId; 4] = [
    class::CLASS_24,
    class::CLASS_57,
    class::CLASS_12,
    class::CLASS_47,
];
/// A target of one of these masters takes the class list only (`FUN_14100c6c0`).
const CLASS_LIST_MASTERS: [MasterId; 2] = [ids::ID_6, ids::ID_7];
/// A target of one of these classes takes the group's units by formation category
/// (`FUN_14100c6c0`).
const CATEGORY_LIST_CLASSES: [ClassId; 12] = [
    class::VILLAGER,
    class::MONK,
    class::CLASS_43,
    class::SIEGE,
    class::CLASS_54,
    class::CLASS_51,
    class::KING,
    class::CLASS_35,
    class::TRANSPORT,
    class::TRADE_BOAT,
    class::FISHING_SHIP,
    class::CLASS_61,
];
/// The task whose range adds the player's attribute 280 (`FUN_1415cd0d0` with the filter `0x68`).
const CONVERSION_TASK: ActionType = ActionType::named(0x68);
/// The player attribute a [`CONVERSION_TASK`]'s range adds (`+0x8c0` of the attribute table).
const CONVERSION_RANGE_ATTRIBUTE: usize = 280;
/// What a task range is before a task gives one (`DAT_14330ec20`).
const NO_RANGE: f32 = -1.0;
/// A member of this type or above works at its weapon's range when the filter names no task
/// (`FUN_14102bbd0`: above `0x31`).
const RANGED_MEMBER_KIND: MasterKind = MasterKind::of_data(0x32);

/// What a group's command on an attack task is told besides the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AttackOrder {
    /// The order's seventh byte, which the group keeps in its target (`+0x3dc`,
    /// `mDisableRetargeting`).
    pub(crate) disable_retargeting: bool,
    /// The order's task filter (`+0x16`), `-1` for any, which the group keeps in its target
    /// (`+0x3de`).
    pub(crate) filter: Option<ActionType>,
}

impl UnitGroup {
    /// The group's target data forgets its target, place, radius and retargeting byte (the
    /// resets in `FUN_14100f7f0`); its range and certain action stay.
    pub(in crate::groups) const fn forget_target_data(&mut self) {
        let t = &mut self.current_target;
        t.target = GroupTarget::None;
        t.original_position = BVector::ZERO;
        t.target_radius = Distance::ZERO;
        t.disable_retargeting = 0;
    }
}

impl World {
    /// `FUN_14100f7f0` of group `group` on `target`: the object the command is on stands for its
    /// main building (`FUN_141189b20`). A group already on it, whose members with a target are all
    /// on it, and without [`GroupFlags::JOINED`] only notes the target's place (false). Else the
    /// target's data resets, the range comes from the members' pass ([`World::reach_pass`], with
    /// fewer than two queued commands) or is the group's smallest unit range, the special units and
    /// the categories' attack ranges take the target ([`World::refill_special_units`],
    /// [`World::raise_attack_ranges`]), the target's data is set, and the group moves to the target
    /// ([`World::command_move_to`]: its range, the move not queued, direct, flag 1). True when the
    /// move began: the group is then in command 9, [`GroupFlags::ARRIVED`] off and
    /// [`GroupFlags::ATTACKING`] on.
    ///
    /// # Errors
    ///
    /// A member within reach, a group or a target the world does not hold, and what the move
    /// does not model.
    pub(crate) fn command_attack(
        &mut self,
        group: GroupId,
        target: ObjectId,
        order: AttackOrder,
    ) -> Result<bool, GenieError> {
        let main = self.main_object(target)?;
        let at = self.moved_place_of(main)?;
        let g = self.groups.held(group)?;
        if g.current_target.target == GroupTarget::Object(main)
            && !g.flags.contains(GroupFlags::JOINED)
            && self.members_on_target(g, main)
        {
            self.groups
                .held_mut(group)?
                .current_target
                .original_position = at;
            return Ok(false);
        }
        let g = self.groups.held_mut(group)?;
        g.forget_target_data();
        for m in &mut g.members {
            m.seconds_since_attacking = Seconds::MAX;
        }
        let (queued, smallest) = (g.command_queue.len(), g.smallest_unit_range);
        let mut range = NO_RANGE;
        if queued < 2 {
            self.reach_pass(group, main, order, &mut range)?;
        }
        if below_or_unordered(range, 0.0) {
            range = smallest.get();
        }
        self.refill_special_units(group, main)?;
        self.raise_attack_ranges(group, main)?;
        let target_radius = self.target_radius(main)?;
        self.groups.held_mut(group)?.current_target = TargetData {
            target: GroupTarget::Object(main),
            original_position: at,
            range: Distance::new(range),
            target_radius,
            disable_retargeting: u8::from(order.disable_retargeting),
            certain_action: order.filter,
        };
        let moved = self.command_move_to(
            group,
            Some(main),
            at.ground(),
            MoveTo {
                range: Distance::new(range),
                keep_flag: true,
                direct: true,
                ..MoveTo::default()
            },
        )?;
        let g = self.groups.held_mut(group)?;
        if !moved {
            g.forget_target_data();
            return Ok(false);
        }
        g.current_command = GroupCommandKind::ATTACK;
        g.flags &= !GroupFlags::ARRIVED;
        g.flags |= GroupFlags::ATTACKING;
        Ok(true)
    }

    /// `FUN_141189b20` of `target`: a building's main building (`+0xbb8`) when it has one, else
    /// the object itself.
    fn main_object(&self, target: ObjectId) -> Result<ObjectId, GenieError> {
        let (player, unit) = self.object(target).ok_or_else(|| member_gone(target))?;
        let building = unit.master(&self.masters, player)?.kind == BUILDING;
        Ok(unit.annex_of.filter(|_| building).unwrap_or(target))
    }

    /// Slot 0x520 (`FUN_14106d530`) of object `id`: where it last moved to ([`moved_place`]).
    pub(super) fn moved_place_of(&self, id: ObjectId) -> Result<BVector, GenieError> {
        let (_, u) = self.object(id).ok_or_else(|| member_gone(id))?;
        Ok(moved_place(u))
    }

    /// The target master's two radii as one length (`+0xb57`, `+0xb5b`: `sqrt(x² + y²)`).
    pub(super) fn target_radius(&self, id: ObjectId) -> Result<Distance, GenieError> {
        let (p, u) = self.object(id).ok_or_else(|| member_gone(id))?;
        let r = u.master(&self.masters, p)?.radius;
        Ok(Distance::new(fmath::sqrt(r.x() * r.x() + r.y() * r.y())))
    }

    /// Whether every member of `g` the world holds that has a top action's target (slot 0x190,
    /// `FUN_14106d090`) has `main` for it.
    fn members_on_target(&self, g: &UnitGroup, main: ObjectId) -> bool {
        g.members.iter().all(|m| {
            self.object(m.id).is_none_or(|(_, u)| {
                let top = self.top_target_object(u);
                top.is_none_or(|t| t == main)
            })
        })
    }

    /// `FUN_14102bbd0` of group `group` against `target`: `range` rises to the largest range the
    /// members work at. A member within the range of its weapon against the target and not
    /// nearer than its minimum range is sent at it at once ([`World::send_in_reach`]); the
    /// others are not. Nothing for an owner the world does not hold.
    ///
    /// # Errors
    ///
    /// A member or a master the world does not hold, and what the sending does not model.
    pub(super) fn reach_pass(
        &mut self,
        group: GroupId,
        target: ObjectId,
        order: AttackOrder,
        range: &mut f32,
    ) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let Some(computer) = g
            .owner
            .and_then(|o| self.players.get(o.index()))
            .map(|p| p.computer)
        else {
            return Ok(());
        };
        let ids: Vec<ObjectId> = g.members.iter().map(|m| m.id).collect();
        for id in ids {
            if self.in_reach_of(id, target, order, range)? {
                self.send_in_reach(group, id, target, (order, computer))?;
            }
        }
        Ok(())
    }

    /// The member `id` against `target` in [`World::reach_pass`]: `range` rises to its reach,
    /// and it is within reach when the footprint gap squared is at most the reach squared (its
    /// weapon range against the target when the reach is negative) and at least its least range
    /// squared. A member the world does not hold is not.
    fn in_reach_of(
        &self,
        id: ObjectId,
        target: ObjectId,
        order: AttackOrder,
        range: &mut f32,
    ) -> Result<bool, GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(false);
        };
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let t_master = self.master(t.player(), t.master_id())?;
        let master = u.master(&self.masters, p)?;
        let gap = crate::movement::footprint_gap_squared(
            u.position(),
            master.radius,
            t.position(),
            t_master.radius,
        );
        let mut reach = self.task_reach(p, master.id, order.filter)?;
        if reach < 0.0 && RANGED_MEMBER_KIND <= master.kind && !self.charge_hits(u, master, &t)? {
            reach = weapon_range(master).get();
        }
        let against = self.range_against(WorldObject::Unit(p, u), Some(t))?;
        let reach_squared = if reach < 0.0 {
            against.squared()
        } else {
            DistanceSquared::new(reach * reach)
        };
        if *range < reach {
            *range = reach;
        }
        Ok(gap <= reach_squared && master.min_range.squared() <= gap)
    }

    /// `FUN_1415cd0d0` of the member master `master` of player `player` with the order's task
    /// `filter`: [`NO_RANGE`] for no filter; else the range of the master's task of the filter's
    /// action type (the conversion's adds the player's attribute 280); no such task [`NO_RANGE`];
    /// a master the player does not have 0.
    pub(super) fn task_reach(
        &self,
        player: PlayerId,
        master: MasterId,
        filter: Option<ActionType>,
    ) -> Result<f32, GenieError> {
        let Some(filter) = filter else {
            return Ok(NO_RANGE);
        };
        let Some(master) = self.masters.of(player).and_then(|m| m.get(master)) else {
            return Ok(0.0);
        };
        for task in &master.tasks {
            if filter == CONVERSION_TASK && task.action_type == CONVERSION_TASK {
                let bonus = self
                    .players
                    .player(player)?
                    .attributes
                    .get(CONVERSION_RANGE_ATTRIBUTE)
                    .copied()
                    .ok_or(GenieError::unreachable(
                        "a player's attribute table without the conversion range",
                    ))?;
                return Ok(bonus.get() + task.work_range.get());
            }
            if task.action_type == filter {
                return Ok(task.work_range.get());
            }
        }
        Ok(NO_RANGE)
    }

    /// `FUN_14100c6c0` of group `group` against `target`: the group's special units list
    /// (`+0x488`) refills. A quarter of the members (one at most for fewer than five, else
    /// four) take part: the first pass lists up to three of the members of class 46 and up to
    /// three of the listed masters ([`LISTED_MASTERS`]); a target of one of the
    /// [`BOTH_LISTS_CLASSES`] takes the masters' list then the class list, one of the
    /// [`CLASS_LIST_MASTERS`] the class list only, and one of the [`CATEGORY_LIST_CLASSES`] the
    /// units by formation category (0 to 5) in the group's statistics' order.
    pub(super) fn refill_special_units(
        &mut self,
        group: GroupId,
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let g = self.groups.held(group)?;
        let limit = if g.members.len() < 5 { 1 } else { SPECIAL_MOST };
        let (mut by_class, mut by_master) = (Vec::new(), Vec::new());
        for m in &g.members {
            let Some(obj) = self.world_object(m.id) else {
                continue;
            };
            let master = self.master(obj.player(), obj.master_id())?;
            if by_class.len() < FIRST_PASS_MOST && master.class == class::CLASS_46 {
                by_class.push(m.id);
            }
            if by_master.len() < FIRST_PASS_MOST && LISTED_MASTERS.contains(&master.id) {
                by_master.push(m.id);
            }
        }
        let t_class = self.master(t.player(), t.master_id())?.class;
        let mut list: Vec<ObjectId> = Vec::new();
        if BOTH_LISTS_CLASSES.contains(&t_class) {
            take_into(&mut list, &by_master, limit);
            take_into(&mut list, &by_class, limit);
        } else if CLASS_LIST_MASTERS.contains(&t.master_id()) {
            take_into(&mut list, &by_class, limit);
        } else if CATEGORY_LIST_CLASSES.contains(&t_class) {
            for category in CATEGORIES {
                if limit <= list.len() {
                    break;
                }
                if let Some(stat) = g
                    .unit_types
                    .iter()
                    .find(|s| s.formation_category == category)
                {
                    take_into(&mut list, &stat.units, limit);
                }
            }
        }
        self.groups.held_mut(group)?.special_units = list;
        Ok(())
    }

    /// `FUN_141007ba0` of group `group` against `target`: each formation category's attack range
    /// (`mAttackRange`) rises to the largest range its members have against the target (slot
    /// 0x238, [`World::range_against`]).
    pub(super) fn raise_attack_ranges(
        &mut self,
        group: GroupId,
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let g = self.groups.held(group)?;
        let mut raised = Vec::new();
        for category in CATEGORIES {
            let Some((at, stat)) = g
                .unit_types
                .iter()
                .enumerate()
                .find(|(_, s)| s.formation_category == category)
            else {
                continue;
            };
            if stat.units.is_empty() {
                continue;
            }
            let mut largest = stat.attack_range;
            for &id in &stat.units {
                if let Some(obj) = self.world_object(id) {
                    let range = self.range_against(obj, Some(t))?;
                    if below_or_unordered(largest.get(), range.get()) {
                        largest = range;
                    }
                }
            }
            raised.push((at, largest));
        }
        let g = self.groups.held_mut(group)?;
        for (at, largest) in raised {
            if let Some(stat) = g.unit_types.get_mut(at) {
                stat.attack_range = largest;
            }
        }
        Ok(())
    }

    /// Slot 0x2f8 of the combat object `u` of master `master` against `target`
    /// (`FUN_14120f510`): a charge of type 6 or 7 that works on the target
    /// ([`crate::combat::damage::charge_works`]) and is ready (slot 0x300: at least 1).
    pub(super) fn charge_hits(
        &self,
        u: &Unit,
        master: &crate::objects::masters::Master,
        target: &WorldObject<'_>,
    ) -> Result<bool, GenieError> {
        if !matches!(master.charge_type, 6 | 7) {
            return Ok(false);
        }
        let tm = self.master(target.player(), target.master_id())?;
        let works = crate::combat::damage::charge_works(
            master.charge_target,
            master.charge_type,
            tm.class,
            tm.kind,
        );
        Ok(works && Charge::new(1.0) <= u.charge)
    }
}

/// `ids` join `list` while it holds fewer than `limit` (the refill's `iVar6 <= count` tests).
fn take_into(list: &mut Vec<ObjectId>, ids: &[ObjectId], limit: usize) {
    let room = limit.saturating_sub(list.len());
    list.extend(ids.iter().take(room));
}

/// Slot 0x520 (`FUN_14106d530`): the place the unit last moved to as the game holds it (x,
/// height, y); an object that does not move holds its own.
pub(crate) fn moved_place(u: &Unit) -> BVector {
    u.body
        .mover()
        .map_or_else(|| u.position(), |m| m.moved_at)
        .vector()
}
