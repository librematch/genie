//! The slice set up from the header save ([`slice`]): the players' technologies, wonder
//! times, ghosts, AI settings and history, and the garrisons' firepower.

use genie_core::map::Distance;
use genie_core::map::GridSize;
use genie_core::map::Tile;
use genie_core::objects::ListIndex;
use genie_core::players::Amount;
use genie_core::world::Millis;
use genie_core::world::Seconds;
use genie_core::world::WorldTime;

use crate::masters::{MasterTable, TerrainTable};
use crate::{groups, market, spatial, unit, work_masters};
use genie_core::map::Position;
use genie_core::objects::ObjectId;
use genie_core::objects::masters::MasterId;
use genie_core::players::PlayerId;
use genie_core::players::attribute::AttributeId;
use genie_core::vision::VisionBits;
use genie_core::world::{Player, World};
use genie_save::{Header, PlayerSection};

/// A player's saved state of one technology (`Tech_Player_Tree`).
///
/// # Errors
///
/// A saved state that names none.
pub(super) fn tech_state(
    t: &genie_save::generated::world::TechPlayerTree,
) -> Result<genie_core::technology::TechState, genie_core::GenieError> {
    let o = |j: usize| {
        t.cost_override.get(j).map_or((-1, None, -2), |e| {
            (
                e.tech_player_tree_cost_amount_override as i16,
                AttributeId::of_data(e.tech_player_tree_cost_attribute_type_override as i16),
                e.tech_player_tree_cost_attribute_used_override as i16,
            )
        })
    };
    Ok(genie_core::technology::TechState {
        research_done: Seconds::new(t.research_done),
        state: genie_core::technology::ResearchState::of_int(t.state)?,
        cost_adj: [0, 1, 2].map(|j| t.cost_adj.get(j).copied().unwrap_or(0)),
        times_researched: t.tech_player_tree_times_researched as i16,
        allow_multiple: t.tech_player_tree_allow_multiple_research != 0,
        stack: t.tech_player_tree_stack_tech != 0,
        overrides: [o(0), o(1), o(2)],
        research_cap: t.tech_player_tree_research_cap as i16,
        pending: t.tech_player_tree_pending_count as i16,
        enabled: t.tech_player_tree_enabled_tech != 0,
        effect_override: t.tech_player_tree_effect_override as i16,
        building_master: MasterId::of_data(t.tech_player_tree_research_building_master_id as i16),
        locations: t
            .location
            .iter()
            .flatten()
            .map(|l| genie_core::technology::Location {
                building: MasterId::of_data(l.location_research_location_id as i16),
                time: l.location_research_time,
                button: l.location_button_location_id,
                adjustment: Seconds::new(f32::from_bits(l.location_research_adjustment)),
            })
            .collect(),
    })
}

/// The slice set up from the header save: every player's live objects in their saved order, and
/// `BaseWorld`'s fields before the map, the frame seed (`mCurrentFrameRandomSeed`, world+0x7d0) and
/// the turn order (`player_turn`, `player_time_delta[i]`).
///
/// Only for a save whose run-time state then comes from the tick oracle's dumps (a per-tick save:
/// the registry, tiles, mesh owners …). A run from a recording's header takes [`header_slice`]:
/// it adds what the game's own load builds, which the save does not hold (an empty
/// objects-in-sight registry, FD-008-D72; the hunted animals' origins; the watched lists).
/// Without it every unit-AI search stops as Unsupported: [`simulate_word9`] once used this and
/// lost MarvinBlue/426048731 player 2's first checkpoint that way.
///
/// [`header_slice`]: crate::header_runs::header_slice
/// [`simulate_word9`]: crate::header_runs::simulate_word9
pub(super) fn slice(header: &Header, masters: Option<&MasterTable>) -> World {
    let victory = &header.world.base_world.victory_conditions;
    // a technology state that names none stops every player once the world stands
    let (mut techs, tech_error) = match header
        .players()
        .iter()
        .map(|p| {
            p.world_player()
                .tech_tree
                .tech_player_tree
                .iter()
                .map(tech_state)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(techs) => (techs, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let players = header
        .players()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let civ = p.world_player().base.civ_id;
            let attributes = &p.world_player().base.attributes;
            Player {
                objects: p
                    .live_objects()
                    .map(|o| {
                        unit(
                            o,
                            p.objects(),
                            &header.world.base_world.path,
                            civ,
                            masters,
                            header.players().len(),
                        )
                    })
                    .collect(),
                sleeping: p
                    .sleeping_objects()
                    .map(|o| {
                        unit(
                            o,
                            p.objects(),
                            &header.world.base_world.path,
                            civ,
                            masters,
                            header.players().len(),
                        )
                    })
                    .collect(),
                computer: false,
                kind: genie_core::players::PlayerType::from(p.player.type_),
                attributes: attributes.iter().map(|&a| Amount::new(a)).collect(),
                // the second attribute array (+0x658), floats as their bits
                generated: p
                    .world_player()
                    .base
                    .attributes_2
                    .iter()
                    .map(|&b| Amount::new(f32::from_bits(b)))
                    .collect(),
                civ: genie_core::players::CivId::new(civ),
                relations: genie_core::players::Relations::of_bytes(
                    p.world_player().base.relation.iter().copied(),
                ),
                auto_farm_queue: p.world_player().auto_farm_queue != 0,
                auto_fishtrap_queue: p.world_player().auto_fish_trap_queue != 0,
                farm_queue: p.world_player().farm_queue_count,
                fish_trap_queue: p.world_player().fish_trap_queue_count,
                production_queue_on: p.world_player().prod_queue_on,
                diplomacy: p
                    .world_player()
                    .per_player
                    .iter()
                    .map(|e| genie_core::players::Stance::from(e.diplomacy))
                    .collect(),
                ai_settings: ai_settings(&p.world_player().base),
                formations: genie_core::world::Formations {
                    auto: p.world_player().base.auto_formations != 0,
                    influence: Distance::new(p.world_player().base.formation_influence_distance),
                    by_speed: p.world_player().base.break_auto_formations_by_speed == 1,
                    min_column_distance: p.world_player().base.min_column_distance,
                    column_to_line_distance: p.world_player().base.column_to_line_distance,
                },
                pending_debit: p.world_player().base.pending_debit,
                techs: techs.get_mut(i).map(std::mem::take).unwrap_or_default(),
                unsupported: None,
                freezes: Vec::new(),
                stopped: false,
                game_status: p.world_player().base.game_status,
                // `+0x8b` holds the drop in its bit 1 (the load moves it to `+0x8c`)
                resigned: p.world_player().base.resigned & 2 == 0
                    && p.world_player().base.resigned != 0,
                dropped: p.world_player().base.resigned & 2 != 0,
                allied_victory: p.world_player().base.allied_victory,
                allied_vision: p.world_player().base.allied_los_enable != 0,
                // the reveal of the enemies' town centres (attribute 209) is done once any
                // mTCFlared bit is set; the saved map then already holds the revealed tiles
                tc_flared: p.world_player().tcflared.iter().map(|&b| b != 0).collect(),
                sharing: p.world_player().sharing_is_caring != 0,
                commodity_volume: p.world_player().commodity_volumn_delta,
                victory: victory.get(i).map_or(0, |v| v.victory),
                fealty: p.world_player().fealty as i16,
                // the load sets it 0 (`FUN_14163e970`)
                posture: 0,
                dodge_chance: p.world_player().chance_to_dodge_missiles,
                keep_distance_chance: p.world_player().chance_for_archers_to_maintain_distance,
                // the load builds the watched lists (World::build_watched)
                watched: Vec::new(),
                // the saved ghosts follow once the world stands (a ghost without an id stops it)
                ghosts: Vec::new(),
                history: history(&p.world_player().history),
                // the load files the objects (World::load_trackers, once the masters are in)
                tracker: genie_core::players::tracker::Tracker::default(),
                // +0x1378 is not saved
                one_military_queued: false,
                update_time: Seconds::new(p.world_player().update_time),
                gaia_update_time: p.player.gaia_player.as_deref().map_or(Seconds::ZERO, |g| {
                    Seconds::new(g.world_player_gaia.update_time)
                }),
                escrow: p.world_player().base.escrow.map(Amount::new),
                escrow_percentage: p.world_player().base.escrow_percentage,
                sudden_death_eliminable: p.world_player().sudden_death_can_be_eliminated != 0,
                victory_times: genie_core::players::world_player::VictoryTimes {
                    // the world's load resets every player's (FUN_140c90040), then a wonder's
                    // action in state 7 sets its time (its load, FUN_140ae8fa0)
                    wonder: wonder_time(p),
                    ruins: p.world_player().ruin_held_time,
                    artifacts: p.world_player().artifact_held_time,
                },
            }
        })
        .collect();
    let world = &header.world.base_world;
    let mut turns = World::new(
        WorldTime::from_millis(world.world_time),
        world.current_frame_random_seed,
        PlayerId::try_from(world.player_turn.max(0) as usize).unwrap_or_default(),
        world
            .player_time_delta
            .iter()
            .copied()
            .map(Millis::new)
            .collect(),
        players,
    );
    // an id of −1 names none: the save is not one a world takes
    let next_ids = ObjectId::try_from(world.next_object_id)
        .and_then(|next| Ok((next, ObjectId::try_from(world.next_reusable_object_id)?)));
    match next_ids {
        Ok((next, reusable)) => {
            turns.next_object_id = next;
            turns.recycling.next_id = reusable;
        }
        Err(e) => stop_all(&mut turns, &e),
    }
    if let Some(e) = tech_error {
        stop_all(&mut turns, &e);
    }
    match header
        .players()
        .iter()
        .map(saved_ghosts)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ghosts) => {
            for (player, ghosts) in turns.players.iter_mut().zip(ghosts) {
                player.ghosts = ghosts;
            }
        }
        Err(e) => stop_all(&mut turns, &e),
    }
    turns.market = market(&header.world);
    turns.local_player = PlayerId::try_from(usize::try_from(world.local_player_index).unwrap_or(0))
        .unwrap_or_default();
    turns.alliance_locked = world.alliance_locked;
    // the world's teams (+0x984, `playerTeams[i]`, saved as bytes)
    for (team, &saved) in turns.teams.iter_mut().zip(&header.world.player_teams) {
        *team = i32::from(saved);
    }
    let none = TerrainTable::default();
    let terrain = masters.map_or(&none, |t| &t.terrain);
    match spatial(header, terrain) {
        Ok(s) => turns.spatial = s,
        Err(e) => stop_all(&mut turns, &e),
    }
    // the players' scout maps (`mIsScoutMapEnabled` and its InfluenceMap, player+0x1370)
    for (i, p) in PlayerId::each(header.players().len()).zip(header.players()) {
        let wp = p.world_player();
        if wp.is_scout_map_enabled.unwrap_or_default() == 0 {
            continue;
        }
        if let Some(m) = wp.scout_map.as_deref() {
            let width = m.x_size_value.max(0) as usize;
            let cells = m
                .row_value
                .iter()
                .flat_map(|r| {
                    let row = r.row_value.as_deref().unwrap_or(&[]);
                    (0..width).map(move |x| row.get(x).copied().unwrap_or(0))
                })
                .collect();
            turns.spatial.set_scout_map(
                i,
                genie_core::movement::explore::InfluenceMap {
                    size: GridSize::new(m.x_size_value, m.y_size_value),
                    origin: Tile::new(m.x_reference_point_value, m.y_reference_point_value),
                    limit: m.unchangeable_limit_value,
                    cells,
                },
            );
        }
    }
    // the computer players' AI modules (player+0x7a8, saved only where the AI was created)
    for (i, p) in PlayerId::each(header.players().len()).zip(header.players()) {
        if let Some(m) = p.world_player().tribe_main_decision_aimodule.as_deref() {
            turns.ai.set(i, ai_module(m));
        }
    }
    let map = &world.map;
    turns.pathing = genie_core::pathing::Pathing::new(
        GridSize::new(map.map_width, map.map_height),
        map.rows
            .iter()
            .flat_map(|r| {
                r.tiles
                    .iter()
                    .map(|t| genie_core::map::TerrainId::named(t.terrain_type))
            })
            .collect(),
        terrain.passable().to_vec(),
        header.world.game_type,
    );
    // the zone maps (`map+0x5e28`) as the save holds them, their adjacency built by the load
    turns.zones = genie_core::pathing::zones::Zones::from_saved(
        map.map_zones
            .zone_map
            .iter()
            .map(|z| {
                genie_core::pathing::zones::ZoneMap::from_saved(
                    GridSize::new(map.map_width, map.map_height),
                    &z.number_tiles_in_zone_value,
                    &z.zone_map,
                    &genie_core::map::Restriction::new(z.float_value.clone()),
                    z.number_zones,
                )
            })
            .collect(),
    );
    // the players' path caps (saved); their plans start over at each player's update
    turns.pathing.budgets = header
        .players()
        .iter()
        .map(|p| {
            let b = &p.world_player().base;
            genie_core::pathing::PlanBudget {
                attempt_cap: b.pathing_attempt_cap_value,
                delay_cap: b.pathing_delay_cap_value,
                plans: 0,
            }
        })
        .collect();
    turns.settings = settings(&header.file_header.game_options_read.game_options);
    // the Empire Wars start's pass, once in the game's first update (the world's +0x6c6): the
    // mode 13 or the sub-mode's bit 0
    turns.start_work_due = turns.settings.empire_wars() && turns.world_time == WorldTime::ZERO;
    turns.game_state = u32::from(world.temp_game_state);
    turns.victory_conquest = world.tribe_scenario.base_scenario.victory_conquest != 0;
    match groups::unit_groups(world) {
        Ok(g) => turns.groups = g,
        Err(e) => stop_all(&mut turns, &e),
    }
    if let Some(table) = masters {
        turns.drop_sites.clone_from(&table.drop_sites);
        turns.linked_techs.clone_from(&table.linked_techs);
        turns.graphics = table.graphics.clone();
        turns.techs.clone_from(&table.techs);
        turns.effects.clone_from(&table.effects);
        turns.history_period = table.time_slice;
        // the unit lines' tables, which the game builds at its first load with Gaia's master
        // count (`FUN_141533cb0` -> `FUN_140fde680`)
        let gaia_masters = header
            .players()
            .first()
            .map_or(0, |p| p.objects().master_object_num);
        turns.lines = genie_core::objects::lines::UnitLines::build(gaia_masters, &table.unit_lines);
        let masters = header
            .players()
            .iter()
            .map(|p| work_masters(p.objects(), p.world_player().base.civ_id, table))
            .collect::<Result<_, _>>();
        match masters {
            Ok(m) => turns.masters = m,
            Err(e) => stop_all(&mut turns, &e),
        }
        // the players' unit masters (type 70), Gaia's left out
        let all = || {
            turns
                .masters
                .iter()
                .skip(1)
                .flat_map(genie_core::objects::masters::Masters::iter)
                .filter(|m| m.kind == genie_core::objects::masters::master_type::COMBAT)
        };
        turns.spatial.fastest = all()
            .filter_map(|m| m.moving.as_ref().map(|s| s.speed.get()))
            .fold(0.0, f32::max);
        turns.spatial.reach = Distance::new(
            all()
                .flat_map(|m| m.tasks.iter().map(|t| t.work_range).chain([m.weapon_range]))
                .map(Distance::get)
                .fold(0.0, f32::max),
        );
        garrison_firepower(&mut turns);
        // the planners the load makes and the shape mesh it builds (a dump of the game's mesh
        // replaces it)
        turns.register_planners();
        if let Err(e) = turns.build_mesh() {
            stop_all(&mut turns, &e);
        }
    }
    // the players' object trackers, which the objects' loads fill
    // the players' vision masks, which the world's load rebuilds (its slot 0x78)
    // (both read the objects' masters: without the masters' table there is nothing to rebuild)
    if masters.is_some()
        && let Err(e) = turns.load_trackers().and_then(|()| turns.rebuild_vision())
    {
        stop_all(&mut turns, &e);
    }
    turns
}

/// Every player stops on `e`, which the slice does not model of the save.
fn stop_all(turns: &mut World, e: &genie_core::GenieError) {
    for p in &mut turns.players {
        let _ = p
            .unsupported
            .get_or_insert_with(|| (0, ListIndex::new(0), e.clone()));
        p.stopped = true;
    }
}

/// The game's wonder countdown of a player (the game object's `+0x4ec[id]`) after a load: the
/// world's load sets every player's to −1 (`FUN_140c90040`), then each wonder action
/// (`TRIBE_Action_Wonder`, type 120) of the player's objects, live then sleeping, in its load
/// (`FUN_140ae8fa0`) when in state 7 sets its timer (`mTimer`, `+0x44`) where it is less or the
/// countdown below 0 (`FUN_140c91500`).
pub(super) fn wonder_time(p: &PlayerSection<'_>) -> f32 {
    let mut countdown = -1.0;
    for o in p.live_objects().chain(p.sleeping_objects()) {
        let Some(actions) = o.actions() else {
            continue;
        };
        for (_, a) in actions.entries() {
            if let Some(w) = a.tribe_action_wonder.as_deref()
                && w.rge_action.state == 7
            {
                let t = w.rge_action.timer;
                if t < countdown || countdown < 0.0 {
                    countdown = t;
                }
            }
        }
    }
    countdown
}

/// A player's ghosts (its doppelganger list) as saved: each one's id, place, state, the original's
/// id (`long_val`), owner (`doppled_player`) and UMV entry (`Multi_Unified_Points`), and the
/// players who cannot see it (`CantSeeBits`). Its last bits (`+0x7e8`) and seen flag (`+0x7f7`)
/// are not saved: a load starts them at 0; its original's link waits for the original's update
/// (the slice links it by id).
/// The game options the simulation reads ([`genie_core::world::settings::Settings`]) from the header's
/// `GameOptions`, as `FUN_141305310` loads them.
#[must_use]
pub const fn settings(
    o: &genie_save::generated::world::GameOptions,
) -> genie_core::world::settings::Settings {
    genie_core::world::settings::Settings {
        game_mode: o.game_mode,
        ending_age: o.ending_age,
        visibility_type: o.visibility_type,
        sub_game_mode_flag: o.sub_game_mode_flag,
        full_tech_tree: o.full_tech_tree_enabled != 0,
        walkable_farms: o.walkable_farms != 0,
        selected_content_mode: o.selected_content_mode,
        villager_force_drop: o.villager_force_drop != 0,
        cheats_enabled: o.cheats_enabled != 0,
        treaty_length: o.treaty_length,
        teams_locked: o.teams_locked != 0,
        starting_age: o.starting_age,
        resource_level: o.resource_level,
        turbo: o.turbo_enabled != 0,
        shared_exploration: o.shared_exploration != 0,
        antiquity: o.antiquity_mode != 0,
    }
}

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
        preferred_trade_distance: b.preferred_trade_distance,
        disable_tower_priority: b.disable_tower_priority != 0,
        trade_attack_evasion: b.trade_attack_evasion != 0,
        villager_attack_reset: b.villager_attack_reset != 0,
        boar_lure_destination: b.boar_lure_destination,
        attack_move_max_distance: b.attack_move_max_distance,
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
            .map(|e| [e.history_civilian_pop, e.history_military_pop])
            .collect(),
        max_entries: h.max_number_of_entries,
        events: h
            .events
            .iter()
            .map(|e| genie_core::players::history::Event {
                kind: genie_core::players::history::EventKind::from(e.temp_event),
                time_slice: e.temp_time_slice,
                world_time: WorldTime::from_millis(e.temp_world_time),
                data1: e.temp_data1,
                data2: e.temp_data2,
                data3: e.temp_data3,
            })
            .collect(),
        number_of_events: h.number_of_events,
        update_count: h.update_history_count,
        old_total_razings: h.old_total_razings,
        old_build_value_razings: h.old_build_value_razings,
        old_total_razings_other: h.old_total_razings_other,
        old_build_value_razings_other: h.old_build_value_razings_other,
        old_total_kills: h.old_total_kills,
        old_build_value_kills: h.old_build_value_kills,
        old_total_kills_other: h.old_total_kills_other,
        old_build_value_kills_other: h.old_build_value_kills_other,
        razings_weight: h.razings_weight,
        kills_weight: h.kills_weight,
        razing_percent: h.razing_percent,
        kills_percent: h.kills_perecent,
        razings_flag: h.razings_flag,
        battle_flag: h.battle_flag,
        old_current_build_value_units: h.old_current_build_value_units as i32,
        old_current_build_value_buildings: h.old_current_build_value_buildings as i32,
        old_player_kill_bv: h.old_player_kill_bv.iter().map(|&v| v as i32).collect(),
        old_player_razing_bv: h.old_player_razing_bv.iter().map(|&v| v as i32).collect(),
        running_ave_bv_percent: h.running_ave_bv_percent,
        running_total_bv_kills: h.running_total_bv_kills as i32,
        running_total_bv_razings: h.running_total_bv_razings as i32,
        running_total_kills: i32::from(h.running_total_kills),
        running_total_razings: i32::from(h.running_total_razings),
    }
}

pub(super) fn saved_ghosts(
    section: &PlayerSection<'_>,
) -> Result<Vec<genie_core::vision::ghost::Ghost>, genie_core::GenieError> {
    section
        .objects()
        .doppleganger_objects
        .objects()
        .filter_map(|o| {
            let d = o
                .rge_object
                .as_deref()?
                .rge_doppleganger_object
                .as_deref()?;
            Some((o.static_part()?, d))
        })
        .map(|(s, d)| {
            let original = ObjectId::of_field(d.long_val as i32);
            Ok(genie_core::vision::ghost::Ghost {
                id: ObjectId::try_from(s.id)?,
                original,
                original_id: original,
                original_master: MasterId::of_data(d.long_val_2 as i16),
                original_owner: u8::try_from(d.doppled_player)
                    .map_or(PlayerId::GAIA, PlayerId::from),
                position: Position::new(s.t_x, s.t_y, s.t_z),
                tile: Tile::new(s.t_x as i32, s.t_y as i32),
                footprint: d.multi_unified_points,
                bits: VisionBits::NONE,
                cant_see: genie_core::players::PlayerSet::from_bits_retain(d.cant_see_bits),
                seen: false,
                state: genie_core::objects::ObjectState::of_byte(s.object_state)?,
            })
        })
        .collect()
}

/// The garrisons of the running game: each object inside another of its player is in that one's
/// list (`+0x2f8`), in the order the objects are listed (the game's is the order they came in,
/// which no save holds); and each unit (type 70) added its share to a combat object's volleys at
/// its enter (`FUN_14120b790`, [`genie_core::garrison::share`]), which the load does not restore
/// (`FUN_14121ec90` sets the master's volley fire). A share the slice does not model leaves the
/// building unmodelled.
pub(super) fn garrison_firepower(turns: &mut World) {
    for (p, player) in turns.players.iter_mut().enumerate() {
        let Some(masters) = turns.masters.get(p) else {
            continue;
        };
        let units: Vec<(ObjectId, ObjectId, MasterId, Seconds)> = player
            .objects
            .iter()
            .chain(&player.sleeping)
            .filter_map(|u| Some((u.id, u.life.inside?, u.life.master_id, u.reload_powerup)))
            .collect();
        for (id, building, master, reload_powerup) in units {
            let Some(b) = player
                .objects
                .iter_mut()
                .chain(&mut player.sleeping)
                .find(|u| u.id == building)
            else {
                continue;
            };
            b.garrison.push(id);
            let Some(me) = masters
                .get(master)
                .filter(|m| m.kind == genie_core::objects::masters::master_type::COMBAT)
            else {
                continue;
            };
            let Some(bm) = masters.get(b.life.master_id) else {
                continue;
            };
            let rate = genie_core::garrison::attack_rate(bm, masters, b.reload_powerup);
            match genie_core::garrison::share(me, masters, reload_powerup, rate) {
                Ok(share) => b.firepower += share,
                Err(e) => b.unmodelled_action = Some(e),
            }
        }
    }
}

#[cfg(test)]
mod tests;

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
