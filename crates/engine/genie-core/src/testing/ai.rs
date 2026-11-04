//! A scene's unit AI run on its unit (FD-043-D47): the unit's update holds it, and its AI is
//! lent out with the real side ([`crate::unit_ai::world_view::UnitView`]) as the game's update
//! lends them ([`Unit::run_ai`](crate::objects::unit::Unit::run_ai)).

use crate::math::rng::Well512;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::unit::Outcome;
use crate::ownership::capture::Captured;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{Surroundings, UnitAi};
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::tests::in_update_filed;
use crate::{GenieError, GenieResult};
use std::mem::take;

/// Runs `work` on object `me` of player `player` of `world` inside its update (dt 0, outside a
/// turn), after the update's start took its sprite's facets
/// ([`Unit::take_sprite_facets`](crate::objects::unit::Unit::take_sprite_facets)): on its unit AI,
/// what the AI reads of the unit and the world, its side, its owner's type and the AI stream. The
/// AI is the unit's again after it, the side's calls made on the world; a takeover they made ends
/// the step as the unit's update ends it, and the list's loop hands the object to its new owner.
///
/// # Errors
///
/// `work`'s, and what the unit's update and its AI's run do not model.
///
/// # Panics
///
/// An object `world` does not hold in `player`'s list.
pub(crate) fn run_ai<R>(
    world: &mut World,
    (player, me): (PlayerId, ObjectId),
    work: impl FnOnce(
        &mut UnitAi,
        &Surroundings,
        &mut UnitView<'_, '_>,
        PlayerKind,
        &mut Well512,
    ) -> GenieResult<R>,
) -> GenieResult<R> {
    let index = world
        .players
        .player(player)?
        .objects
        .iter()
        .position(|unit| unit.id == me)
        .expect("the unit in its player's list");
    in_update_filed(
        world,
        (player, ListIndex::new(index)),
        0,
        |unit, actions, step| {
            unit.take_sprite_facets(step.world.parts.mover_mut(unit.id));
            let mut answer = None;
            unit.run_ai(actions, step, |ai, at, side, kind, rng| {
                answer = Some(work(ai, at, side, kind, rng)?);
                Ok(())
            })?;
            let answer =
                answer.ok_or(GenieError::unreachable("a unit AI's run that did not run"))?;
            // as the unit's update ends after its AI's run (`Unit::ai_part`)
            let outcome = match step.taken_over.take() {
                Some((new, main)) => Outcome::Captured(Captured {
                    new,
                    main,
                    also: Vec::new(),
                    building: take(&mut step.building_handover),
                }),
                None => Outcome::Stay,
            };
            Ok((answer, outcome))
        },
    )
}
