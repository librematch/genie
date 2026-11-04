use crate::{Scenario, ScenarioObject, Tile, Trigger, UnitKindId};
use genie_error::GenieResult;
use nohash_hasher::IntMap;

/// Convert an `AoC` scenario to a WololoKingdoms-compatible one.
///
/// Swaps the unit IDs and terrain IDs that look different in WK.
#[derive(Debug)]
pub struct AocToWk {
    object_ids_map: IntMap<i32, UnitKindId>,
    terrain_ids_map: IntMap<u8, u8>,
}

impl Default for AocToWk {
    fn default() -> Self {
        let object_ids_map = [
            // NOTE: These are special to make the tech tree work
            (1103, 529), // Fire Galley, Fire Ship
            (529, 1103), // Fire Ship, Fire Galley
            (1104, 527), // Demolition Raft, Demolition Ship
            (527, 1104), // Demolition Ship, Demolition Raft
        ]
        .iter()
        .map(|(a, b)| (*a, UnitKindId::from(*b)))
        .collect();

        let terrain_ids_map = [
            (11, 3),  // Dirt 2, Dirt 3
            (16, 0),  // Grass-ish, Grass
            (20, 19), // Oak Forest, Pine Forest
        ]
        .iter()
        .map(|(a, b)| (*a, *b))
        .collect();

        Self {
            object_ids_map,
            terrain_ids_map,
        }
    }
}

impl AocToWk {
    /// Convert an object from `AoC` to WK.
    ///
    /// This updates the object type IDs.
    fn convert_object(&self, object: &mut ScenarioObject) {
        if let Some(new_kind) = self.object_ids_map.get(&object.object_kind.into()) {
            object.object_kind = *new_kind;
        }
    }

    /// Convert a trigger from `AoC` to WK.
    ///
    /// This updates the object type IDs in trigger conditions and effects.
    fn convert_trigger(&self, trigger: &mut Trigger) {
        trigger.conditions_unordered_mut().for_each(|cond| {
            if let Some(new_kind) = cond
                .unit_kind()
                .and_then(|t| self.object_ids_map.get(&t.into()))
            {
                cond.set_unit_kind(*new_kind);
            }
            if let Some(new_kind) = cond
                .object_kind()
                .and_then(|t| self.object_ids_map.get(&t.into()))
            {
                cond.set_object_kind(*new_kind);
            }
        });
        trigger.effects_unordered_mut().for_each(|effect| {
            if let Some(new_kind) = effect
                .unit_kind()
                .and_then(|t| self.object_ids_map.get(&t.into()))
            {
                effect.set_unit_kind(*new_kind);
            }
            if let Some(new_kind) = effect
                .object_kind()
                .and_then(|unit_kind_id| self.object_ids_map.get(&unit_kind_id.into()))
            {
                effect.set_object_kind(*new_kind);
            }
        });
    }

    /// Convert a terrain tile from `AoC` to WK.
    fn convert_terrain(&self, tile: &mut Tile) {
        if let Some(new_kind) = self.terrain_ids_map.get(&tile.terrain) {
            tile.terrain = *new_kind;
        }
    }

    /// Convert a scenario from `AoC` to WK in-place.
    ///
    /// # Errors
    ///
    /// Never fails at present; the `Result` matches the other converters.
    pub fn convert(&self, scen: &mut Scenario) -> GenieResult<()> {
        for object in scen.objects_mut() {
            self.convert_object(object);
        }

        for tile in scen.map_mut().tiles_mut() {
            self.convert_terrain(tile);
        }

        if let Some(trigger_system) = scen.triggers_mut() {
            for trigger in trigger_system.triggers_unordered_mut() {
                self.convert_trigger(trigger);
            }
        }

        Ok(())
    }
}
