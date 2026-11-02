//! The movement actions in a unit's update: the velocity step, the unit's own `MoveTo`, its
//! Explore and a bird's Fly.

use crate::GenieError;
use crate::groups::MoveReason;
use crate::groups::UnitGroups;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::PathAsk;
use crate::movement::Speed;
use crate::movement::move_to::MoveStep;
use crate::movement::move_to::{MoveTo, Shows};
use crate::movement::{GroupSpeed, Me};
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::sprite::standing;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::unit_ai::ActionKind;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::world::SPEED_EPSILON;
use crate::world::Unread;
use crate::world::lets::release;
use crate::world::slots::PlayerSlots;
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
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        // RGE_Moving_Object::update counts the last human order time down before its velocity
        // step
        if c.w.parts.mover_of(self.id).is_some() {
            self.count_down_human_order(c.dt);
        }
        // the velocity step (FUN_140f9ba80) runs for a unit with velocity data (+0x868)
        let group = match c
            .w
            .parts
            .mover_of(self.id)
            .and_then(|m| m.velocity.as_ref())
        {
            Some(_) => self.group_speed(
                (actions, c.w.parts.unread.get(self.id)),
                &c.w.groups,
                OwnerMasters::new(&c.w.parts.own_master, c.w.masters.of(c.player)),
                c.w.parts.unit_ai.get(self.id),
            )?,
            None => None,
        };
        if let Some(mover) = c.w.parts.mover_mut(self.id) {
            let here = self.place.position();
            let at = mover.step_velocity(
                here,
                c.dt.seconds(),
                c.w.spatial.map,
                c.w.spatial.terrain.as_ref(),
                (c.w.world_time, group),
            );
            if at != here {
                mover.new_position(
                    &mut self.place,
                    self.id,
                    at,
                    (&mut c.w.spatial, Some(&mut c.w.pathing)),
                )?;
            }
            // RGE_Moving_Object::update: a unit that moves animates at its speed (+0x7b8)
            if let Some(v) = mover.velocity.as_ref() {
                let [x, y, z] = v.velocity.get();
                let speed = sqrt(x * x + y * y + z * z);
                if SPEED_EPSILON < speed {
                    c.w.parts.set_anim_speed(self.id, Speed::new(speed));
                }
            }
        }
        Ok(())
    }

    /// What slot 0x5e0 (`FUN_14114f300`) reads of the unit's group ([`GroupSpeed`]) among the
    /// world's `groups`, the unit's master among `masters` and its unit AI `ai`, with its action
    /// layer `actions` and why the load could not read it whole `unread`: none outside a group the world holds (`+0x6d0`, `FUN_14152ff40`: none for −1), nor when the
    /// group's motion stands (`FUN_141046600`) and the unit is in its place
    /// ([`UnitGroup::slot_in_place`]), nor for a unit not free for the group's moves
    /// ([`Unit::free_for_group`]). Else its own move's reason (`FUN_141017590`): 2 moved aside,
    /// −1 none, any other an own move at the group's top speed (`+0xbc`).
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
    ) -> Result<Option<GroupSpeed>, GenieError> {
        let Some(g) = self.group.id.and_then(|id| groups.get(id)) else {
            return Ok(None);
        };
        if !g.motion.moving() && g.slot_in_place(self.id) {
            return Ok(None);
        }
        let master = self.master_in(masters)?;
        if !actions.free_for_group(unread, master, ai)? {
            return Ok(None);
        }
        Ok(Some(match g.member_reason(self.id) {
            // A group member's own move's reasons slot 0x5e0 tells apart (`FUN_141017590`): moved aside,
            // and none.
            MoveReason::ASIDE => GroupSpeed::Aside,
            MoveReason::NONE => GroupSpeed::Free,
            _ => GroupSpeed::OwnMove(g.motion.maximum_velocity_forward),
        }))
    }

    /// The unit's own `MoveTo` in its update (`0x140aa2a70`): its update, then a give-up, a moved
    /// target, and its end (done or failed), which posts the notify; its code (the end's 1 or 3,
    /// whose deletion lets the target go and stops the unit).
    pub(crate) fn move_to_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<HeadCode, GenieError> {
        if let Some(code) = self.own_move_held(actions, c)? {
            return Ok(HeadCode::of(code));
        }
        let step = self.own_move_step(actions, c, me)?;
        if step.gave_up {
            self.own_move_give_up(actions, c)?;
        }
        if step.target_moved {
            self.post_own(&mut c.w.parts.unit_ai, NotifyKind::TARGET_MOVED, [0, 0, 0])?;
        }
        let ended = step.done || step.failed;
        if ended {
            self.own_move_end(c, step.failed)?;
        }
        Ok(HeadCode::ended(ended))
    }

    /// [`Unit::move_to_part`]'s give-up ([`Unit::own_move_gave_up`]): a human player's villager
    /// waits, with its unit AI's stance (aggressive without one).
    fn own_move_give_up(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let villager = self.master(c.w.master_tables(), c.player)?.class == class::VILLAGER;
        let stance =
            c.w.parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::AGGRESSIVE, |ai| ai.stance);
        self.own_move_gave_up(
            actions,
            c.w.parts.motion.layer_mut(self.id),
            villager && !c.w.players.player(c.player)?.kind.is_computer(),
            stance,
            (
                OwnerMasters::new(&c.w.parts.own_master, c.w.masters.of(c.player)),
                &c.w.graphics,
                c.w.streams.get(Stream::Sprite),
            ),
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
        c: &mut Step<'_>,
    ) -> Result<Option<UpdateCode>, GenieError> {
        let Some(move_to) = actions.list.move_to_mut() else {
            return Ok(None);
        };
        let target = move_to.target;
        let state = c.w.object_state(target);
        let mut inbox = c.w.parts.unit_ai.inbox(self.id, self.life.state);
        let code = move_to.check_held(state, &mut inbox)?;
        let releases = code.is_some() && move_to.voice.is_loud();
        if releases && let Some(target) = target {
            let by = self.releaser(c)?;
            release(c, self, target, by)?;
        }
        Ok(code)
    }

    /// The own `MoveTo`'s update (`0x140aa2a70`): its state 3 asks the unit's AI for a path to
    /// its point (the goal, or the target's place); none without a `MoveTo`.
    fn own_move_step(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<MoveStep, GenieError> {
        Ok(match actions.list.move_to_mut() {
            Some(move_to) => {
                // its state 3 reads the unit's AI for the path it asks (0x140aa2a70), with its
                // point: its own, or the target's place (a target gone fails the update first)
                let point = match move_to.target {
                    None => Some(move_to.point.ground()),
                    Some(id) => c.w.spatial.get(id).map(|t| t.position.ground()),
                };
                let ask = match point {
                    Some(p) if move_to.state == MoveTo::Start => UnitAi::path_ask(
                        c.w.parts.unit_ai.get_mut(self.id),
                        self.place.position().ground(),
                        p,
                    ),
                    _ => PathAsk::default(),
                };
                // its set_state 1 and 13 show slot 0x798's sprite; slot 0x68 takes the master's
                // standing sprite for a null one
                let master = object_master(
                    c.w.parts.own_master.get(self.id),
                    (self.id, self.life.master_id),
                    c.w.masters.of(c.player),
                )
                .ok();
                let graphic =
                    |id: Option<SpriteId>| id.and_then(|id| c.w.graphics.get(id).copied());
                let look = c.w.parts.combat_of(self.id).abilities.look.as_ref();
                let standing = graphic(master.and_then(|m| standing(m, look, false)));
                let master_standing = graphic(master.and_then(|m| m.standing_sprite));
                let (walk, rng) = c.walk_and_sprites(&mut self.place, me)?;
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
    fn own_move_end(&self, c: &mut Step<'_>, failed: bool) -> Result<(), GenieError> {
        let notify = if failed {
            NotifyKind::ACTION_FAILED
        } else {
            NotifyKind::ACTION_COMPLETED
        };
        self.post_own(
            &mut c.w.parts.unit_ai,
            notify,
            [ActionKind::Move.value(), 0, 0],
        )
    }

    /// The unit's Explore in its update ([`crate::movement::explore::ExploreAction::update`]): nothing without one.
    pub(crate) fn explore_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<(), GenieError> {
        let Some(mut explore) = actions.list.explore().copied() else {
            return Ok(());
        };
        let explored = explore.update(c, self, me);
        if let Some(e) = actions.list.explore_mut() {
            *e = explore;
        }
        explored
    }

    /// A bird's Fly in its update ([`crate::movement::fly::Fly::update`]): nothing without one.
    pub(crate) fn fly_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<(), GenieError> {
        let Some(mut fly) = actions.list.fly().copied() else {
            return Ok(());
        };
        let flown = fly.update(c, self, me);
        if let Some(f) = actions.list.fly_mut() {
            *f = fly;
        }
        flown
    }
}
