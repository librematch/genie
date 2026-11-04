//! The reader of a player's state ([`Player`]) in the recording header.

use crate::ObjectId;
use crate::ai::PlayerAi;
use crate::element::{ReadHeader, ReadOptionalHeader};
use crate::fits;
use crate::player::{
    DiplomacyOffer, GaiaData, HistoryInfo, Player, PlayerTech, UserPatchData, VisibleMap,
    VisibleResources,
};
use crate::reader::RecordingHeaderReader;
use crate::unit::Unit;
use crate::unit_kind::CompactUnitKind;
use byteorder::{LE, ReadBytesExt};
use genie_dat::tech_tree::TechTree;
use genie_error::GenieResult;
use genie_support::{ReadStringsExt, read_bytes, read_le};
use std::io::Read;

impl ReadHeader for Player {
    // Reads the player's fields in file order, section by section; the sections it does not
    // keep it skips.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut player = Self::read_diplomacy_and_name(input)?;
        player.read_attributes_and_views(input)?;
        player.read_civilization_and_status(input)?;
        skip_object_counts(input)?;
        skip_formations_and_escrow(input)?;
        skip_view_ai_and_selection(input)?;
        skip_updates_and_attacks(input)?;
        player.read_techs_and_history(input)?;
        skip_diplomacy_and_trade_routes(input)?;
        skip_market(input)?;
        skip_queues_and_dodging(input)?;
        skip_old_battle_stats(input)?;
        player.read_tech_tree_ai_and_gaia(input)?;
        player.read_unit_kinds(input)?;
        player.read_maps_and_units(input)?;
        Ok(player)
    }
}

/// From 10.55 on the format puts check bytes between the sections (11, 22 or 33).
///
/// # Errors
///
/// Fails if the byte read is not `value`.
fn expect_marker<R: Read>(input: &mut RecordingHeaderReader<R>, value: u8) -> GenieResult<()> {
    if input.version() >= 10.55 {
        assert_eq!(input.read_u8()?, value);
    }
    Ok(())
}

impl Player {
    /// The kind, the relations to each player, the diplomacy, the allied flags and the name:
    /// the first fields.
    fn read_diplomacy_and_name<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut player = Self {
            kind: input.read_u8()?,
            ..Default::default()
        };
        expect_marker(input, 11)?;
        let players = usize::from(input.num_players());
        player.relations = read_bytes(input, players)?;
        input.read_u32_into::<LE>(&mut player.diplomacy)?;
        player.allied_los = input.read_u32::<LE>()? != 0;
        player.allied_victory = input.read_u8()? != 0;
        player.name = input.read_u16_length_prefixed_str()?.unwrap_or_default();
        dbg!(input.version());
        Ok(player)
    }

    /// The attributes behind their count, the initial view and the saved views (11.62; a
    /// negative count is none).
    fn read_attributes_and_views<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        expect_marker(input, 22)?;
        let num_attributes = input.read_u32::<LE>()?;
        expect_marker(input, 33)?;
        self.attributes = read_le(input, fits(num_attributes)?, f32::from_le_bytes)?;
        expect_marker(input, 11)?;
        self.initial_view = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        if input.version() >= 11.62 {
            let num_saved_views = input.read_i32::<LE>()?;
            // saved view count can be negative
            self.saved_views = (0..num_saved_views.max(0))
                .map(|_| Ok((input.read_f32::<LE>()?, input.read_f32::<LE>()?)))
                .collect::<GenieResult<_>>()?;
        }
        Ok(())
    }

    /// The spawn location, the culture and civilization, the game status and resignation,
    /// then the colour and the pathing caps, which the reader skips.
    fn read_civilization_and_status<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        self.spawn_location = (input.read_u16::<LE>()?, input.read_u16::<LE>()?);
        self.culture_id = input.read_u8()?;
        self.civilization_id = input.read_u8()?.into();
        self.game_status = input.read_u8()?;
        self.resigned = input.read_u8()? != 0;
        expect_marker(input, 11)?;
        let _color = input.read_u8()?;
        expect_marker(input, 11)?;
        let _pathing_attempt_cap = input.read_u32::<LE>()?;
        let _pathing_delay_cap = input.read_u32::<LE>()?;
        Ok(())
    }

    /// The `UserPatch` data (at 11.97 only), the technologies, the history and the ruin and
    /// artifact times (5.30).
    fn read_techs_and_history<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        // if is userpatch
        if genie_support::f32_eq!(input.version(), 11.97) {
            self.userpatch_data = Some(UserPatchData::read_from(input)?);
        }
        self.tech_state = PlayerTech::read_from(input)?;
        let _update_history_count = input.read_u32::<LE>()?;
        self.history_info = HistoryInfo::read_from(input)?;
        if input.version() >= 5.30 {
            let _ruin_held_time = input.read_u32::<LE>()?;
            let _artifact_held_time = input.read_u32::<LE>()?;
        }
        expect_marker(input, 11)
    }

    /// The tech tree (9.38), the AI of a computer player (kind 3, behind a flag) and Gaia's
    /// data (kind 2).
    fn read_tech_tree_ai_and_gaia<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.version() >= 9.38 {
            self.tech_tree = Some(TechTree::read_from(&mut *input)?);
        }
        expect_marker(input, 11)?;
        if self.kind == 3 && input.read_u32::<LE>()? == 1 {
            let _player_ai = PlayerAi::read_from(input)?;
        }
        expect_marker(input, 11)?;
        if self.kind == 2 {
            self.gaia = Some(GaiaData::read_from(input)?);
        }
        expect_marker(input, 11)
    }

    /// The unit types: a flag for each, then the available ones (from 10.55 between the check
    /// bytes 22 and 33).
    fn read_unit_kinds<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        let num_unit_kinds = input.read_u32::<LE>()?;
        let available_unit_kinds = (0..num_unit_kinds)
            .map(|_| Ok(input.read_u32::<LE>()? != 0))
            .collect::<GenieResult<Vec<_>>>()?;
        expect_marker(input, 11)?;
        self.unit_kinds = available_unit_kinds
            .into_iter()
            .map(|available| available.then(|| read_unit_kind(input)).transpose())
            .collect::<GenieResult<_>>()?;
        expect_marker(input, 11)
    }

    /// The visible map and resources, then the units, the sleeping units and the
    /// doppelgangers.
    fn read_maps_and_units<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        self.visible_map = VisibleMap::read_from(input)?;
        self.visible_resources = VisibleResources::read_from(input)?;
        expect_marker(input, 11)?;
        self.units = read_unit_list(input)?;
        expect_marker(input, 11)?;
        self.sleeping_units = read_unit_list(input)?;
        expect_marker(input, 11)?;
        self.doppelganger_units = read_unit_list(input)?;
        expect_marker(input, 11)
    }
}

/// An available unit type between its check bytes.
fn read_unit_kind<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<CompactUnitKind> {
    expect_marker(input, 22)?;
    let ty = CompactUnitKind::read_from(input)?;
    expect_marker(input, 33)?;
    Ok(ty)
}

/// A unit list: its size and growth, then units up to one of class 0.
fn read_unit_list<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Vec<Unit>> {
    let _list_size = input.read_u32::<LE>()?;
    let _grow_size = input.read_u32::<LE>()?;
    let mut units = vec![];
    while let Some(unit) = Unit::read_from(input)? {
        units.push(unit);
    }
    Ok(units)
}

/// The counts of objects and built objects per category and group (900 categories from
/// 11.65, 850 from 11.51, 750 before) and their totals: skipped.
fn skip_object_counts<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    let categories = if input.version() >= 11.65 {
        900
    } else if input.version() >= 11.51 {
        850
    } else {
        750
    };
    let mut object_categories_count = vec![0; categories];
    input.read_u16_into::<LE>(&mut object_categories_count)?;
    let mut object_groups_count = vec![0; 100];
    input.read_u16_into::<LE>(&mut object_groups_count)?;

    let mut built_object_categories_count = vec![0; categories];
    input.read_u16_into::<LE>(&mut built_object_categories_count)?;
    let mut built_object_groups_count = vec![0; 100];
    input.read_u16_into::<LE>(&mut built_object_groups_count)?;

    let _total_units_count = input.read_u16::<LE>()?;
    let _total_buildings_count = input.read_u16::<LE>()?;
    let _built_units_count = input.read_u16::<LE>()?;
    let _built_buildings_count = input.read_u16::<LE>()?;
    Ok(())
}

/// The formation ratios and distances (the break by speed from 10.81) and the escrow's
/// debits, amounts and percents: skipped.
fn skip_formations_and_escrow<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    let _line_ratio = input.read_u32::<LE>()?;
    let _column_ratio = input.read_u32::<LE>()?;
    let _min_column_distance = input.read_u32::<LE>()?;
    let _column_to_line_distance = input.read_u32::<LE>()?;
    let _auto_formations = input.read_u32::<LE>()?;
    let _formations_influence_distance = input.read_f32::<LE>()?;
    if input.version() >= 10.81 {
        let _break_auto_formations_by_speed = input.read_f32::<LE>()?;
    }
    let mut pending_debits = [0.0; 4];
    input.read_f32_into::<LE>(&mut pending_debits)?;
    let mut escrow_amounts = [0.0; 4];
    input.read_f32_into::<LE>(&mut escrow_amounts)?;
    let mut escrow_percents = [0.0; 4];
    input.read_f32_into::<LE>(&mut escrow_percents)?;
    Ok(())
}

/// The view scrolling (10.51), the AI's reaction percents (11.45) and the selection
/// (11.72): skipped.
fn skip_view_ai_and_selection<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    if input.version() >= 10.51 {
        let _scroll_vector = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        let _scroll_end = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        let _scroll_start = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        let _scroll_total_distance = input.read_f32::<LE>()?;
        let _scroll_distance = input.read_f32::<LE>()?;
    }
    if input.version() >= 11.45 {
        let _easiest_reaction_percent = input.read_f32::<LE>()?;
        let _easier_reaction_percent = input.read_f32::<LE>()?;
        let _task_ungrouped_soldiers = input.read_u8()?;
    }
    if input.version() >= 11.72 {
        let _selection = read_selection(input)?;
    }
    Ok(())
}

/// The selection: its count, then (when there is one) the object, its properties and the
/// selected ids.
fn read_selection<R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> GenieResult<Option<(ObjectId, u32, Vec<ObjectId>)>> {
    let num_selections = input.read_u32::<LE>()?;
    if num_selections == 0 {
        return Ok(None);
    }
    let object_id: ObjectId = input.read_u32::<LE>()?.into();
    let object_properties = input.read_u32::<LE>()?;
    let selected_ids = (0..num_selections)
        .map(|_| Ok(input.read_u32::<LE>()?.into()))
        .collect::<GenieResult<_>>()?;
    Ok(Some((object_id, object_properties, selected_ids)))
}

/// The type and update counts, the AI's attack data (10.02), the fog update and the update
/// time: skipped.
fn skip_updates_and_attacks<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    expect_marker(input, 11)?;
    expect_marker(input, 11)?;
    let _ty = input.read_u8()?;
    let _update_count = input.read_u32::<LE>()?;
    let _update_count_need_help = input.read_u32::<LE>()?;
    if input.version() >= 10.02 {
        let _alerted_enemy_count = input.read_u32::<LE>()?;
        let _regular_attack_count = input.read_u32::<LE>()?;
        let _regular_attack_mode = input.read_u8()?;
        let _regular_attack_location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        let _town_attack_count = input.read_u32::<LE>()?;
        let _town_attack_mode = input.read_u8()?;
        let _town_attack_location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
    }
    let _fog_update = input.read_u32::<LE>()?;
    let _update_time = input.read_f32::<LE>()?;
    Ok(())
}

/// The diplomacy, intelligence and trade towards each of nine players with their offers
/// and the fealty (9.13), then the off-map trade routes explored (9.17) and being explored
/// (9.18): skipped.
fn skip_diplomacy_and_trade_routes<R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> GenieResult<()> {
    if input.version() >= 9.13 {
        for _ in 0..9 {
            let mut diplomacy_intelligence_trade = [0; 3];
            input.read_exact(&mut diplomacy_intelligence_trade)?;
            let _offer = DiplomacyOffer::read_from(input)?;
        }
        let _fealty = input.read_u16::<LE>()?;
    }
    expect_marker(input, 11)?;
    if input.version() >= 9.17 {
        let mut off_map_trade_route_explored = [0; 20];
        input.read_exact(&mut off_map_trade_route_explored)?;
    }
    if input.version() >= 9.18 {
        let mut off_map_trade_route_being_explored = [0; 20];
        input.read_exact(&mut off_map_trade_route_being_explored)?;
    }
    expect_marker(input, 11)
}

/// The market's limits and rates (9.22): skipped.
fn skip_market<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    if input.version() < 9.22 {
        return Ok(());
    }
    let _max_trade_amount = input.read_u32::<LE>()?;
    let _old_max_trade_amount = input.read_u32::<LE>()?;
    let _max_trade_limit = input.read_u32::<LE>()?;
    let _current_wood_limit = input.read_u32::<LE>()?;
    let _current_food_limit = input.read_u32::<LE>()?;
    let _current_stone_limit = input.read_u32::<LE>()?;
    let _current_ore_limit = input.read_u32::<LE>()?;
    let _commodity_volume_delta = input.read_i32::<LE>()?;
    let _trade_vig_rate = input.read_f32::<LE>()?;
    let _trade_refresh_timer = input.read_u32::<LE>()?;
    let _trade_refresh_rate = input.read_u32::<LE>()?;
    Ok(())
}

/// The production queue flag (9.67), the AI's dodging (9.90), the gates opened for pathing
/// (11.42), the farm queue (11.57) and the nomad build lock (11.75): skipped.
fn skip_queues_and_dodging<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    if input.version() >= 9.67 {
        let _prod_queue_enabled = input.read_u8()?;
    }
    if input.version() >= 9.90 {
        let _chance_to_dodge_missiles = input.read_u8()?;
        let _chance_for_archers_to_maintain_distance = input.read_u8()?;
    }
    if input.version() >= 11.42 {
        let _open_gates_for_pathing_count = input.read_u32::<LE>()?;
    }
    if input.version() >= 11.57 {
        let _farm_queue_count = input.read_u32::<LE>()?;
    }
    if input.version() >= 11.75 {
        let _nomad_build_lock = input.read_u32::<LE>()?;
    }
    Ok(())
}

/// The old kills, razings and modes (9.30), the hit points (9.31) and the old kills per
/// player (9.32): skipped.
fn skip_old_battle_stats<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
    if input.version() >= 9.30 {
        let _old_kills = input.read_u32::<LE>()?;
        let _old_razings = input.read_u32::<LE>()?;
        let _battle_mode = input.read_u32::<LE>()?;
        let _razings_mode = input.read_u32::<LE>()?;
        let _total_kills = input.read_u32::<LE>()?;
        let _total_razings = input.read_u32::<LE>()?;
    }
    if input.version() >= 9.31 {
        let _old_hit_points = input.read_u32::<LE>()?;
        let _total_hit_points = input.read_u32::<LE>()?;
    }
    if input.version() >= 9.32 {
        let mut old_player_kills = [0; 9];
        input.read_u32_into::<LE>(&mut old_player_kills)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
