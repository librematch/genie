//! A unit's update on its owner's turn, action by action.

use crate::GenieError;
use crate::combat::attack::run_down_attack_timers;
use crate::math::rng::Stream;
use crate::movement::{Me, Mover};
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::actions::Action;
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::{Action as LayersAction, Building};
use crate::objects::lifecycle::Class;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::object_master;
use crate::objects::unit::{Outcome, Unit, loses_actions};
use crate::ownership::capture::Captured;
use crate::unit_ai::task_finder::world::wait_progress;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::lets::torn_down;
use crate::world::others::OthersRef;
use crate::world::parts::PartTable;
use crate::world::step::Step;
use crate::world::{IN_THE_WAY, Unread};
use std::mem::take;

/// An action below the head after the head's update: its idle update (`FUN_140a78470`: a target
/// or second target past state 6 let go, its sub-actions' idle update) is not ported, and no list
/// of the slice holds two (every add starts from an empty list, the import takes one).
const BELOW_HEAD: &str = "an action below the head of the list (its idle update, FUN_140a78470)";

impl Unit {
    pub(crate) fn update(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        roles: &Roles,
        slots: Slots,
    ) -> Result<Outcome, GenieError> {
        let Some(leaves) = self.update_to_actions(actions, c, roles)? else {
            return Ok(Outcome::Sleep);
        };
        let captured = self.update_actions(actions, c, roles, slots)?;
        self.ai_part(actions, c, (captured, leaves), roles)
    }

    /// `RGE_Static_Object::update` and the combat object's steps before the actions: the death
    /// blast, the sleep ([`Unit::go_to_sleep`], None), the sprite's facets, a building's links
    /// to removed objects ([`Unit::let_removed_links_go`]), a gate's and a building's update,
    /// the attack timers, the motion, the life step (what the unit AI later reads of it), the
    /// animation and the actions past the unit's life.
    pub(crate) fn update_to_actions(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> Result<Option<Outcome>, GenieError> {
        self.death_blast(actions, c, roles)?;
        if self.go_to_sleep(
            c.w.parts.mover_of(self.id),
            c.w.parts.unit_ai.contains(self.id),
        )? {
            return Ok(None);
        }
        self.take_sprite_facets(c.w.parts.mover_mut(self.id));
        self.let_removed_links_go(c, roles)?;
        self.gate_update(actions, c, roles)?;
        self.complete_on_update(actions, c, roles)?;
        self.building_seconds(c, roles)?;
        // a new object's combat layer (none in the table) has no timer to run down
        if let Some(combat) = c.w.parts.combat.get_mut(self.id) {
            run_down_attack_timers(combat, c.dt.seconds(), c.w.world_time);
        }
        self.motion_step(actions, c, roles)?;
        let leaves = self.life_step(actions, c, roles)?;
        self.animate(c);
        self.actions_past_life(actions, c)?;
        Ok(Some(leaves))
    }

    /// `RGE_Action_Object::update` (`0x14106ac90`) after the moving update: an empty list in state
    /// 2 starts the master's default task, which waits for the next update; an object inside
    /// another (`+0x2f0`) runs its inside update (`FUN_140a98790`, [`Unit::inside_update`]);
    /// any other runs its list's update (`FUN_140a992b0`, [`Unit::update_head`]). Then what
    /// follows the actions; the Artifact's capture, which the unit AI's part reads.
    pub(crate) fn update_actions(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        roles: &Roles,
        slots: Slots,
    ) -> Result<Option<Captured>, GenieError> {
        let started = self.start_default_task(actions, c)?;
        let captured = if started {
            None
        } else if self.life.inside.is_some() {
            self.inside_update(actions, c)?;
            None
        } else {
            self.update_head(actions, c, roles, slots)?
        };
        self.after_actions(c)?;
        Ok(captured)
    }

    /// The list's update (`FUN_140a992b0`): its head's update (slot 0x30), by the head's kind,
    /// then what follows it ([`Unit::end_head_update`]). The Artifact's capture, if it found
    /// one.
    fn update_head(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        roles: &Roles,
        slots: Slots,
    ) -> Result<Option<Captured>, GenieError> {
        let me = self.me(actions);
        let mut captured = None;
        let code = self.head_part(actions, c, me, &mut captured, roles, slots)?;
        self.end_head_update(actions, c, code)?;
        Ok(captured)
    }

    /// The head's update (slot 0x30) by its kind, and its code; the Artifact's capture into
    /// `captured`.
    fn head_part(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        me: Me,
        captured: &mut Option<Captured>,
        roles: &Roles,
        slots: Slots,
    ) -> Result<HeadCode, GenieError> {
        match actions.list.head() {
            None => Ok(HeadCode::Keeps),
            Some(Action::Artifact(_)) => {
                self.artifact_part(actions, c, me, roles, slots)
                    .map(|found| {
                        *captured = found;
                        HeadCode::Keeps
                    })
            }
            Some(Action::Build(_)) => self.build_part(actions, c, me),
            Some(Action::Repair(_)) => self.repair_part(actions, c, me, roles),
            Some(Action::Enter(_)) => self.enter_part(actions, c, me, roles),
            Some(Action::Transport(_)) => self.transport_update(actions, c, me, roles),
            Some(Action::WaitSolidify(_)) => Ok(actions.wait_solidify_part(
                OthersRef::of(&c.w.players, &c.w.index, c.player)?,
                &c.w.parts.building,
            )),
            Some(Action::Missile(_)) => self.fly(actions, c, roles).map(|()| HeadCode::Keeps),
            Some(Action::MoveTo(_)) => self.move_to_part(actions, c, me, roles),
            Some(Action::Explore(_)) => self
                .explore_part(actions, c, me, roles)
                .map(|()| HeadCode::Keeps),
            Some(Action::Fly(_)) => self.fly_part(actions, c, me).map(|()| HeadCode::Keeps),
            Some(Action::Make(_)) => self.make_part(actions, c, roles),
            Some(Action::Gather(_)) => self.gather_update(actions, c, me, roles),
            Some(Action::Hunt(_)) => self.hunter_update(actions, c, me, roles),
            Some(Action::Attack(_)) => {
                let own = self.own_victim(actions, c, roles)?;
                self.attack_part(actions, c, me, own, roles)
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
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        code: HeadCode,
    ) -> Result<(), GenieError> {
        if actions.list.iter().nth(1).is_some() {
            return Err(GenieError::unreachable(BELOW_HEAD));
        }
        if code == HeadCode::Deletes {
            self.delete_own_head(actions, c)?;
        }
        Ok(())
    }

    /// The Hunt's update (`0x140ac7a00`): its targets' check ([`Unit::hunt_targets`]), then its
    /// part by state ([`Unit::hunt_part`]) with the hunter as a combat object, which its hits
    /// read and change.
    fn hunter_update(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        self.hunt_targets(actions, c)?;
        let mut own = self.own_victim(actions, c, roles)?;
        let tree_lost = self.tree_lost(actions, c, roles);
        self.hunt_part(actions, c, me, &mut own, tree_lost, roles)
    }

    /// `RGE_Static_Object::update` (`0x14118ad50`): an object flagged to sleep (`+0x643`) drops
    /// the flag, is marked asleep (`+0x641`) and moves to its player's sleeping list (player
    /// slots 0x2d8 and 0x2d0), and its update ends (true); only static objects (type 10) are
    /// flagged, so one with movement state (its `mover`) or a unit AI is not modelled.
    pub(crate) fn go_to_sleep(
        &mut self,
        mover: Option<&Mover>,
        has_ai: bool,
    ) -> Result<bool, GenieError> {
        if !self.life.goto_sleep {
            return Ok(false);
        }
        if self.life.class != Class::Static || mover.is_some() || has_ai {
            return Err(GenieError::unsupported(
                "an object above the static type goes to sleep",
            ));
        }
        self.life.goto_sleep = false;
        Ok(true)
    }

    /// The yaw and the facing read the current sprite's facets: the movement state (`mover`)
    /// takes them.
    pub(crate) const fn take_sprite_facets(&self, mover: Option<&mut Mover>) {
        if let (Some(m), Some(s)) = (mover, self.sprite) {
            m.sprite_facets = Some(SpriteFacets {
                count: s.graphic.facets,
                turns: s.graphic.flags.contains(SpriteFlags::TURNS),
            });
        }
    }

    /// The animated object's update (`0x141071880`): its sprite's clock at its animation speed,
    /// then its idle swap, which reads the loop this update's clock made.
    pub(crate) fn animate(&mut self, c: &mut Step<'_>) {
        if let (Some(sprite), Some(speed)) =
            (self.sprite.as_mut(), c.w.parts.anim_speed_of(self.id))
        {
            sprite.update(c.dt.seconds(), speed);
            if let Ok(master) = object_master(
                c.w.parts.own_master.get(self.id),
                (self.id, self.life.master_id),
                c.w.model.masters.of(c.player),
            ) {
                let (standing, second) = (master.standing_sprite, master.standing_sprite_2);
                self.idle_swap(
                    c.w.parts.motion_mut(self.id),
                    standing,
                    second,
                    &c.w.model.graphics,
                    c.w.streams.get(Stream::Sprite),
                );
            }
        }
    }

    /// `RGE_Action_Object::update` (`0x14106ac90`) after the moving update: an object past state
    /// 2 (the Wonder past state 5) loses its action list (`FUN_140a98620`). An alive object with
    /// an action the slice does not model fails here.
    pub(crate) fn actions_past_life(
        &mut self,
        actions: &mut LayersAction,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        if let Some(e) =
            c.w.parts
                .unread
                .get(self.id)
                .map(Unread::error)
                .filter(|_| self.life.state == ObjectState::Alive)
        {
            return Err(e);
        }
        if loses_actions(&self.life, &c.w.model.roles) {
            let by = self.releaser(c)?;
            let (mut releases, mut leaves) = (Vec::new(), Vec::new());
            self.clear_own_actions(
                actions,
                c.w.parts.motion.layer_mut(self.id),
                &mut releases,
                &mut leaves,
                by,
            )?;
            torn_down(c, self, (leaves, releases))?;
        }
        Ok(())
    }

    /// After the actions: the units its walk met take their pauses (slot 0x5f0: their walked
    /// level's pause; the walk read them from its own list till now, `Mover::pause_of`). A unit
    /// whose walk met others in its way is not modelled.
    pub(crate) fn after_actions(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let Some(pauses) = c.w.parts.mover_mut(self.id).map(|m| take(&mut m.pauses)) else {
            return Ok(());
        };
        for (id, pause) in pauses {
            if c.w.object(id).is_none() {
                continue;
            }
            let Some(level) = c.w.parts.mover_mut(id).and_then(|m| m.paths.first_mut()) else {
                continue;
            };
            level.pause_time = pause;
        }
        if c.w
            .parts
            .mover_of(self.id)
            .is_some_and(|m| !m.in_the_way.is_empty())
        {
            return Err(IN_THE_WAY);
        }
        Ok(())
    }

    /// [`Unit::ai_part`]'s first step: the pathing grid takes a unit of a master of kind 40 or
    /// more where it stands, moving or not.
    pub(crate) fn refresh_pathing(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let m = self.master(c.w.master_tables(), c.player)?;
        if MasterKind::ACTION <= m.kind
            && let Some(p) = c.w.spatial.get(self.id).copied()
        {
            let moving = c.w.parts.mover_of(self.id).is_some_and(Mover::is_moving);
            c.w.pathing.refresh(self.id, &p, moving, m.outline);
        }
        Ok(())
    }
}

impl LayersAction {
    /// The wait for a foundation in its update (`FUN_140ae8b70`) on its build points, none when
    /// the world does not hold it; its 1 deletes it (its destructor lets no target go).
    fn wait_solidify_part(
        &self,
        others: OthersRef<'_>,
        buildings: &PartTable<Building>,
    ) -> HeadCode {
        let ended = self
            .list
            .wait_solidify()
            .is_some_and(|w| w.update(wait_progress(w.target, others, buildings)));
        HeadCode::ended(ended)
    }
}
