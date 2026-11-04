//! A player's saved parts the world's load reads: its AI settings, its history, its AI module
//! and its technologies' state.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Tile;
use crate::map::WholeDistance;
use crate::movement::explore::InfluenceMap;
use crate::objects::masters::CostAmount;
use crate::objects::masters::MasterId;
use crate::players::ai_module::AiModule;
use crate::players::attribute::AttributeId;
use crate::players::history::HistoryEntry;
use crate::players::history::HistoryUpdates;
use crate::players::history::Population;
use crate::players::history::Share;
use crate::players::history::Tally;
use crate::players::history::TimeSlice;
use crate::players::history::{Event, EventKind, History};
use crate::production::LocationButton;
use crate::technology::CostOverride;
use crate::technology::EffectId;
use crate::technology::OverrideUse;
use crate::technology::ResearchCap;
use crate::technology::ResearchCount;
use crate::technology::{Location, ResearchState, TechState};
use crate::world::AiSettings;
use crate::world::AiSwitches;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::WorldTime;
use crate::world::{LureDestination, Percent, TargetingMode, VillagerGarrison};
use genie_save::generated::world::TechPlayerTree;
use genie_save::generated::world::{History as WorldHistory, TribeMainDecisionAi, WorldPlayerBase};

/// A player's AI settings as saved (`WorldPlayerBase::save`, `FUN_14166b820`), which GAME's
/// sub-command 0xb sets.
#[must_use]
pub fn ai_settings(world_player_base: &WorldPlayerBase) -> AiSettings {
    AiSettings {
        easiest_reaction_percentage: Percent::new(world_player_base.easiest_reaction_percentage),
        easier_reaction_percentage: Percent::new(world_player_base.easier_reaction_percentage),
        task_ungrouped_soldiers: world_player_base.task_ungrouped_soldiers,
        switches: {
            let mut flags = AiSwitches::empty();
            flags.set(
                AiSwitches::MAY_ATTACK_WALLS,
                world_player_base.may_attack_walls != 0,
            );
            flags.set(
                AiSwitches::LIVESTOCK_ONLY_TO_TC,
                world_player_base.livestock_only_to_tc != 0,
            );
            flags.set(
                AiSwitches::DISABLE_BUILD_HELP,
                world_player_base.disable_build_help != 0,
            );
            flags.set(
                AiSwitches::DISABLE_TOWER_PRIORITY,
                world_player_base.disable_tower_priority != 0,
            );
            flags.set(
                AiSwitches::TRADE_ATTACK_EVASION,
                world_player_base.trade_attack_evasion != 0,
            );
            flags.set(
                AiSwitches::VILLAGER_ATTACK_RESET,
                world_player_base.villager_attack_reset != 0,
            );
            flags.set(
                AiSwitches::ALLOW_SERJEANT_BUILDING,
                world_player_base.allow_serjeant_building != 0,
            );
            flags
        },
        local_targeting_mode: TargetingMode::of_data(world_player_base.local_targeting_mode),
        targeting_factor_scalar: Percent::new(world_player_base.targeting_factor_scalar),
        preferred_trade_distance: Distance::new(world_player_base.preferred_trade_distance),
        boar_lure_destination: LureDestination::of_data(world_player_base.boar_lure_destination),
        attack_move_max_distance: WholeDistance::of_data(
            world_player_base.attack_move_max_distance,
        ),
        disable_villager_garrison: VillagerGarrison::of_data(
            world_player_base.disable_villager_garrison,
        ),
    }
}

/// A player's history as saved (`TRIBE_History_Info::load`, `FUN_141560f20`): the entries, the
/// events in the list's order (the load links each after the last), and the running totals'
/// low 16 bits into the zeroed ints.
#[must_use]
pub fn history(h: &WorldHistory) -> History {
    History {
        entries: h
            .history
            .iter()
            .map(|history_entry| HistoryEntry {
                civilian: Population::of_data(history_entry.history_civilian_pop),
                military: Population::of_data(history_entry.history_military_pop),
            })
            .collect(),
        max_entries: TimeSlice::of_data(h.max_number_of_entries),
        events: h
            .events
            .iter()
            .map(|history_event| Event {
                kind: EventKind::from(history_event.temp_event),
                time_slice: TimeSlice::of_data(history_event.temp_time_slice),
                world_time: WorldTime::from_millis(history_event.temp_world_time),
                data1: history_event.temp_data1,
                data2: history_event.temp_data2,
                data3: history_event.temp_data3,
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
/// ([`crate::players::ai_module`]). The explorer and its search points are not saved; the
/// load makes them none and empty.
#[must_use]
pub fn ai_module(tribe_main_decision_ai: &TribeMainDecisionAi) -> AiModule {
    use crate::players::ai_module::{AiModule, InfoModule, TacticalModule};
    let info = &tribe_main_decision_ai.tribe_information_ai_module;
    let grid = &info.tribe_information_ai_module;
    let width = grid.xsize_value.max(0) as usize;
    let cells = grid
        .row_value
        .iter()
        .flat_map(|entry| {
            let row = entry.row_value.as_deref().unwrap_or(&[]);
            (0..width).map(move |x| row.get(x).copied().unwrap_or(0))
        })
        .collect();
    AiModule {
        info: InfoModule {
            size: GridSize::new(info.map_width, info.map_height),
            path_map: InfluenceMap {
                size: GridSize::new(grid.xsize_value, grid.ysize_value),
                origin: Tile::new(grid.xreference_point_value, grid.yreference_point_value),
                limit: grid.unchangeable_limit_value,
                cells,
            },
            quadrants: info.quadrant_scores,
            explorer: None,
            search: Vec::new(),
        },
        tactical: TacticalModule {
            slot_times: tribe_main_decision_ai
                .tribe_tactical_ai_module
                .slot_times
                .clone(),
        },
    }
}

/// A player's saved state of one technology (`Tech_Player_Tree`).
///
/// # Errors
///
/// A saved state that names none.
pub fn tech_state(tech_player_tree: &TechPlayerTree) -> GenieResult<TechState> {
    let o = |j: usize| {
        tech_player_tree
            .cost_override
            .get(j)
            .map_or(CostOverride::UNSET, |e| CostOverride {
                amount: CostAmount::of_data(e.tech_player_tree_cost_amount_override as i16),
                attribute: AttributeId::of_data(
                    e.tech_player_tree_cost_attribute_kind_override as i16,
                ),
                used: OverrideUse::of_data(e.tech_player_tree_cost_attribute_used_override as i16),
            })
    };
    Ok(TechState {
        research_done: Seconds::new(tech_player_tree.research_done),
        state: ResearchState::of_int(tech_player_tree.state)?,
        cost_adj: [0, 1, 2]
            .map(|j| CostAmount::of_data(tech_player_tree.cost_adj.get(j).copied().unwrap_or(0))),
        times_researched: ResearchCount::of_data(
            tech_player_tree.tech_player_tree_times_researched as i16,
        ),
        allow_multiple: tech_player_tree.tech_player_tree_allow_multiple_research != 0,
        stack: tech_player_tree.tech_player_tree_stack_tech != 0,
        overrides: [o(0), o(1), o(2)],
        research_cap: ResearchCap::of_data(tech_player_tree.tech_player_tree_research_cap as i16),
        pending: ResearchCount::of_data(tech_player_tree.tech_player_tree_pending_count as i16),
        enabled: tech_player_tree.tech_player_tree_enabled_tech != 0,
        effect_override: EffectId::of_data(
            tech_player_tree.tech_player_tree_effect_override as i16,
        ),
        building_master: MasterId::of_data(
            tech_player_tree.tech_player_tree_research_building_master_id as i16,
        ),
        locations: tech_player_tree
            .location
            .iter()
            .flatten()
            .map(|entry| Location {
                building: MasterId::of_data(entry.location_research_location_id as i16),
                time: WholeSeconds::of_data(entry.location_research_time.cast_signed()),
                button: LocationButton::of_data(entry.location_button_location_id),
                adjustment: Seconds::new(f32::from_bits(entry.location_research_adjustment)),
            })
            .collect(),
    })
}
