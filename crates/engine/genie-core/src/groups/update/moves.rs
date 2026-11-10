//! The current command's update: a move, the members' moves and steps, the order queue's
//! due orders and the idle timer.

use crate::groups::GroupCommandKind;
use crate::groups::GroupFate;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::LeavingWalk;
use crate::groups::MotionFlags;
use crate::groups::MoveFlags;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::Progress;
use crate::groups::SlotState;
use crate::groups::command::MOVE_COMMAND;
use crate::groups::command::MoveTo;
use crate::groups::formation::top_path;
use crate::groups::group_list::member_gone;
use crate::groups::update::{ATTACK, FOLLOW, IDLE_LIMIT, IDLE_SPAN, NO_COMMAND, OTHER_COMMANDS};
use crate::groups::{SpecificMove, UnitGroup};
use crate::map::Distance;
use crate::math::fmath::BVector;
use crate::math::fmath::{below_or_unordered, sqrt};
use crate::math::rng::Stream;
use crate::movement::PathFlags;
use crate::objects::{ObjectId, ObjectKey};
use crate::pathing::DelayCount;
use crate::world::Millis;
use crate::world::World;
use crate::world::index::Place;
use crate::world::motions::BesideParts;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::world::{IN_THE_WAY, WALKER_UNMOVING};
use crate::{GenieError, GenieResult};
use std::mem::take;

impl World {
    /// The current command's update (`0x14103e5f7`..`0x14103e865`): deleted, the world deletes the
    /// group. No command does nothing; a move ([`World::move_update`], `FUN_141041420`) that
    /// finished hibernates the group (`+0x13c` 1) while commands are queued, else deletes it. A
    /// follow (2) with its target held and a lead (the member nearest the target's place, [`World::
    /// nearest_to_target`]) moves the group to the target (`commandMoveTo` with the target's x and
    /// y, the range half the lead's master's line of sight, at once) when the target is that range
    /// or more (`COMISS` then `JC`: a NaN is not) from where the group's target was (`+0x3c8`, the
    /// height left out), and sets [`GroupFlags::FOLLOWED`]; without the target or a lead it ends as
    /// a finished move. An attack ([`World::attack_update`], `FUN_14103f4e0`) that finished deletes
    /// the group when no command is queued and its target is no longer pending
    /// ([`World::target_pending`]), else hibernates it. The other commands (`FUN_141040bd0`) are
    /// not modelled.
    pub(super) fn command_update(
        &mut self,
        group: GroupId,
        relayout: &mut bool,
    ) -> GenieResult<GroupFate> {
        let held = self.groups.held(group)?;
        let finished = match held.current_command {
            NO_COMMAND => Progress::Going,
            MOVE_COMMAND => self.move_update(group, relayout)?,
            FOLLOW => {
                let target = held.current_target.target.key();
                let from = held.current_target.original_position;
                let to = self
                    .world_object(target)
                    .map(|world_object| (world_object.moved_vector(), world_object.position()));
                let lead = if to.is_some() {
                    let lead = self.nearest_to_target(group);
                    self.groups.held_mut(group)?.motion.lead = lead;
                    lead
                } else {
                    None
                };
                match to {
                    Some((moved, at)) if lead.is_some() => {
                        let unit_group = self.groups.held(group)?;
                        let leader = self
                            .lead_object(&unit_group.motion)
                            .ok_or_else(|| member_gone(unit_group.motion.lead))?;
                        let sight = self
                            .master(leader.player(), leader.master_id(self.model.slots))?
                            .line_of_sight;
                        let range = sight * self.model.tuning.group.front_share;
                        let offset = from - moved;
                        let squared = offset.x() * offset.x() + 0.0 + offset.y() * offset.y();
                        if !below_or_unordered(squared, range.squared().get()) {
                            let _ = self.command_move_to(
                                group,
                                target,
                                at.ground(),
                                MoveTo {
                                    range,
                                    flags: MoveFlags::DIRECT,
                                },
                            )?;
                        }
                        self.groups.held_mut(group)?.flags |= GroupFlags::FOLLOWED;
                        return Ok(GroupFate::Kept);
                    }
                    _ => Progress::Finished,
                }
            }
            ATTACK => {
                if self.attack_update(group, relayout)? == Progress::Finished {
                    let held = self.groups.held(group)?;
                    if held.command_queue.is_empty() && !self.target_pending(group)? {
                        return Ok(GroupFate::Deleted);
                    }
                    self.groups.held_mut(group)?.hibernating = 1;
                }
                return Ok(GroupFate::Kept);
            }
            _ => return Err(OTHER_COMMANDS),
        };
        if finished == Progress::Finished {
            if self.groups.held(group)?.command_queue.is_empty() {
                return Ok(GroupFate::Deleted);
            }
            self.groups.held_mut(group)?.hibernating = 1;
        }
        Ok(GroupFate::Kept)
    }

    /// `FUN_141041420`, a move's update: finished when the move finished. [`MotionFlags::NEW`] asks
    /// for a relayout. A moving group (`FUN_141046600`) asks for one too, unless with
    /// [`GroupFlags::FOLLOWED`] (a follow's) its target the world holds stands a squared 1 or more
    /// (`COMISS` then `JNC`: not a NaN) from the top path's end (`FUN_140f95fb0`: its last point,
    /// the static zero point `DAT_143f0dc78` for fewer than two), when the command becomes a follow
    /// (2); such a group without its target does nothing. A standing group with
    /// [`GroupFlags::FOLLOWED`] follows too. A group with wait delays (`+0xc0`) or waiting for a
    /// path (`FUN_141046640`) does nothing. Then, off a patrol (the top path's
    /// [`PathFlags::LOOPS`]) and with [`GroupFlags::PATROL`] clear, without queued commands but
    /// with a formation whose slots' array exists: the units of the slots in state 3 leave the
    /// group (`removeUnit` with the group's target); some left and some not: the rest move to their
    /// places ([`World::move_to_desired_positions`]) and the update ends. With a path walked (the
    /// walked level's path, `+0x18`, holding points) it goes on; without, a formation none of whose
    /// slots was in state 3 locks to its kind ([`World::lock_formation`], `FUN_141024e70`) and the
    /// command ends (-1); else the command ends. Any other group without a path walked ends its
    /// command. Then [`GroupFlags::LINE`] clears, [`GroupFlags::MEMBERS_SENT`] sets and the members
    /// move to their places; finished: no path walked and not the patrol's case.
    ///
    /// # Errors
    ///
    /// What the leaves, the lock and the members' moves do not model.
    ///
    /// When the world does not hold `group`: the group update passes its own.
    pub fn move_update(&mut self, group: GroupId, relayout: &mut bool) -> GenieResult<Progress> {
        let held = self.groups.held(group)?;
        if held.motion.flags.contains(MotionFlags::NEW) {
            *relayout = true;
            return Ok(Progress::Going);
        }
        let walks = held
            .motion
            .paths
            .first()
            .is_some_and(|path_data| path_data.has_path && !path_data.points.is_empty());
        let follows = held.flags.contains(GroupFlags::FOLLOWED);
        if held.motion.moving() {
            return self.moving_step(group, follows, relayout);
        }
        if follows {
            self.groups.held_mut(group)?.current_command = FOLLOW;
            return Ok(Progress::Going);
        }
        if held.motion.number_wait_delays != DelayCount::ZERO || held.motion.waits_to_move() {
            return Ok(Progress::Going);
        }
        let patrol = top_path(&held.motion.paths)
            .is_some_and(|path_data| path_data.path_flags.contains(PathFlags::LOOPS));
        let mut patrolling = true;
        if !patrol && !held.flags.contains(GroupFlags::PATROL) {
            patrolling = false;
            let slots = held
                .current_formation
                .as_ref()
                .filter(|formation| {
                    !formation.slots.is_empty() || !formation.spare_slots.is_empty()
                })
                .map(|formation| {
                    (
                        formation.kind,
                        formation
                            .slots
                            .iter()
                            .map(|slot| (slot.unit_id, slot.state))
                            .collect::<Vec<_>>(),
                    )
                });
            if held.command_queue.is_empty()
                && let Some((kind, slots)) = slots
            {
                let target = held.current_target.target.key();
                let mut kept = 0;
                for &(unit, state) in &slots {
                    if state == SlotState::InPlace {
                        // a slot without a unit names no member: the removal finds none
                        if let Some(unit) = unit {
                            let _ = self.remove_unit(group, unit, target, LeavingWalk::Stands)?;
                        }
                    } else {
                        kept += 1;
                    }
                }
                if kept != 0 && kept != slots.len() {
                    self.move_to_desired_positions(group, MOVE_COMMAND, None)?;
                    return Ok(Progress::Going);
                }
                if !walks {
                    if kept == slots.len() {
                        self.lock_formation(group, kind)?;
                        self.groups.held_mut(group)?.current_command = NO_COMMAND;
                        return Ok(Progress::Going);
                    }
                    self.groups.held_mut(group)?.current_command = NO_COMMAND;
                }
                return self.move_members_on(group, walks, patrolling);
            }
        }
        if !walks {
            self.groups.held_mut(group)?.current_command = NO_COMMAND;
        }
        self.move_members_on(group, walks, patrolling)
    }

    /// [`World::move_update`] of a group whose motion moves: a following group
    /// ([`GroupFlags::FOLLOWED`]) whose top path ends at least `GroupTuning::close` from its target
    /// goes on following; any other asks for a layout anew. The update does not end.
    fn moving_step(
        &mut self,
        group: GroupId,
        follows: bool,
        relayout: &mut bool,
    ) -> GenieResult<Progress> {
        let held = self.groups.held(group)?;
        if follows {
            let Some(target) = self.world_object(held.current_target.target) else {
                return Ok(Progress::Going);
            };
            if let Some(top) = top_path(&held.motion.paths) {
                let end = match top.points.as_slice() {
                    [.., _, last] => *last,
                    // DAT_143f0dc78, zero-initialised (.bss) and written by nothing
                    _ => BVector::ZERO,
                };
                let offset = end - target.moved_vector();
                let squared = offset.x() * offset.x()
                    + offset.height() * offset.height()
                    + offset.y() * offset.y();
                if !below_or_unordered(squared, self.model.tuning.group.close) {
                    self.groups.held_mut(group)?.current_command = FOLLOW;
                    return Ok(Progress::Going);
                }
            }
        }
        *relayout = true;
        Ok(Progress::Going)
    }

    /// `FUN_141041420`'s end (`0x1410416dc`): [`GroupFlags::LINE`] clears,
    /// [`GroupFlags::MEMBERS_SENT`] sets, the members move to their places
    /// ([`World::move_to_desired_positions`] with a move and no target); the move finished when no
    /// path is walked and it was no patrol.
    pub(super) fn move_members_on(
        &mut self,
        group: GroupId,
        walks: bool,
        patrolling: bool,
    ) -> GenieResult<Progress> {
        let unit_group = self.groups.held_mut(group)?;
        unit_group.flags = (unit_group.flags & !GroupFlags::LINE) | GroupFlags::MEMBERS_SENT;
        self.move_to_desired_positions(group, MOVE_COMMAND, None)?;
        Ok(if !walks && !patrolling {
            Progress::Finished
        } else {
            Progress::Going
        })
    }

    /// `AutomaticMoveUnitsToDesiredPositions` (`FUN_14100aaa0`) of group `group`'s formation
    /// for command `command` on target `target` (the group update's are a move and -1); its log
    /// lines aside. Nothing without a formation or its slots' array (`FUN_140fd75d0` lets go of
    /// the formation's reference). For each slot with a unit (`-1` none) the world holds that is
    /// free ([`World::member_idle`], `FUN_141017f00`): one in its place (slot state 3) on an
    /// attack (9) on a held target takes the target (slot 0x128 with its x and y); any other
    /// gets a path to the slot.
    ///
    /// # Errors
    ///
    /// A unit's attack on the target (its slot 0x128) and its path to the slot (a world path
    /// record `FUN_14152eea0`, the long or short pather, the path's length against 1.01,
    /// `DAT_142eb6e48`, which has no image role, then slot 0x5d0, `FUN_140f9b6c0`, slot 0x548
    /// and the unit's move, slot 0x128): not modelled.
    ///
    /// When the world does not hold `group`: its callers pass their own.
    pub fn move_to_desired_positions(
        &mut self,
        group: GroupId,
        command: GroupCommandKind,
        target: Option<ObjectId>,
    ) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        let Some(formation) = held
            .current_formation
            .as_ref()
            .filter(|formation| !formation.slots.is_empty() || !formation.spare_slots.is_empty())
        else {
            return Ok(());
        };
        let held_target = self.world_object(target).is_some();
        for slot in &formation.slots {
            if slot.unit_id.is_none() || self.world_object(slot.unit_id).is_none() {
                continue;
            }
            if !self.member_idle(slot.unit_id)? {
                continue;
            }
            if slot.state == SlotState::InPlace {
                if command == ATTACK && held_target {
                    return Err(GenieError::unsupported(
                        "AutomaticMoveUnitsToDesiredPositions' attack on the target (slot 0x128)",
                    ));
                }
                continue;
            }
            return Err(GenieError::unsupported(
                "AutomaticMoveUnitsToDesiredPositions' path to a slot (FUN_14100aaa0: the unit's path, 1.01 of DAT_142eb6e48, slots 0x548 and 0x128)",
            ));
        }
        Ok(())
    }

    /// The members' own moves past the slots' (`0x14103efc4`..`0x14103f355`): each member whose
    /// move is active (`mSpecificMoves[index]`) and that is free ([`World::member_idle`]) and held:
    /// one moved aside (reason 2) stops (slot 0x5d0), shows standing (slot 0x660), counts its
    /// timer down by the update's seconds (`FUN_1415300e0`) and, once at most 0 (`COMISS` then
    /// `JC`: a NaN keeps it), drops its move (active 0, path and reason -1, the rest 0) and stands
    /// (slot 0x560). Any other steps (slot 0x608, [`World::member_step`]): once the step is done
    /// (not 0) its move is dropped; else it shows its walking sprite (slot 0x658) and its move's
    /// distance loses the ground distance from its last position to where it last moved to
    /// (slot 0x520), which becomes its last position.
    ///
    /// # Errors
    ///
    /// What the motion step, the stop, the stand and the walking and standing sprites do not
    /// model.
    pub(super) fn specific_moves_step(&mut self, group: GroupId) -> GenieResult<()> {
        let mut index = 0;
        loop {
            let held = self.groups.held(group)?;
            let Some(member) = held.members.get(index).copied() else {
                break;
            };
            index += 1;
            if member.specific_move.active == 0 || !self.member_idle(member.id)? {
                continue;
            }
            if self.object(member.id).is_none() {
                continue;
            }
            if member.specific_move.reason != MoveReason::Aside {
                // slot 0x608 (FUN_1411581e0): the unit's own motion step (FUN_141150f00), then
                // its facet from its angle (FUN_14114d8b0, not held)
                if self.member_step(member.id)? == Progress::Finished {
                    self.groups
                        .held_mut(group)?
                        .members
                        .checked_mut(index - 1)?
                        .specific_move = SpecificMove {
                        path_id: MovePath::NONE,
                        reason: MoveReason::None,
                        ..SpecificMove::default()
                    };
                    continue;
                }
                let _ = self.show_walking(member.id)?;
                let at = self
                    .object(member.id)
                    .map(|(_, unit)| unit.moved_vector(self.parts.mover_of(unit.id)))
                    .ok_or_else(|| member_gone(member.id))?;
                let own = &mut self
                    .groups
                    .held_mut(group)?
                    .members
                    .checked_mut(index - 1)?
                    .specific_move;
                let mut offset = at - own.last_position;
                offset = offset.with_height(0.0);
                own.distance -= Distance::new(sqrt(
                    offset.x() * offset.x() + 0.0 + offset.y() * offset.y(),
                ));
                own.last_position = at;
                continue;
            }
            if let Some(mover) = self.parts.mover_mut(member.id) {
                mover.stop()?;
            }
            self.show_standing(member.id)?;
            let dt = self.dt_seconds;
            let unit_group = self.groups.held_mut(group)?;
            let timer = &mut unit_group
                .members
                .checked_mut(index - 1)?
                .specific_move
                .timer;
            *timer -= dt;
            if !below_or_unordered(0.0, timer.get()) {
                unit_group.members.checked_mut(index - 1)?.specific_move = SpecificMove {
                    path_id: MovePath::NONE,
                    reason: MoveReason::None,
                    ..SpecificMove::default()
                };
                if self.object(member.id).is_some()
                    && let Some(mover) = self.parts.mover_mut(member.id)
                {
                    mover.stand()?;
                }
            }
        }
        Ok(())
    }

    /// Slot 0x608 (`FUN_1411581e0`) of member `id`: the unit's own motion step (`FUN_141150f00`,
    /// [`crate::movement::Mover::motion`] with its top action's state, the update's seconds and
    /// the world's planners); finished when it is done (the step answers other than 0: arrived or
    /// given up). Its place follows the world's; the units its walk paused take their pauses
    /// (slot 0x5f0). The walk reads the other objects' gaits as they are now (the group's
    /// members as its update left them); the member itself is out of its slot meanwhile, as an
    /// updating object is. Not held: its facet from its angle (`FUN_14114d8b0`).
    ///
    /// # Errors
    ///
    /// What the motion step does not model, and a walk's unit in the way (notify 0x215).
    pub(crate) fn member_step(&mut self, id: ObjectId) -> GenieResult<Progress> {
        use crate::movement::{Motion, Planning};
        let Some(place) = self.place(id) else {
            return Ok(Progress::Finished);
        };
        let Place::Live(player_id, idx) = place else {
            return Err(GenieError::unsupported("a sleeping member's own move"));
        };
        let player_kind = self.players.player(player_id)?.kind;
        let top_speed = {
            let unit = self.players.live(player_id, idx)?;
            self.top_speed_terms(player_id, unit)?
        };
        // the member is out of its slot for its step, as the updating object is
        let mut unit = self.take_out(player_id, idx)?;
        let me = unit.me(self.parts.actions_of(unit.id), top_speed);
        let (mover, by_id) = self.parts.lend_mover(
            unit.id,
            BesideParts {
                players: &self.players,
                index: &self.index,
                model: &self.model,
                treaty: self.treaty.as_ref(),
            },
        );
        let stepped = match mover {
            Some(mover) => {
                let mut plan = Planning {
                    pathing: &mut self.pathing,
                    now: self.world_time,
                    player_kind,
                    object: self.streams.get_mut(Stream::Object),
                    groups: &self.groups,
                };
                mover
                    .motion(
                        &mut unit.place,
                        &me,
                        self.dt_seconds,
                        &mut self.spatial,
                        Some(&mut plan),
                        by_id,
                    )
                    .map(|motion| {
                        let pauses = take(&mut mover.pauses);
                        (motion, pauses, mover.in_the_way.is_empty())
                    })
            }
            None => Err(WALKER_UNMOVING),
        };
        let _ = self.put_back_own(player_id, unit)?;
        let (motion, pauses, clear) = stepped?;
        if !clear {
            return Err(IN_THE_WAY);
        }
        for (other, pause) in pauses {
            self.pause(other, pause);
        }
        Ok(if motion == Motion::Moving {
            Progress::Going
        } else {
            Progress::Finished
        })
    }

    /// `FUN_14100d940` (its log lines aside): the order queue's orders whose time (`+0x18`) is not
    /// past the world's time go, in order; the others stay.
    pub fn drop_due_orders(&mut self, group: GroupId) {
        let now = self.world_time;
        if let Some(group) = self.groups.get_mut(group) {
            group
                .order_queue
                .retain(|group_order| now < group_order.execute_time);
        }
    }

    /// The idle timer (`0x14103f370`..`0x14103f3a3`): an idle group ([`World::is_idle`]) adds
    /// the update's milliseconds (`world+0x7c`) to its timer (`+0x3e4`, wrapping), and past
    /// [`IDLE_LIMIT`] (unsigned) draws it anew ([`World::reset_idle_timer`]); a group that is not
    /// idle draws it anew.
    pub(super) fn idle_timer_step(&mut self, group: GroupId) -> GenieResult<()> {
        if self.is_idle(group) {
            let dt = self.dt_ms();
            let unit_group = self.groups.held_mut(group)?;
            unit_group.idle_timer = unit_group.idle_timer.wrapping_add(dt);
            if unit_group.idle_timer <= IDLE_LIMIT {
                return Ok(());
            }
        }
        self.reset_idle_timer(group);
        Ok(())
    }

    /// The update's milliseconds (`world+0x7c`), of which [`World::dt_seconds`] (`world+0x164`)
    /// is [`Millis::seconds`]: the round trip gives the milliseconds back exactly for
    /// any update below 2^22 of them.
    pub(super) fn dt_ms(&self) -> Millis {
        Millis::new((self.dt_seconds.get() * 1000.0).round() as u32)
    }
}

impl World {
    /// `FUN_141017630`: a member of group `group` the world holds (false for a group it does not
    /// hold).
    #[must_use]
    pub fn any_member_held(&self, group: GroupId) -> bool {
        self.groups.get(group).is_some_and(|group| {
            group
                .members
                .iter()
                .any(|member| self.object(member.id).is_some())
        })
    }

    /// `resetIdleTimer` (`FUN_14102d370`): group `group`'s idle timer (`+0x3e4`) is a draw of the
    /// World stream (`rand15`, `DAT_143f07040`) modulo [`IDLE_SPAN`]; nothing for a group the
    /// world does not hold (the game draws for its own).
    pub fn reset_idle_timer(&mut self, group: GroupId) {
        if self.groups.get(group).is_none() {
            return;
        }
        let draw = self.streams.get_mut(Stream::World).rand15();
        if let Some(group) = self.groups.get_mut(group) {
            group.idle_timer = Millis::new(draw % IDLE_SPAN.get());
        }
    }

    /// `IsIdle` (`FUN_141017ad0`) of group `group`: idle with [`GroupFlags::MEMBERS_SENT`]
    /// (`cConsiderIdle`), or on a move (command 1) along a patrol (the top path's
    /// [`PathFlags::LOOPS`], `FUN_141046500`) or with [`GroupFlags::PATROL`]; else not while its
    /// motion moves or waits to move; else idle. False for a group the world does not hold.
    #[must_use]
    pub fn is_idle(&self, group: GroupId) -> bool {
        self.groups.get(group).is_some_and(UnitGroup::is_idle)
    }
}
