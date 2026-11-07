//! The world as a save holds it ([`saved_world`]): every player's objects in their saved
//! order, its technologies, wonder time, ghosts, AI settings and history, the map, the
//! groups and the garrisons' firepower; and as the game's load leaves it
//! ([`loaded_world`]).
//!
//! [`loaded_world`]: crate::world::load::world::loaded::loaded_world

use crate::gathering::QueueCount;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Tile;
use crate::map::WholeDistance;
use crate::objects::ListIndex;
use crate::players::game_status::Fealty;
use crate::players::game_status::GameStatus;
use crate::players::game_status::VictoryState;
use crate::players::{Amount, Chance, CivId, Culture, EscrowShare, LotVolume, Team};
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::Model;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::Tick;
use crate::world::WorldTime;
use crate::world::{AllySharing, AutoQueues, Departures};
use crate::{GenieError, GenieResult};
use genie_model::layer::BuildLayer;
use std::sync::Arc;

pub mod garrisons;
pub mod loaded;
pub mod market;
pub mod options;
pub mod players;

use crate::world::load::world::players::ai_settings;
use crate::world::load::world::players::tech_state;
use crate::world::load::world::players::{ai_module, history};

use crate::map::Position;
use crate::map::{Restriction, TerrainId};
use crate::movement::explore::InfluenceMap;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lines::UnitLines;
use crate::objects::masters::MasterId;
use crate::objects::unit::Unit;
use crate::pathing::zones::{ZoneMap, Zones};
use crate::pathing::{Pathing, PlanBudget, PlanCap, PlanCount};
use crate::players::PlayerId;
use crate::players::tracker::Tracker;
use crate::players::world_player::VictoryTimes;
use crate::players::{PlayerKind, PlayerSet, Posture, Relations, Stance};
use crate::vision::VisionBits;
use crate::vision::ghost::Ghost;
use crate::world::Formations;
use crate::world::load::groups::unit_groups;
use crate::world::load::map::spatial;
use crate::world::load::object::object;
use crate::world::load::world::garrisons::garrison_firepower;
use crate::world::load::world::market::market;
use crate::world::load::world::options::settings;
use crate::world::parts::ObjectParts;
use crate::world::{Player, World};
use genie_import::dat::{MasterTable, TerrainTable};
use genie_import::save::work_masters;
use genie_save::{Header, PlayerSection};
use std::mem::take;

/// The world as the save `header` holds it, with the masters `masters` and the build layer
/// `build`: every player's live objects in their saved order, and `BaseWorld`'s fields before
/// the map, the frame seed (`mCurrentFrameRandomSeed`, world+0x7d0) and the turn order
/// (`player_turn`, `player_time_delta[player_id]`).
///
/// Only for a save whose run-time state then comes from elsewhere (the tick oracle's dumps of a
/// per-tick save: the registry, tiles, mesh owners …). A run from a recording's header takes
/// [`loaded_world`]: it adds what the game's own load builds, which the save does not hold (an
/// empty objects-in-sight registry, FD-008-D72; the hunted animals' origins; the watched
/// lists). Without it every unit-AI search stops as Unsupported: word 9's simulation once
/// used this and lost `MarvinBlue`/426048731 player 2's first checkpoint that way.
///
/// [`loaded_world`]: crate::world::load::world::loaded::loaded_world
///
/// # Errors
///
/// None yet: what the world does not model of the save stops every player instead.
#[expect(
    clippy::too_many_lines,
    reason = "the world's saved fields read in the save's order"
)]
pub fn saved_world(
    header: &Header,
    masters: Option<&MasterTable>,
    build: &BuildLayer,
) -> GenieResult<World> {
    let missile_tuning = &build.tuning.missile;
    let victory = &header.world.base_world.victory_conditions;
    // a technology state that names none stops every player once the world stands
    let (mut techs, tech_error) = match header
        .players()
        .iter()
        .map(|player_section| {
            player_section
                .world_player()
                .tech_tree
                .tech_player_tree
                .iter()
                .map(tech_state)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(techs) => (techs, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    // each object's parts, for the world's tables once its lists stand
    let mut parts: Vec<(ObjectId, ObjectParts)> = Vec::new();
    let players = header
        .players()
        .iter()
        .enumerate()
        .map(|(index, player_section)| {
            let civ = CivId::new(player_section.world_player().base.civ_id);
            let attributes = &player_section.world_player().base.attributes;
            let mut rows = |saved: &mut dyn Iterator<Item = &genie_save::Object>| -> Vec<Unit> {
                saved
                    .map(|saved| {
                        let whole = object(
                            saved,
                            player_section.objects(),
                            &header.world.base_world.path,
                            civ,
                            masters,
                            header.players().len(),
                            missile_tuning,
                        );
                        if whole.parts != ObjectParts::default() {
                            parts.push((whole.unit.id, whole.parts));
                        }
                        whole.unit
                    })
                    .collect()
            };
            let live = rows(&mut player_section.live_objects());
            let sleeping = rows(&mut player_section.sleeping_objects());
            Ok(Player {
                objects: live.into(),
                sleeping: sleeping.into(),
                kind: PlayerKind::try_from(player_section.player.kind)?,
                attributes: attributes
                    .iter()
                    .map(|&amount| Amount::new(amount))
                    .collect(),
                // the second attribute array (+0x658), floats as their bits
                generated: player_section
                    .world_player()
                    .base
                    .attributes_2
                    .iter()
                    .map(|&bits| Amount::new(f32::from_bits(bits)))
                    .collect(),
                civ,
                culture: Culture::of_data(player_section.world_player().base.culture),
                relations: Relations::of_bytes(
                    player_section.world_player().base.relation.iter().copied(),
                ),
                auto_queues: {
                    let mut flags = AutoQueues::empty();
                    flags.set(
                        AutoQueues::FARMS,
                        player_section.world_player().auto_farm_queue != 0,
                    );
                    flags.set(
                        AutoQueues::FISH_TRAPS,
                        player_section.world_player().auto_fish_trap_queue != 0,
                    );
                    flags
                },
                farm_queue: QueueCount::of_data(player_section.world_player().farm_queue_count),
                fish_trap_queue: QueueCount::of_data(
                    player_section.world_player().fish_trap_queue_count,
                ),
                production_queue_on: player_section.world_player().prod_queue_on,
                diplomacy: player_section
                    .world_player()
                    .per_player
                    .iter()
                    .map(|entry| Stance::from(entry.diplomacy))
                    .collect(),
                ai_settings: ai_settings(&player_section.world_player().base),
                formations: Formations {
                    auto: player_section.world_player().base.auto_formations != 0,
                    influence: Distance::new(
                        player_section
                            .world_player()
                            .base
                            .formation_influence_distance,
                    ),
                    by_speed: player_section
                        .world_player()
                        .base
                        .break_auto_formations_by_speed
                        == 1,
                    min_column_distance: WholeDistance::of_data(
                        player_section.world_player().base.min_column_distance,
                    ),
                    column_to_line_distance: WholeDistance::of_data(
                        player_section.world_player().base.column_to_line_distance,
                    ),
                },
                pending_debit: player_section
                    .world_player()
                    .base
                    .pending_debit
                    .map(Amount::new),
                techs: techs.get_mut(index).map(take).unwrap_or_default(),
                unsupported: None,
                freezes: Vec::new(),
                stopped: false,
                game_status: GameStatus::of_data(player_section.world_player().base.game_status),
                // `+0x8b` holds the drop in its bit 1 (the load moves it to `+0x8c`)
                departures: {
                    let mut flags = Departures::empty();
                    flags.set(
                        Departures::RESIGNED,
                        player_section.world_player().base.resigned & 2 == 0
                            && player_section.world_player().base.resigned != 0,
                    );
                    flags.set(
                        Departures::DROPPED,
                        player_section.world_player().base.resigned & 2 != 0,
                    );
                    flags
                },
                allied_victory: player_section.world_player().base.allied_victory,
                ally_sharing: {
                    let mut flags = AllySharing::empty();
                    flags.set(
                        AllySharing::VISION,
                        player_section.world_player().base.allied_los_enable != 0,
                    );
                    flags.set(
                        AllySharing::RESOURCES,
                        player_section.world_player().sharing_is_caring != 0,
                    );
                    flags
                },
                // the reveal of the enemies' town centres (attribute 209) is done once any
                // mTCFlared bit is set; the saved map then already holds the revealed tiles
                tc_flared: player_section
                    .world_player()
                    .tc_flared
                    .iter()
                    .map(|&byte| byte != 0)
                    .collect(),
                commodity_volume: LotVolume::of_data(
                    player_section.world_player().commodity_volumn_delta,
                ),
                victory: VictoryState::of_data(
                    victory
                        .get(index)
                        .map_or(0, |rge_victory_conditions| rge_victory_conditions.victory),
                ),
                fealty: Fealty::of_data(player_section.world_player().fealty as i16),
                // the load sets it 0 (`FUN_14163e970`)
                posture: Posture::Aggressive,
                dodge_chance: Chance::of_data(
                    player_section.world_player().chance_to_dodge_missiles,
                ),
                keep_distance_chance: Chance::of_data(
                    player_section
                        .world_player()
                        .chance_for_archers_to_maintain_distance,
                ),
                // the load builds the watched lists (World::build_watched)
                watched: Vec::new(),
                // the saved ghosts follow once the world stands (a ghost without an id stops it)
                ghosts: Vec::new(),
                history: history(&player_section.world_player().history),
                // the load files the objects (World::load_trackers, once the masters are in)
                tracker: Tracker::default(),
                // +0x1378 is not saved
                one_military_queued: false,
                update_time: Seconds::new(player_section.world_player().update_time),
                gaia_update_time: player_section
                    .player
                    .gaia_player
                    .as_deref()
                    .map_or(Seconds::ZERO, |gaia_player| {
                        Seconds::new(gaia_player.world_player_gaia.update_time)
                    }),
                escrow: player_section.world_player().base.escrow.map(Amount::new),
                escrow_percentage: player_section
                    .world_player()
                    .base
                    .escrow_percentage
                    .map(EscrowShare::of_data),
                sudden_death_eliminable: player_section
                    .world_player()
                    .sudden_death_can_be_eliminated
                    != 0,
                victory_times: VictoryTimes {
                    // the world's load resets every player's (FUN_140c90040), then a wonder's
                    // action in state 7 sets its time (its load, FUN_140ae8fa0)
                    wonder: wonder_time(player_section),
                    ruins: Seconds::new(player_section.world_player().ruin_held_time),
                    artifacts: Seconds::new(player_section.world_player().artifact_held_time),
                },
            })
        })
        .collect::<GenieResult<Vec<_>>>()?;
    let world = &header.world.base_world;
    let mut turns = World::new(
        WorldTime::from_millis(world.world_time),
        FrameSeed::of_data(world.current_frame_random_seed),
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
        Err(error) => stop_all(&mut turns, &error),
    }
    if let Some(error) = tech_error {
        stop_all(&mut turns, &error);
    }
    file_parts(&mut turns, parts);
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
        Err(error) => stop_all(&mut turns, &error),
    }
    turns.market = market(&header.world);
    turns.local_player = PlayerId::try_from(usize::try_from(world.local_player_index).unwrap_or(0))
        .unwrap_or_default();
    turns.alliance_locked = world.alliance_locked;
    // the world's teams (+0x984, `playerTeams[player_id]`, saved as bytes)
    for (team, &saved) in turns.teams.iter_mut().zip(&header.world.player_teams) {
        *team = Team::of_data(i32::from(saved));
    }
    let none = TerrainTable::default();
    let terrain = masters.map_or(&none, |master_table| &master_table.terrain);
    match spatial(header, terrain, build) {
        Ok(spatial) => turns.spatial = spatial,
        Err(error) => stop_all(&mut turns, &error),
    }
    // the players' scout maps (`mIsScoutMapEnabled` and its InfluenceMap, player+0x1370)
    for (player_id, player_section) in PlayerId::each(header.players().len()).zip(header.players())
    {
        let wp = player_section.world_player();
        if wp.is_scout_map_enabled.unwrap_or_default() == 0 {
            continue;
        }
        if let Some(tribe_information_ai_module) = wp.scout_map.as_deref() {
            let width = tribe_information_ai_module.xsize_value.max(0) as usize;
            let cells = tribe_information_ai_module
                .row_value
                .iter()
                .flat_map(|entry| {
                    let row = entry.row_value.as_deref().unwrap_or(&[]);
                    (0..width).map(move |x| row.get(x).copied().unwrap_or(0))
                })
                .collect();
            turns.spatial.set_scout_map(
                player_id,
                InfluenceMap {
                    size: GridSize::new(
                        tribe_information_ai_module.xsize_value,
                        tribe_information_ai_module.ysize_value,
                    ),
                    origin: Tile::new(
                        tribe_information_ai_module.xreference_point_value,
                        tribe_information_ai_module.yreference_point_value,
                    ),
                    limit: tribe_information_ai_module.unchangeable_limit_value,
                    cells,
                },
            );
        }
    }
    // the computer players' AI modules (player+0x7a8, saved only where the AI was created)
    for (player_id, player_section) in PlayerId::each(header.players().len()).zip(header.players())
    {
        if let Some(tribe_main_decision_ai) = player_section
            .world_player()
            .tribe_main_decision_ai_module
            .as_deref()
        {
            turns.ai.set(player_id, ai_module(tribe_main_decision_ai));
        }
    }
    let map = &world.map;
    turns.pathing = Pathing::new(
        GridSize::new(map.map_width, map.map_height),
        map.rows
            .iter()
            .flat_map(|rge_map_row| {
                rge_map_row
                    .tiles
                    .iter()
                    .map(|rge_tile| TerrainId::named(rge_tile.terrain_kind))
            })
            .collect(),
        terrain.passable().to_vec(),
        header.world.game_kind,
    );
    // the zone maps (`map+0x5e28`) as the save holds them, their adjacency built by the load
    turns.zones = Zones::from_saved(
        map.map_zones
            .zone_map
            .iter()
            .map(|z| {
                ZoneMap::from_saved(
                    GridSize::new(map.map_width, map.map_height),
                    &z.number_tiles_in_zone_value,
                    &z.zone_map,
                    &Restriction::new(z.float_value.clone()),
                    z.number_zones,
                )
            })
            .collect(),
    );
    // the players' path caps (saved); their plans start over at each player's update
    turns.pathing.budgets = header
        .players()
        .iter()
        .map(|player_section| {
            let world_player_base = &player_section.world_player().base;
            PlanBudget {
                attempt_cap: PlanCap::of_data(world_player_base.pathing_attempt_cap_value),
                delay_cap: PlanCap::of_data(world_player_base.pathing_delay_cap_value),
                plans: PlanCount::ZERO,
            }
        })
        .collect();
    turns.settings = settings(&header.file_header.game_options_read.game_options);
    // the Empire Wars start's pass, once in the game's first update (the world's +0x6c6): the
    // mode 13 or the sub-mode's bit 0
    turns.start_work_due = turns.settings.empire_wars() && turns.world_time == WorldTime::ZERO;
    turns.game_state = PlayState::of_data(u32::from(world.temp_game_state));
    turns.victory_conquest = world.tribe_scenario.base_scenario.victory_conquest != 0;
    match unit_groups(world) {
        Ok(groups) => turns.groups = groups,
        Err(error) => stop_all(&mut turns, &error),
    }
    if let Some(table) = masters {
        turns.history_period = table.time_slice;
        // the unit lines' tables, which the game builds at its first load with Gaia's master
        // count (`FUN_141533cb0` -> `FUN_140fde680`)
        let gaia_masters = header.players().first().map_or(0, |player_section| {
            player_section.objects().master_object_num
        });
        let masters = header
            .players()
            .iter()
            .map(|player_section| {
                work_masters(
                    player_section.objects(),
                    CivId::new(player_section.world_player().base.civ_id),
                    table,
                )
                .map(Arc::new)
            })
            .collect::<Result<_, _>>()
            .unwrap_or_else(|error| {
                stop_all(&mut turns, &error);
                Vec::new()
            });
        // the tech tree is not loaded (MISSING.md): the default table
        turns.model = Model {
            masters,
            graphics: Arc::new(table.graphics.clone()),
            drop_sites: Arc::new(table.drop_sites.clone()),
            lines: Arc::new(UnitLines::build(gaia_masters, &table.unit_lines)),
            linked_techs: Arc::new(table.linked_techs.clone()),
            techs: Arc::new(table.techs.clone()),
            effects: Arc::new(table.effects.clone()),
            ..Model::default()
        };
        turns.model = take(&mut turns.model).with_layer(build);
        garrison_firepower(&mut turns);
        // the planners the load makes and the shape mesh it builds (a dump of the game's mesh
        // replaces it)
        turns.register_planners();
        if let Err(error) = turns.build_mesh() {
            stop_all(&mut turns, &error);
        }
    }
    // the players' object trackers, which the objects' loads fill
    // the players' vision masks, which the world's load rebuilds (its slot 0x78)
    // (both read the objects' masters: without the masters' table there is nothing to rebuild)
    if masters.is_some()
        && let Err(error) = turns.load_trackers().and_then(|()| turns.rebuild_vision())
    {
        stop_all(&mut turns, &error);
    }
    // the spatial index's views of the loaded annexes, relations and task 0x99 masters
    turns.sync_world();
    Ok(turns)
}

/// The saved objects' parts into the world's tables ([`World::load_parts`]); a part of an object
/// the world does not hold stops every player.
fn file_parts(turns: &mut World, parts: Vec<(ObjectId, ObjectParts)>) {
    if let Err(error) = parts
        .into_iter()
        .try_for_each(|(id, part)| turns.load_parts(id, part))
    {
        stop_all(turns, &error);
    }
}

/// Every player stops on `error`, which the world does not model of the save.
fn stop_all(turns: &mut World, error: &GenieError) {
    for player in &mut turns.players {
        let _ = player
            .unsupported
            .get_or_insert_with(|| (Tick::ZERO, ListIndex::new(0), error.clone()));
        player.stopped = true;
    }
}

/// The game's wonder countdown of a player (the game object's `+0x4ec[id]`) after a load: the
/// world's load sets every player's to −1 (`FUN_140c90040`), then each wonder action
/// (`TRIBE_Action_Wonder`, type 120) of the player's objects, live then sleeping, in its load
/// (`FUN_140ae8fa0`) when in state 7 sets its timer (`mTimer`, `+0x44`) where it is less or the
/// countdown below 0 (`FUN_140c91500`).
#[must_use]
pub fn wonder_time(player_section: &PlayerSection<'_>) -> Seconds {
    let mut countdown = -1.0;
    for object in player_section
        .live_objects()
        .chain(player_section.sleeping_objects())
    {
        let Some(actions) = object.actions() else {
            continue;
        };
        for (_, action) in actions.entries() {
            if let Some(tribe_action_wonder) = action.tribe_action_wonder.as_deref()
                && tribe_action_wonder.rge_action.state == 7
            {
                let timer = tribe_action_wonder.rge_action.timer;
                if timer < countdown || countdown < 0.0 {
                    countdown = timer;
                }
            }
        }
    }
    Seconds::new(countdown)
}

/// A player's ghosts (its doppelganger list) as saved: each one's id, place, state, the original's
/// id (`long_val`), owner (`doppled_player`) and UMV entry (`Multi_Unified_Points`), and the
/// players who cannot see it (`CantSeeBits`). Its last bits (`+0x7e8`) and seen flag (`+0x7f7`)
/// are not saved: a load starts them at 0; its original's link waits for the original's update
/// (the world links it by id).
///
/// # Errors
///
/// A ghost's id of −1 or a state the game does not have.
pub fn saved_ghosts(section: &PlayerSection<'_>) -> GenieResult<Vec<Ghost>> {
    section
        .objects()
        .doppleganger_objects
        .objects()
        .filter_map(|object| {
            let rge_doppleganger_object = object
                .rge_object
                .as_deref()?
                .rge_doppleganger_object
                .as_deref()?;
            Some((object.static_part()?, rge_doppleganger_object))
        })
        .map(|(rge_static_object, rge_doppleganger_object)| {
            let original = ObjectId::of_field(rge_doppleganger_object.long_val as i32);
            Ok(Ghost {
                id: ObjectId::try_from(rge_static_object.id)?,
                original,
                original_id: original,
                original_master: MasterId::of_data(rge_doppleganger_object.long_val_2 as i16),
                original_owner: u8::try_from(rge_doppleganger_object.doppled_player)
                    .map_or(PlayerId::GAIA, PlayerId::from),
                position: Position::new(
                    rge_static_object.tx,
                    rge_static_object.ty,
                    rge_static_object.tz,
                ),
                tile: Tile::new(rge_static_object.tx as i32, rge_static_object.ty as i32),
                footprint: rge_doppleganger_object.multi_unified_points,
                bits: VisionBits::NONE,
                cant_see: PlayerSet::from_bits_retain(rge_doppleganger_object.cant_see_bits),
                seen: false,
                state: ObjectState::of_byte(rge_static_object.object_state)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
