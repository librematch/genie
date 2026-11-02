//! The attack command's update (FD-036): `FUN_14103f4e0`, which `BUnitGroup::update` runs while
//! the group's current command is an attack (9).

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::attack::AttackOrder;
use crate::groups::command::MoveTo;
use crate::groups::group_list::member_gone;
use crate::groups::stats::weapon_range;
use crate::groups::update::{ATTACK, NO_COMMAND};
use crate::groups::{GroupTarget, UnitTypeStatistic, WorldObject};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::math::fmath;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::math::image::group::{CHASING_SLACK, CLOSE, CONTACT_SLACK, FARTHEST, RANGED_SLACK};
use crate::movement::Mover;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// The formation categories a group keeps its units by, which the update walks in order.
const CATEGORIES: [FormationCategory; 6] = FormationCategory::EACH;

impl World {
    /// `FUN_14103f4e0` of group `group`: the attack command's update. True when the command is
    /// finished (the caller deletes the group or lets it hibernate, [`World::command_update`]);
    /// `relayout` is the update's relayout flag. Without a first member the world holds, nothing
    /// happens (false). The target is the object the group's target names or, for a target
    /// group, its first member.
    ///
    /// With the target not pending ([`World::target_pending`]) a group with queued commands is
    /// finished; any other loses its path, its command and its target data (range included,
    /// [`World::attack_target_lost`]), and goes back to where the target was first seen, or
    /// finishes when its members stand at the range already. With the target pending, a target
    /// of the first member's own player finishes the command; else [`World::release_categories`]
    /// walks the six categories and [`World::attack_chase`] follows a target that moved.
    ///
    /// # Errors
    ///
    /// What the leaves, the moves and the members' commands do not model; a pending target the
    /// world does not hold.
    pub(super) fn attack_update(
        &mut self,
        group: GroupId,
        relayout: &mut bool,
    ) -> Result<bool, GenieError> {
        let g = self.groups.held(group)?;
        let Some(owner) = g
            .members
            .first()
            .and_then(|m| self.world_object(m.id))
            .map(|o| o.player())
        else {
            return Ok(false);
        };
        let target = self.attacked_object(group);
        if !self.target_pending(group)? {
            return self.attack_target_lost(group, target, relayout);
        }
        let target = target.ok_or_else(|| {
            GenieError::unreachable("a pending attack target the world does not hold")
                .with("group", group)
        })?;
        let target_player = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?
            .player();
        if owner == target_player {
            return Ok(true);
        }
        let finished = self.release_categories(group, target)?;
        if let Some(done) = self.attack_chase(group, target, owner)? {
            return Ok(done);
        }
        self.attack_settle(group, target, finished, relayout)
    }

    /// The object of group `group`'s target (`plVar8`): the object the target names, or the
    /// first member of the group it names (none when that has no members); None for an object
    /// or a group the world does not hold.
    fn attacked_object(&self, group: GroupId) -> Option<ObjectId> {
        let g = self.groups.get(group)?;
        let target = g.current_target.target;
        if let GroupTarget::Object(id) = target {
            return self.world_object(id).map(|o| o.id());
        }
        // any other kind names a group, whose first member is the target
        let first = self.groups.get(target.group()?)?.members.first()?.id;
        self.world_object(first).map(|o| o.id())
    }

    /// The target is not pending (`FUN_14103f4e0`'s first branch): a group with queued
    /// commands is finished (true). Any other forgets the target for good: its walk stops
    /// ([`crate::groups::Motion::stop_walk`], `FUN_14104a050`), its command is none, and its
    /// target's data, range too, resets. A target the world holds has the members' nearest to
    /// it found (a member other than the target, by the footprint gap, `FUN_14118f680`, below
    /// [`FARTHEST`]); the range rises to that member's line of sight; with the gap within the
    /// range the command is finished (true). Else the group moves to the place the target was
    /// first seen at, with that range, forgetting the target, and asks for a relayout (false).
    ///
    /// # Errors
    ///
    /// A master the world lacks, and what the move does not model.
    fn attack_target_lost(
        &mut self,
        group: GroupId,
        target: Option<ObjectId>,
        relayout: &mut bool,
    ) -> Result<bool, GenieError> {
        let g = self.groups.held_mut(group)?;
        if !g.command_queue.is_empty() {
            return Ok(true);
        }
        let to = g.current_target.original_position.ground();
        let mut range = g.current_target.range;
        g.motion.stop_walk()?;
        g.current_command = NO_COMMAND;
        g.forget_target_data();
        g.current_target.range = Distance::ZERO;
        if let Some(target) = target {
            let (nearest, gap) = self.nearest_member(group, target)?;
            if let Some(member) = nearest.and_then(|m| self.world_object(m)) {
                let sight = self
                    .master(member.player(), member.master_id())?
                    .line_of_sight;
                if range <= sight {
                    range = sight;
                }
            }
            if gap <= range.get() {
                return Ok(true);
            }
        }
        let _ = self.command_move_to(
            group,
            None,
            to,
            MoveTo {
                range,
                forget_target: true,
                direct: true,
                ..MoveTo::default()
            },
        )?;
        *relayout = true;
        Ok(false)
    }

    /// The member of group `group`, other than `target`, nearest to it by the footprint gap
    /// (`FUN_14118f680`) below [`FARTHEST`], with that gap ([`FARTHEST`] for none).
    fn nearest_member(
        &self,
        group: GroupId,
        target: ObjectId,
    ) -> Result<(Option<ObjectId>, f32), GenieError> {
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let mut best = (None, FARTHEST);
        for m in &self.groups.held(group)?.members {
            if m.id == target {
                continue;
            }
            let Some(object) = self.world_object(m.id) else {
                continue;
            };
            let gap = self.footprint_gap(object, t)?;
            if gap < Distance::new(best.1) {
                best = (Some(m.id), gap.get());
            }
        }
        Ok(best)
    }

    /// The target is pending, and not the first member's player's: each formation category
    /// (0 to 5) with units, the first of its number, either is released ([`World::release_units`])
    /// or keeps `finished`. With [`GroupFlags::ARRIVED`] the category is released at
    /// once; else it is when one of its units is within its attack range of the target: the
    /// footprint gap less a slack, at most the category's range (negative or unordered: 0). The
    /// slack is [`CONTACT_SLACK`] for a category without a range, [`RANGED_SLACK`] for one
    /// with, and [`CHASING_SLACK`] when the target moves (slot 0x5c8) and the category's range
    /// is above the target's own weapon range (slot 0x230) plus [`RANGED_SLACK`]. A category
    /// without one in range ends `finished` (the returned flag, false). A released category
    /// stops the group's walk, clears [`GroupFlags::ATTACKING`] and sends its units at the target.
    ///
    /// # Errors
    ///
    /// A master the world lacks, and what the release does not model.
    fn release_categories(&mut self, group: GroupId, target: ObjectId) -> Result<bool, GenieError> {
        let mut finished = true;
        for category in CATEGORIES {
            let Some((units, range)) = self
                .groups
                .held(group)?
                .unit_types
                .iter()
                .find(|s| s.formation_category == category)
                .filter(|s| !s.units.is_empty())
                .map(|s: &UnitTypeStatistic| (s.units.clone(), s.attack_range))
            else {
                continue;
            };
            let releases = if self.groups.held(group)?.flags.contains(GroupFlags::ARRIVED) {
                true
            } else {
                let in_reach = self.category_in_reach(&units, range, target)?;
                finished &= in_reach;
                in_reach
            };
            if releases {
                let g = self.groups.held_mut(group)?;
                g.motion.stop_walk()?;
                g.flags &= !GroupFlags::ATTACKING;
                self.release_units(group, &units, target)?;
            }
        }
        Ok(finished)
    }

    /// One of the `units` is within `range` of the target as [`World::release_categories`]
    /// tests: the footprint gap less the slack by the category's `listed` range.
    fn category_in_reach(
        &self,
        units: &[ObjectId],
        listed: Distance,
        target: ObjectId,
    ) -> Result<bool, GenieError> {
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let master = self.master(t.player(), t.master_id())?;
        let moves = match t {
            WorldObject::Unit(_, u, _) => self.parts.mover_of(u.id).is_some_and(Mover::is_moving),
            WorldObject::Ghost(..) => false,
        };
        let outranged =
            moves && below_or_unordered(weapon_range(master).get() + RANGED_SLACK, listed.get());
        let range = if Distance::ZERO <= listed {
            listed
        } else {
            Distance::ZERO
        };
        let slack = if Distance::ZERO < range {
            if outranged {
                CHASING_SLACK
            } else {
                RANGED_SLACK
            }
        } else {
            CONTACT_SLACK
        };
        for &id in units {
            let Some(unit) = self.world_object(id) else {
                continue;
            };
            if self.footprint_gap(unit, t)? - Distance::new(slack) <= range {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The group follows a target that moved (`FUN_14103f4e0`'s object-target tail): a target more
    /// than [`CLOSE`] from where it stood (`+0x3c8`, the ground length of the difference of the
    /// places slot 0x520 gives). One the first member's player cannot see ([`World::sees_object`],
    /// `FUN_141188ea0`) sends the group back to that place, forgetting the target, and the update
    /// ends (`Some(false)`). One it sees is attacked anew ([`World::command_attack`]): a move begun
    /// ends the update (`Some(false)`); else the group moves to the target's place within its
    /// range, is in command 9 with [`GroupFlags::ARRIVED`] off and [`GroupFlags::ATTACKING`] on,
    /// and the update ends (`Some(false)`). None for a group whose target is no object or has not
    /// moved.
    ///
    /// # Errors
    ///
    /// A master the world lacks, a target `command_attack` does not take, and what the moves
    /// do not model.
    fn attack_chase(
        &mut self,
        group: GroupId,
        target: ObjectId,
        owner: PlayerId,
    ) -> Result<Option<bool>, GenieError> {
        let g = self.groups.held(group)?;
        if !matches!(g.current_target.target, GroupTarget::Object(_)) {
            return Ok(None);
        }
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let at = t.moved_vector();
        let from = g.current_target.original_position;
        let d = fmath::sub(at, from);
        if at_most_or_unordered(fmath::sqrt(d.y() * d.y() + d.x() * d.x()), CLOSE) {
            return Ok(None);
        }
        if !self.sees_object(t, owner)? {
            let _ = self.command_move_to(
                group,
                None,
                from.ground(),
                MoveTo {
                    range: Distance::ZERO,
                    forget_target: true,
                    direct: true,
                    ..MoveTo::default()
                },
            )?;
            return Ok(Some(false));
        }
        let order = AttackOrder {
            disable_retargeting: g.current_target.disable_retargeting != 0,
            filter: g.current_target.certain_action,
        };
        let id = Some(target).ok_or_else(|| {
            GenieError::unreachable("an attack target without an id").with("group", group)
        })?;
        if self.command_attack(group, id, order)? {
            return Ok(Some(false));
        }
        let range = self.groups.held(group)?.current_target.range;
        let at = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?
            .moved_vector();
        let _ = self.command_move_to(
            group,
            Some(target),
            at.ground(),
            MoveTo {
                range,
                direct: true,
                ..MoveTo::default()
            },
        )?;
        let g = self.groups.held_mut(group)?;
        g.current_command = ATTACK;
        g.flags &= !GroupFlags::ARRIVED;
        g.flags |= GroupFlags::ATTACKING;
        Ok(Some(false))
    }

    /// The end of the update (`FUN_14103f4e0` after the chase): a moving group asks for a relayout,
    /// a standing one that does not wait for a path has arrived ([`GroupFlags::ARRIVED`]); a group
    /// whose footprint-gap to the target, from the motion's place, is within its range stops its
    /// walk and has arrived. A group arrived and on its way to attack ([`GroupFlags::ATTACKING`])
    /// clears it and sends all its members at the target ([`World::release_units`]) and is not
    /// finished. Any other is finished as the categories left `finished` while a member is held,
    /// and not finished without one.
    ///
    /// # Errors
    ///
    /// A master the world lacks, and what the release does not model.
    fn attack_settle(
        &mut self,
        group: GroupId,
        target: ObjectId,
        finished: bool,
        relayout: &mut bool,
    ) -> Result<bool, GenieError> {
        let t = self
            .world_object(target)
            .ok_or_else(|| member_gone(target))?;
        let radius = self.master(t.player(), t.master_id())?.radius;
        let at = t.position().ground();
        let g = self.groups.held_mut(group)?;
        if g.motion.moving() {
            *relayout = true;
        } else if !g.motion.waits_to_move() {
            g.flags |= GroupFlags::ARRIVED;
        }
        if point_gap(at, radius, g.motion.position[0].ground()) <= g.current_target.range.get() {
            g.motion.stop_walk()?;
            g.flags |= GroupFlags::ARRIVED;
        }
        let flags = g.flags;
        if !flags.contains(GroupFlags::ARRIVED) || !flags.contains(GroupFlags::ATTACKING) {
            return Ok(if self.any_member_held(group) {
                finished
            } else {
                true
            });
        }
        g.flags &= !GroupFlags::ATTACKING;
        let members: Vec<ObjectId> = g.members.iter().map(|m| m.id).collect();
        self.release_units(group, &members, target)?;
        Ok(true)
    }
}

/// `FUN_14118faf0`: the ground length of the gap from a footprint of radii `radius` at `at` to
/// the point `to`, each axis's gap at least 0 (an unordered one stays).
fn point_gap(at: Point, radius: Footprint, to: Point) -> f32 {
    let gap = |d: f32, r: f32| {
        let g = d.abs() - r;
        if g <= 0.0 { 0.0 } else { g }
    };
    let gx = gap(to.x() - at.x(), radius.x());
    let gy = gap(to.y() - at.y(), radius.y());
    fmath::sqrt(gy * gy + gx * gx)
}
