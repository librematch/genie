//! The player's history (`TRIBE_History_Info`, TribeHistoryInfo.cpp; player `+0xdb8`, saved with
//! the player): a population entry each history period and a list of events, newest first.
//!
//! The world player's update (`WorldPlayer::update`, `0x14163b100`) runs [`History::update`] once
//! a game second; every `time_slice` runs (the `.dat`'s, `world+0x8b8`: 30) it adds an entry
//! (attributes 37 and 40, the civilian and military population) and follows the player's kills
//! and razings (attributes 20, 43, 152 to 155, 170 and 172 against its own value, 164 and 165)
//! for battles. The events: an age reached (research of 101 to 103, `FUN_1413cdcd0`), a castle
//! or wonder built (the building's `set_state`, slot 20, `FUN_1411dbb70`) or destroyed (on its
//! destroyer's history, slot 29, `FUN_1411cea90`), a battle ([`History::update`]) and a defeat
//! (the status setter, slot 1, `FUN_141639860`).
//!
//! The simulation reads it back: the AI's current age time (`FUN_140b077d0`,
//! [`History::age_time`]) is the time since the age's event. The post-game timeline reads the
//! entries and every event ([`History::events_at`], `FUN_1419f7b90`); the achievements and the
//! post-game statistics read the age events.
//!
//! 158041: `add_history_event` `FUN_141560ae0`, `add_new_history_entry` `FUN_141560b90`, the
//! first event of a kind `FUN_141560e20`, `get_history_event_by_time` `FUN_141560ea0`,
//! `mark_new_battle` `FUN_141565470`, `update` `FUN_141569450`; the names are the 2016 HD PDB's.

use genie_model::players::history::history_updates;
mod values;

pub use self::history_updates::{HISTORY_PERIOD, HistoryUpdates};
pub use values::{EntryKind, HistoryEntry, Population, Share, Tally, TimeSlice};

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::WorldTime;
use crate::world::layer::Slots;
use crate::world::layer::generated::HistoryTuning;
use crate::world::slots::CheckedIndex;

/// A history event's kind (`event`, `+0x0`): an age reached, a castle or wonder built or
/// destroyed, a battle or a defeat. Every byte is a kind (a save may hold kinds the slice
/// does not name).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// Feudal Age reached (research 101).
    #[default]
    FeudalAge,
    /// Castle Age reached (research 102).
    CastleAge,
    /// Imperial Age reached (research 103).
    ImperialAge,
    /// A wonder built (master 276); `data1` is its owner.
    WonderBuilt,
    /// A wonder destroyed, on its destroyer's history; `data1` is its owner.
    WonderDestroyed,
    /// A castle built (master 82); `data1` is its owner.
    CastleBuilt,
    /// A castle destroyed, on its destroyer's history; `data1` is its owner.
    CastleDestroyed,
    /// A battle: `data1` the opponent, `data2` its share of the player's value, `data3` 1.
    Battle,
    /// A defeat: `data1` the player with the most of attribute 300 + its own id, `data2` that value.
    Defeat,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`EventKind`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl EventKind {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::FeudalAge,
            1 => Self::CastleAge,
            2 => Self::ImperialAge,
            4 => Self::WonderBuilt,
            5 => Self::WonderDestroyed,
            6 => Self::CastleBuilt,
            7 => Self::CastleDestroyed,
            11 => Self::Battle,
            20 => Self::Defeat,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::FeudalAge => 0,
            Self::CastleAge => 1,
            Self::ImperialAge => 2,
            Self::WonderBuilt => 4,
            Self::WonderDestroyed => 5,
            Self::CastleBuilt => 6,
            Self::CastleDestroyed => 7,
            Self::Battle => 11,
            Self::Defeat => 20,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }
}

impl From<u8> for EventKind {
    /// The kind a saved event's byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self::of_raw(value)
    }
}

impl genie_diff::Diff for EventKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

/// The entries' buffer grows by this many (`add_new_history_entry`).
const ENTRY_GROWTH: TimeSlice = TimeSlice::of_data(60);
/// The battles a history keeps before a new one has to push one out (`mark_new_battle`).
const BATTLES_KEPT: usize = 3;
/// The newest battles `mark_new_battle` looks among for one to push out, the new one included.
const BATTLES_COMPARED: i32 = 3;
/// The first of the attributes that count the player's kills' value of each player (400 + i).
const KILL_VALUE_OF: usize = 400;
/// The first of the attributes that count the player's razings' value of each player (425 + i).
const RAZING_VALUE_OF: usize = 425;

/// An event (`Time_Slice_History_Event`, 0x20 bytes, `calloc`ed).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Event {
    /// `event` (`+0x0`): its kind.
    pub kind: EventKind,
    /// `time_slice` (`+0x4`): the number of entries when it happened.
    pub time_slice: TimeSlice,
    /// `world_time` (`+0x8`): the world's time then, in milliseconds.
    pub world_time: WorldTime,
    /// `data1` (`+0xc`).
    pub data1: f32,
    /// `data2` (`+0x10`).
    pub data2: f32,
    /// `data3` (`+0x14`).
    pub data3: f32,
}

/// The player's history (`TRIBE_History_Info`), every field the save holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// `history` (`+0x0`) up to `number_of_entries` (`+0x1c`): each entry's civilian and
    /// military population.
    pub entries: Vec<HistoryEntry>,
    /// `max_number_of_entries` (`+0x20`): the entries' buffer, grown by 60.
    pub max_entries: TimeSlice,
    /// `events` (`+0x8`), newest first (the list from its head).
    pub events: Vec<Event>,
    /// `number_of_events` (`+0x18`): as loaded; no writer counts it.
    pub number_of_events: i32,
    /// `update_history_count` (`+0x28`): the update's runs since the last period.
    pub update_count: HistoryUpdates,
    /// `old_total_razings` (`+0x2c`): attribute 43 at the last period.
    pub old_total_razings: Tally,
    /// `old_build_value_razings` (`+0x30`): attribute 172.
    pub old_build_value_razings: Tally,
    /// `old_total_razings_other` (`+0x34`): attribute 155.
    pub old_total_razings_other: Tally,
    /// `old_build_value_razings_other` (`+0x38`): attribute 153.
    pub old_build_value_razings_other: Tally,
    /// `old_total_kills` (`+0x3c`): attribute 20.
    pub old_total_kills: Tally,
    /// `old_build_value_kills` (`+0x40`): attribute 170.
    pub old_build_value_kills: Tally,
    /// `old_total_kills_other` (`+0x44`): attribute 154.
    pub old_total_kills_other: Tally,
    /// `old_build_value_kills_other` (`+0x48`): attribute 152.
    pub old_build_value_kills_other: Tally,
    /// `razings_weight` (`+0x4c`): saved; no routine of 158041 reads it.
    pub razings_weight: f32,
    /// `kills_weight` (`+0x50`): saved; no routine reads it.
    pub kills_weight: f32,
    /// `razing_percent` (`+0x54`): saved; no routine reads it.
    pub razing_percent: Share,
    /// `kills_perecent` (`+0x58`): saved; no routine reads it.
    pub kills_percent: Share,
    /// `razings_flag` (`+0x5c`): saved; no routine reads it.
    pub razings_flag: u32,
    /// `battle_flag` (`+0x60`): a battle goes on.
    pub battle_flag: u32,
    /// `old_current_build_value_units` (`+0x64`): attribute 164.
    pub old_current_build_value_units: Tally,
    /// `old_current_build_value_buildings` (`+0x68`): attribute 165.
    pub old_current_build_value_buildings: Tally,
    /// `old_player_kill_BV` (`+0xb0`, `numMaxPlayers` of them): attribute 400 + i, outside a
    /// battle.
    pub old_player_kill_bv: Vec<Tally>,
    /// `old_player_razing_BV` (`+0xc8`): attribute 425 + i, outside a battle.
    pub old_player_razing_bv: Vec<Tally>,
    /// `running_ave_BV_percent` (`+0xe0`): the battle's share of the player's value so far.
    pub running_ave_bv_percent: Share,
    /// `running_total_BV_kills` (`+0xe4`).
    pub running_total_bv_kills: Tally,
    /// `running_total_BV_razings` (`+0xe8`).
    pub running_total_bv_razings: Tally,
    /// `running_total_kills` (`+0xec`; the save keeps its low 16 bits).
    pub running_total_kills: Tally,
    /// `running_total_razings` (`+0xf0`; the save keeps its low 16 bits).
    pub running_total_razings: Tally,
}

impl History {
    /// `add_new_history_entry` (`FUN_141560b90`): the buffer grows by 60 once full (its old
    /// capacity copied, the rest zero); an entry of the population kind holds the two
    /// populations (the floats the update hands over, as shorts: [`Population::of_count`]), any
    /// other kind leaves its slot zero; the count goes up either way.
    pub fn add_entry(&mut self, kind: EntryKind, civilian: Population, military: Population) {
        if self.max_entries <= TimeSlice::of_count(self.entries.len()) {
            self.max_entries = self.max_entries.wrapping_add(ENTRY_GROWTH);
        }
        self.entries.push(if kind == EntryKind::POPULATION {
            HistoryEntry { civilian, military }
        } else {
            HistoryEntry::default()
        });
    }

    /// `add_history_event` (`FUN_141560ae0`): with at least one entry, an event of `kind` at
    /// the entries' count and the world's time joins the list's head (`data3` the byte as a
    /// float); a battle then goes through [`History::mark_new_battle`].
    pub fn add_event(
        &mut self,
        kind: EventKind,
        data1: f32,
        data2: f32,
        data3: u8,
        world_time: WorldTime,
    ) {
        if self.entries.is_empty() {
            return;
        }
        self.events.insert(
            0,
            Event {
                kind,
                time_slice: TimeSlice::of_count(self.entries.len()),
                world_time,
                data1,
                data2,
                data3: f32::from(data3),
            },
        );
        if kind == EventKind::Battle {
            let _ = self.mark_new_battle();
        }
    }

    /// `mark_new_battle` (`FUN_141565470`) on the new battle at the list's head: its `data3`
    /// becomes 1. With more than three battles, the one among the three newest (the new one
    /// first) whose `data2` lies furthest below the new one's goes (`COMISS` with `JBE`: a NaN
    /// never qualifies, the first of equal gaps wins); with none below it the new battle goes
    /// instead. True unless the new battle went.
    pub fn mark_new_battle(&mut self) -> bool {
        let battles = self
            .events
            .iter()
            .filter(|e| e.kind == EventKind::Battle)
            .count();
        let Some(new) = self.events.first_mut() else {
            return true;
        };
        new.data3 = 1.0;
        let new = new.data2;
        if battles <= BATTLES_KEPT {
            return true;
        }
        let mut seen = 0;
        let mut widest = 0.0f32;
        let mut furthest = None;
        for (i, e) in self.events.iter().enumerate() {
            if BATTLES_COMPARED <= seen {
                break;
            }
            if e.kind != EventKind::Battle {
                continue;
            }
            seen += 1;
            if new > e.data2 {
                let gap = new - e.data2;
                if gap > widest {
                    widest = gap;
                    furthest = Some(i);
                }
            }
        }
        if let Some(i) = furthest {
            let _ = self.events.remove(i);
            true
        } else {
            let _ = self.events.remove(0);
            false
        }
    }

    /// `FUN_141560e20`: the newest event of `kind`.
    #[must_use]
    pub fn event_of_kind(&self, kind: EventKind) -> Option<&Event> {
        self.events.iter().find(|e| e.kind == kind)
    }

    /// `get_history_event_by_time` (`FUN_141560ea0`, the post-game timeline's): the events of
    /// `time_slice`, as the position of the first from the head (1 based) and their count (a
    /// byte, wrapping); none gives `None` (the game's -1 and -1).
    #[must_use]
    pub fn events_at(&self, time_slice: TimeSlice) -> Option<(i32, i8)> {
        let mut first = None;
        let mut count = 0i8;
        for (i, e) in self.events.iter().enumerate() {
            if e.time_slice == time_slice {
                count = count.wrapping_add(1);
                let _ = first.get_or_insert(i as i32 + 1);
            }
        }
        first.map(|f| (f, count))
    }

    /// The AI's current age time (`FUN_140b077d0`, its fact's): the world's seconds, less those
    /// of the event of the player's age (attribute 6 through `CVTTSS2SI`: 1 Feudal, 2 Castle, 3
    /// Imperial) when the history holds one; for another age the world's seconds.
    #[must_use]
    pub fn age_time(&self, current_age: Amount, world_time: WorldTime) -> u32 {
        let now = world_time.millis() / 1000;
        let kind = match cvttss2si(current_age.get()) {
            1 => EventKind::FeudalAge,
            2 => EventKind::CastleAge,
            3 => EventKind::ImperialAge,
            _ => return now,
        };
        self.event_of_kind(kind)
            .map_or(now, |e| now.wrapping_sub(e.world_time.millis() / 1000))
    }

    /// `update` (`FUN_141569450`), once a game second with the player's `attributes`, the world's
    /// time and the history period (`world+0x8b8`, the `.dat`'s `time_slice`). Every period:
    /// - an entry of the civilian and military population (attributes 37 and 40 as shorts; both
    ///   0 when their sum is below 0),
    /// - the kills and razings since the last period against the player's value at the last period
    ///   (attributes 164 and 165): the share of its own kills' and razings' value (attributes 170
    ///   and 172) at 0.12 or more
    ///   ([`HistoryTuning::battle_start`](crate::world::layer::generated::HistoryTuning::battle_start),
    ///   as doubles) starts or goes on with a battle, which sums them and the share of all four
    ///   (with 152 and 153); a battle that falls below it ends, recorded as an event when its
    ///   summed share reached 0.15
    ///   ([`HistoryTuning::battle_record`](crate::world::layer::generated::HistoryTuning::battle_record))
    ///   with the opponent whose razings' value (attribute 425 + i) over the player's value plus
    ///   kills' value (400 + i) since the battle began is the largest (the first of equals, none
    ///   above 0 gives player 0),
    /// - and the attributes are kept for the next period, the per-player ones outside a battle.
    pub fn update(
        &mut self,
        attributes: &[Amount],
        world_time: WorldTime,
        period: HistoryUpdates,
        slots: Slots,
        history_tuning: &HistoryTuning,
    ) -> Result<(), GenieError> {
        self.update_count = self.update_count.next();
        if self.update_count < period {
            return Ok(());
        }
        self.update_count = self.update_count.wrapping_sub(period);
        // the save holds every attribute of the .dat's count, these among them
        let a = |i: usize| attributes.get(i).copied().unwrap_or(Amount::ZERO);
        let tally = |i: usize| Tally::of_amount(a(i));
        let civilian = a(slots.attribute.civilian_population.index());
        let military = a(slots.attribute.military_population.index());
        if military + civilian < Amount::ZERO {
            self.add_entry(EntryKind::POPULATION, Population::ZERO, Population::ZERO);
        } else {
            self.add_entry(
                EntryKind::POPULATION,
                Population::of_count(civilian.get()),
                Population::of_count(military.get()),
            );
        }
        let kills = tally(slots.attribute.kills.index()).wrapping_sub(self.old_total_kills);
        let kills_value =
            tally(slots.attribute.kills_value.index()).wrapping_sub(self.old_build_value_kills);
        let razings = tally(slots.attribute.razings.index()).wrapping_sub(self.old_total_razings);
        let razings_value =
            tally(slots.attribute.razings_value.index()).wrapping_sub(self.old_build_value_razings);
        let kills_other =
            tally(slots.attribute.kills_other.index()).wrapping_sub(self.old_total_kills_other);
        let kills_value_other = tally(slots.attribute.kills_value_other.index())
            .wrapping_sub(self.old_build_value_kills_other);
        let razings_other =
            tally(slots.attribute.razings_other.index()).wrapping_sub(self.old_total_razings_other);
        let razings_value_other = tally(slots.attribute.razings_value_other.index())
            .wrapping_sub(self.old_build_value_razings_other);
        let value = self
            .old_current_build_value_buildings
            .wrapping_add(self.old_current_build_value_units);
        let (share, total_share) = if value <= Tally::ZERO {
            (Share::ZERO, Share::ZERO)
        } else {
            let own = razings_value.wrapping_add(kills_value);
            let all = razings_value_other
                .wrapping_add(kills_value_other)
                .wrapping_add(razings_value)
                .wrapping_add(kills_value);
            (Share::of(own, value), Share::of(all, value))
        };
        let battling = share.battling(history_tuning);
        let goes_on = if self.battle_flag == 0 {
            if battling {
                self.battle_flag = 1;
            }
            battling
        } else if battling {
            true
        } else {
            if self.running_ave_bv_percent.recorded(history_tuning) {
                self.record_battle(attributes, value, world_time)?;
            }
            self.battle_flag = 0;
            self.running_total_bv_kills = Tally::ZERO;
            self.running_total_bv_razings = Tally::ZERO;
            self.running_total_kills = Tally::ZERO;
            self.running_total_razings = Tally::ZERO;
            self.running_ave_bv_percent = Share::ZERO;
            false
        };
        if goes_on {
            self.running_total_bv_kills = self
                .running_total_bv_kills
                .wrapping_add(kills_value_other.wrapping_add(kills_value));
            self.running_total_bv_razings = self
                .running_total_bv_razings
                .wrapping_add(razings_value_other.wrapping_add(razings_value));
            self.running_total_kills = self
                .running_total_kills
                .wrapping_add(kills.low_short().wrapping_add(kills_other.low_short()));
            self.running_total_razings = self
                .running_total_razings
                .wrapping_add(razings.low_short().wrapping_add(razings_other.low_short()));
            // total_share + running in the game; the share is never a NaN, so the order is moot
            self.running_ave_bv_percent = self.running_ave_bv_percent.plus(total_share);
        }
        self.old_total_razings = tally(slots.attribute.razings.index());
        self.old_build_value_razings = tally(slots.attribute.razings_value.index());
        self.old_total_razings_other = tally(slots.attribute.razings_other.index());
        self.old_build_value_razings_other = tally(slots.attribute.razings_value_other.index());
        self.old_total_kills = tally(slots.attribute.kills.index());
        self.old_build_value_kills = tally(slots.attribute.kills_value.index());
        self.old_total_kills_other = tally(slots.attribute.kills_other.index());
        self.old_build_value_kills_other = tally(slots.attribute.kills_value_other.index());
        self.old_current_build_value_units = tally(slots.attribute.units_value.index());
        self.old_current_build_value_buildings = tally(slots.attribute.buildings_value.index());
        if self.battle_flag == 0 {
            for (i, kill) in self.old_player_kill_bv.iter_mut().enumerate() {
                *kill = Tally::of_amount_64(a(KILL_VALUE_OF + i));
                if let Some(r) = self.old_player_razing_bv.get_mut(i) {
                    *r = Tally::of_amount_64(a(RAZING_VALUE_OF + i));
                }
            }
        }
        Ok(())
    }

    /// The end of a battle worth recording (`FUN_141569450` from `0x141569a2a`): each player's
    /// kills' and razings' value since the battle began (`CVTTSS2SI` into 64 bits, the low 32
    /// kept), the opponent and the event, with the summed share as its `data2`.
    fn record_battle(
        &mut self,
        attributes: &[Amount],
        value: Tally,
        world_time: WorldTime,
    ) -> Result<(), GenieError> {
        let a = |i: usize| attributes.get(i).copied().unwrap_or(Amount::ZERO);
        let players = self.old_player_kill_bv.len();
        let kills: Vec<Tally> = self
            .old_player_kill_bv
            .iter()
            .enumerate()
            .map(|(i, &old)| Tally::of_amount_64(a(KILL_VALUE_OF + i)).wrapping_sub(old))
            .collect();
        let razings: Vec<Tally> = (0..players)
            .map(|i| {
                let old = self
                    .old_player_razing_bv
                    .get(i)
                    .copied()
                    .unwrap_or(Tally::ZERO);
                Tally::of_amount_64(a(RAZING_VALUE_OF + i)).wrapping_sub(old)
            })
            .collect();
        let mut best = 0.0f32;
        let mut opponent = PlayerId::GAIA;
        for (i, q) in PlayerId::each(players).enumerate() {
            let weight = razings.checked(i)?.ratio(value) + kills.checked(i)?.get() as f32;
            if weight > best {
                best = weight;
                opponent = q;
            }
        }
        if self.entries.is_empty() {
            return Ok(());
        }
        self.events.insert(
            0,
            Event {
                kind: EventKind::Battle,
                time_slice: TimeSlice::of_count(self.entries.len()),
                world_time,
                data1: opponent.index() as f32,
                data2: self.running_ave_bv_percent.get(),
                data3: 0.0,
            },
        );
        let _ = self.mark_new_battle();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::attribute;
    use crate::testing::slots;
    use crate::testing::tuning;

    fn entry(civilian: i16, military: i16) -> HistoryEntry {
        HistoryEntry {
            civilian: Population::of_data(civilian),
            military: Population::of_data(military),
        }
    }

    fn tallies(values: &[i32]) -> Vec<Tally> {
        values.iter().copied().map(Tally::of_data).collect()
    }

    /// The update with the period of 30 runs.
    fn update(h: &mut History, a: &[Amount], millis: u32) {
        h.update(
            a,
            WorldTime::from_millis(millis),
            HistoryUpdates::of_data(30),
            slots(),
            &tuning().history,
        )
        .unwrap();
    }

    /// A saved byte is its kind; the default is the Feudal Age's 0.
    #[test]
    fn saved_byte_is_its_event_kind() {
        assert_eq!(EventKind::from(11), EventKind::Battle);
        assert_eq!(EventKind::Defeat.get(), 20);
        assert_eq!(EventKind::default(), EventKind::FeudalAge);
    }

    fn battle(data2: f32) -> Event {
        Event {
            kind: EventKind::Battle,
            data2,
            ..Event::default()
        }
    }

    fn with_entry() -> History {
        History {
            entries: vec![HistoryEntry::default()],
            max_entries: TimeSlice::of_data(60),
            ..History::default()
        }
    }

    /// `add_new_history_entry`: a full buffer grows by 60 first; an entry of kind 0 holds the
    /// values through `CVTTSS2SI` as shorts, another kind leaves zeros, and NaN is the integer
    /// indefinite's low half, 0.
    #[test]
    fn an_entry_grows_the_buffer_by_sixty_and_keeps_shorts() {
        let pop = |c: f32, m: f32| (Population::of_count(c), Population::of_count(m));
        let mut h = History::default();
        let (c, m) = pop(7.9, 70_000.0);
        h.add_entry(EntryKind::POPULATION, c, m);
        assert_eq!(
            (h.max_entries, h.entries.clone()),
            (TimeSlice::of_data(60), vec![entry(7, 70_000_i32 as i16)])
        );
        let (c, m) = pop(3.0, 4.0);
        h.add_entry(EntryKind::of_data(1), c, m);
        let (c, m) = pop(f32::NAN, -2.5);
        h.add_entry(EntryKind::POPULATION, c, m);
        assert_eq!(h.entries, vec![entry(7, 4464), entry(0, 0), entry(0, -2)]);
        h.entries.resize(60, HistoryEntry::default());
        let (c, m) = pop(1.0, 1.0);
        h.add_entry(EntryKind::POPULATION, c, m);
        assert_eq!(
            (h.max_entries, h.entries.len()),
            (TimeSlice::of_data(120), 61)
        );
    }

    /// `add_history_event`: nothing without an entry; else the event at the head, at the
    /// entries' count and the world's time, its byte as `data3`.
    #[test]
    fn an_event_needs_an_entry_and_joins_the_head() {
        let mut h = History::default();
        h.add_event(
            EventKind::CastleBuilt,
            2.0,
            0.0,
            0,
            WorldTime::from_millis(5000),
        );
        assert_eq!(h.events, []);
        h.entries = vec![HistoryEntry::default(); 3];
        h.add_event(
            EventKind::FeudalAge,
            0.0,
            0.0,
            0,
            WorldTime::from_millis(5000),
        );
        h.add_event(
            EventKind::CastleBuilt,
            2.0,
            0.5,
            7,
            WorldTime::from_millis(9000),
        );
        assert_eq!(
            h.events,
            vec![
                Event {
                    kind: EventKind::CastleBuilt,
                    time_slice: TimeSlice::of_data(3),
                    world_time: WorldTime::from_millis(9000),
                    data1: 2.0,
                    data2: 0.5,
                    data3: 7.0,
                },
                Event {
                    kind: EventKind::FeudalAge,
                    time_slice: TimeSlice::of_data(3),
                    world_time: WorldTime::from_millis(5000),
                    ..Event::default()
                },
            ]
        );
        assert_eq!(h.number_of_events, 0);
    }

    /// `mark_new_battle`: up to three battles all stay (the new one's `data3` 1); a fourth
    /// pushes out the battle among the three newest furthest below it, or goes itself.
    #[test]
    fn fourth_battle_pushes_out_the_weakest_of_the_newest_three() {
        let mut h = with_entry();
        for d in [0.2, 0.9, 0.3] {
            h.add_event(EventKind::Battle, 1.0, d, 0, WorldTime::ZERO);
        }
        assert_eq!(h.events.len(), 3);
        assert!(h.events.iter().all(|e| e.data3 == 1.0));
        // the newest three are 0.5 (new), 0.3, 0.9: 0.3 lies furthest below
        h.add_event(EventKind::Battle, 1.0, 0.5, 0, WorldTime::ZERO);
        let d: Vec<f32> = h.events.iter().map(|e| e.data2).collect();
        assert_eq!(d, vec![0.5, 0.9, 0.2]);
        // a new battle below the newest three goes itself; 0.2 is fourth and not compared
        h.events.insert(
            0,
            Event {
                kind: EventKind::FeudalAge,
                ..Event::default()
            },
        );
        h.add_event(EventKind::Battle, 1.0, 0.4, 0, WorldTime::ZERO);
        assert_eq!(
            h.events
                .iter()
                .filter(|e| e.kind == EventKind::Battle)
                .count(),
            3
        );
        assert_eq!(h.events[0].kind, EventKind::FeudalAge);
        // a NaN gap never qualifies
        h.events.insert(0, battle(f32::NAN));
        assert!(!h.mark_new_battle());
        assert_eq!(h.events.len(), 4);
    }

    /// `mark_new_battle` scans the newest battles in list order: the first of two equal gaps
    /// wins, and other events between them do not count.
    #[test]
    fn equal_gaps_push_out_the_newer_battle() {
        let mut h = with_entry();
        h.events = vec![battle(0.1), Event::default(), battle(0.1), battle(0.9)];
        h.add_event(EventKind::Battle, 3.0, 0.6, 0, WorldTime::ZERO);
        let d: Vec<f32> = h.events.iter().map(|e| e.data2).collect();
        assert_eq!(d, vec![0.6, 0.0, 0.1, 0.9]);
    }

    /// `FUN_141560e20` and `get_history_event_by_time`: the newest of a kind; the first position
    /// (1 based) and count of a time slice's events.
    #[test]
    fn the_readers_find_the_newest_event_and_time_slice_events() {
        let h = History {
            events: vec![
                Event {
                    kind: EventKind::CastleAge,
                    time_slice: TimeSlice::of_data(4),
                    world_time: WorldTime::from_millis(9000),
                    ..Event::default()
                },
                Event {
                    kind: EventKind::FeudalAge,
                    time_slice: TimeSlice::of_data(2),
                    world_time: WorldTime::from_millis(4000),
                    ..Event::default()
                },
                Event {
                    kind: EventKind::FeudalAge,
                    time_slice: TimeSlice::of_data(2),
                    world_time: WorldTime::from_millis(1000),
                    ..Event::default()
                },
            ],
            ..History::default()
        };
        assert_eq!(
            h.event_of_kind(EventKind::FeudalAge).map(|e| e.world_time),
            Some(WorldTime::from_millis(4000))
        );
        assert_eq!(h.event_of_kind(EventKind::ImperialAge), None);
        assert_eq!(h.events_at(TimeSlice::of_data(2)), Some((2, 2)));
        assert_eq!(h.events_at(TimeSlice::of_data(4)), Some((1, 1)));
        assert_eq!(h.events_at(TimeSlice::of_data(3)), None);
    }

    /// `FUN_140b077d0`: the seconds since the current age's event, else the world's seconds.
    #[test]
    fn the_age_time_counts_from_the_age_event() {
        let h = History {
            events: vec![Event {
                kind: EventKind::CastleAge,
                world_time: WorldTime::from_millis(600_999),
                ..Event::default()
            }],
            ..History::default()
        };
        assert_eq!(
            h.age_time(Amount::new(2.0), WorldTime::from_millis(900_500)),
            300
        );
        assert_eq!(
            h.age_time(Amount::new(1.9), WorldTime::from_millis(900_500)),
            900
        );
        assert_eq!(
            h.age_time(Amount::new(3.0), WorldTime::from_millis(900_500)),
            900
        );
        assert_eq!(
            h.age_time(Amount::new(0.0), WorldTime::from_millis(900_500)),
            900
        );
    }

    fn attributes() -> Vec<Amount> {
        vec![Amount::ZERO; 450]
    }

    /// The update counts its runs: the period's run adds the entry and keeps the attributes.
    #[test]
    fn the_period_update_adds_an_entry_and_keeps_the_attributes() {
        let mut h = History {
            update_count: HistoryUpdates::of_data(28),
            old_player_kill_bv: vec![Tally::ZERO; 3],
            old_player_razing_bv: vec![Tally::ZERO; 3],
            ..History::default()
        };
        let mut a = attributes();
        a[attribute::CIVILIAN_POPULATION.index()] = Amount::new(12.0);
        a[attribute::MILITARY_POPULATION.index()] = Amount::new(3.0);
        a[attribute::KILLS.index()] = Amount::new(2.0);
        a[attribute::UNITS_VALUE.index()] = Amount::new(500.0);
        a[attribute::BUILDINGS_VALUE.index()] = Amount::new(900.7);
        a[KILL_VALUE_OF + 2] = Amount::new(45.0);
        a[RAZING_VALUE_OF + 1] = Amount::new(5_000_000_000.0);
        update(&mut h, &a, 0);
        assert_eq!(
            (h.update_count, h.entries.len()),
            (HistoryUpdates::of_data(29), 0)
        );
        update(&mut h, &a, 0);
        assert_eq!(
            (h.update_count, h.entries.clone()),
            (HistoryUpdates::of_data(0), vec![entry(12, 3)])
        );
        assert_eq!(
            (h.old_total_kills, h.old_current_build_value_buildings),
            (Tally::of_data(2), Tally::of_data(900))
        );
        assert_eq!(h.old_player_kill_bv, tallies(&[0, 0, 45]));
        // 5e9 through the 64-bit conversion keeps its low half
        assert_eq!(
            h.old_player_razing_bv,
            tallies(&[0, 5_000_000_000_i64 as i32, 0])
        );
        // a negative population sum gives an entry of zeros
        a[attribute::MILITARY_POPULATION.index()] = Amount::new(-13.0);
        h.update_count = HistoryUpdates::of_data(29);
        update(&mut h, &a, 0);
        assert_eq!(h.entries[1], entry(0, 0));
    }

    /// A share of the player's value of 0.12 or more starts a battle and sums it; falling below
    /// ends it, with an event when the summed share reached 0.15, against the player whose
    /// razings over the value plus kills since the start weigh most.
    #[test]
    fn battle_starts_sums_and_ends_with_its_event() {
        let mut h = History {
            update_count: HistoryUpdates::of_data(29),
            old_current_build_value_units: Tally::of_data(600),
            old_current_build_value_buildings: Tally::of_data(400),
            old_player_kill_bv: vec![Tally::ZERO; 3],
            old_player_razing_bv: vec![Tally::ZERO; 3],
            ..History::default()
        };
        let mut a = attributes();
        a[attribute::UNITS_VALUE.index()] = Amount::new(600.0);
        a[attribute::BUILDINGS_VALUE.index()] = Amount::new(400.0);
        a[attribute::KILLS_VALUE.index()] = Amount::new(100.0);
        a[attribute::RAZINGS_VALUE.index()] = Amount::new(30.0);
        a[attribute::KILLS_VALUE_OTHER.index()] = Amount::new(30.0);
        a[attribute::KILLS.index()] = Amount::new(70_000.0);
        a[attribute::KILLS_OTHER.index()] = Amount::new(3.0);
        a[attribute::RAZINGS.index()] = Amount::new(1.0);
        a[KILL_VALUE_OF + 1] = Amount::new(50.0);
        a[KILL_VALUE_OF + 2] = Amount::new(40.0);
        a[RAZING_VALUE_OF + 2] = Amount::new(20_000.0);
        // (30 + 100) / 1000: the battle starts; all four: 160 / 1000
        update(&mut h, &a, 30_000);
        assert_eq!(h.battle_flag, 1);
        assert_eq!(
            (h.running_total_bv_kills, h.running_total_bv_razings),
            (Tally::of_data(130), Tally::of_data(30))
        );
        assert_eq!(
            (h.running_total_kills, h.running_total_razings),
            (Tally::of_data(4464 + 3), Tally::of_data(1))
        );
        assert_eq!(h.running_ave_bv_percent, Share::of_data(160.0 / 1000.0));
        // in a battle the per-player values stay as they were
        assert_eq!(h.old_player_kill_bv, tallies(&[0, 0, 0]));
        // nothing more: the share falls to 0, the battle ends with its event; player 2's
        // 20000 / 1000 + 40 outweighs player 1's 50
        h.update_count = HistoryUpdates::of_data(29);
        update(&mut h, &a, 60_000);
        assert_eq!(h.battle_flag, 0);
        assert_eq!(h.running_ave_bv_percent, Share::ZERO);
        assert_eq!(
            h.events,
            vec![Event {
                kind: EventKind::Battle,
                time_slice: TimeSlice::of_data(2),
                world_time: WorldTime::from_millis(60_000),
                data1: 2.0,
                data2: 160.0 / 1000.0,
                data3: 1.0,
            }]
        );
        assert_eq!(h.old_player_kill_bv, tallies(&[0, 50, 40]));
        assert_eq!(h.old_player_razing_bv, tallies(&[0, 0, 20_000]));
    }

    /// A battle whose summed share stays below 0.15 ends without an event; a saved battle flag
    /// other than 1 goes on as a battle.
    #[test]
    fn small_battle_ends_without_its_event() {
        let mut h = History {
            update_count: HistoryUpdates::of_data(29),
            battle_flag: 2,
            running_ave_bv_percent: Share::of_data(0.149_999),
            old_current_build_value_units: Tally::of_data(1000),
            old_player_kill_bv: vec![Tally::ZERO; 2],
            old_player_razing_bv: vec![Tally::ZERO; 2],
            ..History::default()
        };
        let mut a = attributes();
        a[attribute::UNITS_VALUE.index()] = Amount::new(1000.0);
        a[attribute::KILLS_VALUE.index()] = Amount::new(130.0);
        update(&mut h, &a, 0);
        assert_eq!(h.battle_flag, 2);
        h.update_count = HistoryUpdates::of_data(29);
        h.running_ave_bv_percent = Share::of_data(0.149_999);
        update(&mut h, &a, 0);
        assert_eq!((h.battle_flag, h.events.len()), (0, 0));
        // a NaN sum fails the record's COMISD (JC): no event either
        h.update_count = HistoryUpdates::of_data(29);
        h.battle_flag = 1;
        h.running_ave_bv_percent = Share::of_data(f32::NAN);
        update(&mut h, &a, 0);
        assert_eq!((h.battle_flag, h.events.len()), (0, 0));
        assert_eq!(h.running_ave_bv_percent, Share::ZERO);
    }

    /// The share is a float compared as a double: 120 of 1000 as an f32 lies just below the
    /// double 0.12 and starts no battle.
    #[test]
    fn share_of_exactly_twelve_percent_starts_no_battle() {
        let mut h = History {
            update_count: HistoryUpdates::of_data(29),
            old_current_build_value_units: Tally::of_data(1000),
            ..History::default()
        };
        let mut a = attributes();
        a[attribute::UNITS_VALUE.index()] = Amount::new(1000.0);
        a[attribute::KILLS_VALUE.index()] = Amount::new(120.0);
        update(&mut h, &a, 0);
        assert_eq!(h.battle_flag, 0);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_event_kind_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 4, 5, 6, 7, 11, 20, 3] {
            assert_eq!(EventKind::of_raw(value).raw(), value);
        }
        assert!(matches!(EventKind::of_raw(3), EventKind::Other(_)));
    }
}
