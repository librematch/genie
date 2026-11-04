//! The Build's `MoveTo`s (`0x140aae200` cases 4, 8, 11 and 223) through [`MoveToReads`]: the one to
//! its foundation, and the sub-action list's update.

use crate::GenieResult;
use crate::construction::FoundationId;
use crate::construction::build::{BuildAction, BuildReads, BuildTask, SITE_GONE};
use crate::movement::Me;
use crate::movement::move_to::{ListUpdate, SubCode, update_sub_actions};
use crate::movement::move_to::{MoveToAction, MoveToReads, TargetMoved, WalkTo};
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;
use crate::world::step::Step;

impl BuildTask {
    /// The `MoveTo` of `set_state(4)` with this task (`0x140aae200` case 4: `FUN_140aa0df0` with
    /// the task's range `+0x30` and moving sprite `+0x40`) to the foundation `to`, or without
    /// one ([`MoveToReads::to_nothing`]), with what it reads of the builder (`move_reads`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn walk(
        &self,
        move_reads: &MoveToReads<'_>,
        to: Option<WalkTo>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Option<MoveToAction>> {
        to.map_or_else(
            || move_reads.to_nothing(self.range, self.moving, move_to_tuning),
            |to| {
                move_reads.to_object(
                    to,
                    self.range,
                    self.moving,
                    TargetMoved::Follows,
                    move_to_tuning,
                )
            },
        )
    }
}

impl BuildAction {
    /// The `MoveTo` of `set_state(4)` toward the Build's foundation (its place and the
    /// builder's footprint gap to it, its state as the world holds it now), or without one
    /// ([`BuildTask::walk`]).
    ///
    /// # Errors
    ///
    /// A builder without movement state or its master, a foundation without a place, a sprite
    /// the table lacks.
    pub(super) fn walk_to_foundation(
        &self,
        step: &Step<'_>,
        builder: &Unit,
        roles: &Roles,
    ) -> GenieResult<(Option<WalkTo>, Option<MoveToAction>)> {
        let move_reads = builder.move_to_reads(
            step.w.parts.motion.mover(builder.id),
            MasterTables::new(&step.w.model.masters, &step.w.parts.own_master),
            step.player,
            (
                &step.w.model.graphics,
                step.w.parts.combat_of(builder.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let to = if self.target.is_none() {
            None
        } else {
            Some(
                move_reads
                    .walk_to(step.w, self.target.map(FoundationId::id))?
                    .ok_or(SITE_GONE)?,
            )
        };
        Ok((
            to,
            self.task
                .walk(&move_reads, to, &step.w.model.tuning.move_to)?,
        ))
    }

    /// The sub-action list's update (`FUN_140a992b0`: its first action's, the quiet `MoveTo`'s,
    /// [`MoveToAction::sub_update`]) and its [`SubCode`]; [`SubCode::Done`] for an empty list
    /// ("There is no list."). Its start shows its sprite; the motion step gives up as
    /// [`BuildAction::gives_up`] says.
    ///
    /// # Errors
    ///
    /// The walk's and the notifies'.
    pub(super) fn step_walk(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        me: Me,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<SubCode> {
        let how = ListUpdate {
            gives_up: Self::gives_up(step, builder, reads, roles)?,
            ..ListUpdate::default()
        };
        match self.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, step, builder, me, how),
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, step, builder, me, how),
        }
    }
}
