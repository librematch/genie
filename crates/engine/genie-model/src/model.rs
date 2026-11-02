//! The game's data a world plays on as one value (FD-046-D02, D04): the masters per player, the
//! technologies and their effects, the tech tree, the unit lines, the drop sites and the
//! graphics. Each table is shared between worlds until its first write (`Arc::make_mut`), so
//! forks and parallel instances hold one stock model.

use crate::layer::BuildLayer;
use crate::layer::Roles;
use crate::layer::generated::Slots;
use crate::layer::generated::Tuning;
use crate::objects::Graphics;
use crate::objects::lines::UnitLines;
use crate::objects::masters::Masters;
use crate::players::world_player::TechTree;
use crate::technology::EffectCommand;
use crate::technology::Linked;
use crate::technology::TechDef;
use crate::technology::TechId;
use crate::unit_ai::task_finder::DropSite;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The game's data a world plays on (FD-046-D02, D04): each table shared between worlds until
/// its first write (`Arc::make_mut`), so forks and parallel instances hold one stock model.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Model {
    /// Per player, the masters (the game's per-player copies), which BUILD reads; a player
    /// without them stops at its own BUILD.
    pub masters: Vec<Arc<Masters>>,
    /// What the sprite clock reads of each graphic (the `.dat`'s).
    pub graphics: Arc<Graphics>,
    /// The game's drop-site table (`dropsites.json`).
    pub drop_sites: Arc<Vec<DropSite>>,
    /// The game's unit lines (`unitlines.json`); empty when not loaded.
    pub lines: Arc<UnitLines>,
    /// The game's linked technologies (`linkedTechs.json`, the table at the game settings'
    /// `+0x18` that `FUN_140f39f40` looks up), by technology; empty when not loaded.
    pub linked_techs: Arc<BTreeMap<TechId, Linked>>,
    /// The `.dat`'s technologies, by id.
    pub techs: Arc<Vec<TechDef>>,
    /// The world's effect table (`+0x128`, `TRIBE_Effects`): every effect's commands by id, the
    /// `.dat`'s (the patch table, D11).
    pub effects: Arc<Vec<Vec<EffectCommand>>>,
    /// The game's tech tree table and its civilisations' unique units, which the defeat check's
    /// count of the units a building will train reads.
    pub tech_tree: Arc<TechTree>,
    /// The roles of the ids, the build layer's (FD-046-D04, D08).
    pub roles: Roles,
    /// The ids the engine names by role, the build layer's.
    pub slots: Slots,
    /// The tuning values, the build layer's (D32).
    pub tuning: Arc<Tuning>,
}

impl Model {
    /// The model with the build layer's roles, slots and tuning values (D09's step 2); its
    /// tables stay.
    #[must_use]
    pub fn with_layer(self, layer: &BuildLayer) -> Self {
        Self {
            roles: layer.roles.clone(),
            slots: layer.slots,
            tuning: Arc::new(layer.tuning.clone()),
            ..self
        }
    }
}

#[cfg(test)]
mod tests;
