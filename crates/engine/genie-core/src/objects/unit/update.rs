//! A unit's update on its owner's turn, action by action.

use crate::GenieError;
use crate::math::rng::Stream;
use crate::movement::{Me, Mover};
use crate::objects::ActionState;
use crate::objects::ObjectState;
use crate::objects::actions::Action;
use crate::objects::lifecycle::Class;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::master_type::ACTION;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::object_master;
use crate::objects::unit::{Outcome, Unit, loses_actions};
use crate::unit_ai::task_finder::world::wait_progress;
use crate::world::others::OthersRef;
use crate::world::step::Step;

/// An action below the head after the head's update: its idle update (`FUN_140a78470`: a target
/// or second target past state 6 let go, its sub-actions' idle update) is not ported, and no list
/// of the slice holds two (every add starts from an empty list, the import takes one).
const BELOW_HEAD: &str = "an action below the head of the list (its idle update, FUN_140a78470)";

impl Unit {
    pub(crate) fn update(&mut self, c: &mut Step<'_>) -> Result<Outcome, GenieError> {
        let Some(leaves) = self.update_to_actions(c)? else {
            return Ok(Outcome::Sleep);
        };
        let captured = self.update_actions(c)?;
        self.ai_part(c, (captured, leaves))
    }

    /// `RGE_Static_Object::update` and the combat object's steps before the actions: the death
    /// blast, the sleep ([`Unit::go_to_sleep`], None), the sprite's facets, a gate's and a
    /// building's update, the attack timers, the motion, the life step (what
    /// the unit AI later reads of it), the animation and the actions past the unit's life.
    pub(crate) fn update_to_actions(
        &mut self,
        c: &mut Step<'_>,
    ) -> Result<Option<Outcome>, GenieError> {
        self.death_blast(c)?;
        if self.go_to_sleep()? {
            return Ok(None);
        }
        self.take_sprite_facets();
        self.gate_update(c)?;
        self.complete_on_update(c)?;
        self.building_seconds(c)?;
        self.run_down_attack_timers(c);
        self.motion_step(c)?;
        let leaves = self.life_step(c)?;
        self.animate(c);
        self.actions_past_life(c)?;
        Ok(Some(leaves))
    }

    /// `RGE_Action_Object::update` (`0x14106ac90`) after the moving update: an empty list in state
    /// 2 starts the master's default task, which waits for the next update; an object inside
    /// another (`+0x2f0`) runs its inside update (`FUN_140a98790`, [`Unit::inside_update`]);
    /// any other runs its list's update (`FUN_140a992b0`, [`Unit::update_head`]). Then what
    /// follows the actions; the Artifact's capture, which the unit AI's part reads.
    pub(crate) fn update_actions(
        &mut self,
        c: &mut Step<'_>,
    ) -> Result<Option<crate::ownership::capture::Captured>, GenieError> {
        let started = self.start_default_task(c)?;
        let captured = if started {
            None
        } else if self.life.inside.is_some() {
            self.inside_update(c)?;
            None
        } else {
            self.update_head(c)?
        };
        self.after_actions(c)?;
        Ok(captured)
    }

    /// The list's update (`FUN_140a992b0`): its head's update (slot 0x30), by the head's kind,
    /// then what follows it ([`Unit::end_head_update`]). The Artifact's capture, if it found
    /// one.
    fn update_head(
        &mut self,
        c: &mut Step<'_>,
    ) -> Result<Option<crate::ownership::capture::Captured>, GenieError> {
        let me = Me {
            id: self.id,
            group: self.group.id,
            action_state: self
                .top_action()
                .map_or(ActionState::NO_ACTION, |a| a.state),
        };
        let mut captured = None;
        let code = self.head_part(c, me, &mut captured)?;
        self.end_head_update(c, code)?;
        Ok(captured)
    }

    /// The head's update (slot 0x30) by its kind, and its code; the Artifact's capture into
    /// `captured`.
    fn head_part(
        &mut self,
        c: &mut Step<'_>,
        me: Me,
        captured: &mut Option<crate::ownership::capture::Captured>,
    ) -> Result<HeadCode, GenieError> {
        match self.actions.head() {
            None => Ok(HeadCode::Keeps),
            Some(Action::Artifact(_)) => self.artifact_part(c, me).map(|found| {
                *captured = found;
                HeadCode::Keeps
            }),
            Some(Action::Build(_)) => self.build_part(c, me),
            Some(Action::Repair(_)) => self.repair_part(c, me),
            Some(Action::Enter(_)) => self.enter_part(c, me),
            Some(Action::Transport(_)) => self.transport_update(c, me),
            Some(Action::WaitSolidify(_)) => {
                Ok(self.wait_solidify_part(OthersRef::of(&c.w.players, c.player)?))
            }
            Some(Action::Missile(_)) => self.fly(c).map(|()| HeadCode::Keeps),
            Some(Action::MoveTo(_)) => self.move_to_part(c, me),
            Some(Action::Explore(_)) => self.explore_part(c, me).map(|()| HeadCode::Keeps),
            Some(Action::Fly(_)) => self.fly_part(c, me).map(|()| HeadCode::Keeps),
            Some(Action::Make(_)) => self.make_part(c),
            Some(Action::Gather(_)) => self.gather_update(c, me),
            Some(Action::Hunt(_)) => self.hunter_update(c, me),
            Some(Action::Attack(_)) => {
                let own = self.own_victim(c)?;
                self.attack_part(c, me, own)
            }
        }
    }

    /// What follows the head's update (its `code`): the idle update of the actions below it
    /// ([`BELOW_HEAD`]), then `RGE_Action_Object::update`'s deletion of a head whose code is 1 or
    /// 3 ([`Unit::delete_own_head`]).
    ///
    /// # Errors
    ///
    /// An action below the head, and the deletion's.
    pub(crate) fn end_head_update(
        &mut self,
        c: &mut Step<'_>,
        code: HeadCode,
    ) -> Result<(), GenieError> {
        if self.actions.iter().nth(1).is_some() {
            return Err(GenieError::unreachable(BELOW_HEAD));
        }
        if code == HeadCode::Deletes {
            self.delete_own_head(c)?;
        }
        Ok(())
    }

    /// The wait for a foundation in its update (`FUN_140ae8b70`) on its build points, none when
    /// the world does not hold it; its 1 deletes it (its destructor lets no target go).
    fn wait_solidify_part(&self, others: OthersRef<'_>) -> HeadCode {
        let ended = self
            .actions
            .wait_solidify()
            .is_some_and(|w| w.update(wait_progress(w.target, others)));
        HeadCode::ended(ended)
    }

    /// The Hunt's update (`0x140ac7a00`): its targets' check ([`Unit::hunt_targets`]), then its
    /// part by state ([`Unit::hunt_part`]) with the hunter as a combat object, which its hits
    /// read and change.
    fn hunter_update(&mut self, c: &mut Step<'_>, me: Me) -> Result<HeadCode, GenieError> {
        self.hunt_targets(c)?;
        let mut own = self.own_victim(c)?;
        let tree_lost = self.tree_lost(c);
        self.hunt_part(c, me, &mut own, tree_lost)
    }

    /// `RGE_Static_Object::update` (`0x14118ad50`): an object flagged to sleep (`+0x643`) drops
    /// the flag, is marked asleep (`+0x641`) and moves to its player's sleeping list (player
    /// slots 0x2d8 and 0x2d0), and its update ends (true); only static objects (type 10) are
    /// flagged, so one with movement state or a unit AI is not modelled.
    pub(crate) fn go_to_sleep(&mut self) -> Result<bool, GenieError> {
        if !self.life.goto_sleep {
            return Ok(false);
        }
        if self.life.class != Class::Static || self.body.mover().is_some() || self.ai.is_some() {
            return Err(GenieError::unsupported(
                "an object above the static type goes to sleep",
            ));
        }
        self.life.goto_sleep = false;
        Ok(true)
    }

    /// The yaw and the facing read the current sprite's facets: the movement state takes them.
    pub(crate) const fn take_sprite_facets(&mut self) {
        if let (Some(m), Some(s)) = (self.body.mover_mut(), self.sprite) {
            m.sprite_facets = Some(crate::objects::facing::SpriteFacets {
                count: s.graphic.facets,
                flag_1: s.graphic.flags & 2 != 0,
            });
        }
    }

    /// The animated object's update (`0x141071880`): its sprite's clock at its animation speed,
    /// then its idle swap, which reads the loop this update's clock made.
    pub(crate) fn animate(&mut self, c: &mut Step<'_>) {
        if let (Some(sprite), Some(speed)) = (self.sprite.as_mut(), self.body.anim_speed()) {
            sprite.update(c.dt.seconds(), speed);
            if let Ok(master) = object_master(
                self.own_master.as_deref(),
                (self.id, self.life.master_id),
                c.w.masters.of(c.player),
            ) {
                let (standing, second) = (master.standing_sprite, master.standing_sprite_2);
                self.idle_swap(
                    standing,
                    second,
                    &c.w.graphics,
                    c.w.streams.get(Stream::Sprite),
                );
            }
        }
    }

    /// `RGE_Action_Object::update` (`0x14106ac90`) after the moving update: an object past state
    /// 2 (the Wonder past state 5) loses its action list (`FUN_140a98620`). An alive object with
    /// an action the slice does not model fails here.
    pub(crate) fn actions_past_life(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        if let Some(e) = self
            .unmodelled_action
            .clone()
            .filter(|_| self.life.state == ObjectState::Alive)
        {
            return Err(e);
        }
        if loses_actions(&self.life) {
            let by = self.releaser(c)?;
            let (mut releases, mut leaves) = (Vec::new(), Vec::new());
            self.clear_own_actions(&mut releases, &mut leaves, by)?;
            crate::world::lets::torn_down(c, self, (leaves, releases))?;
        }
        Ok(())
    }

    /// After the actions: the units its walk met take their pauses (slot 0x5f0: the walked
    /// level's pause and their gait, in the world; the walk read them from its own list till
    /// now, `Mover::pause_of`). A unit whose walk met others in its way is not modelled.
    pub(crate) fn after_actions(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        if let Some(m) = self.body.mover_mut() {
            for (id, pause) in std::mem::take(&mut m.pauses) {
                let Some((_, u)) = c.w.object_mut(id) else {
                    continue;
                };
                let Some(level) = u.body.mover_mut().and_then(|m| m.paths.first_mut()) else {
                    continue;
                };
                level.pause_time = pause;
                let gait = u.gait();
                c.w.spatial.set_gait(id, gait);
            }
            if !m.in_the_way.is_empty() {
                return Err(crate::world::IN_THE_WAY);
            }
        }
        Ok(())
    }

    /// [`Unit::ai_part`]'s first step: the pathing grid takes a unit of a master of kind 40 or
    /// more where it stands, moving or not.
    pub(crate) fn refresh_pathing(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let m = self.master(&c.w.masters, c.player)?;
        if ACTION <= m.kind
            && let Some(p) = c.w.spatial.get(self.id).copied()
        {
            let moving = self.body.mover().is_some_and(Mover::is_moving);
            c.w.pathing.refresh(self.id, &p, moving, m.outline);
        }
        Ok(())
    }
}
