//! An awake group's update: its lead, what follows the motion, the formation's turn and front,
//! the lines' ranges and the relayout.

use crate::GenieError;
use crate::groups::FormationKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::LeavingWalk;
use crate::groups::MotionFlags;
use crate::groups::SlotState;
use crate::groups::UnitGroup;
use crate::groups::formation::top_path;
use crate::groups::group_list::{NO_FORMATION, member_gone};
use crate::groups::stats::weapon_range;
use crate::groups::update::{ATTACK, DIFFERENTIATORS, NO_COMMAND, OTHER_COMMANDS, TURN_LIMIT};
use crate::map::Distance;
use crate::math::fmath::BVector;
use crate::math::fmath::{angle_between, at_most_or_unordered, scale};
use crate::movement::PathFlags;
use crate::objects::ObjectId;
use crate::world::Millis;
use crate::world::World;

impl World {
    /// `BUnitGroup::update` from `0x14103dd65` on, for an awake group (or a hibernating one whose
    /// motion has [`GroupFlags::RELAYOUT`]). Without queued commands: when the group keeps every
    /// member apart (`+0x468` equal to the member count), its target is not pending ([`World::
    /// target_pending`]), it walks no patrol (the top path's [`PathFlags::LOOPS`]) and
    /// [`GroupFlags::PATROL`] is clear, each kept-apart unit leaves (`removeUnit` with target -1;
    /// one that is no member leaves the list instead, `FUN_140a63bd0`). The statistics' far flag is
    /// no queued command and [`MotionFlags::NEW`] clear. With [`GroupFlags::LEAD_TO_TARGET`] the
    /// lead is the member nearest the target's place ([`World::nearest_to_target`]); a lead found
    /// has the motion update with 0 ([`World::motion_update`]), then without [`MotionFlags::NEW`]
    /// the statistics ([`World::group_statistics`]) and the centroid (`+0x144`) as the motion's
    /// position and new position (`+0x8`, `+0x38`), and bit 18 clears. Then the group goes (false)
    /// without members; without a queued command or a current one but with a formation, it goes
    /// when every slot is in state 3 (or it has none). With a member held and
    /// [`GroupFlags::STATISTICS`] the statistics run; with [`GroupFlags::NO_PATH_UNIT`] the group
    /// goes; the lead is again the member nearest the target, and without a held lead
    /// (`FUN_1410465d0`) the group goes. A held lead re-places the group
    /// ([`World::set_initial_position`]) when [`MotionFlags::NEW`] is set, [`GroupFlags::PLACED`]
    /// is clear and the formation is not a horde (kind 0), or [`GroupFlags::PLACE_AGAIN`] is set
    /// (which then clears); the sub-formations without units go. Then `BMotion`'s update, with a
    /// budget while [`GroupFlags::TURN`] is clear, and the rest ([`World::after_motion`]).
    ///
    /// # Errors
    ///
    /// What the update does not model.
    pub(super) fn awake_update(&mut self, group: GroupId) -> Result<bool, GenieError> {
        let g = self.groups.held(group)?;
        let far = if g.command_queue.is_empty() {
            self.kept_apart_leave(group)?
        } else {
            false
        };
        if self
            .groups
            .held(group)?
            .flags
            .contains(GroupFlags::LEAD_TO_TARGET)
        {
            self.lead_to_target(group, far)?;
        }
        let g = self.groups.held(group)?;
        if awake_group_ends(g) {
            return Ok(false);
        }
        if self.any_member_held(group) && g.flags.contains(GroupFlags::STATISTICS) {
            self.group_statistics(group, far)?;
        }
        if self
            .groups
            .held(group)?
            .flags
            .contains(GroupFlags::NO_PATH_UNIT)
        {
            return Ok(false);
        }
        let lead = self.nearest_to_target(group);
        self.groups.held_mut(group)?.motion.lead = lead;
        let g = self.groups.held(group)?;
        if self.lead_object(&g.motion).is_none() {
            return Ok(false);
        }
        let horde = g
            .current_formation
            .as_ref()
            .is_some_and(|f| f.kind == FormationKind::Horde);
        if (g.motion.flags.contains(MotionFlags::NEW)
            && !g.flags.contains(GroupFlags::PLACED)
            && !horde)
            || g.flags.contains(GroupFlags::PLACE_AGAIN)
        {
            self.set_initial_position(group)?;
            self.groups.held_mut(group)?.flags &= !GroupFlags::PLACE_AGAIN;
        }
        let g = self.groups.held_mut(group)?;
        g.unit_kinds.retain(|s| !s.units.is_empty());
        let budget = !g.flags.contains(GroupFlags::TURN);
        let forward = g.motion.position[1];
        self.motion_update(group, budget)?;
        self.after_motion(group, far, forward)
    }

    /// [`World::awake_update`] without queued commands: when the group keeps every member apart
    /// (`+0x468` equal to the member count), its target is not pending, it walks no patrol and
    /// [`GroupFlags::PATROL`] is clear, each kept-apart unit leaves (`removeUnit` with target −1;
    /// one that is no member leaves the list instead, `FUN_140a63bd0`). The statistics' far flag:
    /// no queued command and [`MotionFlags::NEW`] clear.
    ///
    /// # Errors
    ///
    /// What the target's test and the leaving do not model.
    pub(super) fn kept_apart_leave(&mut self, group: GroupId) -> Result<bool, GenieError> {
        let g = self.groups.held(group)?;
        let patrol =
            top_path(&g.motion.paths).is_some_and(|p| p.path_flags.contains(PathFlags::LOOPS));
        // the dword compare at 0x14103dd78: the list's size (64 bits) against the count
        if g.objects.len() as u32 == g.members.len() as u32
            && !self.target_pending(group)?
            && !patrol
            && !g.flags.contains(GroupFlags::PATROL)
        {
            while let Some(&id) = self.groups.held(group)?.objects.first() {
                if !self.remove_unit(group, id, None, LeavingWalk::Stands)? {
                    let _ = self.groups.held_mut(group)?.objects.remove(0);
                }
            }
        }
        let g = self.groups.held(group)?;
        Ok(g.command_queue.is_empty() && !g.motion.flags.contains(MotionFlags::NEW))
    }

    /// [`World::awake_update`] with [`GroupFlags::LEAD_TO_TARGET`]: the lead is the member nearest
    /// the target's place ([`World::nearest_to_target`]); a lead found has the motion update with
    /// 0, then without [`MotionFlags::NEW`] the statistics (`far`) and the centroid as the motion's
    /// position and new position, and bit 18 clears.
    ///
    /// # Errors
    ///
    /// What the motion update and the statistics do not model.
    pub(super) fn lead_to_target(&mut self, group: GroupId, far: bool) -> Result<(), GenieError> {
        let lead = self.nearest_to_target(group);
        self.groups.held_mut(group)?.motion.lead = lead;
        if lead.is_none() {
            return Ok(());
        }
        self.motion_update(group, false)?;
        if !self
            .groups
            .held(group)?
            .motion
            .flags
            .contains(MotionFlags::NEW)
        {
            self.group_statistics(group, far)?;
            let g = self.groups.held_mut(group)?;
            g.motion.new_position[0] = g.centroid;
            g.motion.position[0] = g.centroid;
        }
        self.groups.held_mut(group)?.flags &= !GroupFlags::LEAD_TO_TARGET;
        Ok(())
    }

    /// `BUnitGroup::update` after `BMotion`'s update (`0x14103e017` on), `forward` the motion's
    /// forward before it. With [`GroupFlags::TURN`] the formation turns
    /// ([`World::turn_formation`]). The member count is kept; without [`GroupFlags::STATISTICS`]
    /// the statistics run (far as before), and bit 16 clears; the group's log (`FUN_141015370`)
    /// holds no state. The differentiator groups' members (`DAT_143f15860`, at most `0x8c`:
    /// [`crate::groups::UnitGroups::differentiator_units`]) are only read by the members' moves
    /// below. Each formation line's attack range is its members' least ([`World::line_ranges`]);
    /// the relayout is chosen ([`World::relayout_choice`]) and the current command updates
    /// ([`World::command_update`], which may delete the group or let it hibernate). Without a
    /// member held there is no relayout; else the formation is laid out anew
    /// (`UpdateDesiredPositions(-1)`). An awake group's motion test (`FUN_141046600`) is unread,
    /// [`GroupFlags::CATCHING_UP`] clears, and a formation with slots moves its members to them
    /// ([`World::members_to_slots`], with the differentiator groups' members listed). Then the
    /// members' own moves ([`World::specific_moves_step`]), the order queue's due orders go
    /// (`FUN_14100d940`, [`World::drop_due_orders`], when it has any), and the idle timer
    /// ([`World::idle_timer_step`]). The debug drawing (`DAT_143f15a9c`) and the owner's computer
    /// test (`FUN_141659cf0`, its answer unread) hold no state. True while the group has members.
    ///
    /// # Errors
    ///
    /// What the steps do not model: a member's way round a record to its slot, a member's own
    /// move's step, an attack's update, among others.
    pub(super) fn after_motion(
        &mut self,
        group: GroupId,
        far: bool,
        forward: BVector,
    ) -> Result<bool, GenieError> {
        if self.groups.held(group)?.flags.contains(GroupFlags::TURN) {
            self.turn_formation(group, forward)?;
        }
        let count = self.groups.held(group)?.members.len();
        if !self
            .groups
            .held(group)?
            .flags
            .contains(GroupFlags::STATISTICS)
        {
            self.group_statistics(group, far)?;
        }
        self.groups.held_mut(group)?.flags &= !GroupFlags::STATISTICS;
        // DAT_143f15860 (0x14103e3a5..0x14103e442), which only the members' moves read
        let g = self.groups.held(group)?;
        let differentiators = self.groups.differentiator_units(g, DIFFERENTIATORS);
        self.line_ranges(group)?;
        let (mut relayout, refill) = self.relayout_choice(group, count)?;
        if !self.command_update(group, &mut relayout)? {
            return Ok(false);
        }
        if !self.any_member_held(group) {
            relayout = false;
        }
        let g = self.groups.held(group)?;
        if refill && g.current_formation.is_some() {
            // FUN_1410163f0: an attack's formation of kind 6 fills its slots anew; only an
            // attack (9) chooses it, whose update stops above
            return Err(OTHER_COMMANDS);
        }
        if relayout {
            self.update_desired_positions(group, None)?;
        }
        self.slots_step(group, &differentiators)?;
        self.update_end_steps(group)
    }

    /// [`World::after_motion`]'s members' moves to their slots: an awake group (not hibernating)
    /// clears [`GroupFlags::CATCHING_UP`] and, with a formation holding slots, moves its members to
    /// them ([`World::members_to_slots`]).
    fn slots_step(
        &mut self,
        group: GroupId,
        differentiators: &[ObjectId],
    ) -> Result<(), GenieError> {
        let g = self.groups.held_mut(group)?;
        if g.hibernating == 0 {
            g.flags &= !GroupFlags::CATCHING_UP;
            if g.current_formation
                .as_ref()
                .is_some_and(|f| !f.slots.is_empty())
            {
                self.members_to_slots(group, differentiators)?;
            }
        }
        Ok(())
    }

    /// [`World::after_motion`]'s last steps: the members' own moves, the order queue's due
    /// orders and the idle timer. True while the group has members.
    fn update_end_steps(&mut self, group: GroupId) -> Result<bool, GenieError> {
        self.specific_moves_step(group)?;
        if !self.groups.held(group)?.order_queue.is_empty() {
            self.drop_due_orders(group);
        }
        self.idle_timer_step(group)?;
        Ok(!self.groups.held(group)?.members.is_empty())
    }

    /// The formation's turn (`0x14103e037`..`0x14103e37b`, [`GroupFlags::TURN`]): when the motion
    /// turned more than π/4 from `forward` (the angle `FUN_14087e650` in size; `COMISS` then `JA`,
    /// a NaN does not), or [`MotionFlags::NEW`] or [`GroupFlags::RELAYOUT_AT_TURN`] is set, the
    /// stored positions lose their points (`FUN_14102bae0`), the group moves to its front
    /// ([`World::move_position_to_front`], 100 ahead; [`World::front_by_centroid`] when the walked
    /// level holds no path, its id `+0x20` -1), lays its formation out anew
    /// (`UpdateDesiredPositions(-1)`) whose slots lose their units and take them again
    /// (`FUN_141041810`, [`World::populate_slots`]), and [`GroupFlags::RELAYOUT_AT_TURN`] clears.
    /// Then the walked levels below the top go (`FUN_1410481f0`), `BMotion`'s update runs again
    /// with a budget, the formation transforms at the new place ([`World::formation_transform`],
    /// `param_4` 0) and [`GroupFlags::TURN`] clears. The debug drawing (`DAT_143f15a9c`) holds no
    /// state.
    pub(super) fn turn_formation(
        &mut self,
        group: GroupId,
        forward: BVector,
    ) -> Result<(), GenieError> {
        if self.turns_formation(group, forward)? {
            let g = self.groups.held(group)?;
            let without_path = g.motion.paths.first().is_some_and(|l| !l.has_path);
            self.groups.held_mut(group)?.stored_positions.clear_points();
            if without_path {
                self.front_by_centroid(group)?;
            } else {
                self.move_position_to_front(
                    group,
                    Distance::new(self.model.tuning.movement.look_ahead),
                )?;
            }
            self.update_desired_positions(group, None)?;
            if let Some(mut f) = self.groups.held_mut(group)?.current_formation.take() {
                f.clear_units();
                let g = self.groups.held(group)?.clone();
                let placed = self.populate_slots(&mut f, &g);
                self.groups.held_mut(group)?.current_formation = Some(f);
                placed?;
            }
            self.groups.held_mut(group)?.flags &= !GroupFlags::RELAYOUT_AT_TURN;
        }
        self.groups.held_mut(group)?.motion.drop_to_high_level()?;
        self.motion_update(group, true)?;
        if let Some(mut f) = self.groups.held_mut(group)?.current_formation.take() {
            let moved = self.formation_transform(&mut f, group, false);
            self.groups.held_mut(group)?.current_formation = Some(f);
            let _ = moved?;
        }
        self.groups.held_mut(group)?.flags &= !GroupFlags::TURN;
        Ok(())
    }

    /// Whether the formation's turn lays it out anew: the motion turned more than π/4 from
    /// `forward` (the size of `FUN_14087e650`'s angle; `COMISS` then `JA` at `0x14103e154`, a NaN
    /// does not), or [`MotionFlags::NEW`] or [`GroupFlags::RELAYOUT_AT_TURN`] is set.
    pub(super) fn turns_formation(
        &self,
        group: GroupId,
        forward: BVector,
    ) -> Result<bool, GenieError> {
        let g = self.groups.held(group)?;
        let turn = angle_between(g.motion.position[1], forward).abs();
        Ok(TURN_LIMIT < turn
            || g.motion.flags.contains(MotionFlags::NEW)
            || g.flags.contains(GroupFlags::RELAYOUT_AT_TURN))
    }

    /// `movePositionToFrontUsingCentroid` (inline at `0x14103e199`, its log aside): the group's
    /// place, and its new place, are its members' mean place (slot 0x520's, summed in order and
    /// divided by the member count, `CVTDQ2PS`) plus the forward scaled by 0 (`FUN_14087e530`:
    /// a zero but for an infinite or NaN forward).
    ///
    /// # Errors
    ///
    /// When the world does not hold a member: the game reads its null object; the members'
    /// check at the update's start leaves only held ones.
    pub(super) fn front_by_centroid(&mut self, group: GroupId) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let mut sum = BVector::ZERO;
        for m in &g.members {
            let v = self
                .world_object(m.id)
                .ok_or_else(|| member_gone(m.id))?
                .moved_vector();
            sum = BVector::new(sum.x() + v.x(), sum.height() + v.height(), sum.y() + v.y());
        }
        let n = g.members.len() as i32 as f32;
        let ahead = scale(0.0, g.motion.position[1]);
        let point = BVector::new(
            sum.x() / n + ahead.x(),
            sum.height() / n + ahead.height(),
            sum.y() / n + ahead.y(),
        );
        let g = self.groups.held_mut(group)?;
        g.motion.new_position[0] = point;
        g.motion.position[0] = point;
        Ok(())
    }

    /// Each formation line in use's (`mNumberLines`, `+0x2c`) attack range (`+0x10`,
    /// `0x14103e448`..`0x14103e535`): `f32::MAX`
    /// (the immediate `0x7f7fffff`), lowered to the weapon range (slot 0x230, a combat object's,
    /// else 0) of each of its slots' units the world holds that is below it (`COMISS` then
    /// `JBE`: a NaN range is not).
    ///
    /// # Errors
    ///
    /// A line place without a slot (index -1 or past the slots, where the game reads outside the
    /// slots' array), and a unit of a master the slice does not have.
    pub(crate) fn line_ranges(&mut self, group: GroupId) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let Some(f) = g.current_formation.as_ref() else {
            return Ok(());
        };
        let lines = usize::try_from(f.number_lines).unwrap_or(0);
        let mut ranges = Vec::with_capacity(lines);
        for line in f.lines.iter().take(lines) {
            let mut range = f32::MAX;
            for &index in &line.indices {
                // the slots' array: those in use, then the spare ones
                let slot = usize::try_from(index as i32)
                    .ok()
                    .and_then(|i| {
                        f.slots
                            .get(i)
                            .or_else(|| f.spare_slots.get(i.checked_sub(f.slots.len())?))
                    })
                    .ok_or(GenieError::unsupported(
                        "a formation line's place without a slot (the update reads outside the slots, 0x14103e4ce)",
                    ))?;
                let Some(object) = self.world_object(slot.unit_id) else {
                    continue;
                };
                let master = self.master(object.player(), object.master_id(self.model.slots))?;
                let reach = weapon_range(master);
                if range > reach.get() {
                    range = reach.get();
                }
            }
            ranges.push(range);
        }
        let f = self
            .groups
            .held_mut(group)?
            .current_formation
            .as_mut()
            .ok_or(NO_FORMATION)?;
        for (line, range) in f.lines.iter_mut().zip(ranges) {
            line.attack_range = Distance::new(range);
        }
        Ok(())
    }

    /// The relayout's choice (`0x14103e53f`..`0x14103e5f7`) after the update's start counted
    /// `count` members: (relayout, refill). A changed count refills an attack's (9) formation of
    /// kind 6 (`FUN_1410163f0` later), else sets [`GroupFlags::RELAYOUT_SPREAD`] for a total
    /// spacing (`+0x47c`) of at most 6 (`COMISS` then `JBE`: a NaN too), else relays out. Flags bit
    /// 7 with a spacing above 6 relays out and clears bits 7 and 1; then [`GroupFlags::RELAYOUT`]
    /// with a spacing above 6 and an idle group ([`World::is_idle`]) relays out and clears bit 1.
    pub(super) fn relayout_choice(
        &mut self,
        group: GroupId,
        count: usize,
    ) -> Result<(bool, bool), GenieError> {
        let idle = self.is_idle(group);
        let g = self.groups.held_mut(group)?;
        let (mut relayout, mut refill) = (false, false);
        if count != g.members.len() {
            if g.current_command == ATTACK
                && g.current_formation
                    .as_ref()
                    .is_some_and(|f| f.kind == FormationKind::Kind6)
            {
                refill = true;
            } else if at_most_or_unordered(
                g.total_spacing.get(),
                Distance::new(self.model.tuning.missile.volley_reach_margin).get(),
            ) {
                g.flags |= GroupFlags::RELAYOUT_SPREAD;
            } else {
                relayout = true;
            }
        }
        if g.flags.contains(GroupFlags::RELAYOUT_SPREAD)
            && Distance::new(self.model.tuning.missile.volley_reach_margin) < g.total_spacing
        {
            relayout = true;
            g.flags &= !(GroupFlags::RELAYOUT | GroupFlags::RELAYOUT_SPREAD);
        }
        if g.flags.contains(GroupFlags::RELAYOUT)
            && Distance::new(self.model.tuning.missile.volley_reach_margin) < g.total_spacing
            && idle
        {
            g.flags &= !GroupFlags::RELAYOUT;
            relayout = true;
        }
        Ok((relayout, refill))
    }
}

/// The idle timer's span (`FUN_14102d370`'s `% 0x2ee`, its division at `0x14102d395`).
pub const IDLE_SPAN: Millis = Millis::new(750);

/// [`World::awake_update`]'s end before the motion: the group goes without members, and without
/// a queued command or a current one when its formation has every slot in state 3 (or has none).
pub(super) fn awake_group_ends(g: &UnitGroup) -> bool {
    if g.members.is_empty() {
        return true;
    }
    g.command_queue.is_empty()
        && g.current_command == NO_COMMAND
        && g.current_formation
            .as_ref()
            .is_some_and(|f| f.slots.iter().all(|s| s.state == SlotState::InPlace))
}
