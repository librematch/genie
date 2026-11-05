//! The `MoveTo`s the Hunt's `set_state` makes (`0x140ac60e0`) and the update of its sub-action
//! list (`FUN_140a992b0`).

use crate::GenieResult;
use crate::gathering::gather::Walks;
use crate::gathering::hunt::{HuntAction, HuntReads};
use crate::map::Distance;
use crate::movement::move_to::{GiveUp, MoveTo, MoveToAction, SpriteReads, TargetMoved, WalkTo};
use crate::movement::move_to::{ListUpdate, SubCode, update_sub_actions};
use crate::movement::{Me, Mover};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::unit::Unit;
use crate::world::layer::generated::MoveToTuning;
use crate::world::step::Step;

impl HuntReads {
    /// What the `MoveTo`s the update makes read of `unit` with its mover, with the sprites'
    /// table `graphics`.
    pub(crate) fn walks<'graphics>(
        &self,
        graphics: &'graphics Graphics,
        (unit, mover): (&Unit, Option<&Mover>),
    ) -> Walks<'graphics> {
        let sprites = SpriteReads {
            defaults: self.defaults,
            graphics,
        };
        Walks::of(sprites, (unit, mover, self.attack.top_speed))
    }
}

impl HuntAction {
    /// The quiet `MoveTo` to the object `to` within `range` (`FUN_140aa0df0` with the
    /// task's walking sprite, `+0x61` 1: it asks again when its target moves; `FUN_140a7a170`
    /// with 1), as the action list adds and starts it ([`MoveToAction::added`]): a unit within
    /// 0.01 of the target's footprint (`to.gap`) is there already, state 1; else state 3, or 13 for
    /// a unit without speed. None for a unit whose state is 3 or more: the list deletes it.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn move_to_object(
        &self,
        to: WalkTo,
        range: Distance,
        mover: &Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Option<MoveToAction>> {
        let given = self.task.walk_sprite_id(walks.held);
        walks
            .live(mover)
            .to_object(to, range, given, TargetMoved::AsksAgain, move_to_tuning)
    }

    /// The quiet `MoveTo` to the hunt's point within the task's range (`FUN_140aa0d30` with the
    /// task's walking sprite, `+0x61` 1), as the action list adds and starts it: the point's gap
    /// is far, so state 3, or 13 for a unit without speed. None for a unit whose
    /// state is 3 or more.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn move_to_point(
        &self,
        mover: &Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Option<MoveToAction>> {
        let given = self.task.walk_sprite_id(walks.held);
        let walk =
            walks
                .live(mover)
                .to_point(self.point, self.task.range, given, move_to_tuning)?;
        Ok(walk.map(|move_to_action| MoveToAction {
            on_target_moved: TargetMoved::AsksAgain,
            ..move_to_action
        }))
    }

    /// The Hunt's sub-action list's update (`FUN_140a992b0`: its first action's, the quiet
    /// `MoveTo`'s, [`MoveToAction::sub_update`]) and its code, which the state's case reads: 0
    /// while it walks or waits, 1 once it ended, 3 once it failed, 5 once its target was let go;
    /// 1 for an empty list ("There is no list."). The start shows the task's sprite, the motion
    /// step gives up as `gives_up` says, and a start or a move posts the units in the unit's way
    /// to its unit AI at once.
    ///
    /// # Errors
    ///
    /// The walk's and the notifies'.
    pub(super) fn update_list(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        mut me: Me,
        gives_up: GiveUp,
    ) -> GenieResult<SubCode> {
        me.action_state = ActionState::new(self.state.get());
        let state = self
            .state
            .move_to()
            .map(|move_to_action| move_to_action.state);
        let how = ListUpdate {
            gives_up,
            ..ListUpdate::default()
        };
        let code = match self.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, step, unit, me, how)?,
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, step, unit, me, how)?,
        };
        if matches!(state, Some(MoveTo::Start | MoveTo::Moving)) {
            unit.post_in_the_way(
                step.world.parts.motion.mover_mut(unit.id),
                &mut step.world.parts.unit_ai,
            )?;
        }
        Ok(code)
    }
}
