//! The Build action's start (state 4 toward the foundation) and its update, which runs the
//! action's state; the AI's build orders whose foundation is gone.

use crate::combat::attack::Aim;
use crate::combat::attack::face_target;
use crate::construction::FoundationId;
use crate::construction::build::BuildState;
use crate::construction::build::{BUILDER_UNMOVING, BuildAction, BuildReads, BuildTask};
use crate::map::Position;
use crate::movement::Me;
use crate::movement::move_to::{GiveUp, MoveToAction, MoveToReads, WalkTo};
use crate::movement::move_to::{Patience, SubCode};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::lifecycle::Held;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{AiAction, NotifyKind};
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;
use crate::world::lets::release;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl BuildAction {
    /// The action's start (slot 5, `0x140aadd80`) on the foundation `to`: state 4, whose
    /// `set_state` (`0x140aae200`) takes the foundation's place as the point and makes the
    /// `MoveTo` to it ([`BuildTask::walk`] with what it reads of the builder, `move_reads`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn start(
        to: WalkTo,
        task: BuildTask,
        move_reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Self> {
        Ok(Self {
            target: to.id.map(FoundationId::new),
            state: BuildState::Move {
                move_to: task.walk(move_reads, Some(to), move_to_tuning)?,
            },
            point: to.at,
            task,
            added: false,
            timer: Seconds::ZERO,
            site: None,
        })
    }

    /// `set_state(4)` (and slot 0x48 or 0x68 before it) on `target` at `at`, with the `MoveTo`
    /// its case 4 makes ([`BuildTask::walk`]); the second target and the timer stay.
    #[must_use]
    pub(super) const fn restarted(
        &self,
        target: Option<FoundationId>,
        at: Position,
        move_to: Option<MoveToAction>,
    ) -> Self {
        Self {
            target,
            state: BuildState::Move { move_to },
            point: at,
            task: self.task,
            added: false,
            timer: self.timer,
            site: self.site,
        }
    }

    /// One update (slot 6, `0x140ab0370`) of `builder` (`me`) through its step, with what it
    /// reads as it starts (`reads`). Before its state's case it checks its foundation: one the
    /// world lost ([`BuildAction::lost_foundation`]), a human player's pasture already built
    /// ([`BuildAction::pasture_built`]), and one no longer under construction
    /// ([`BuildAction::foundation_done`]); then the end of the work (224), the drop-off (8) and
    /// the state's case ([`BuildAction::update_state`]); the walking states update their
    /// sub-action list ([`BuildAction::step_walk`]).
    /// Each effect on the builder and the world happens where the game makes it; the code is
    /// the update's (its 1 and 3: the action list deletes the Build).
    ///
    /// # Errors
    ///
    /// What the states, the walks and the work do not model.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        me: Me,
        reads: &BuildReads,
    ) -> GenieResult<HeadCode> {
        let roles = &c.w.model.roles.clone();
        let slots = c.w.model.slots;
        // state 3 without a target: the update does nothing (0x140ab0370's default case)
        if self.state == BuildState::Idle && self.target.is_none() {
            return Ok(HeadCode::Keeps);
        }
        if self.lost_foundation(c, reads) {
            self.idle(c, builder, reads)?;
            return Ok(HeadCode::Keeps);
        }
        // the second target's id names no object of the world: FUN_140a7a190 with none, whose
        // release finds nothing to call
        if self.site.is_some() && c.w.spatial.get(self.site).is_none() {
            self.site = None;
        }
        if reads.site.is_some() {
            if self.pasture_built(c, builder, reads)? {
                return Ok(HeadCode::Deletes);
            }
            self.foundation_done(c, builder, reads, roles)?;
        }
        if self.state == BuildState::Finished {
            return self.after_work(c, builder, actions, reads, roles);
        }
        if matches!(self.state, BuildState::Drop { .. }) {
            return self.update_drop(c, builder, me, reads, roles, slots);
        }
        self.update_state(c, builder, actions, me, reads, roles)
    }

    /// The target's id names no object of the world (`FUN_1415417a0`; the builder's player
    /// does not hold it either, `reads`): slot 0x68 with none (its builders' count went with
    /// it); then a Build in state 2 or 8 goes on, any other waits (true: the caller's
    /// `set_state(3)`, and the update returns 0).
    fn lost_foundation(&mut self, c: &Step<'_>, reads: &BuildReads) -> bool {
        if self.target.is_none() || reads.site.is_some() || c.w.spatial.get(self.target).is_some() {
            return false;
        }
        self.added = false;
        self.target = None;
        !matches!(self.state, BuildState::Halted | BuildState::Drop { .. })
    }

    /// A human player's builder of a pasture (master 1889) already built: its unit AI has no
    /// order and no action (`FUN_140c64b60`, `FUN_140c64b50` with −1), the unit tells it (notify
    /// 0x21d, the build action's value and the pasture the stack unit made, `FUN_1411c5f70`; 0
    /// when the slice does not hold it), the foundation goes (slot 0x68 with none: out of the
    /// builders, the foundation let go), the load goes to the player (`FUN_140aadc50`: a kind
    /// the player has, the foods as food) and the Build ends (`set_state(1)`); the update
    /// returns 1. True when it ended.
    fn pasture_built(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> GenieResult<bool> {
        let player_kind = c.w.players.player(c.player)?.kind;
        let site = Self::site_unit(c, reads)?;
        let pasture = match site {
            Some(u) => {
                c.w.model
                    .roles
                    .of_master(u.master(c.w.master_tables(), c.player)?.id)
                    .contains(MasterRoles::PASTURE_1889)
            }
            None => false,
        };
        let built = site.is_some_and(|u| u.life.state != ObjectState::Foundation);
        if player_kind.is_computer() || !pasture || !built {
            return Ok(false);
        }
        let Some(ai) = c.w.parts.unit_ai.get_mut(builder.id) else {
            return Ok(false);
        };
        ai.end_order_and_action()?;
        builder.post_own(
            &mut c.w.parts.unit_ai,
            NotifyKind::PASTURE_BUILT,
            [
                AiAction::Build.value(),
                // the notify's record holds the stack unit's id, 0 for none
                reads.stack_made.map_or(0, ObjectId::get),
                0,
            ],
        )?;
        self.uncount(c)?;
        if self.target.is_some() {
            let target = self.target.take();
            release(c, builder, target, reads.by)?;
        }
        let Held { kind, amount } = builder.life.held();
        let attributes = c.w.players.player(c.player)?.attributes.len();
        if let Some(kind) = kind.filter(|k| k.index() < attributes) {
            // the foods a unit carries (15 to 17) as food
            let kind = if kind.is_food_load() {
                c.w.model.slots.attribute.food
            } else {
                kind
            };
            Self::drop_load(c, builder, kind, amount)?;
        }
        self.end(c, builder, BuildState::Done)?;
        Ok(true)
    }

    /// A foundation no longer under construction (a state above 0) unless the Build already
    /// ended (224, 4, 8): master 621 and the gates (class 39) end the work once built
    /// (`set_state(224)`), as any other below state 3 does; else `set_state(4)` toward it, then
    /// slot 0x68 with none lets it go (its `MoveTo` keeps walking to it unless it refused it).
    fn foundation_done(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        let Some(site) = Self::site_unit(c, reads)? else {
            return Ok(());
        };
        let state = site.life.state;
        if state == ObjectState::Foundation
            || matches!(
                self.state,
                BuildState::Finished | BuildState::Move { .. } | BuildState::Drop { .. }
            )
        {
            return Ok(());
        }
        let master = site.master(c.w.master_tables(), c.player)?;
        let special = roles
            .of_master(master.id)
            .contains(MasterRoles::TOWN_CENTRE_621)
            || roles.of_class(master.class).contains(ClassRoles::GATE);
        let unbuilt =
            c.w.parts
                .building_of(site.id)
                .construction
                .is_some_and(|k| k.progress < Seconds::of_master(k.build_time));
        if (special || state < ObjectState::Dying)
            && !(special && ObjectState::Alive < state && unbuilt)
        {
            return self.finish(c, builder, reads);
        }
        self.restart(c, builder, roles)?;
        let target = self.target.take();
        release(c, builder, target, reads.by)?;
        Ok(())
    }

    /// The update's case by the state, and the update's code.
    fn update_state(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        me: Me,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        match self.state {
            // case 0: the update returns 1, the action list removes it
            BuildState::Done => return Ok(HeadCode::Deletes),
            // case 1: set_state(13); the update returns 0
            BuildState::Halted => self.end(c, builder, BuildState::Failed)?,
            BuildState::Move { .. } | BuildState::Clear { .. } => {
                self.walking(c, builder, me, reads, roles)?;
            }
            BuildState::Work => self.face_and_place(c, builder, actions, reads, roles)?,
            BuildState::Place => self.work(c, builder, actions, reads)?,
            BuildState::WalkPoint { .. } => self.walking_to_point(c, builder, me, reads, roles)?,
            // 13 (`0xd`): the Build failed; its next update tells its unit AI and halts it
            BuildState::Failed => return self.failed(c, builder, reads),
            BuildState::Wait => self.wait_out(c, builder, actions, reads, roles)?,
            // the default case (3 with a target, and the bytes without a state, which no step
            // sets): the update returns 0; 8 and 224 returned before the switch
            BuildState::Idle | BuildState::Drop { .. } | BuildState::Finished => {}
        }
        Ok(HeadCode::Keeps)
    }

    /// Cases 3 and 0xde, the walks to the foundation (4) and off its footprint (223), by the
    /// sub-action list's code ([`BuildAction::step_walk`]): its end (1; the game's 2, which only
    /// an Attack gives, too) ends the walk ([`BuildAction::walk_ended`]), 3 failed it
    /// ([`BuildAction::walk_failed`]), its target let go (5; the game's 4 too, which only a loud
    /// `MoveTo` gives: [`SubCode`]) lets the foundation go and walks to the point (slot 0x68
    /// with none, then `set_state(11)`: [`BuildAction::lose_foundation`]); 0 (it walks or
    /// waits) leaves the update's 0.
    ///
    /// # Errors
    ///
    /// The walk's and the next states'.
    fn walking(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        me: Me,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        match self.step_walk(c, builder, me, reads, roles)? {
            SubCode::Done => self.walk_ended(c, builder, reads, roles),
            SubCode::Failed => self.walk_failed(c, builder, reads, roles),
            SubCode::TargetGone => self.lose_foundation(c, builder, reads, roles),
            SubCode::UnderWay => Ok(()),
        }
    }

    /// Case 10, the walk to the point (11), by the sub-action list's code: at its end (1),
    /// failed (3) or its target let go (5) the Build waits (`set_state(3)`,
    /// [`BuildAction::idle`]); 0 (it walks or waits) leaves the update's 0. The game's arm for
    /// 4 (the foundation let go, the walk to the point again) never runs: only a loud `MoveTo`
    /// gives 4 ([`SubCode`]).
    ///
    /// # Errors
    ///
    /// The walk's and the next states'.
    fn walking_to_point(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        me: Me,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        match self.step_walk(c, builder, me, reads, roles)? {
            SubCode::Done | SubCode::Failed | SubCode::TargetGone => self.idle(c, builder, reads),
            SubCode::UnderWay => Ok(()),
        }
    }

    /// Case 0xc: the unit's slot 0x328 posts notify 0x1f9 (the Build's action value, 0, 0) to its
    /// unit AI; `set_state(2)` ([`BuildAction::halt_in`]: out of the builders, the `MoveTo`'s
    /// teardown, the unit's standing sprite, the targets let go) and the update returns 3: the
    /// action list removes the Build (`FUN_140a98e20`), as for 1.
    ///
    /// # Errors
    ///
    /// The notify's and the halt's.
    fn failed(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> GenieResult<HeadCode> {
        builder.post_own(
            &mut c.w.parts.unit_ai,
            NotifyKind::ACTION_FAILED,
            [AiAction::Build.value(), 0, 0],
        )?;
        self.halt_in(c, builder, reads)?;
        Ok(HeadCode::Deletes)
    }

    /// Case 0xdd: the wait runs down by the world's `dt`, then `set_state(7)` again
    /// ([`BuildAction::place`]); the update returns 0 either way.
    ///
    /// # Errors
    ///
    /// The placing's.
    fn wait_out(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.timer -= c.dt.seconds();
        if Seconds::ZERO < self.timer {
            return Ok(());
        }
        self.timer = Seconds::ZERO;
        self.place(c, builder, actions, reads, roles)
    }

    /// What the `MoveTo`'s answer to the motion step giving up reads ([`GiveUp`]): a human
    /// player's villager waits (by its unit AI's stance).
    pub(super) fn gives_up(
        c: &Step<'_>,
        builder: &Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<GiveUp> {
        let player_kind = c.w.players.player(c.player)?.kind;
        Ok(GiveUp {
            patience: if roles
                .of_some_class(reads.class)
                .contains(ClassRoles::VILLAGER)
                && !player_kind.is_computer()
            {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: c
                .w
                .parts
                .unit_ai
                .get(builder.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
        })
    }

    /// Case 5: `FUN_141162db0` turns the builder to face its foundation (a builder that turns over
    /// time is not modelled), then `set_state(7)` places it ([`BuildAction::place`]).
    fn face_and_place(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        let at = builder.position();
        let to =
            c.w.spatial
                .get_known(self.target)?
                .ok_or(GenieError::unsupported(
                    "a builder's foundation without a place",
                ))?
                .position;
        let now = c.w.world_time;
        let mover = c.w.parts.mover_mut(builder.id).ok_or(BUILDER_UNMOVING)?;
        if face_target(mover, at, to, now)? == Aim::Turning {
            return Err(GenieError::unsupported(
                "a builder that turns over time to face its foundation",
            ));
        }
        self.place(c, builder, actions, reads, roles)
    }
}

/// `FUN_140c5dcd0`: the build orders (`0x2be`) of `ai`'s queue whose foundation is gone or no
/// longer in state 0 leave it; `site` is the Build's own foundation with its state now, the
/// others' states are as the update started (`build_targets`, None for one that is gone).
pub(super) fn drop_done_orders(
    ai: &mut UnitAi,
    site: Option<(ObjectId, ObjectState)>,
    build_targets: &[(Option<ObjectId>, Option<ObjectState>)],
) {
    ai.orders.retain(|o| {
        if o.order != OrderKind::Build {
            return true;
        }
        let state = match site {
            Some((id, state)) if Some(id) == o.target => Some(state),
            _ => build_targets
                .iter()
                .find(|&&(id, _)| id == o.target)
                .and_then(|&(_, s)| s),
        };
        state == Some(ObjectState::Foundation)
    });
}
