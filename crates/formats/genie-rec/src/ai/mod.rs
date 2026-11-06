//! Read and write player AI state.
//!
//! A computer player's AI state is saved as a chain of modules (main, build, construction,
//! diplomacy, emotional, information, resource, strategy). The field names follow the genie-rs
//! reader and the AI's own terms; values whose meaning is not known say so.

use crate::ObjectId;
use crate::element::ReadHeader;
use crate::header_reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use std::io::Read;

mod build;
mod information;
mod memories;
mod strategy;

pub use self::build::{BuildAi, BuildItem, ConstructionAi, ConstructionItem};
pub use self::information::InformationAi;
pub use self::memories::{
    AttackMemory, BuildingLot, ImportantObjectMemory, InfluenceMap, PerimeterWall, QuadrantLog,
    ResourceMemory, WallLine,
};
pub use self::strategy::{DiplomacyAi, EmotionalAi, ResourceAi, StrategyAi, TacticalAi};

/// The main AI module.
#[derive(Debug, Default, Clone)]
pub struct MainAi {
    /// The object IDs the main module lists.
    pub objects: Vec<ObjectId>,
}

impl ReadHeader for MainAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let num_objects = input.read_u32::<LE>()?;
        let mut objects = vec![];
        for _ in 0..num_objects {
            objects.push(input.read_u32::<LE>()?.into());
        }
        Ok(Self { objects })
    }
}

/// The AI state of one computer player: its AI modules in file order.
#[derive(Debug, Default, Clone)]
pub struct PlayerAi {
    /// The main AI module.
    pub main_ai: MainAi,
    /// The build AI module.
    pub build_ai: BuildAi,
    /// The construction AI module.
    pub construction_ai: ConstructionAi,
    /// The diplomacy AI module.
    pub diplomacy_ai: DiplomacyAi,
    /// The emotional AI module.
    pub emotional_ai: EmotionalAi,
    /// The information AI module.
    pub information_ai: InformationAi,
    /// The resource AI module.
    pub resource_ai: ResourceAi,
    /// The strategy AI module.
    pub strategy_ai: StrategyAi,
}

impl ReadHeader for PlayerAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let main_ai = MainAi::read_from(input)?;
        let build_ai = BuildAi::read_from(input)?;
        let construction_ai = ConstructionAi::read_from(input)?;
        let diplomacy_ai = DiplomacyAi::read_from(input)?;
        let emotional_ai = EmotionalAi::read_from(input)?;
        let information_ai = InformationAi::read_from(input)?;
        let resource_ai = ResourceAi::read_from(input)?;
        let strategy_ai = StrategyAi::read_from(input)?;

        Ok(Self {
            main_ai,
            build_ai,
            construction_ai,
            diplomacy_ai,
            emotional_ai,
            information_ai,
            resource_ai,
            strategy_ai,
        })
    }
}

fn read_id_list<T: From<u32>, R: Read>(input: &mut R) -> GenieResult<Vec<T>> {
    let len = input.read_u32::<LE>()?;
    let mut ids = vec![];
    for _ in 0..len {
        ids.push(input.read_u32::<LE>()?.into());
    }
    Ok(ids)
}

#[cfg(test)]
mod tests;
