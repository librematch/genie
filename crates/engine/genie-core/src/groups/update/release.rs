//! A group's units sent at the target one by one (FD-036): `FUN_14103d460` (the log's
//! "`UnlockedAttackWithoutPlan`"), which the attack update ([`World::attack_update`]) runs for the
//! units of a category in reach.

use crate::GenieError;
use crate::groups::GroupId;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SpecificMove;
use crate::groups::group_list::member_gone;
use crate::groups::update::send::Travel;
use crate::objects::{ObjectId, ObjectKey};
use crate::orders::Work;
use crate::world::World;
use crate::world::index::Place;

impl World {
    /// `FUN_14103d460` of group `group` for the units `units` (the ids a category lists) and
    /// the target `target`: nothing without a target the world holds. Each unit with a unit AI
    /// (the world holds it live) that is not idle ([`World::member_idle`]) and already works on
    /// the group's target is left alone. Any other takes the part of the target
    /// ([`World::gate_part`]) as its own: with no queued command and the target not pending
    /// ([`World::target_pending`]) it leaves the group (`removeUnit` with the part); it stops
    /// (slot 0x560, [`crate::movement::Mover::stand`]), its own move in the group is cleared
    /// (path and reason -1), and it takes the work command on the part at the part's place (slot
    /// 0x128: carry test 1, `+0xa14` the group's retargeting byte, the group's certain action as
    /// the filter). A unit on the special list ([`crate::groups::UnitGroup::special_units`])
    /// with a speed above [`EPSILON`] draws its retarget timer from the time to the part at that
    /// speed ([`crate::unit_ai::UnitAi::set_randomized_retarget_timer`]).
    ///
    /// # Errors
    ///
    /// A group, a target or a master the world does not hold; a unit that left the group and
    /// whose move record the game then writes before the array; what the work command, the stop
    /// and the speed do not model.
    pub(super) fn release_units(
        &mut self,
        group: GroupId,
        units: &[ObjectId],
        target: ObjectId,
    ) -> Result<(), GenieError> {
        if self.world_object(target).is_none() {
            return Ok(());
        }
        for &id in units {
            self.release_unit(group, id, target)?;
        }
        Ok(())
    }

    /// [`World::release_units`]' step for the unit `id`.
    fn release_unit(
        &mut self,
        group: GroupId,
        id: ObjectId,
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let Some(Place::Live(..)) = self.place(id) else {
            return Ok(());
        };
        let Some((_, unit)) = self.object(id) else {
            return Ok(());
        };
        if !self.parts.unit_ai.contains(unit.id) {
            return Ok(());
        }
        let top = self.top_target_object(unit);
        let g = self.groups.held(group)?;
        let (retarget, certain) = (
            g.current_target.disable_retargeting,
            g.current_target.certain_action,
        );
        if !self.member_idle(id)? && top.is_some() && top == g.current_target.target.key() {
            return Ok(());
        }
        let part = self.gate_part(id, target)?;
        if self.groups.held(group)?.command_queue.is_empty() && !self.target_pending(group)? {
            let (id, part) = (id, Some(part));
            let _ = self.remove_unit(group, id, part, false)?;
        }
        self.stand_unit(id)?;
        self.clear_own_move(group, id)?;
        let at = self
            .world_object(part)
            .ok_or_else(|| member_gone(part))?
            .position();
        self.give_work(
            id,
            &Work {
                target: Some(part),
                point: at,
                carry_test: true,
                filter: certain,
                notified: retarget != 0,
            },
        )?;
        if self.groups.held(group)?.special_units.contains(&id) {
            self.set_travel_timer(id, part, Travel::Fast)?;
        }
        Ok(())
    }

    /// The unit `id`'s own move in group `group` clears: no path, no reason (-1), the rest 0.
    ///
    /// # Errors
    ///
    /// A unit that is no member: the game writes the record before the array.
    fn clear_own_move(&mut self, group: GroupId, id: ObjectId) -> Result<(), GenieError> {
        let record = self
            .groups
            .held_mut(group)?
            .members
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| {
                GenieError::unreachable("a released unit's move record before the member array")
                    .with("group", group)
                    .with("unit", id)
            })?;
        record.specific_move = SpecificMove {
            path_id: MovePath::NONE,
            reason: MoveReason::NONE,
            ..SpecificMove::default()
        };
        Ok(())
    }
}
