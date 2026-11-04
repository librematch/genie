//! A repair's entry into its new state: the part of its `set_state` by the state.

use crate::GenieResult;
use crate::construction::RepairTargetId;
use crate::construction::repair::RepairState;
use crate::construction::repair::action::notify;
use crate::construction::repair::action::show;
use crate::construction::repair::action::standing;
use crate::construction::repair::{ONE, RepairAction, RepairReads, SHIP_REPAIR};
use crate::objects::unit::Unit;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::step::Step;

impl RepairAction {
    /// [`RepairAction::set_state`]'s part by the new state (`0x140ad9c60`'s switch).
    pub(super) fn enter(
        &mut self,
        step: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        match self.state {
            RepairState::Done => self.enter_done(step, repairer, reads)?,
            RepairState::Halted => standing(step, repairer)?,
            RepairState::Idle => {
                notify(step, repairer, NotifyKind::REQUIRES_SEARCH)?;
                standing(step, repairer)?;
            }
            RepairState::Move { .. } => self.enter_walk(step, repairer, reads, roles)?,
            RepairState::Face => {
                if let Some(t) = reads.target {
                    self.point = t.position;
                }
                show(step, repairer, self.task.moving, reads.standing)?;
            }
            RepairState::Work => {
                show(step, repairer, self.task.proceeding, reads.standing)?;
                if reads.counted_in.is_some() {
                    Self::count(step, reads, 1)?;
                    self.counted = true;
                }
            }
            RepairState::Wait => {
                standing(step, repairer)?;
                self.timer = Seconds::new(ONE);
                if reads.target.is_some_and(|repairee| {
                    roles
                        .of_class(repairee.class)
                        .contains(ClassRoles::REPAIRED_AT_SHORE)
                }) {
                    return Err(SHIP_REPAIR);
                }
            }
            RepairState::WalkPoint { .. } => self.enter_walk_point(step, repairer, roles)?,
            // 13 has no case in set_state
            RepairState::Failed => {}
        }
        Ok(())
    }

    /// `set_state(1)`'s case: the target goes, a loud repair tells its unit AI it completed,
    /// and the repairer shows its standing sprite.
    fn enter_done(
        &mut self,
        step: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
    ) -> GenieResult<()> {
        self.drop_target(step, repairer, reads)?;
        if self.voice.is_loud() {
            notify(step, repairer, NotifyKind::ACTION_COMPLETED)?;
        }
        standing(step, repairer)
    }

    /// `set_state(11)`'s case: the `MoveTo` to the action's point starts.
    fn enter_walk_point(
        &mut self,
        step: &Step<'_>,
        repairer: &Unit,
        roles: &Roles,
    ) -> GenieResult<()> {
        let move_reads = repairer.move_to_reads(
            step.w.parts.motion.mover(repairer.id),
            step.w.master_tables(),
            step.player,
            (
                &step.w.model.graphics,
                step.w.parts.combat_of(repairer.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        self.state = RepairState::WalkPoint {
            move_to: self.to_point(self.point, &move_reads, &step.w.model.tuning.move_to)?,
        };
        Ok(())
    }

    /// `set_state(4)`'s case: the target's place is the point, and the `MoveTo` to it starts
    /// ([`RepairAction::walk`], none for a target the world does not hold).
    ///
    /// # Errors
    ///
    /// A repairer without its master, a target whose master its table lacks, a sprite the table
    /// lacks.
    fn enter_walk(
        &mut self,
        step: &Step<'_>,
        repairer: &Unit,
        reads: &RepairReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        if let Some(repairee) = reads.target {
            self.point = repairee.position;
        }
        let move_reads = repairer.move_to_reads(
            step.w.parts.motion.mover(repairer.id),
            step.w.master_tables(),
            step.player,
            (
                &step.w.model.graphics,
                step.w.parts.combat_of(repairer.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let to = move_reads.walk_to(step.w, self.target.map(RepairTargetId::id))?;
        self.state = RepairState::Move {
            move_to: self.walk(&move_reads, to, &step.w.model.tuning.move_to)?,
        };
        Ok(())
    }
}
