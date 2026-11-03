//! The sub-action list's update (`FUN_140a992b0`) for an owner action whose list holds a quiet
//! `MoveTo`: its first action's update, 1 for an empty list ("There is no list.").

use crate::GenieError;
use crate::movement::move_to::{GiveUp, MoveTo, MoveToAction, Shown};
use crate::movement::{Me, PathAsk};
use crate::objects::UpdateCode;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::world::step::Step;

/// What an owner gives its sub-action list's update.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListUpdate {
    /// The owner's answer to a motion step that gives up (the villager test and the stance).
    pub gives_up: GiveUp,
    /// What the unit's AI asks of the path the start plans.
    pub ask: PathAsk,
    /// The sprite the start shows for a `MoveTo` without one (an Attack's standing sprite).
    pub otherwise: Option<Graphic>,
}

/// The update of `list` (the owner's one quiet `MoveTo`, [`MoveToAction::sub_update`]) for
/// `unit` and its code: 0 while it walks or waits, 1 once it ended (or for an empty list), 3
/// once it failed, 4 and 5 once its target was let go ([`MoveToAction::check_held`]). Its start
/// shows its sprite (`set_state(4)`); the motion step gives up as `how` says.
///
/// # Errors
///
/// The walk's and the notifies'.
pub(crate) fn update_sub_actions(
    list: &mut Option<MoveToAction>,
    c: &mut Step<'_>,
    unit: &mut Unit,
    me: Me,
    how: ListUpdate,
) -> Result<UpdateCode, GenieError> {
    let Some(move_to) = list else {
        return Ok(UpdateCode::Done);
    };
    let held = c.w.object_state(move_to.target);
    let (mut walk, beside) = c.w.walk_beside(c.pace()?, &mut unit.place, me);
    let shown = (move_to.state == MoveTo::Start).then_some(Shown {
        sprite: &mut unit.sprite,
        rng: beside.sprite_rng,
        graphic: move_to.sprite.or(how.otherwise),
    });
    let mut inbox = beside.ais.inbox(unit.id, unit.life.state);
    move_to.sub_update(&mut walk, how.ask, how.gives_up, held, shown, &mut inbox)
}
