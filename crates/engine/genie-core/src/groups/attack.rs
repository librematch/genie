//! A group's command on an attack task (FD-036): `FUN_14100f7f0`, which `BUnitGroup::command`
//! runs for the tasks 1, 7, 9, `0x68` and `0x7d` on a target, with the member passes it makes
//! (`FUN_14100c6c0`, `FUN_141007ba0`, `FUN_14102bbd0`).

use crate::combat::Charge;
use crate::combat::damage::charge_works;
use crate::construction::MainBuildingId;
use crate::groups::FormationCategory;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::GroupMove;
use crate::groups::MoveFlags;
use crate::groups::command::MoveTo;
use crate::groups::group_list::member_gone;
use crate::groups::stats::weapon_range;
use crate::groups::{GroupTarget, TargetData, UnitGroup, WorldObject};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::math::fmath::sqrt;
use crate::math::fmath::{BVector, below_or_unordered};
use crate::movement::{Mover, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlots;
use crate::{GenieError, GenieResult};

/// The formation categories a group keeps its units by (`FUN_141007ba0`, `FUN_14100c6c0`).
const CATEGORIES: [FormationCategory; 6] = FormationCategory::EACH;
/// What the special units list holds at most for a group of five members or more
/// (`FUN_14100c6c0`); a smaller group's holds one.
const SPECIAL_MOST: usize = 4;
/// What each of the refill's two first-pass lists holds (`auStack_68`, `auStack_58`).
const FIRST_PASS_MOST: usize = 3;

/// The task whose range adds the player's attribute 280 (`FUN_1415cd0d0` with the filter `0x68`).
const CONVERSION_TASK: ActionKind = ActionKind::named(0x68);
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
    pub(crate) filter: Option<ActionKind>,
}

impl UnitGroup {
    /// The group's target data forgets its target, place, radius and retargeting byte (the
    /// resets in `FUN_14100f7f0`); its range and certain action stay.
    pub(in crate::groups) const fn forget_target_data(&mut self) {
        let target_data = &mut self.current_target;
        target_data.target = GroupTarget::None;
        target_data.original_position = BVector::ZERO;
        target_data.target_radius = Distance::ZERO;
        target_data.disable_retargeting = 0;
    }
}

impl World {
    /// `FUN_14100f7f0` of group `group` on `target`: the object the command is on stands for its
    /// main building (`FUN_141189b20`). A group already on it, whose members with a target are all
    /// on it, and without [`GroupFlags::JOINED`] only notes the target's place (not begun). Else
    /// the target's data resets, the range comes from the members' pass ([`World::reach_pass`],
    /// with fewer than two queued commands) or is the group's smallest unit range, the special
    /// units and the categories' attack ranges take the target ([`World::refill_special_units`],
    /// [`World::raise_attack_ranges`]), the target's data is set, and the group moves to the target
    /// ([`World::command_move_to`]: its range, the move not queued, direct, flag 1). Begun when the
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
    ) -> GenieResult<GroupMove> {
        let main = self.main_object(target)?;
        let at = self.moved_place_of(main)?;
        let held = self.groups.held(group)?;
        if held.current_target.target == GroupTarget::Object(main)
            && !held.flags.contains(GroupFlags::JOINED)
            && self.members_on_target(held, main)
        {
            self.groups
                .held_mut(group)?
                .current_target
                .original_position = at;
            return Ok(GroupMove::NotBegun);
        }
        let g = self.groups.held_mut(group)?;
        g.forget_target_data();
        for member in &mut g.members {
            member.seconds_since_attacking = Seconds::MAX;
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
                flags: MoveFlags::KEEP_FLAG | MoveFlags::DIRECT,
            },
        )?;
        let unit_group = self.groups.held_mut(group)?;
        if moved == GroupMove::NotBegun {
            unit_group.forget_target_data();
            return Ok(GroupMove::NotBegun);
        }
        unit_group.current_command = GroupCommandKind::ATTACK;
        unit_group.flags &= !GroupFlags::ARRIVED;
        unit_group.flags |= GroupFlags::ATTACKING;
        Ok(GroupMove::Begun)
    }

    /// `FUN_141189b20` of `target`: a building's main building (`+0xbb8`) when it has one, else
    /// the object itself.
    fn main_object(&self, target: ObjectId) -> GenieResult<ObjectId> {
        let (player, unit) = self.object(target).ok_or_else(|| member_gone(target))?;
        let building = unit.master(self.master_tables(), player)?.kind == MasterKind::BUILDING;
        Ok(self
            .parts
            .building_of(unit.id)
            .annex_of
            .filter(|_| building)
            .map_or(target, MainBuildingId::id))
    }

    /// Slot 0x520 (`FUN_14106d530`) of object `id`: where it last moved to ([`moved_place`]).
    pub(super) fn moved_place_of(&self, id: ObjectId) -> GenieResult<BVector> {
        let (_, unit) = self.object(id).ok_or_else(|| member_gone(id))?;
        Ok(moved_place(unit, self.parts.mover_of(id)))
    }

    /// The target master's two radii as one length (`+0xb57`, `+0xb5b`: `sqrt(x² + y²)`).
    pub(super) fn target_radius(&self, id: ObjectId) -> GenieResult<Distance> {
        let (player_id, unit) = self.object(id).ok_or_else(|| member_gone(id))?;
        let footprint = unit.master(self.master_tables(), player_id)?.radius;
        Ok(Distance::new(sqrt(
            footprint.x() * footprint.x() + footprint.y() * footprint.y(),
        )))
    }

    /// Whether every member of `group` the world holds that has a top action's target (slot 0x190,
    /// `FUN_14106d090`) has `main` for it.
    fn members_on_target(&self, group: &UnitGroup, main: ObjectId) -> bool {
        group.members.iter().all(|member| {
            self.object(member.id).is_none_or(|(_, unit)| {
                let top = self.top_target_object(unit.id);
                top.is_none_or(|object_id| object_id == main)
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
    ) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        let Some(player_kind) = held
            .owner
            .and_then(|player_id| self.players.get(player_id.index()))
            .map(|player| player.kind)
        else {
            return Ok(());
        };
        let ids: Vec<ObjectId> = held.members.iter().map(|member| member.id).collect();
        for id in ids {
            if self.in_reach_of(id, target, order, range)? {
                self.send_in_reach(group, id, target, (order, player_kind))?;
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
    ) -> GenieResult<bool> {
        let Some((player_id, unit)) = self.object(id) else {
            return Ok(false);
        };
        let world_object = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let target_master = self.master(
            world_object.player(),
            world_object.master_id(self.model.slots),
        )?;
        let master = unit.master(self.master_tables(), player_id)?;
        let gap = footprint_gap_squared(
            unit.position(),
            master.radius,
            world_object.position(),
            target_master.radius,
        );
        let mut reach = self.task_reach(player_id, master.id, order.filter)?;
        if reach < 0.0
            && RANGED_MEMBER_KIND <= master.kind
            && !self.charge_hits(unit, master, &world_object)?
        {
            reach = weapon_range(master).get();
        }
        let against = self.range_against(
            WorldObject::Unit(player_id, unit, self.parts.mover_of(unit.id)),
            Some(world_object),
        )?;
        let reach_squared = if reach < 0.0 {
            against.squared()
        } else {
            DistanceSquared::new(reach * reach)
        };
        if *range < reach {
            *range = reach;
        }
        Ok(gap <= reach_squared && master.combat_part().min_range.squared() <= gap)
    }

    /// `FUN_1415cd0d0` of the member master `master` of player `player` with the order's task
    /// `filter`: [`NO_RANGE`] for no filter; else the range of the master's task of the filter's
    /// action type (the conversion's adds the player's attribute 280); no such task [`NO_RANGE`];
    /// a master the player does not have 0.
    pub(super) fn task_reach(
        &self,
        player: PlayerId,
        master: MasterId,
        filter: Option<ActionKind>,
    ) -> GenieResult<f32> {
        let Some(filter) = filter else {
            return Ok(NO_RANGE);
        };
        let Some(master) = self
            .model
            .masters
            .of(player)
            .and_then(|masters| masters.get(master))
        else {
            return Ok(0.0);
        };
        for task in &master.action_part().tasks {
            if filter == CONVERSION_TASK && task.action_kind == CONVERSION_TASK {
                let bonus = self
                    .players
                    .player(player)?
                    .attributes
                    .get(CONVERSION_RANGE_ATTRIBUTE)
                    .copied()
                    .ok_or(GenieError::missing_data(
                        "a player's attribute table without the conversion range",
                    ))?;
                return Ok(bonus.get() + task.work_range.get());
            }
            if task.action_kind == filter {
                return Ok(task.work_range.get());
            }
        }
        Ok(NO_RANGE)
    }

    /// `FUN_14100c6c0` of group `group` against `target`: the group's special units list (`+0x488`)
    /// refills. A quarter of the members (one at most for fewer than five, else four) take part:
    /// the first pass lists up to three of the members of class 46 and up to three of the listed
    /// masters ([`MasterRoles::REFILL_LISTED`](crate::world::layer::MasterRoles::REFILL_LISTED)); a
    /// target of one of the
    /// [`ClassRoles::BOTH_LISTS_TARGET`](crate::world::layer::ClassRoles::BOTH_LISTS_TARGET) takes
    /// the masters' list then the class list, one of the
    /// [`MasterRoles::CLASS_LIST_ONLY`](crate::world::layer::MasterRoles::CLASS_LIST_ONLY) the
    /// class list only, and one of the
    /// [`ClassRoles::CATEGORY_LIST_TARGET`](crate::world::layer::ClassRoles::CATEGORY_LIST_TARGET)
    /// the units by formation category (0 to 5) in the group's statistics' order.
    pub(super) fn refill_special_units(
        &mut self,
        group: GroupId,
        target: ObjectId,
    ) -> GenieResult<()> {
        let world_object = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let held = self.groups.held(group)?;
        let limit = if held.members.len() < 5 {
            1
        } else {
            SPECIAL_MOST
        };
        let (mut by_class, mut by_master) = (Vec::new(), Vec::new());
        for member in &held.members {
            let Some(obj) = self.world_object(member.id) else {
                continue;
            };
            let master = self.master(obj.player(), obj.master_id(self.model.slots))?;
            if by_class.len() < FIRST_PASS_MOST
                && self
                    .model
                    .roles
                    .of_class(master.class)
                    .contains(ClassRoles::PIKEMAN)
            {
                by_class.push(member.id);
            }
            if by_master.len() < FIRST_PASS_MOST
                && self
                    .model
                    .roles
                    .of_master(master.id)
                    .contains(MasterRoles::REFILL_LISTED)
            {
                by_master.push(member.id);
            }
        }
        let target_class = self
            .master(
                world_object.player(),
                world_object.master_id(self.model.slots),
            )?
            .class;
        let mut list: Vec<ObjectId> = Vec::new();
        if self
            .model
            .roles
            .of_class(target_class)
            .contains(ClassRoles::BOTH_LISTS_TARGET)
        {
            take_into(&mut list, &by_master, limit);
            take_into(&mut list, &by_class, limit);
        } else if self
            .model
            .roles
            .of_master(world_object.master_id(self.model.slots))
            .contains(MasterRoles::CLASS_LIST_ONLY)
        {
            take_into(&mut list, &by_class, limit);
        } else if self
            .model
            .roles
            .of_class(target_class)
            .contains(ClassRoles::CATEGORY_LIST_TARGET)
        {
            for category in CATEGORIES {
                if limit <= list.len() {
                    break;
                }
                if let Some(stat) = held
                    .unit_kinds
                    .iter()
                    .find(|unit_kind_statistic| unit_kind_statistic.formation_category == category)
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
    ) -> GenieResult<()> {
        let world_object = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let held = self.groups.held(group)?;
        let mut raised = Vec::new();
        for category in CATEGORIES {
            let Some((at, stat)) =
                held.unit_kinds
                    .iter()
                    .enumerate()
                    .find(|(_, unit_kind_statistic)| {
                        unit_kind_statistic.formation_category == category
                    })
            else {
                continue;
            };
            if stat.units.is_empty() {
                continue;
            }
            let mut largest = stat.attack_range;
            for &id in &stat.units {
                if let Some(obj) = self.world_object(id) {
                    let range = self.range_against(obj, Some(world_object))?;
                    if below_or_unordered(largest.get(), range.get()) {
                        largest = range;
                    }
                }
            }
            raised.push((at, largest));
        }
        let unit_group = self.groups.held_mut(group)?;
        for (at, largest) in raised {
            if let Some(stat) = unit_group.unit_kinds.get_mut(at) {
                stat.attack_range = largest;
            }
        }
        Ok(())
    }

    /// Slot 0x2f8 of the combat object `unit` of master `master` against `target`
    /// (`FUN_14120f510`): a charge of type 6 or 7 that works on the target
    /// ([`crate::combat::damage::charge_works`]) and is ready (slot 0x300: at least 1).
    pub(super) fn charge_hits(
        &self,
        unit: &Unit,
        master: &Master,
        target: &WorldObject<'_>,
    ) -> GenieResult<bool> {
        if !master.creatable_part().charge_kind.ranged() {
            return Ok(false);
        }
        let tm = self.master(target.player(), target.master_id(self.model.slots))?;
        let works = charge_works(
            master.creatable_part().charge_target,
            master.creatable_part().charge_kind,
            tm.class,
            tm.kind,
            &self.model.roles,
        );
        Ok(works && Charge::new(1.0) <= self.parts.combat_of(unit.id).charge)
    }
}

/// `ids` join `list` while it holds fewer than `limit` (the refill's `iVar6 <= count` tests).
fn take_into(list: &mut Vec<ObjectId>, ids: &[ObjectId], limit: usize) {
    let room = limit.saturating_sub(list.len());
    list.extend(ids.iter().take(room));
}

/// Slot 0x520 (`FUN_14106d530`): the place the unit with its `mover` last moved to as the
/// game holds it (x, height, y); an object that does not move holds its own.
pub(crate) fn moved_place(unit: &Unit, mover: Option<&Mover>) -> BVector {
    mover
        .map_or_else(|| unit.position(), |m| m.moved_at)
        .vector()
}
