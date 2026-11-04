//! Gaia's extra state: its creatures' growth and its wolves.

use crate::ObjectId;
use crate::element::{ReadableHeaderElement, WritableHeaderElement};
use crate::reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::read_opt_u32;
use std::io::{Read, Write};

/// Gaia's extra state: creature (animal) growth and the wolf attacks on villagers.
#[derive(Debug, Default, Clone)]
pub struct GaiaData {
    #[allow(dead_code)]
    pub(super) update_time: u32,
    #[allow(dead_code)]
    pub(super) update_nature: u32,
    pub(super) creatures: [GaiaCreature; 5],
    pub(super) next_wolf_attack_update_time: u32,
    pub(super) wolf_attack_update_interval: u32,
    pub(super) wolf_attack_stop_time: u32,
    pub(super) min_villager_distance: f32,
    pub(super) tc_positions: [(f32, f32); 9],
    pub(super) wolf_current_player: u32,
    pub(super) wolf_current_villagers: [u32; 10],
    pub(super) wolf_current_villager: Option<ObjectId>,
    pub(super) wolf_villager_count: u32,
    pub(super) wolves: [GaiaWolfInfo; 25],
    pub(super) current_wolf: Option<ObjectId>,
    pub(super) wolf_counts: [u32; 10],
}

impl ReadableHeaderElement for GaiaData {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut gaia = Self {
            update_time: input.read_u32::<LE>()?,
            update_nature: input.read_u32::<LE>()?,
            ..Default::default()
        };
        for creature in &mut gaia.creatures {
            *creature = GaiaCreature::read_from(input)?;
        }
        gaia.next_wolf_attack_update_time = input.read_u32::<LE>()?;
        gaia.wolf_attack_update_interval = input.read_u32::<LE>()?;
        gaia.wolf_attack_stop_time = input.read_u32::<LE>()?;
        gaia.min_villager_distance = input.read_f32::<LE>()?;
        for pos in &mut gaia.tc_positions {
            pos.0 = input.read_f32::<LE>()?;
        }
        for pos in &mut gaia.tc_positions {
            pos.1 = input.read_f32::<LE>()?;
        }
        gaia.wolf_current_player = input.read_u32::<LE>()?;
        for v in &mut gaia.wolf_current_villagers {
            *v = input.read_u32::<LE>()?;
        }
        gaia.wolf_current_villager = read_opt_u32(input)?;
        gaia.wolf_villager_count = input.read_u32::<LE>()?;
        for wolf in &mut gaia.wolves {
            *wolf = GaiaWolfInfo::read_from(input)?;
        }
        gaia.current_wolf = read_opt_u32(input)?;
        input.read_u32_into::<LE>(&mut gaia.wolf_counts[..])?;
        Ok(gaia)
    }
}

/// The growth of one kind of Gaia creature (animal).
#[derive(Debug, Default, Clone, Copy)]
pub struct GaiaCreature {
    /// How fast the creatures grow in number.
    pub growth_rate: f32,
    /// The fraction of growth carried over to the next update.
    pub remainder: f32,
    /// The most creatures of this kind there can be.
    pub max: u32,
}

impl ReadableHeaderElement for GaiaCreature {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            growth_rate: input.read_f32::<LE>()?,
            remainder: input.read_f32::<LE>()?,
            max: input.read_u32::<LE>()?,
        })
    }
}

impl WritableHeaderElement for GaiaCreature {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.growth_rate)?;
        output.write_f32::<LE>(self.remainder)?;
        output.write_u32::<LE>(self.max)?;
        Ok(())
    }
}

/// A wolf that takes part in Gaia's wolf attacks.
#[derive(Debug, Default, Clone, Copy)]
pub struct GaiaWolfInfo {
    /// The wolf's object id.
    pub id: u32,
    /// Its distance to its target villager.
    pub distance: f32,
}

impl ReadableHeaderElement for GaiaWolfInfo {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            id: input.read_u32::<LE>()?,
            distance: input.read_f32::<LE>()?,
        })
    }
}

impl WritableHeaderElement for GaiaWolfInfo {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.id)?;
        output.write_f32::<LE>(self.distance)?;
        Ok(())
    }
}
