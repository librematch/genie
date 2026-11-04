//! The world as a recording's header load leaves it ([`loaded_world`]): what the game's load
//! builds that the save does not hold.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::objects::masters::ActionKind;
use crate::unit_ai::{Hunted, UnitAiKind};
use crate::vision::registry::{CLASSES, Registry};
use crate::world::World;
use crate::world::load::world::{saved_world, stop_all};
use genie_import::dat::MasterTable;
use genie_model::layer::BuildLayer;
use genie_save::Header;
use std::collections::BTreeMap;

/// The world as a recording's header load leaves it, with the masters `masters` and the build
/// layer `build`: [`saved_world`] with the objects-in-sight registry empty, every object's mask
/// 0, as the game's load makes it, the hunted animals' unit AI fields their load made
/// ([`hunted_origins`]) and the watched lists ([`watch_at_load`]).
///
/// # Errors
///
/// None yet: what the world does not model of the save stops every player instead.
pub fn loaded_world(
    header: &Header,
    masters: Option<&MasterTable>,
    build: &BuildLayer,
) -> Result<World, GenieError> {
    let mut turns = saved_world(header, masters, build)?;
    if let Some(sight) = turns.spatial.sight.as_mut() {
        sight.registry = Some(Registry::new(header.players().len(), CLASSES));
    }
    // the hunted animals' origins are their places in this very save
    hunted_origins(&mut turns, header);
    watch_at_load(&mut turns);
    Ok(turns)
}

/// The ghosts' watched lists as the game's load builds them ([`World::build_watched`]), once the
/// world holds its registry; what it cannot build stops every player.
pub fn watch_at_load(turns: &mut World) {
    if let Err(e) = turns.build_watched() {
        stop_all(turns, &e);
    }
}

/// The hunted animals' unit AI fields the save does not hold (`TribeHuntedAnimalUnitAIModule`,
/// made with the unit: `FUN_140c11920`): the origin is the animal's place at the recording's
/// start (`start`, the recording's header save, whose load made the AI), the leash its master's
/// last task of action type 6's work range (the unit's `+0xb74`, `FUN_141206d50`).
pub fn hunted_origins(turns: &mut World, start: &Header) {
    let places: BTreeMap<i32, Position> = start
        .players()
        .iter()
        .flat_map(|p| {
            p.live_objects()
                .chain(p.sleeping_objects())
                .collect::<Vec<_>>()
        })
        .filter_map(|o| {
            o.static_part()
                .map(|s| (s.id, Position::new(s.tx, s.ty, s.tz)))
        })
        .collect();
    for (p, player) in turns.players.iter().enumerate() {
        let masters = turns.model.masters.get(p);
        for unit in player.objects.iter().chain(player.sleeping.iter()) {
            let Some(ai) = turns
                .parts
                .unit_ai
                .get_mut(unit.id)
                .filter(|ai| ai.ai_kind == UnitAiKind::Hunted)
            else {
                continue;
            };
            let Some(&origin) = places.get(&unit.id.get()) else {
                continue;
            };
            let leash = masters
                .and_then(|m| m.get(unit.life.master_id))
                .and_then(|m| {
                    m.action_part()
                        .tasks
                        .iter()
                        .rev()
                        .find(|t| t.action_kind == ActionKind::HUNT_RANGE)
                })
                .map_or(Distance::ZERO, |t| t.work_range);
            ai.hunted = Some(Hunted { origin, leash });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::object_id;
    use crate::testing::layer;
    use crate::world::load::fixtures::{
        combat_mut, combat_object, gaia, header, human, static_mut,
    };
    use genie_save::generated::world::{Object, UnitAiModule};

    /// A hunted animal (`needUnitAI` 2) at (3, 4) whose unit AI has no order or action.
    fn deer(id: i32) -> Object {
        let mut deer = combat_object(id, 4);
        let at = static_mut(&mut deer);
        at.tx = 3.0;
        at.ty = 4.0;
        let combat = combat_mut(&mut deer);
        combat.need_unit_ai = 2;
        combat.unit_ai_module = Some(Box::new(UnitAiModule {
            current_order_value: -1,
            last_order_value: -1,
            current_action_value: -1,
            last_action_value: -1,
            ..UnitAiModule::default()
        }));
        deer
    }

    /// The load makes the objects-in-sight registry, which the save does not hold.
    #[test]
    fn the_load_makes_the_objects_in_sight_registry() {
        let save = header(vec![gaia(Vec::new()), human(Vec::new())]);
        let registry = |world: World| world.spatial.sight.is_some_and(|s| s.registry.is_some());
        assert!(!registry(saved_world(&save, None, layer()).unwrap()));
        assert!(registry(loaded_world(&save, None, layer()).unwrap()));
    }

    /// A hunted animal's origin is its place in the save; without its master's hunt task its
    /// leash is 0.
    #[test]
    fn hunted_animal_starts_at_its_saved_place() {
        let save = header(vec![gaia(vec![deer(7)]), human(Vec::new())]);
        let world = loaded_world(&save, None, layer()).unwrap();
        let hunted = world
            .parts
            .unit_ai
            .get(object_id(7))
            .and_then(|ai| ai.hunted);
        assert_eq!(
            hunted,
            Some(Hunted {
                origin: Position::new(3.0, 4.0, 0.0),
                leash: Distance::ZERO,
            })
        );
        let saved = saved_world(&save, None, layer()).unwrap();
        assert_eq!(
            saved.parts.unit_ai.get(object_id(7)).map(|ai| ai.hunted),
            Some(None)
        );
    }
}
