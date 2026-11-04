//! The `MoveTo`s the Gather's `set_state` makes (`0x140a8cd40`): what their constructors read of
//! the unit, and the two the Gather uses, one to an object and one to a point.

use crate::GenieResult;
use crate::gathering::gather::{GatherAction, GatherReads};
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Mover;
use crate::movement::move_to::{MoveToAction, MoveToReads, SpriteReads, TargetMoved, WalkTo};
use crate::objects::Graphics;
use crate::objects::lifecycle::Held;
use crate::objects::unit::Unit;
use crate::world::layer::generated::MoveToTuning;

/// What the Gather's and the Hunt's `MoveTo` constructors read of the unit as `set_state` makes
/// the `MoveTo` ([`MoveToReads`]), and what the unit carries: the task's walking sprite
/// (`FUN_140a8b660`) depends on the load.
#[derive(Clone, Copy, Debug)]
pub struct Walks<'a> {
    /// What every new sub-action `MoveTo` reads of the unit.
    pub(crate) reads: MoveToReads<'a>,
    /// What the unit carries: its resource kind (`+0x308`) and amount (`+0x310`).
    pub(crate) held: Held,
}

impl<'a> Walks<'a> {
    /// The reads of `unit` with the sprites `sprites`, and what it carries.
    pub(crate) fn of(sprites: SpriteReads<'a>, (unit, mover): (&Unit, Option<&Mover>)) -> Self {
        Self {
            reads: MoveToReads::with_sprites(sprites, (unit, mover)),
            held: unit.life.held(),
        }
    }

    /// The reads with the speed `mover` has now (slot 0x208, which a start reads as the
    /// constructor runs).
    pub(crate) const fn live(&self, mover: &Mover) -> MoveToReads<'a> {
        MoveToReads {
            speed: mover.stats.speed,
            ..self.reads
        }
    }
}

impl GatherReads {
    /// What the `MoveTo`s the update makes read of `unit` with its mover, with the sprites'
    /// table `graphics`.
    pub(crate) fn walks<'g>(
        &self,
        graphics: &'g Graphics,
        unit: (&Unit, Option<&Mover>),
    ) -> Walks<'g> {
        Walks::of(self.sprites_of(graphics), unit)
    }

    /// The sprites a `MoveTo` takes when its maker gives none.
    const fn sprites_of<'g>(&self, graphics: &'g Graphics) -> SpriteReads<'g> {
        SpriteReads {
            defaults: self.defaults,
            graphics,
        }
    }
}

impl GatherAction {
    /// The quiet `MoveTo` to the object `to` within the task's range
    /// (`FUN_140aa0df0` with the task's range and walking sprite, then `+0x61` 1: it asks again
    /// when its target moves; `FUN_140a7a170` with 1), as the action list adds and starts it
    /// ([`MoveToAction::added`]): a unit within 0.01 of the target's footprint
    /// (`to.gap`) is there already, state 1; else state 3, or 13 for a unit without speed. None for
    /// a unit whose state is 3 or more: the list deletes it.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn move_to_object(
        &self,
        to: WalkTo,
        mover: &Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Option<MoveToAction>> {
        let given = self.task.walk_sprite_id(walks.held);
        walks.live(mover).to_object(
            to,
            self.task.range,
            given,
            TargetMoved::AsksAgain,
            move_to_tuning,
        )
    }

    /// The quiet `MoveTo` to `point` within `range` (`FUN_140aa0d30` with the task's walking
    /// sprite, `FUN_140a7a170` with 1), whose `+0x61` is `on_target_moved` (a farm's stop sets 1,
    /// the walks of states 11, 15 and 0xf2 leave 0), as the action list adds and starts it: the
    /// point's gap is far, so state 3, or 13 for a unit without speed. None for a
    /// unit whose state is 3 or more.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn move_to_point(
        &self,
        point: Position,
        range: Distance,
        on_target_moved: TargetMoved,
        mover: &Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<Option<MoveToAction>> {
        let given = self.task.walk_sprite_id(walks.held);
        let walk = walks
            .live(mover)
            .to_point(point, range, given, move_to_tuning)?;
        Ok(walk.map(|move_to_action| MoveToAction {
            on_target_moved,
            ..move_to_action
        }))
    }
}
