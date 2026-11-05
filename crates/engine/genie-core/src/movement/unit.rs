//! The movement actions in a unit's update: the velocity step, the unit's own `MoveTo`, its
//! Explore and a bird's Fly.

use crate::GenieResult;
use crate::groups::MoveReason;
use crate::groups::UnitGroups;
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::math::rng::Stream;
use crate::movement::PathAsk;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::footprint_gap_squared;
use crate::movement::move_to::MoveStep;
use crate::movement::move_to::Patience;
use crate::movement::move_to::StepEnd;
use crate::movement::move_to::{MoveTo, Shows};
use crate::movement::step::VelocityStep;
use crate::movement::{GroupSpeed, Me};
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::layers::Speedup;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::sprite::Cargo;
use crate::objects::sprite::standing;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::world::NO_ATTRIBUTE;
use crate::world::SPEED_EPSILON;
use crate::world::Unread;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

impl Unit {
    /// `RGE_Moving_Object::update` (`0x14114dbb0`) before the actions: the last human order's time
    /// counts down, the velocity step moves the unit (at the speed its group steers,
    /// [`Unit::group_speed`]), and a moving unit animates at its speed.
    ///
    /// # Errors
    ///
    /// A unit its group may steer whose master the slice does not have or whose action it does
    /// not model.
    pub(crate) fn motion_step(
        &mut self,
        actions: &Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        // RGE_Moving_Object::update counts the last human order time down before its velocity
        // step
        if step.world.parts.mover_of(self.id).is_some() {
            self.count_down_human_order(step.dt);
        }
        // the velocity step (FUN_140f9ba80) runs for a unit with velocity data (+0x868)
        let group = match step
            .world
            .parts
            .mover_of(self.id)
            .and_then(|mover| mover.velocity.as_ref())
        {
            Some(_) => self.group_speed(
                (actions, step.world.parts.unread.get(self.id)),
                &step.world.groups,
                OwnerMasters::new(
                    &step.world.parts.own_master,
                    step.world.model.masters.of(step.player),
                ),
                step.world.parts.unit_ai.get(self.id),
                roles,
            )?,
            None => None,
        };
        let top = if step.world.parts.mover_of(self.id).is_some() {
            step.world.top_speed_terms(step.player, self)?
        } else {
            TopSpeed::Own
        };
        if let Some(mover) = step.world.parts.mover_mut(self.id) {
            let here = self.place.position();
            let at = mover.step_velocity(
                VelocityStep {
                    position: here,
                    dt: step.dt.seconds(),
                    map: step.world.spatial.map,
                    terrain: step.world.spatial.terrain.as_ref(),
                    now: step.world.world_time,
                    group,
                    top,
                },
                &step.world.model.tuning.movement,
            );
            if at != here {
                mover.new_position(
                    &mut self.place,
                    self.id,
                    at,
                    (&mut step.world.spatial, Some(&mut step.world.pathing)),
                    roles,
                    step.world.model.tuning.visibility,
                )?;
            }
            // RGE_Moving_Object::update: a unit that moves animates at its speed (+0x7b8)
            if let Some(velocity) = mover.velocity.as_ref() {
                let [x, y, z] = velocity.velocity.get();
                let speed = sqrt(x * x + y * y + z * z);
                if SPEED_EPSILON < speed {
                    step.world.parts.set_anim_speed(self.id, Speed::new(speed));
                }
            }
        }
        self.speedup_drop(step)
    }

    /// `RGE_Moving_Object::update`'s speed-up check (`0x14114de0a`): while the unit's mover holds
    /// a speed-up building (`+0x808` not −1), its owner's speed-up range (attribute 256) is read,
    /// then the building is looked up among the world's objects (`FUN_1415417a0`). A building
    /// gone, or one whose squared footprint gap to the unit (`FUN_14118f8c0`) is above the range
    /// squared (`COMISS`, `JBE`: a NaN gap keeps it), is dropped: `+0x808` takes −1 and `+0x80c`
    /// 0 ([`Speedup::NONE`]), and slot 0x208 adds no share any more ([`Mover::own_speed`]).
    ///
    /// [`Mover::own_speed`]: crate::movement::Mover::own_speed
    ///
    /// # Errors
    ///
    /// An owner without the attribute, and a unit or a building whose master the world lacks.
    fn speedup_drop(&self, step: &mut Step<'_>) -> GenieResult<()> {
        let Some(building) = step
            .world
            .parts
            .mover_of(self.id)
            .and_then(|mover| mover.speedup.building)
        else {
            return Ok(());
        };
        let range = step
            .world
            .players
            .player(step.player)?
            .attribute(step.world.model.slots.attribute.speedup_range)
            .ok_or(NO_ATTRIBUTE)?
            .get();
        let keeps = match step.world.world_object(Some(building)) {
            Some(found) => {
                let radius = self.master(step.world.master_tables(), step.player)?.radius;
                let other = step.world.speed_reads().object_master(found)?.radius;
                let gap = footprint_gap_squared(self.position(), radius, found.position(), other);
                at_most_or_unordered(gap.get(), range * range)
            }
            None => false,
        };
        if !keeps && let Some(mover) = step.world.parts.mover_mut(self.id) {
            mover.speedup = Speedup::NONE;
        }
        Ok(())
    }

    /// What slot 0x5e0 (`FUN_14114f300`) reads of the unit's group ([`GroupSpeed`]) among the
    /// world's `groups`, the unit's master among `masters` and its unit AI `ai`, with its action
    /// layer `actions` and why the load could not read it whole `unread`: none outside a group the
    /// world holds (`+0x6d0`, `FUN_14152ff40`: none for −1), nor when the group's motion stands
    /// (`FUN_141046600`) and the unit is in its place ([`UnitGroup::slot_in_place`]), nor for a
    /// unit not free for the group's moves
    /// ([`Action::free_for_group`](crate::objects::layers::Action::free_for_group)). Else its own
    /// move's reason (`FUN_141017590`): 2 moved aside, −1 none, any other an own move at the
    /// group's top speed (`+0xbc`).
    ///
    /// # Errors
    ///
    /// A unit whose master the slice does not have, or whose action it does not model.
    ///
    /// [`UnitGroup::slot_in_place`]: crate::groups::UnitGroup::slot_in_place
    pub(crate) fn group_speed(
        &self,
        (actions, unread): (&Action, Option<&Unread>),
        groups: &UnitGroups,
        masters: OwnerMasters<'_>,
        ai: Option<&UnitAi>,
        roles: &Roles,
    ) -> GenieResult<Option<GroupSpeed>> {
        let Some(group) = self.group.id.and_then(|id| groups.get(id)) else {
            return Ok(None);
        };
        if !group.motion.moving() && group.slot_in_place(self.id) {
            return Ok(None);
        }
        let master = self.master_in(masters)?;
        if !actions.free_for_group(unread, master, ai, roles)? {
            return Ok(None);
        }
        Ok(Some(match group.member_reason(self.id) {
            // A group member's own move's reasons slot 0x5e0 tells apart (`FUN_141017590`): moved aside,
            // and none.
            MoveReason::Aside => GroupSpeed::Aside,
            MoveReason::None => GroupSpeed::Free,
            _ => GroupSpeed::OwnMove(group.motion.maximum_velocity_forward),
        }))
    }

    /// The unit's own `MoveTo` in its update (`0x140aa2a70`): its update, then a give-up, a moved
    /// target, and its end (done or failed), which posts the notify; its code (the end's 1 or 3,
    /// whose deletion lets the target go and stops the unit).
    pub(crate) fn move_to_part(
        &mut self,
        actions: &mut Action,
        context: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        if let Some(code) = self.own_move_held(actions, context)? {
            return Ok(HeadCode::of(code));
        }
        let step = self.own_move_step(actions, context, me, roles)?;
        if step.gave_up {
            self.own_move_give_up(actions, context, roles)?;
        }
        if step.target_moved {
            self.post_own(
                &mut context.world.parts.unit_ai,
                NotifyKind::TARGET_MOVED,
                [0, 0, 0],
            )?;
        }
        let ended = step.end != StepEnd::Going;
        if ended {
            self.own_move_end(context, step.end)?;
        }
        Ok(HeadCode::ended(ended))
    }

    /// [`Unit::move_to_part`]'s give-up ([`Unit::own_move_gave_up`]): a human player's villager
    /// waits, with its unit AI's stance (aggressive without one).
    fn own_move_give_up(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let villager = roles
            .of_class(self.master(step.world.master_tables(), step.player)?.class)
            .contains(ClassRoles::VILLAGER);
        let stance = step
            .world
            .parts
            .unit_ai
            .get(self.id)
            .map_or(UnitStance::Aggressive, |ai| ai.stance);
        self.own_move_gave_up(
            actions,
            step.world.parts.motion.layer_mut(self.id),
            if villager && !step.world.players.player(step.player)?.kind.is_computer() {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance,
            (
                OwnerMasters::new(
                    &step.world.parts.own_master,
                    step.world.model.masters.of(step.player),
                ),
                &step.world.model.graphics,
                step.world.streams.get_mut(Stream::Sprite),
            ),
            step.world.model.tuning.sprite,
        )
    }

    /// The own `MoveTo`'s first check of its target (`0x140aa2a70`, [`MoveToAction::check_held`])
    /// by the target's state as the world holds it: a target let go is released at once (slot
    /// 0x180 from slot 0x68: an own `MoveTo` is not quiet). Returns the update's code when it
    /// ends there (4: the list keeps it).
    ///
    /// [`MoveToAction::check_held`]: crate::movement::move_to::MoveToAction::check_held
    fn own_move_held(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<Option<UpdateCode>> {
        let Some(move_to) = actions.list.move_to_mut() else {
            return Ok(None);
        };
        let target = move_to.target;
        let state = step.world.object_state(target);
        let mut inbox = step.world.parts.unit_ai.inbox(self.id, self.life.state);
        let code = move_to.check_held(state, &mut inbox)?;
        let releases = code.is_some() && move_to.voice.is_loud();
        if releases && let Some(target) = target {
            let by = self.releaser(step)?;
            release(step, self, target, by)?;
        }
        Ok(code)
    }

    /// The own `MoveTo`'s update (`0x140aa2a70`): its state 3 asks the unit's AI for a path to
    /// its point (the goal, or the target's place); none without a `MoveTo`.
    fn own_move_step(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<MoveStep> {
        Ok(match actions.list.move_to_mut() {
            Some(move_to) => {
                // its state 3 reads the unit's AI for the path it asks (0x140aa2a70), with its
                // point: its own, or the target's place (a target gone fails the update first)
                let point = match move_to.target {
                    None => Some(move_to.point.ground()),
                    Some(id) => step
                        .world
                        .spatial
                        .get(id)
                        .map(|placed| placed.position.ground()),
                };
                let ask = match point {
                    Some(point) if move_to.state == MoveTo::Start => UnitAi::path_ask(
                        step.world.parts.unit_ai.get_mut(self.id),
                        self.place.position().ground(),
                        point,
                        &step.world.model.tuning.unit_ai,
                    ),
                    _ => PathAsk::default(),
                };
                // its set_state 1 and 13 show slot 0x798's sprite; slot 0x68 takes the master's
                // standing sprite for a null one
                let master = object_master(
                    step.world.parts.own_master.get(self.id),
                    (self.id, self.life.master_id),
                    step.world.model.masters.of(step.player),
                )
                .ok();
                let graphic = |id: Option<SpriteId>| {
                    id.and_then(|id| step.world.model.graphics.get(id).copied())
                };
                let look = step.world.parts.combat_of(self.id).abilities.look.as_ref();
                let standing =
                    graphic(master.and_then(|master| standing(master, look, Cargo::Empty, roles)));
                let master_standing = graphic(master.and_then(|master| master.standing_sprite));
                let (walk, rng) = step.walk_and_sprites(&mut self.place, me)?;
                let shows = Shows {
                    sprite: &mut self.sprite,
                    rng,
                    standing,
                    master_standing,
                };
                move_to.update(walk, ask, shows)?
            }
            None => MoveStep::default(),
        })
    }

    /// The own `MoveTo`'s end, done or `failed`: its update in state 1 posts its completion
    /// (0x1fa with the move action value; an own `MoveTo` is never quiet), in state 13 its
    /// failure (0x1f9) and returns 3.
    fn own_move_end(&self, step: &mut Step<'_>, end: StepEnd) -> GenieResult<()> {
        let notify = if end == StepEnd::Failed {
            NotifyKind::ACTION_FAILED
        } else {
            NotifyKind::ACTION_COMPLETED
        };
        self.post_own(
            &mut step.world.parts.unit_ai,
            notify,
            [AiAction::Move.value(), 0, 0],
        )
    }

    /// The unit's Explore in its update ([`crate::movement::explore::ExploreAction::update`]): nothing without one.
    pub(crate) fn explore_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<()> {
        let Some(mut explore) = actions.list.explore().copied() else {
            return Ok(());
        };
        let explored = explore.update(step, self, me, roles);
        if let Some(explore_action) = actions.list.explore_mut() {
            *explore_action = explore;
        }
        explored
    }

    /// A bird's Fly in its update ([`crate::movement::fly::Fly::update`]): nothing without one.
    pub(crate) fn fly_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
    ) -> GenieResult<()> {
        let Some(mut fly) = actions.list.fly().copied() else {
            return Ok(());
        };
        let flown = fly.update(step, self, me);
        if let Some(listed) = actions.list.fly_mut() {
            *listed = fly;
        }
        flown
    }
}
