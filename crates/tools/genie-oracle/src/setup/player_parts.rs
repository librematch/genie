//! A player's saved parts the setup imports: its AI settings, its history and its AI module.

use genie_core::map::Distance;
use genie_core::map::GridSize;
use genie_core::map::Tile;
use genie_core::map::WholeDistance;
use genie_core::players::history::HistoryEntry;
use genie_core::players::history::HistoryUpdates;
use genie_core::players::history::Population;
use genie_core::players::history::Share;
use genie_core::players::history::Tally;
use genie_core::players::history::TimeSlice;
use genie_core::world::WorldTime;

/// A player's AI settings as saved (`WorldPlayerBase::save`, `FUN_14166b820`), which GAME's
/// sub-command 0xb sets.
pub(super) const fn ai_settings(
    b: &genie_save::generated::world::WorldPlayerBase,
) -> genie_core::world::AiSettings {
    genie_core::world::AiSettings {
        easiest_reaction_percentage: b.easiest_reaction_percentage,
        easier_reaction_percentage: b.easier_reaction_percentage,
        task_ungrouped_soldiers: b.task_ungrouped_soldiers,
        may_attack_walls: b.may_attack_walls != 0,
        livestock_only_to_tc: b.livestock_only_to_tc != 0,
        disable_build_help: b.disable_build_help != 0,
        local_targeting_mode: b.local_targeting_mode,
        targeting_factor_scalar: b.targeting_factor_scalar,
        preferred_trade_distance: Distance::new(b.preferred_trade_distance),
        disable_tower_priority: b.disable_tower_priority != 0,
        trade_attack_evasion: b.trade_attack_evasion != 0,
        villager_attack_reset: b.villager_attack_reset != 0,
        boar_lure_destination: b.boar_lure_destination,
        attack_move_max_distance: WholeDistance::of_data(b.attack_move_max_distance),
        disable_villager_garrison: b.disable_villager_garrison,
        allow_serjeant_building: b.allow_serjeant_building != 0,
    }
}

/// A player's history as saved (`TRIBE_History_Info::load`, `FUN_141560f20`): the entries, the
/// events in the list's order (the load links each after the last), and the running totals'
/// low 16 bits into the zeroed ints.
#[must_use]
pub fn history(h: &genie_save::generated::world::History) -> genie_core::players::history::History {
    genie_core::players::history::History {
        entries: h
            .history
            .iter()
            .map(|e| HistoryEntry {
                civilian: Population::of_data(e.history_civilian_pop),
                military: Population::of_data(e.history_military_pop),
            })
            .collect(),
        max_entries: TimeSlice::of_data(h.max_number_of_entries),
        events: h
            .events
            .iter()
            .map(|e| genie_core::players::history::Event {
                kind: genie_core::players::history::EventKind::from(e.temp_event),
                time_slice: TimeSlice::of_data(e.temp_time_slice),
                world_time: WorldTime::from_millis(e.temp_world_time),
                data1: e.temp_data1,
                data2: e.temp_data2,
                data3: e.temp_data3,
            })
            .collect(),
        number_of_events: h.number_of_events,
        update_count: HistoryUpdates::of_data(h.update_history_count),
        old_total_razings: Tally::of_data(h.old_total_razings),
        old_build_value_razings: Tally::of_data(h.old_build_value_razings),
        old_total_razings_other: Tally::of_data(h.old_total_razings_other),
        old_build_value_razings_other: Tally::of_data(h.old_build_value_razings_other),
        old_total_kills: Tally::of_data(h.old_total_kills),
        old_build_value_kills: Tally::of_data(h.old_build_value_kills),
        old_total_kills_other: Tally::of_data(h.old_total_kills_other),
        old_build_value_kills_other: Tally::of_data(h.old_build_value_kills_other),
        razings_weight: h.razings_weight,
        kills_weight: h.kills_weight,
        razing_percent: Share::of_data(h.razing_percent),
        kills_percent: Share::of_data(h.kills_perecent),
        razings_flag: h.razings_flag,
        battle_flag: h.battle_flag,
        old_current_build_value_units: Tally::of_data(h.old_current_build_value_units as i32),
        old_current_build_value_buildings: Tally::of_data(
            h.old_current_build_value_buildings as i32,
        ),
        old_player_kill_bv: h
            .old_player_kill_bv
            .iter()
            .map(|&v| Tally::of_data(v as i32))
            .collect(),
        old_player_razing_bv: h
            .old_player_razing_bv
            .iter()
            .map(|&v| Tally::of_data(v as i32))
            .collect(),
        running_ave_bv_percent: Share::of_data(h.running_ave_bv_percent),
        running_total_bv_kills: Tally::of_data(h.running_total_bv_kills as i32),
        running_total_bv_razings: Tally::of_data(h.running_total_bv_razings as i32),
        running_total_kills: Tally::of_data(i32::from(h.running_total_kills)),
        running_total_razings: Tally::of_data(i32::from(h.running_total_razings)),
    }
}

/// A player's AI module as saved: the parts the simulation reads
/// ([`genie_core::players::ai_module`]). The explorer and its search points are not saved; the
/// load makes them none and empty.
pub fn ai_module(
    m: &genie_save::generated::world::TribeMainDecisionAi,
) -> genie_core::players::ai_module::AiModule {
    use genie_core::players::ai_module::{AiModule, InfoModule, TacticalModule};
    let info = &m.tribe_information_aimodule;
    let grid = &info.tribe_information_aimodule;
    let width = grid.x_size_value.max(0) as usize;
    let cells = grid
        .row_value
        .iter()
        .flat_map(|r| {
            let row = r.row_value.as_deref().unwrap_or(&[]);
            (0..width).map(move |x| row.get(x).copied().unwrap_or(0))
        })
        .collect();
    AiModule {
        info: InfoModule {
            size: GridSize::new(info.map_width, info.map_height),
            path_map: genie_core::movement::explore::InfluenceMap {
                size: GridSize::new(grid.x_size_value, grid.y_size_value),
                origin: Tile::new(grid.x_reference_point_value, grid.y_reference_point_value),
                limit: grid.unchangeable_limit_value,
                cells,
            },
            quadrants: info.quadrant_scores,
            explorer: None,
            search: Vec::new(),
        },
        tactical: TacticalModule {
            slot_times: m.tribe_tactical_aimodule.slot_times.clone(),
        },
    }
}
