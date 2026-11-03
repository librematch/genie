//! The slice set up from the header save ([`slice`]): the players' technologies, wonder
//! times, ghosts, AI settings and history, and the garrisons' firepower.

use genie_core::GenieError;
use genie_core::gathering::QueueCount;
use genie_core::map::Distance;
use genie_core::map::GridSize;
use genie_core::map::Tile;
use genie_core::map::WholeDistance;
use genie_core::objects::ListIndex;
use genie_core::players::Amount;
use genie_core::players::Chance;
use genie_core::players::CivId;
use genie_core::players::EscrowShare;
use genie_core::players::LotVolume;
use genie_core::players::Team;
use genie_core::players::game_status::Fealty;
use genie_core::players::game_status::GameStatus;
use genie_core::players::game_status::VictoryState;
use genie_core::world::FrameSeed;
use genie_core::world::Millis;
use genie_core::world::Model;
use genie_core::world::PlayState;
use genie_core::world::Seconds;
use genie_core::world::Tick;
use genie_core::world::WorldTime;
use genie_model::layer::BuildLayer;
use genie_nickel::layer::build_layer;
use std::sync::Arc;
use std::sync::LazyLock;

mod player_parts;

use self::player_parts::ai_settings;
pub(super) use self::player_parts::tech_state;
pub use self::player_parts::{ai_module, history};

use crate::masters::{MasterTable, TerrainTable};
use crate::{groups, market, object, spatial, work_masters};
use genie_core::combat::Firepower;
use genie_core::garrison::{attack_rate, share as garrison_share};
use genie_core::map::Position;
use genie_core::map::{Restriction, TerrainId};
use genie_core::movement::explore::InfluenceMap;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::lines::UnitLines;
use genie_core::objects::masters::MasterId;
use genie_core::objects::masters::MasterKind;
use genie_core::objects::unit::Unit;
use genie_core::pathing::zones::{ZoneMap, Zones};
use genie_core::pathing::{Pathing, PlanBudget, PlanCap, PlanCount};
use genie_core::players::PlayerId;
use genie_core::players::tracker::Tracker;
use genie_core::players::world_player::VictoryTimes;
use genie_core::players::{PlayerSet, PlayerType, Posture, Relations, Stance};
use genie_core::vision::VisionBits;
use genie_core::vision::ghost::Ghost;
use genie_core::world::parts::ObjectParts;
use genie_core::world::parts::Part;
use genie_core::world::settings::Settings;
use genie_core::world::{
    Age, ContentMode, Formations, GameMode, ResourceLevel, SubModeFlags, TreatyLength, Unread,
    Visibility,
};
use genie_core::world::{Player, World};
use genie_save::generated::world::GameOptions;
use genie_save::{Header, PlayerSection};
use std::mem::take;

/// The build genie-oracle's import reads, its saves' and its `.dat`'s: its layer applies.
const LAYER_BUILD: u32 = 158_041;

/// The build layer, evaluated once from its Nickel source (FD-046-D08, D09's step 2).
static LAYER: LazyLock<Result<BuildLayer, GenieError>> = LazyLock::new(|| build_layer(LAYER_BUILD));

/// The build layer genie-oracle's import reads: build 158041's, from `builds/158041.ncl`.
///
/// # Errors
///
/// Its Nickel source does not evaluate or breaks its contracts.
pub fn layer() -> Result<&'static BuildLayer, GenieError> {
    LAYER.as_ref().map_err(Clone::clone)
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
///
/// # Errors
///
/// The build layer does not evaluate ([`layer`]); what else the slice does not model of the save
/// stops every player instead.
pub(super) fn slice(header: &Header, masters: Option<&MasterTable>) -> Result<World, GenieError> {
    let build = layer()?;
    let missile_tuning = &build.tuning.missile;
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
    // each object's parts, for the world's tables once its lists stand
    let mut parts: Vec<(ObjectId, ObjectParts)> = Vec::new();
    let players = header
        .players()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let civ = CivId::new(p.world_player().base.civ_id);
            let attributes = &p.world_player().base.attributes;
            let mut rows = |saved: &mut dyn Iterator<Item = &genie_save::Object>| -> Vec<Unit> {
                saved
                    .map(|o| {
                        let whole = object(
                            o,
                            p.objects(),
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
            let live = rows(&mut p.live_objects());
            let sleeping = rows(&mut p.sleeping_objects());
            Player {
                objects: live.into(),
                sleeping: sleeping.into(),
                kind: PlayerType::from(p.player.type_),
                attributes: attributes.iter().map(|&a| Amount::new(a)).collect(),
                // the second attribute array (+0x658), floats as their bits
                generated: p
                    .world_player()
                    .base
                    .attributes_2
                    .iter()
                    .map(|&b| Amount::new(f32::from_bits(b)))
                    .collect(),
                civ,
                relations: Relations::of_bytes(p.world_player().base.relation.iter().copied()),
                auto_farm_queue: p.world_player().auto_farm_queue != 0,
                auto_fishtrap_queue: p.world_player().auto_fish_trap_queue != 0,
                farm_queue: QueueCount::of_data(p.world_player().farm_queue_count),
                fish_trap_queue: QueueCount::of_data(p.world_player().fish_trap_queue_count),
                production_queue_on: p.world_player().prod_queue_on,
                diplomacy: p
                    .world_player()
                    .per_player
                    .iter()
                    .map(|e| Stance::from(e.diplomacy))
                    .collect(),
                ai_settings: ai_settings(&p.world_player().base),
                formations: Formations {
                    auto: p.world_player().base.auto_formations != 0,
                    influence: Distance::new(p.world_player().base.formation_influence_distance),
                    by_speed: p.world_player().base.break_auto_formations_by_speed == 1,
                    min_column_distance: WholeDistance::of_data(
                        p.world_player().base.min_column_distance,
                    ),
                    column_to_line_distance: WholeDistance::of_data(
                        p.world_player().base.column_to_line_distance,
                    ),
                },
                pending_debit: p.world_player().base.pending_debit.map(Amount::new),
                techs: techs.get_mut(i).map(take).unwrap_or_default(),
                unsupported: None,
                freezes: Vec::new(),
                stopped: false,
                game_status: GameStatus::of_data(p.world_player().base.game_status),
                // `+0x8b` holds the drop in its bit 1 (the load moves it to `+0x8c`)
                resigned: p.world_player().base.resigned & 2 == 0
                    && p.world_player().base.resigned != 0,
                dropped: p.world_player().base.resigned & 2 != 0,
                allied_victory: p.world_player().base.allied_victory,
                allied_vision: p.world_player().base.allied_los_enable != 0,
                // the reveal of the enemies' town centres (attribute 209) is done once any
                // mTCFlared bit is set; the saved map then already holds the revealed tiles
                tc_flared: p.world_player().tc_flared.iter().map(|&b| b != 0).collect(),
                sharing: p.world_player().sharing_is_caring != 0,
                commodity_volume: LotVolume::of_data(p.world_player().commodity_volumn_delta),
                victory: VictoryState::of_data(victory.get(i).map_or(0, |v| v.victory)),
                fealty: Fealty::of_data(p.world_player().fealty as i16),
                // the load sets it 0 (`FUN_14163e970`)
                posture: Posture::AGGRESSIVE,
                dodge_chance: Chance::of_data(p.world_player().chance_to_dodge_missiles),
                keep_distance_chance: Chance::of_data(
                    p.world_player().chance_for_archers_to_maintain_distance,
                ),
                // the load builds the watched lists (World::build_watched)
                watched: Vec::new(),
                // the saved ghosts follow once the world stands (a ghost without an id stops it)
                ghosts: Vec::new(),
                history: history(&p.world_player().history),
                // the load files the objects (World::load_trackers, once the masters are in)
                tracker: Tracker::default(),
                // +0x1378 is not saved
                one_military_queued: false,
                update_time: Seconds::new(p.world_player().update_time),
                gaia_update_time: p.player.gaia_player.as_deref().map_or(Seconds::ZERO, |g| {
                    Seconds::new(g.world_player_gaia.update_time)
                }),
                escrow: p.world_player().base.escrow.map(Amount::new),
                escrow_percentage: p
                    .world_player()
                    .base
                    .escrow_percentage
                    .map(EscrowShare::of_data),
                sudden_death_eliminable: p.world_player().sudden_death_can_be_eliminated != 0,
                victory_times: VictoryTimes {
                    // the world's load resets every player's (FUN_140c90040), then a wonder's
                    // action in state 7 sets its time (its load, FUN_140ae8fa0)
                    wonder: wonder_time(p),
                    ruins: Seconds::new(p.world_player().ruin_held_time),
                    artifacts: Seconds::new(p.world_player().artifact_held_time),
                },
            }
        })
        .collect();
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
        Err(e) => stop_all(&mut turns, &e),
    }
    if let Some(e) = tech_error {
        stop_all(&mut turns, &e);
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
        Err(e) => stop_all(&mut turns, &e),
    }
    turns.market = market(&header.world);
    turns.local_player = PlayerId::try_from(usize::try_from(world.local_player_index).unwrap_or(0))
        .unwrap_or_default();
    turns.alliance_locked = world.alliance_locked;
    // the world's teams (+0x984, `playerTeams[i]`, saved as bytes)
    for (team, &saved) in turns.teams.iter_mut().zip(&header.world.player_teams) {
        *team = Team::of_data(i32::from(saved));
    }
    let none = TerrainTable::default();
    let terrain = masters.map_or(&none, |t| &t.terrain);
    match layer().and_then(|l| spatial(header, terrain, l)) {
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
            let width = m.xsize_value.max(0) as usize;
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
                InfluenceMap {
                    size: GridSize::new(m.xsize_value, m.ysize_value),
                    origin: Tile::new(m.xreference_point_value, m.yreference_point_value),
                    limit: m.unchangeable_limit_value,
                    cells,
                },
            );
        }
    }
    // the computer players' AI modules (player+0x7a8, saved only where the AI was created)
    for (i, p) in PlayerId::each(header.players().len()).zip(header.players()) {
        if let Some(m) = p.world_player().tribe_main_decision_ai_module.as_deref() {
            turns.ai.set(i, ai_module(m));
        }
    }
    let map = &world.map;
    turns.pathing = Pathing::new(
        GridSize::new(map.map_width, map.map_height),
        map.rows
            .iter()
            .flat_map(|r| r.tiles.iter().map(|t| TerrainId::named(t.terrain_type)))
            .collect(),
        terrain.passable().to_vec(),
        header.world.game_type,
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
        .map(|p| {
            let b = &p.world_player().base;
            PlanBudget {
                attempt_cap: PlanCap::of_data(b.pathing_attempt_cap_value),
                delay_cap: PlanCap::of_data(b.pathing_delay_cap_value),
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
    match groups::unit_groups(world) {
        Ok(g) => turns.groups = g,
        Err(e) => stop_all(&mut turns, &e),
    }
    if let Some(table) = masters {
        turns.history_period = table.time_slice;
        // the unit lines' tables, which the game builds at its first load with Gaia's master
        // count (`FUN_141533cb0` -> `FUN_140fde680`)
        let gaia_masters = header
            .players()
            .first()
            .map_or(0, |p| p.objects().master_object_num);
        let masters = header
            .players()
            .iter()
            .map(|p| {
                work_masters(p.objects(), CivId::new(p.world_player().base.civ_id), table)
                    .map(Arc::new)
            })
            .collect::<Result<_, _>>()
            .unwrap_or_else(|e| {
                stop_all(&mut turns, &e);
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
        // the players' unit masters (type 70), Gaia's left out
        let all = || {
            turns
                .model
                .masters
                .iter()
                .skip(1)
                .flat_map(|m| m.iter())
                .filter(|m| m.kind == MasterKind::COMBAT)
        };
        turns.spatial.fastest = all()
            .filter_map(|m| m.moving.as_ref().map(|s| s.speed.get()))
            .fold(0.0, f32::max);
        turns.spatial.reach = Distance::new(
            all()
                .flat_map(|m| {
                    m.tasks
                        .iter()
                        .map(|t| t.work_range)
                        .chain([m.combat_part().weapon_range])
                })
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
    // the spatial index's views of the loaded annexes, relations and task 0x99 masters
    turns.sync_world();
    Ok(turns)
}

/// The saved objects' parts into the world's tables ([`World::load_parts`]); a part of an object
/// the world does not hold stops every player.
fn file_parts(turns: &mut World, parts: Vec<(ObjectId, ObjectParts)>) {
    if let Err(e) = parts
        .into_iter()
        .try_for_each(|(id, part)| turns.load_parts(id, part))
    {
        stop_all(turns, &e);
    }
}

/// Every player stops on `e`, which the slice does not model of the save.
fn stop_all(turns: &mut World, e: &GenieError) {
    for p in &mut turns.players {
        let _ = p
            .unsupported
            .get_or_insert_with(|| (Tick::ZERO, ListIndex::new(0), e.clone()));
        p.stopped = true;
    }
}

/// The game's wonder countdown of a player (the game object's `+0x4ec[id]`) after a load: the
/// world's load sets every player's to −1 (`FUN_140c90040`), then each wonder action
/// (`TRIBE_Action_Wonder`, type 120) of the player's objects, live then sleeping, in its load
/// (`FUN_140ae8fa0`) when in state 7 sets its timer (`mTimer`, `+0x44`) where it is less or the
/// countdown below 0 (`FUN_140c91500`).
pub(super) fn wonder_time(p: &PlayerSection<'_>) -> Seconds {
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
    Seconds::new(countdown)
}

/// A player's ghosts (its doppelganger list) as saved: each one's id, place, state, the original's
/// id (`long_val`), owner (`doppled_player`) and UMV entry (`Multi_Unified_Points`), and the
/// players who cannot see it (`CantSeeBits`). Its last bits (`+0x7e8`) and seen flag (`+0x7f7`)
/// are not saved: a load starts them at 0; its original's link waits for the original's update
/// (the slice links it by id).
/// The game options the simulation reads ([`genie_core::world::settings::Settings`]) from the header's
/// `GameOptions`, as `FUN_141305310` loads them.
#[must_use]
pub const fn settings(o: &GameOptions) -> Settings {
    Settings {
        game_mode: GameMode::of_data(o.game_mode),
        ending_age: Age::of_data(o.ending_age),
        visibility_type: Visibility::of_data(o.visibility_type),
        sub_game_mode_flag: SubModeFlags::from_bits_retain(o.sub_game_mode_flag),
        full_tech_tree: o.full_tech_tree_enabled != 0,
        walkable_farms: o.walkable_farms != 0,
        selected_content_mode: ContentMode::of_data(o.selected_content_mode),
        villager_force_drop: o.villager_force_drop != 0,
        cheats_enabled: o.cheats_enabled != 0,
        treaty_length: TreatyLength::of_minutes(o.treaty_length),
        teams_locked: o.teams_locked != 0,
        starting_age: Age::of_data(o.starting_age),
        resource_level: ResourceLevel::of_data(o.resource_level),
        turbo: o.turbo_enabled != 0,
        shared_exploration: o.shared_exploration != 0,
        antiquity: o.antiquity_mode != 0,
    }
}

pub(super) fn saved_ghosts(section: &PlayerSection<'_>) -> Result<Vec<Ghost>, GenieError> {
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
            Ok(Ghost {
                id: ObjectId::try_from(s.id)?,
                original,
                original_id: original,
                original_master: MasterId::of_data(d.long_val_2 as i16),
                original_owner: u8::try_from(d.doppled_player)
                    .map_or(PlayerId::GAIA, PlayerId::from),
                position: Position::new(s.tx, s.ty, s.tz),
                tile: Tile::new(s.tx as i32, s.ty as i32),
                footprint: d.multi_unified_points,
                bits: VisionBits::NONE,
                cant_see: PlayerSet::from_bits_retain(d.cant_see_bits),
                seen: false,
                state: ObjectState::of_byte(s.object_state)?,
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
    for p in 0..turns.players.len() {
        let entered = entered_shares(turns, p);
        file_garrisons(turns, p, entered);
    }
}

/// A unit inside a building, the building, and the unit's share of its volleys (none for a
/// unit that is not of type 70 or a building whose master the table lacks).
type Entered = (ObjectId, ObjectId, Option<Result<Firepower, GenieError>>);

/// Player `p`'s units inside a building of its own, in list order, each with its building and
/// its share ([`garrison_firepower`]).
fn entered_shares(turns: &World, p: usize) -> Vec<Entered> {
    let (Some(masters), Some(player)) = (turns.model.masters.get(p), turns.players.get(p)) else {
        return Vec::new();
    };
    let lists = || player.objects.iter().chain(&player.sleeping);
    let mut entered = Vec::new();
    for u in lists() {
        let Some(building) = u.life.inside else {
            continue;
        };
        let Some(b) = lists().find(|b| b.id == building.id()) else {
            continue;
        };
        let share = masters
            .get(u.life.master_id)
            .filter(|m| m.kind == MasterKind::COMBAT)
            .and_then(|me| {
                let bm = masters.get(b.life.master_id)?;
                let rate = attack_rate(
                    bm,
                    masters,
                    turns.parts.combat_of(b.id).reload_powerup,
                    &turns.model.roles,
                    &turns.model.tuning.garrison,
                );
                Some(garrison_share(
                    me,
                    masters,
                    turns.parts.combat_of(u.id).reload_powerup,
                    rate,
                    &turns.model.roles,
                ))
            });
        entered.push((b.id, u.id, share));
    }
    entered
}

/// Each building of player `p` lists the units `entered` inside it and adds their shares to
/// its volleys; a share the slice does not model leaves the building unmodelled.
fn file_garrisons(turns: &mut World, p: usize, entered: Vec<Entered>) {
    for (b, id, share) in entered {
        let unmodelled = match share {
            Some(Ok(share)) => {
                let mut combat = turns.parts.combat_of(b).clone();
                combat.firepower += share;
                turns.set_part(b, Part::Combat(Box::new(combat))).err()
            }
            Some(Err(e)) => Some(e),
            None => None,
        };
        let Some(building) = turns.players.get_mut(p).and_then(|q| {
            q.objects
                .iter_mut()
                .chain(&mut q.sleeping)
                .find(|u| u.id == b)
        }) else {
            continue;
        };
        building.garrison.push(id);
        if let Some(e) = unmodelled {
            let unread = Part::Unread(Some(Unread::new(e)));
            // the world holds the building (it is in its player's lists): the write cannot fail
            if let Err(e) = turns.set_part(b, unread) {
                stop_all(turns, &e);
            }
        }
    }
}

#[cfg(test)]
mod tests;
