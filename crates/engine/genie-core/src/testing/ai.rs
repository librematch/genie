//! A scene's unit AI run on its unit (FD-043-D47): the unit's update holds it, and its AI is
//! lent out with the real side ([`crate::unit_ai::world_view::UnitView`]) as the game's update
//! lends them ([`Unit::run_ai`](crate::objects::unit::Unit::run_ai)).

use crate::GenieError;
use crate::math::rng::Well512;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::UnitSide;
use crate::unit_ai::{Surroundings, UnitAi};
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::tests::in_update;

/// Runs `f` on object `me` of player `player` of `w` inside its update (dt 0, outside a turn):
/// on its unit AI, what the AI reads of the unit and the world, its side, its owner's type and
/// the AI stream. The AI is the unit's again after it, the side's calls made on the world.
///
/// # Errors
///
/// `f`'s, and what the unit's update and its AI's run do not model.
///
/// # Panics
///
/// An object `w` does not hold in `player`'s list.
pub(crate) fn run_ai<R>(
    w: &mut World,
    (player, me): (PlayerId, ObjectId),
    f: impl FnOnce(
        &mut UnitAi,
        &Surroundings,
        &mut dyn UnitSide,
        PlayerKind,
        &mut Well512,
    ) -> Result<R, GenieError>,
) -> Result<R, GenieError> {
    let i = w
        .players
        .player(player)?
        .objects
        .iter()
        .position(|u| u.id == me)
        .expect("the unit in its player's list");
    in_update(w, (player, ListIndex::new(i)), 0, |unit, actions, c| {
        let mut answer = None;
        unit.run_ai(actions, c, |ai, at, side, kind, rng| {
            answer = Some(f(ai, at, side, kind, rng)?);
            Ok(())
        })?;
        answer.ok_or(GenieError::unreachable("a unit AI's run that did not run"))
    })
}
