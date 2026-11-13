//! The Enter's walks to a point ([`EnterAction`]): its walk to its target's place (state 11)
//! and its walk without a target.

use crate::GenieResult;
use crate::garrison::EnterAction;
use crate::garrison::EnterState;
use crate::map::Distance;
use crate::map::Point;
use crate::movement::move_to::MoveToAction;
use crate::movement::move_to::MoveToReads;
use crate::movement::move_to::TargetMoved;
use crate::world::layer::generated::MoveToTuning;

impl EnterAction {
    /// `set_state(11)` (`FUN_140a87320` case 11) once its target has no room for the unit (its
    /// sub-actions' teardown, which stops a walking unit, is the caller's): `FUN_140a85f30`,
    /// whose room test (slot 0x198) answers as the caller's, meets the target at its place
    /// `meet` (its code goes unread), and a quiet `MoveTo` (`FUN_140a7a170` with 1, `+0x61` 2)
    /// walks there within 0 at the action's height (`FUN_140aa0d30` to `+0x38`..`+0x40`, with the
    /// task's moving sprite), holding no target, as the list adds it (`FUN_140a977e0`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(crate) fn walk_to_point(
        &mut self,
        meet: Point,
        reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.point = self.point.with_x(meet.x()).with_y(meet.y());
        let walk = reads.to_point(
            self.point,
            Distance::ZERO,
            self.task.sprites.moving,
            move_to_tuning,
        )?;
        self.state = EnterState::WalkPoint {
            move_to: walk.map(ignoring_moves),
        };
        Ok(())
    }

    /// `set_state(4)` (`FUN_140a87320` case 4) of an Enter that holds no target (`+0x20` null;
    /// its sub-actions' teardown is the caller's): no room test and no enter. `FUN_140a85f30`
    /// meets nothing and leaves the point (`+0x38`..`+0x40`), and a quiet `MoveTo` (`+0x61` 2)
    /// walks there within 0 (`FUN_140aa0d30`, with the task's moving sprite); a point below 0
    /// ([`EnterAction::point_below_zero`]) makes one to no target instead (`FUN_140aa0df0`,
    /// [`MoveToReads::to_nothing`]). No transport moves.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(crate) fn walk_unheld(
        &mut self,
        reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        let sprite = self.task.sprites.moving;
        let walk = if self.point_below_zero() {
            reads.to_nothing(Distance::ZERO, sprite, move_to_tuning)?
        } else {
            reads.to_point(self.point, Distance::ZERO, sprite, move_to_tuning)?
        };
        self.state = EnterState::Walk {
            move_to: walk.map(ignoring_moves),
        };
        Ok(())
    }
}

/// `move_to` as an Enter makes it: it leaves a target that moves (`+0x61` 2,
/// [`TargetMoved::Ignores`]).
const fn ignoring_moves(move_to: MoveToAction) -> MoveToAction {
    MoveToAction {
        on_target_moved: TargetMoved::Ignores,
        ..move_to
    }
}
