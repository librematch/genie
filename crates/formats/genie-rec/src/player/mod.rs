use crate::header_reader::RecordingHeaderReader;
use crate::unit::Unit;
use crate::unit_kind::CompactUnitKind;
use genie_dat::CivilizationId;
use genie_dat::tech_tree::TechTree;
use genie_error::GenieResult;
use genie_scx::VictoryConditions;
use std::io::Read;

mod gaia;
mod history;
mod reader;
mod sight;
mod techs;

pub use self::gaia::{GaiaCreature, GaiaData, GaiaWolfInfo};
use self::history::DiplomacyOffer;
pub use self::history::{HistoryEntry, HistoryEvent, HistoryInfo};
pub use self::sight::{VisibleMap, VisibleResource, VisibleResources};
pub use self::techs::{PlayerTech, TechState, UserPatchData};

/// A player's state in the recording header: settings, resources (attributes), technologies,
/// unit types, visibility and units.
#[derive(Debug, Default, Clone)]
pub struct Player {
    /// The player type: 2 is Gaia, 3 a computer (AI) player.
    kind: u8,
    relations: Vec<u8>,
    diplomacy: [u32; 9],
    allied_los: bool,
    allied_victory: bool,
    name: String,
    /// The player's attributes (resources): food, wood, stone, gold, population …, indexed by
    /// attribute id.
    pub attributes: Vec<f32>,
    initial_view: (f32, f32),
    saved_views: Vec<(f32, f32)>,
    spawn_location: (u16, u16),
    culture_id: u8,
    /// The player's civilization.
    pub civilization_id: CivilizationId,
    game_status: u8,
    resigned: bool,
    /// `UserPatch` 1.5's extra player data (save version 11.97 only).
    pub userpatch_data: Option<UserPatchData>,
    /// The state of each technology for this player.
    pub tech_state: PlayerTech,
    /// The player's population history and statistics.
    pub history_info: HistoryInfo,
    /// The player's tech tree, from save version 9.38 on.
    pub tech_tree: Option<TechTree>,
    /// Gaia's extra data (creature growth, wolf attacks); only for the Gaia player.
    pub gaia: Option<GaiaData>,
    /// The player's copy of each unit type, by unit type id; `None` for the ones it does not have.
    pub unit_kinds: Vec<Option<CompactUnitKind>>,
    /// Which tiles the player has explored and sees.
    pub visible_map: VisibleMap,
    /// The resource objects the player knows of.
    pub visible_resources: VisibleResources,
    /// The player's active units.
    pub units: Vec<Unit>,
    /// The player's sleeping (inactive) units.
    pub sleeping_units: Vec<Unit>,
    /// The player's doppelganger objects: the stand-ins for objects seen under the fog of war.
    pub doppelganger_units: Vec<Unit>,
    /// The player's victory conditions.
    pub victory: VictoryConditions,
}

impl Player {
    /// Return the name of this player.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Read the player's victory conditions, which the header stores after all players.
    ///
    /// # Errors
    ///
    /// Returns an error if they cannot be read.
    pub fn read_info<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        self.victory = VictoryConditions::read_from(input, true)?;
        Ok(())
    }
}
