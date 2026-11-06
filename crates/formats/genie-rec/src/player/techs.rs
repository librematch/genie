//! The technologies' states, and `UserPatch`'s extra player data.

use crate::element::ReadHeader;
use crate::header_reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use genie_support::read_list;
use std::io::Read;

/// The state of one technology for a player.
#[derive(Debug, Default, Clone, Copy)]
pub struct TechState {
    /// How far the research has progressed.
    pub progress: f32,
    /// The research state (unavailable, available, researching, done …).
    pub state: i16,
    /// Changes to the technology's three costs.
    pub modifiers: (i16, i16, i16),
    /// The change to its research time.
    pub time_modifier: i16,
}

impl ReadHeader for TechState {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            progress: input.read_f32::<LE>()?,
            state: input.read_i16::<LE>()?,
            modifiers: (
                input.read_i16::<LE>()?,
                input.read_i16::<LE>()?,
                input.read_i16::<LE>()?,
            ),
            time_modifier: input.read_i16::<LE>()?,
        })
    }
}

/// The state of every technology for a player.
#[derive(Debug, Default, Clone)]
pub struct PlayerTech {
    /// The state of each technology, by technology id.
    pub tech_states: Vec<TechState>,
}

impl ReadHeader for PlayerTech {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let num_techs = input.read_u16::<LE>()?;
        let tech_states = read_list(usize::from(num_techs), || TechState::read_from(input))?;
        Ok(Self { tech_states })
    }
}

/// `UserPatch` 1.5's extra player data. Read past but not kept yet.
#[derive(Debug, Clone, Copy)]
pub struct UserPatchData {}

impl ReadHeader for UserPatchData {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        {
            let mut bytes = vec![0; 4080];
            input.read_exact(&mut bytes)?;
        }

        let mut category_priorities = vec![0; 900];
        let mut group_priorities = vec![0; 100];

        input.read_u16_into::<LE>(&mut category_priorities)?;
        input.read_u16_into::<LE>(&mut group_priorities)?;

        {
            let mut bytes = vec![0; 2096];
            input.read_exact(&mut bytes)?;
        }

        Ok(Self {})
    }
}
