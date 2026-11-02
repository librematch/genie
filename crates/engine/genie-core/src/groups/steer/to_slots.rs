//! The members' moves to their slots, a free member's stop, the line test and a member's
//! steering toward its slot (`FUN_141025fb0`).

use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SlotState;
use crate::groups::SpecificMove;
use crate::groups::group_list::{NO_FORMATION, NO_MOVER, member_gone};
use crate::groups::motion::hold;
use crate::groups::steer::{ASIDE_TIMER, CLEARANCE, EPSILON, KEEPS_MOVING, MOST_FAILURES, MOVE_TO};
use crate::map::Distance;
use crate::map::spatial::RecordId;
use crate::math::fmath::{BVector, below_or_unordered};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::masters::master_type::ACTION;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

impl World {
    /// The members' moves to their slots (`0x14103e8e8`..`0x14103efba`) of group `group`, whose
    /// formation has slots; `differentiators` the differentiator groups' members the update
    /// listed (`DAT_143f15860`, `DAT_143f15a90`).
    ///
    /// First the group's way ahead (`0x14103e8e8`..`0x14103ee8d`): gated by a lobby setting
    /// (`FUN_140c89690` non-null and `FUN_1412ffe10`) and a top path, a group more than 8 tiles
    /// from the top path's end (64, squared) looks for the units in its way (`FUN_1411570c0`) to
    /// push aside (`FUN_140f88750`). That routine returns at once while `DAT_143f01836` is 0, as
    /// nothing writes it; the query, the pathers on a path of the stack and the list before it
    /// change nothing, so the check holds no state.
    ///
    /// Then each member (its count read anew each turn) the world holds: one in a slot of the
    /// formation (by its unit's id, the first) whose own move is active (1) is tested on the line
    /// from where it last moved to (slot 0x520) to the slot's place (`FUN_1411570c0`: the records
    /// grown by its radius times [`CLEARANCE`], the members ignored, its terrain mask's tiles
    /// added): with none in the way it stands (slot 0x560) and its own move ends (the move
    /// zeroed, its path and reason -1); else the move's timer runs down by the update's seconds
    /// (`FUN_1415300e0`), and with the timer and the move's distance both above 0 (`COMISS`
    /// with `JNC`, then `JC`: a NaN keeps it) the member waits this update; else it stands and its
    /// move ends the same way. A member in a slot that does not wait steers to it
    /// ([`World::steer_to_slot`], `FUN_141025fb0`). A member in no slot stops (slot 0x5d0) when it
    /// moves (slot 0x5c8), has no `MoveTo` in its action list (slot 0x538 with 1) and its top
    /// action is not in state 4, 5 or 11 (slot 0x2b0). The member's id in `DAT_143cc29a8` is the
    /// log's.
    ///
    /// # Errors
    ///
    /// What the members' tests and steering do not model.
    ///
    /// When the world does not hold `group`: the group update passes its own.
    pub fn members_to_slots(
        &mut self,
        group: GroupId,
        differentiators: &[ObjectId],
    ) -> Result<(), GenieError> {
        let mut i = 0;
        loop {
            let g = self.groups.held(group)?;
            let Some(m) = g.members.get(i).copied() else {
                break;
            };
            let index = i;
            i += 1;
            // members are units: the world's table holds ghosts only at negative ids
            let Some((_, u)) = self.object(m.id) else {
                continue;
            };
            let slot = g
                .current_formation
                .as_ref()
                .and_then(|f| f.slots.iter().position(|s| s.unit_id == Some(m.id)));
            let Some(slot) = slot else {
                self.stop_free_member(m.id)?;
                continue;
            };
            if m.specific_move.active == 1 {
                let to = g
                    .current_formation
                    .as_ref()
                    .ok_or(NO_FORMATION)?
                    .slots
                    .checked(slot)?
                    .transform_position;
                let members: Vec<ObjectId> = g.members.iter().map(|m| m.id).collect();
                let from = u.moved_vector(self.parts.mover_of(u.id));
                let clear = self.line_clear(m.id, from, to, CLEARANCE, &members)?;
                let dt = self.dt_seconds;
                let g = self.groups.held_mut(group)?;
                let own = &mut g.members.checked_mut(index)?.specific_move;
                let ends = if clear {
                    true
                } else {
                    own.timer -= dt;
                    !below_or_unordered(0.0, own.timer.get())
                        || !below_or_unordered(0.0, own.distance.get())
                };
                if !ends {
                    continue;
                }
                if self.object(m.id).is_some()
                    && let Some(mover) = self.parts.mover_mut(m.id)
                {
                    mover.stand()?;
                }
                self.groups
                    .held_mut(group)?
                    .members
                    .checked_mut(index)?
                    .specific_move = SpecificMove {
                    path_id: MovePath::NONE,
                    reason: MoveReason::NONE,
                    ..SpecificMove::default()
                };
            }
            self.steer_to_slot(group, index, slot, differentiators)?;
        }
        Ok(())
    }

    /// A member in no slot (`0x14103ef2e`): it stops (slot 0x5d0) when its action list holds no
    /// `MoveTo` (slot 0x538 with 1), its top action is not in state 4, 5 or 11 (slot 0x2b0) and
    /// it moves (slot 0x5c8).
    pub(super) fn stop_free_member(&mut self, id: ObjectId) -> Result<(), GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(());
        };
        let (keeps_moving, _) = self.action_list(p, u, self.parts.actions_of(u.id))?;
        if keeps_moving
            || !self
                .parts
                .mover_of(u.id)
                .is_some_and(crate::movement::Mover::is_moving)
        {
            return Ok(());
        }
        if self.object(id).is_some()
            && let Some(mover) = self.parts.mover_mut(id)
        {
            mover.stop()?;
        }
        Ok(())
    }

    /// What the members' stops read of a unit's actions: whether its action list holds a
    /// `MoveTo` (slot 0x538 with 1, `FUN_14106d0c0`) or its top action is in state 4, 5 or 11
    /// (slot 0x2b0, `RGE_Action_Object::actionState`), and that state (0 without an action, and
    /// for an object below type 40). The slice holds the top action alone, as the list's.
    ///
    /// # Errors
    ///
    /// An action the slice does not model, and an object of a master the slice does not have.
    pub(in crate::groups) fn action_list(
        &self,
        p: PlayerId,
        u: &crate::objects::unit::Unit,
        actions: &crate::objects::layers::Action,
    ) -> Result<(bool, ActionState), GenieError> {
        if u.master(self.master_tables(), p)?.kind < ACTION {
            return Ok((false, ActionState::NO_ACTION));
        }
        if let Some(e) = actions.unmodelled_action.clone() {
            return Err(e);
        }
        let top = actions.top_action();
        let state = top.map_or(ActionState::NO_ACTION, |a| a.state);
        let move_to = top.is_some_and(|a| a.kind == MOVE_TO);
        Ok((move_to || KEEPS_MOVING.contains(&state.get()), state))
    }

    /// `FUN_1411570c0` for unit `u` on the line `from` -> `to` (x, height, y; held inside the
    /// map, [`hold`]): no record of its terrain mask's tiles (`FUN_141052b90` with its
    /// restriction's mask) or of an object but the `ignore`d ones (itself alone for none) meets
    /// it, each grown by `grow` times its master's radius on x.
    ///
    /// # Errors
    ///
    /// A record whose place the slice does not know near the line.
    ///
    /// For a unit without movement state: a group's members move (see `NO_MOVER`).
    pub(super) fn line_clear(
        &mut self,
        id: ObjectId,
        from: BVector,
        to: BVector,
        grow: Distance,
        ignore: &[ObjectId],
    ) -> Result<bool, GenieError> {
        let (_, u) = self.object(id).ok_or(GenieError::unreachable(
            "a steered member the world does not hold",
        ))?;
        let mover = self.parts.mover_of(u.id).ok_or(NO_MOVER)?;
        let (radius, mask) = (mover.stats.radius.x(), mover.stats.restriction_mask);
        let own = [id];
        let ignore: Vec<RecordId> = if ignore.is_empty() { &own[..] } else { ignore }
            .iter()
            .map(|&i| RecordId::from(i))
            .collect();
        // FUN_1411570c0 sets the manager's ignore list to it (FUN_1410562e0)
        let map = self.spatial.map;
        let hits = self.spatial.line_query(
            hold(from, map),
            hold(to, map),
            grow * radius,
            Some(mask),
            &ignore,
        )?;
        Ok(hits.is_empty())
    }

    /// `FUN_141025fb0`: member `index` of group `group` steers toward formation slot `slot`
    /// (its place `+0x1c`, time `+0x28` and state `+0x2c`); `differentiators` the update's list
    /// of the differentiator groups' members. Nothing for a unit the world does not hold or
    /// without a unit AI. The gap is the ground distance from where the unit last moved to
    /// (slot 0x520) to the slot's place.
    ///
    /// - In place (the gap at most 1e-7, `COMISS` with `JC`: not a NaN) and in slot state 3:
    ///   the member's failures (`+0x1c` of its own move) clear, and the end.
    /// - More than 10 failures: the member's own move becomes a move aside
    ///   ([`World::set_specific_move`], reason 2, its place, [`ASIDE_TIMER`]), it stops (slot
    ///   0x5d0) and the group takes [`GroupFlags::CATCHING_UP`]; the end without the goal.
    /// - A member that is not free ([`World::member_idle`]) stops as one in no slot does, its
    ///   failures clear and a slot in state 1 or 2 takes state 3; the end without the goal. A
    ///   free one in a slot of state 3 whose group moves or waits for a path (`FUN_141046600`,
    ///   `FUN_141046640`) has the slot take state 0.
    /// - A slot whose time is past (unsigned, below the world's time): state 1 becomes 2 and the
    ///   member stops (slot 0x5d0). In state 2, a group that does not move tries the slot: the
    ///   long pather (`FUN_140f9eb70`, from the unit's tile, range 0) for a unit without a
    ///   footprint and a height (`FUN_141194060` with 1), else the short pather (`FUN_140f9e540`,
    ///   range 0, the members ignored); a way found (the short pather's result not 3 or 4) puts
    ///   the unit on the slot ([`World::place_member`], slot 0x58) facing the group's forward
    ///   (slot 0x568 with the group's up). Either way the slot takes state 3. In state 2 or 3 the
    ///   member shows its standing sprite (slot 0x660, [`World::show_standing`]), faces the
    ///   group's forward again when it was put, its failures clear, and the end without the
    ///   goal.
    /// - Then the line to the slot ([`World::line_clear`] with [`CLEARANCE`], the update's list
    ///   ignored): one that meets a record is the way round ([`World::way_round`]). A clear line
    ///   steers the unit: the direction to the slot (slot 0x610), the turn from its forward (slot
    ///   0x618 with dt, [`crate::movement::Mover::turn_dt`]) and the yaw (slot 0x5c0), where its
    ///   speed takes it this update (slot 0x638) and the slot's state from that
    ///   ([`passes_waypoint`], `FUN_141046130`). A slot in state 2 stops the unit (slot 0x5d0),
    ///   which shows its standing sprite when paused (slot 0x600) or when neither the group's top
    ///   path (`FUN_141046500`) is a patrol ([`crate::movement::PathFlags::LOOPS`]) nor its walked
    ///   level holds points. Else by the gap and the radius (master `+0x5d8`; `COMISS` with
    ///   `SETNC`, then `JC`: a NaN is neither): between the radius and five radii, the unit's top
    ///   speed (slot 0x208) and its walking sprite (slot 0x658, [`World::show_walking`]); within
    ///   the radius, the group's top speed (`+0xbc`) and the walking sprite; farther, the group
    ///   takes [`GroupFlags::CATCHING_UP`], and from five tiles on (or a NaN) the top speed and the
    ///   walking sprite (`CATCHING_UP` again), nearer the top speed times 1.2 from three tiles on
    ///   (or a NaN), below three `speed + ((speed × 1.2 − speed) × gap) / 3`, with its running
    ///   sprite (slot 0x650, [`World::show_running`]) or else its walking one. The speed is slot
    ///   0x598's ([`crate::movement::Mover::set_speed`]).
    /// - The goal: the unit AI's `state_position` (`+0x1ac`, `+0x1b0`) takes the slot's x and y.
    ///
    /// The logs (`FUN_140a7f7e0`, `FUN_140f3ee00`) and the slots whose answers only they read
    /// (0x5c8, 0x538, 0x2b0, 0x530, 0x208, 0x590, 0x1f8) hold no state.
    ///
    /// # Errors
    ///
    /// What the steps and the way round do not model.
    ///
    /// When the world does not hold `group`, or `index` or `slot` is outside it: the members'
    /// moves pass their own.
    ///
    /// [`passes_waypoint`]: crate::groups::motion::passes_waypoint
    pub fn steer_to_slot(
        &mut self,
        group: GroupId,
        index: usize,
        slot: usize,
        differentiators: &[ObjectId],
    ) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let id = g.members.checked(index)?.id;
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        if !self.parts.unit_ai.contains(u.id) {
            return Ok(());
        }
        let s = g
            .current_formation
            .as_ref()
            .ok_or(NO_FORMATION)?
            .slots
            .checked(slot)?;
        let to = s.transform_position;
        let late = s.time < self.world_time;
        let at = u.moved_vector(self.parts.mover_of(u.id));
        let mut d = fmath::sub(to, at);
        d = d.with_height(0.0);
        let gap = fmath::sqrt(d.x() * d.x() + 0.0 + d.y() * d.y());
        if !below_or_unordered(EPSILON, gap) && s.state == SlotState::InPlace {
            self.groups
                .held_mut(group)?
                .members
                .checked_mut(index)?
                .specific_move
                .consecutive_move_failures = 0;
            self.set_state_position(id, to);
            return Ok(());
        }
        if MOST_FAILURES
            < g.members
                .checked(index)?
                .specific_move
                .consecutive_move_failures
        {
            // A member's own move for being moved aside (`mReason`).
            let _ = self.set_specific_move(
                group,
                id,
                (MovePath::NONE, MoveReason::ASIDE),
                (at, ASIDE_TIMER, 0.0),
            )?;
            self.stop_member(id)?;
            self.groups.held_mut(group)?.flags |= GroupFlags::CATCHING_UP;
            return Ok(());
        }
        if !self.member_idle(id)? {
            return self.steer_busy_member(group, (index, slot), id);
        }
        let g = self.groups.held_mut(group)?;
        let moving = g.motion.moving();
        let waits = g.motion.waits_to_move();
        let state = &mut g
            .current_formation
            .as_mut()
            .ok_or(NO_FORMATION)?
            .slots
            .checked_mut(slot)?
            .state;
        if *state == SlotState::InPlace && (moving || waits) {
            *state = SlotState::Steering;
        }
        if late && self.slot_due(group, index, slot)? {
            return Ok(());
        }
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        let from = u.moved_vector(self.parts.mover_of(u.id));
        if !self.line_clear(id, from, to, CLEARANCE, differentiators)? {
            return self.way_round(group, index, slot, Distance::new(gap), differentiators);
        }
        self.steer_member(group, slot, id, Distance::new(gap))?;
        self.set_state_position(id, to);
        Ok(())
    }

    /// [`World::steer_to_slot`] of a member that is not idle: it stops unless its action keeps
    /// it moving, its failures reset and a slot in state 1 or 2 takes state 3.
    fn steer_busy_member(
        &mut self,
        group: GroupId,
        (index, slot): (usize, usize),
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let (p, u) = self.object(id).ok_or_else(|| member_gone(id))?;
        let (keeps_moving, _) = self.action_list(p, u, self.parts.actions_of(u.id))?;
        if !keeps_moving
            && self
                .parts
                .mover_of(u.id)
                .is_some_and(crate::movement::Mover::is_moving)
        {
            self.stop_member(id)?;
        }
        let g = self.groups.held_mut(group)?;
        g.members
            .checked_mut(index)?
            .specific_move
            .consecutive_move_failures = 0;
        let state = &mut g
            .current_formation
            .as_mut()
            .ok_or(NO_FORMATION)?
            .slots
            .checked_mut(slot)?
            .state;
        if matches!(*state, SlotState::Arriving | SlotState::Stopping) {
            *state = SlotState::InPlace;
        }
        Ok(())
    }
}
