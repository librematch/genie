//! A player's diplomacy offers and its history: the timeline's events and entries.

use crate::element::{ReadableHeaderElement, read_counted};
use crate::reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use genie_support::ReadStringsExt;
use std::io::Read;

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub(super) struct DiplomacyOffer {
    pub(super) sequence: u8,
    pub(super) started_by: u8,
    pub(super) actual_time: u32,
    pub(super) game_time: u32,
    pub(super) declare: u8,
    pub(super) old_diplomacy: u8,
    pub(super) new_diplomacy: u8,
    pub(super) old_intelligence: u8,
    pub(super) new_intelligence: u8,
    pub(super) old_trade: u8,
    pub(super) new_trade: u8,
    pub(super) demand: u8,
    pub(super) gold: u32,
    pub(super) message: Option<String>,
    pub(super) status: u8,
}

impl ReadableHeaderElement for DiplomacyOffer {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut offer = Self {
            sequence: input.read_u8()?,
            started_by: input.read_u8()?,
            actual_time: 0,
            game_time: input.read_u32::<LE>()?,
            declare: input.read_u8()?,
            old_diplomacy: input.read_u8()?,
            new_diplomacy: input.read_u8()?,
            old_intelligence: input.read_u8()?,
            new_intelligence: input.read_u8()?,
            old_trade: input.read_u8()?,
            new_trade: input.read_u8()?,
            demand: input.read_u8()?,
            gold: input.read_u32::<LE>()?,
            ..Default::default()
        };
        let message_len = input.read_u8()?;
        offer.message = input.read_str(usize::from(message_len))?;
        offer.status = input.read_u8()?;
        Ok(offer)
    }
}

/// A player's history, the data of the post-game timeline.
#[derive(Debug, Default, Clone)]
pub struct HistoryInfo {
    /// The population at each history update.
    pub entries: Vec<HistoryEntry>,
    /// The events (age advances, battles …) on the timeline.
    pub events: Vec<HistoryEvent>,
}

impl ReadableHeaderElement for HistoryInfo {
    // Reads the entries and events behind their counts, then skips the battle statistics.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let _padding = input.read_u8()?;
        let num_entries = input.read_u32::<LE>()?;
        let _num_events = input.read_u32::<LE>()?;
        let _entries_capacity = input.read_u32::<LE>()?;
        let entries = (0..num_entries)
            .map(|_| HistoryEntry::read_from(input))
            .collect::<GenieResult<_>>()?;

        let _padding = input.read_u8()?;

        let events = read_counted(input)?;

        skip_battle_totals(input)?;
        skip_old_battle_values(input)?;

        let _padding = input.read_u8()?;

        Ok(Self { entries, events })
    }
}

/// The battle totals the reader skips: razings and kills with their hit points and weights,
/// the modes and the old counts of units and buildings.
fn skip_battle_totals<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    let _razings = input.read_i32::<LE>()?;
    let _hit_points_razed = input.read_i32::<LE>()?;
    let _razed_by_others = input.read_i32::<LE>()?;
    let _hit_points_razed_by_others = input.read_i32::<LE>()?;
    let _kills = input.read_i32::<LE>()?;
    let _hit_points_killed = input.read_i32::<LE>()?;
    let _killed_by_others = input.read_i32::<LE>()?;
    let _hit_points_killed_by_others = input.read_i32::<LE>()?;
    let _razings_weight = input.read_i32::<LE>()?;
    let _kills_weight = input.read_i32::<LE>()?;
    let _razings_percent = input.read_i32::<LE>()?;
    let _kills_percent = input.read_i32::<LE>()?;
    let _razing_mode = input.read_i32::<LE>()?;
    let _battle_mode = input.read_i32::<LE>()?;
    let _update_count = input.read_i32::<LE>()?;
    let _old_current_units_created = input.read_i32::<LE>()?;
    let _old_current_buildings_built = input.read_i32::<LE>()?;
    Ok(())
}

/// The old kills and razings per player with their battle values, and the running totals:
/// skipped too.
fn skip_old_battle_values<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    let mut old_kills = [0; 8];
    input.read_u16_into::<LE>(&mut old_kills[..])?;
    let mut old_kill_bvs = [0; 8];
    input.read_u32_into::<LE>(&mut old_kill_bvs[..])?;
    let mut old_razings = [0; 8];
    input.read_u16_into::<LE>(&mut old_razings[..])?;
    let mut old_razing_bvs = [0; 8];
    input.read_u32_into::<LE>(&mut old_razing_bvs[..])?;
    let _running_average_bv_percent = input.read_i32::<LE>()?;
    let _running_total_bv_kills = input.read_i32::<LE>()?;
    let _running_total_bv_razings = input.read_i32::<LE>()?;
    let _running_total_kills = input.read_i16::<LE>()?;
    let _running_total_razings = input.read_i16::<LE>()?;
    Ok(())
}

/// An event on a player's timeline.
#[derive(Debug, Default, Clone, Copy)]
pub struct HistoryEvent {
    /// The kind of event.
    pub event_kind: i8,
    /// The history update the event falls into.
    pub time_slice: u32,
    /// The game time of the event.
    pub world_time: u32,
    /// The event's three parameters; their meaning depends on the event type.
    pub params: (f32, f32, f32),
}

impl ReadableHeaderElement for HistoryEvent {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            event_kind: input.read_i8()?,
            time_slice: input.read_u32::<LE>()?,
            world_time: input.read_u32::<LE>()?,
            params: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
        })
    }
}

/// A player's population at one history update.
#[derive(Debug, Default, Clone, Copy)]
pub struct HistoryEntry {
    /// The number of civilian units.
    pub civilian_population: u16,
    /// The number of military units.
    pub military_population: u16,
}

impl ReadableHeaderElement for HistoryEntry {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let civilian_population = input.read_u16::<LE>()?;
        let military_population = input.read_u16::<LE>()?;
        Ok(Self {
            civilian_population,
            military_population,
        })
    }
}

#[cfg(test)]
mod tests;
