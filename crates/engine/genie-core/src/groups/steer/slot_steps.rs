//! A due slot's try, the steering on a clear line, and the setters of a member's state,
//! speed, facing, place and own move.

use crate::GenieResult;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::MemberMove;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::SlotState;
use crate::groups::SlotTry;
use crate::groups::SpecificMove;
use crate::groups::SpriteShow;
use crate::groups::formation::top_path;
use crate::groups::group_list::{NO_FORMATION, NO_MOVER, member_gone, slot_gone};
use crate::groups::motion::passes_waypoint;
use crate::map::Distance;
use crate::math::fmath::{BVector, at_most_or_unordered, below_or_unordered};
use crate::math::rng::Stream;
use crate::movement::PathFlags;
use crate::movement::Speed;
use crate::movement::{LongWalker, Mover, Planning, long_path_for, short_path_for};
use crate::objects::ObjectId;
use crate::objects::SpriteFlags;
use crate::objects::facing::SpriteFacets;
use crate::objects::masters::MasterKind;
use crate::pathing::tile_path::Path;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::{ObjectsRef, Place};
use crate::world::layer::MasterRoles;
use crate::world::slots::{CheckedIndex, PlayerSlots};

impl World {
    /// `FUN_141025fb0` for a slot whose time is past (`0x1410263e4`..`0x1410267c5`): true when
    /// it ends the routine (slot state 2 or 3). See [`World::steer_to_slot`]. `to` is the slot's
    /// place and `state` its state as the steer left it.
    pub(super) fn slot_due(
        &mut self,
        group: GroupId,
        index: usize,
        (slot, to, state): (usize, BVector, SlotState),
    ) -> GenieResult<bool> {
        let id = self.groups.held(group)?.members.checked(index)?.id;
        let mut state = state;
        if state == SlotState::Arriving {
            state = SlotState::Stopping;
            self.set_slot_state(group, slot, state)?;
            self.stop_member(id)?;
        }
        let held = self.groups.held(group)?;
        let mut placed = SlotTry::NoWay;
        match state {
            SlotState::Stopping => {
                if !held.motion.moving() {
                    placed = self.try_slot(group, id, to)?;
                }
                self.set_slot_state(group, slot, SlotState::InPlace)?;
            }
            SlotState::InPlace => {}
            SlotState::Steering | SlotState::Arriving => return Ok(false),
        }
        self.show_standing(id)?;
        if placed == SlotTry::Placed {
            self.face_group(group, id)?;
        }
        self.groups
            .held_mut(group)?
            .members
            .checked_mut(index)?
            .specific_move
            .consecutive_move_failures = 0;
        Ok(true)
    }

    /// A due slot's try (`0x141026420`..`0x14102677c`): whether member `id` of group `group`
    /// has a way to `to` (the long pather's, or the short pather's for a unit with a footprint
    /// and a height) and so stands on it, facing the group's forward (slot 0x568).
    ///
    /// # Errors
    ///
    /// What the pathers and the position setter do not model.
    pub(super) fn try_slot(
        &mut self,
        group: GroupId,
        id: ObjectId,
        to: BVector,
    ) -> GenieResult<SlotTry> {
        let (player_id, unit) = self.object(id).ok_or_else(|| member_gone(id))?;
        let mover = self.parts.mover_of(unit.id).ok_or(NO_MOVER)?;
        let held = self.groups.held(group)?;
        let player_kind = self
            .players
            .get(player_id.index())
            .map_or(PlayerKind::Phantom, |player| player.kind);
        let solid = mover.stats.radius.sized();
        let found = if solid {
            let members: Vec<ObjectId> = held.members.iter().map(|member| member.id).collect();
            let at = unit.moved_vector(self.parts.mover_of(unit.id));
            let walker = mover.walker(id);
            let mut plan = Planning {
                pathing: &mut self.pathing,
                now: self.world_time,
                player_kind,
                object: self.streams.get(Stream::Object),
                groups: &self.groups,
            };
            let mut path = Path::default();
            let short_found = short_path_for(
                walker,
                at.ground(),
                (
                    &mut self.spatial,
                    ObjectsRef::new(
                        &self.players,
                        &self.index,
                        &self.model.masters,
                        &self.parts,
                        &self.model.roles,
                        &self.model.tuning,
                    ),
                ),
                &mut plan,
                (to, Distance::ZERO, None),
                &members,
                &mut path,
            )?;
            !short_found.fails()
        } else {
            let walker = LongWalker {
                id,
                position: unit.position(),
                radius: mover.stats.radius.ground(),
                mask: mover.stats.restriction_mask,
                moves: mover.stats.speed != Speed::ZERO
                    || self
                        .model
                        .roles
                        .of_master(unit.life.master_id)
                        .contains(MasterRoles::LONG_PATH_MOVER),
                ignore_units: &mover.ignore_units,
                group_units: self.groups.group_units(unit.group.id),
            };
            long_path_for(
                &walker,
                unit.position().ground(),
                &self.spatial,
                player_kind,
                (to, 0.0),
                None,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                ),
            )?
            .0
        };
        if found {
            self.place_member(id, to)?;
            self.face_group(group, id)?;
        }
        Ok(if found {
            SlotTry::Placed
        } else {
            SlotTry::NoWay
        })
    }

    /// The steering of a member on a clear line to its slot (`0x1410277eb`..`0x141027bff`):
    /// see [`World::steer_to_slot`]. `gap` is the ground distance to the slot.
    pub(super) fn steer_member(
        &mut self,
        group: GroupId,
        (slot, to): (usize, BVector),
        id: ObjectId,
        gap: Distance,
    ) -> GenieResult<()> {
        let dt = self.dt_seconds;
        let (_, unit) = self.object(id).ok_or_else(|| member_gone(id))?;
        let facets = unit.sprite.map(|active_sprite| SpriteFacets {
            count: active_sprite.graphic.facets,
            turns: active_sprite.graphic.flags.contains(SpriteFlags::TURNS),
        });
        let at = unit.moved_vector(self.parts.mover_of(id));
        let mover = self.parts.mover_mut(id).ok_or(NO_MOVER)?;
        let direction = mover.direction_to(at, to);
        let (turn, _) = mover.turn_toward(
            mover.forward,
            direction,
            mover.turn_dt(dt, &self.model.tuning.movement),
        );
        let facets = facets.or_else(|| mover.facets());
        mover.yaw(turn, facets)?;
        let next = mover.projected(direction, dt);
        let state = SlotState::of_passing(passes_waypoint(to, at, next, &self.model.tuning.group));
        let radius = mover.stats.radius.x();
        let speed = mover.stats.speed;
        self.set_slot_state(group, slot, state)?;
        let within = !below_or_unordered(radius, gap.get());
        let near =
            !within && !below_or_unordered(radius * self.model.tuning.group.slot_reach, gap.get());
        if state == SlotState::Stopping {
            self.stop_at_slot(group, id)?;
        } else if near {
            self.set_member_speed(id, speed);
            let _ = self.show_walking(id)?;
        } else if within {
            let top = self.groups.held(group)?.motion.maximum_velocity_forward;
            self.set_member_speed(id, top);
            let _ = self.show_walking(id)?;
        } else {
            self.catch_up_to_slot(group, id, (gap.get(), speed.get()))?;
        }
        Ok(())
    }

    /// A member that passed its slot's waypoint (state 2) stops, and stands when it is paused
    /// or the group's walk has no path to go on.
    fn stop_at_slot(&mut self, group: GroupId, id: ObjectId) -> GenieResult<()> {
        self.stop_member(id)?;
        let (_, unit) = self.object(id).ok_or_else(|| member_gone(id))?;
        let paused = self.parts.mover_of(unit.id).is_some_and(Mover::paused);
        let held = self.groups.held(group)?;
        let patrol =
            top_path(&held.motion.paths).is_some_and(|p| p.path_flags.contains(PathFlags::LOOPS));
        let walks = held
            .motion
            .paths
            .first()
            .is_some_and(|path_data| path_data.has_path && !path_data.points.is_empty());
        if paused || (!patrol && !walks) {
            self.show_standing(id)?;
        }
        Ok(())
    }

    /// A member far from its slot: the group's flag 0x200, and the member walks at its speed within
    /// `GroupTuning::slot_reach`, else runs, faster the farther it is (up to
    /// `GroupTuning::catch_up`).
    fn catch_up_to_slot(
        &mut self,
        group: GroupId,
        id: ObjectId,
        (gap, speed): (f32, f32),
    ) -> GenieResult<()> {
        self.groups.held_mut(group)?.flags |= GroupFlags::CATCHING_UP;
        if at_most_or_unordered(self.model.tuning.group.slot_reach, gap) {
            self.set_member_speed(id, Speed::new(speed));
            let _ = self.show_walking(id)?;
            self.groups.held_mut(group)?.flags |= GroupFlags::CATCHING_UP;
        } else {
            let v = if at_most_or_unordered(self.model.tuning.group.catch_up_span, gap) {
                speed * self.model.tuning.group.catch_up
            } else {
                speed
                    + ((speed * self.model.tuning.group.catch_up - speed) * gap)
                        / self.model.tuning.group.catch_up_span
            };
            self.set_member_speed(id, Speed::new(v));
            if self.show_running(id)? == SpriteShow::NoSprite {
                let _ = self.show_walking(id)?;
            }
        }
        Ok(())
    }

    /// The unit AI's `state_position` (`+0x1ac`, `+0x1b0`) of unit `id` takes the slot place
    /// `to`'s x and y (`LAB_141027c88`).
    pub(super) fn set_state_position(&mut self, id: ObjectId, to: BVector) {
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.state_position = to.ground();
        }
    }

    /// Slot `slot` of group `group`'s formation takes state `state`. The steer runs while the
    /// group holds the formation its update tested, and no routine of the steer replaces it.
    pub(super) fn set_slot_state(
        &mut self,
        group: GroupId,
        slot: usize,
        state: SlotState,
    ) -> GenieResult<()> {
        let unit_group = self.groups.held_mut(group)?;
        let formation = unit_group.current_formation.as_mut().ok_or(NO_FORMATION)?;
        let held = formation
            .slots
            .get_mut(slot)
            .ok_or_else(|| slot_gone(slot))?;
        held.state = state;
        Ok(())
    }

    /// Slot 0x5d0 of member `id`: it stops ([`crate::movement::Mover::stop`]).
    pub(super) fn stop_member(&mut self, id: ObjectId) -> GenieResult<()> {
        if self.object(id).is_some()
            && let Some(mover) = self.parts.mover_mut(id)
        {
            mover.stop()?;
        }
        Ok(())
    }

    /// Slot 0x598 of member `id` ([`crate::movement::Mover::set_speed`]).
    pub(super) fn set_member_speed(&mut self, id: ObjectId, speed: Speed) {
        if self.object(id).is_some()
            && let Some(mover) = self.parts.mover_mut(id)
        {
            mover.set_speed(speed);
        }
    }

    /// Slot 0x568 of member `id` with group `group`'s forward and up vectors (`+0x14`,
    /// `+0x20`, [`crate::movement::Mover::set_orientation`]).
    pub(super) fn face_group(&mut self, group: GroupId, id: ObjectId) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        let [_, forward, up, _] = held.motion.position;
        if self.object(id).is_some()
            && let Some(mover) = self.parts.mover_mut(id)
        {
            mover.set_orientation(forward, up);
        }
        Ok(())
    }

    /// Slot 0x58 of member `id` (`FUN_14108c320`) with the place `to` (x, height, y): the
    /// position setter (`FUN_141161950`: held inside the map for a unit that moves,
    /// [`crate::movement::Mover::set_position`]; the ground's height by its fly mode, the place
    /// and the time of its last move, the world's places and the planners' records follow, as
    /// the motion step's). Then, for an object of an id from 0 below type 80 whose top speed
    /// (slot 0x208) is 0 (`UCOMISS` with `JP`: not a NaN) and whose radius on x or y is above
    /// 0.5 (`COMISS` with `JA`, then `JBE`), its tiles link to its new place (`FUN_140f73900`,
    /// [`crate::vision::visibility::Visibility::link_building`]). The moved records of the objects
    /// fastened to a combat object (`+0x8b0`..`+0x8f0`), which the slice does not hold, are a
    /// unit's none.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn place_member(&mut self, id: ObjectId, to: BVector) -> GenieResult<()> {
        let Some(place) = self.place(id) else {
            return Ok(());
        };
        let (p, master_id) = {
            let (p, u) = self.object(id).ok_or_else(|| member_gone(id))?;
            (p, u.life.master_id)
        };
        let kind = self.master(p, master_id)?.kind;
        let unit = match place {
            Place::Live(p, i) => self.players.live_mut(p, i)?,
            Place::Sleeping(player_id, list_index) => {
                self.players.asleep_mut(player_id, list_index)?
            }
        };
        let Some(mover) = self.parts.mover_mut(unit.id) else {
            return Ok(());
        };
        let map = self.spatial.map;
        let at = mover.set_position(to.position(), map, &self.model.tuning.movement);
        let at = mover.grounded(at, self.spatial.terrain.as_ref());
        mover.mark_move(at, self.world_time);
        let (speed, radius) = (mover.stats.speed, mover.stats.radius);
        mover.new_position(
            &mut unit.place,
            id,
            at,
            (&mut self.spatial, Some(&mut self.pathing)),
            &self.model.roles,
            self.model.tuning.visibility,
        )?;
        if !id.is_recyclable()
            // The types of action objects (`RGE_Action_Object`) and of buildings (`TRIBE_Building_Object`).
            && kind < MasterKind::BUILDING
            && speed == Speed::ZERO
            && (self.model.tuning.group.linked_radius < radius.x() || self.model.tuning.group.linked_radius < radius.y())
            && let Some(visibility) = self.spatial.sight.as_mut()
        {
            let _ = visibility.link_building(at, radius.ground(), self.model.tuning.visibility)?;
        }
        Ok(())
    }

    /// `FUN_141039f60`: member `id`'s own move in group `group`: with a reason other than -1 it
    /// is active (1) with path `path`, reason `reason`, the goal, timer and distance of `goal`
    /// and the place it last moved to (slot 0x520) as its last position, its failures (`+0x1c`)
    /// left; with -1 it ends (zeroed, its path and reason -1). No member (0) for a unit that is no
    /// member or that the world does not hold.
    pub(crate) fn set_specific_move(
        &mut self,
        group: GroupId,
        id: ObjectId,
        (path, reason): (MovePath, MoveReason),
        (goal, timer, distance): (BVector, f32, f32),
    ) -> GenieResult<MemberMove> {
        let Some(index) = self
            .groups
            .get(group)
            .and_then(|g| g.members.iter().position(|member| member.id == id))
        else {
            return Ok(MemberMove::NoMember);
        };
        let Some((_, unit)) = self.object(id) else {
            return Ok(MemberMove::NoMember);
        };
        let last = unit.moved_vector(self.parts.mover_of(unit.id));
        let own = &mut self
            .groups
            .held_mut(group)?
            .members
            .checked_mut(index)?
            .specific_move;
        *own = if reason == MoveReason::None {
            SpecificMove {
                path_id: MovePath::NONE,
                reason: MoveReason::None,
                ..SpecificMove::default()
            }
        } else {
            SpecificMove {
                active: 1,
                path_id: path,
                reason,
                goal,
                timer: Seconds::new(timer),
                distance: Distance::new(distance),
                last_position: last,
                ..*own
            }
        };
        Ok(MemberMove::Set)
    }
}
