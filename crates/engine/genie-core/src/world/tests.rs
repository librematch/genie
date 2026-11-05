use super::*;
use crate::combat::Firepower;
use crate::combat::KillState;
use crate::combat::abilities::Abilities;
use crate::combat::attack::AttackState;
use crate::combat::attack::{Arms, Attack};
use crate::combat::damage::ArmorPair;
use crate::combat::damage::{Kill, KillCount, Razed, Victim};
use crate::combat::missile::ShooterId;
use crate::combat::missile::SmartMode;
use crate::combat::missile::{MissSpread, Missile, MissileState, ProjectileKind, StrikeResult};
use crate::combat::victims::{KillLeft, file_kill, write_back};
use crate::combat::volley::Aimed;
use crate::combat::{AttackCount, AttackTargetId, MissileCount};
use crate::commands::Bell;
use crate::commands::Departure;
use crate::commands::GameSub;
use crate::commands::GatherPointState;
use crate::commands::ResourceWord;
use crate::commands::Shift;
use crate::commands::UnloadPick;
use crate::commands::game::GameCommand;
use crate::commands::{
    AllBackToWorkCommand, AttackMoveCommand, AutoScoutCommand, BuildCommand, BuildWallCommand,
    BuildingQueueCommand, Command, DeleteCommand, FlareCommand, GateCommand, GatherPoint,
    GatherPointCommand, GoBackToWork2Command, Order, OrderCommand, PatrolCommand, QueueCommand,
    ResearchCommand, ResignCommand, SeekShelterCommand, SetGatherStateCommand, StopCommand,
    TownBellCommand, TradeCommand, TransformObjectCommand, TributeCommand, UnitAiStateCommand,
    UnitList, UnitOrderAction, UnitOrderCommand, UnloadCommand, UnmodelledCommand, Waypoints,
};
use crate::commands::{QueueId, SpriteOverride, TradeLots};
use crate::construction::BuildProgress;
use crate::construction::FoundationId;
use crate::construction::MainBuildingId;
use crate::construction::build::BuildState;
use crate::construction::build::{BUILDER_UNMOVING, CART_TASK};
use crate::construction::build::{BuildAction, BuildTask, Construction};
use crate::construction::gates::gate_units;
use crate::construction::repair::RepairState;
use crate::construction::repair::{BUILDING_REPAIR_COST, REPAIR};
use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidify};
use crate::construction::{Annexes, BuilderCount, RepairTargetId};
use crate::garrison::ContainerId;
use crate::garrison::GarrisonScope;
use crate::garrison::tests::enter_task;
use crate::garrison::{ENTER, EnterAction, EnterState, RaiderCountdown, UNREACHED};
use crate::gathering::DropSiteId;
use crate::gathering::PastureId;
use crate::gathering::QueueCount;
use crate::gathering::closest_resource::search_origin;
use crate::gathering::drop_sites::DropMeasure;
use crate::gathering::drop_sites::SitePlace;
use crate::gathering::drop_sites::SiteSearch;
use crate::gathering::farm_queue::Reseed;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::WalkEnd;
use crate::gathering::gather::unit::give_load;
use crate::gathering::gather::{GATHER, GatherAction, Resource, StopRate};
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::TreeSearch;
use crate::gathering::hunt::tests::{hunting, walker};
use crate::gathering::hunt::{HUNT, HuntAction, HuntTask};
use crate::gathering::make::MAKE;
use crate::gathering::make::MakeState;
use crate::gathering::{GatherTargetId, HuntTargetId};
use crate::groups::GroupFlags;
use crate::groups::{Member, UnitGroup};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Offset;
use crate::map::Point;
use crate::map::Position;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::Snow;
use crate::map::TerrainFlags;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::{BLOCKED, Obstruction, Tiles};
use crate::map::spatial::{Placed, Spatial};
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::{Stream, Streams, Well512};
use crate::movement::MoveAlgorithm;
use crate::movement::PathFlags;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::TurnRate;
use crate::movement::ai_moves::MoveSprites;
use crate::movement::ai_moves::PointMove;
use crate::movement::explore::{ExploreAction, ExploreState, InfluenceMap, bare_walk};
use crate::movement::fly::FLY;
use crate::movement::fly::Fly;
use crate::movement::fly::FlyState;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::Patience;
use crate::movement::move_to::{MOVE_TO, MoveWait, TargetMoved};
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::movement::{
    Me, MoveStats, Mover, PathData, Velocity, WaypointIndex, footprint_gap_squared,
};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::Wake;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::facing::{SpriteFacets, drawn_angle};
use crate::objects::layers::{
    Action, Animated, Building, Combat, DefaultTask, Motion, MotionMut, Static, Unstartable,
};
use crate::objects::lifecycle::death_parts::{count_class, storage_leaves};
use crate::objects::lifecycle::{Class, Held, Lifecycle, MasterData};
use crate::objects::lines::UnitLines;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::AutoGatherMode;
use crate::objects::masters::Availability;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::BuiltMode;
use crate::objects::masters::CarryCapacity;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CostAmount;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::FlyMode;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::FormationClass;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::HitChance;
use crate::objects::masters::MasterField;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MissilePart;
use crate::objects::masters::MovingPart;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::RecycleMode;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskIndex;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::VolleySpread;
use crate::objects::masters::{
    BuildingMaster, DamageShare, Task, TrainLocation, WorkValue, player_master,
};
use crate::objects::masters::{Master, MasterTables, Masters, OwnerMasters};
use crate::objects::object_list::ObjectList;
use crate::objects::release::{Releaser, released};
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Presence;
use crate::objects::unit::UnitFlags;
use crate::objects::unit::{Outcome, TAKEN_OUT};
use crate::objects::unit::{Unit, loses_actions};
use crate::objects::{DamagedTimer, FacetCount, Frame, Regeneration, WorkerCount};
use crate::orders::ControlKey;
use crate::orders::Dispatch;
use crate::orders::OrderFlags;
use crate::orders::SavedPatrols;
use crate::orders::town_bell::BackToWork;
use crate::orders::{Interact, MoveOrder, Sweep, Work};
use crate::ownership::CaptureKind;
use crate::ownership::artifact::Artifact;
use crate::ownership::artifact::ArtifactState;
use crate::ownership::artifact::{ARTIFACT_KIND, ArtifactSub};
use crate::ownership::change::change_hands;
use crate::ownership::change::dead_master_of;
use crate::pathing::tile_path::{Path, PlanRect};
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::ByPlayer;
use crate::players::ByRelation;
use crate::players::CivId;
use crate::players::PlayerKind;
use crate::players::Relation;
use crate::players::TributeFee;
use crate::players::attribute::AttributeId;
use crate::players::game_status::GameStatus;
use crate::players::history::{EventKind, HistoryEntry};
use crate::players::relation_row;
use crate::players::relations;
use crate::players::tracker::ListKey;
use crate::players::tracker::{Tracker, key};
use crate::production::Batch;
use crate::production::LocationButton;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::production::{HotkeyId, MakeObjState, MakeTechState, Making, Production, Queue};
use crate::technology::CostPaid;
use crate::technology::Linked;
use crate::technology::ResearchState;
use crate::technology::{CostOverride, OverrideUse, ResearchCap, TechKind};
use crate::test_ids::attack_category;
use crate::test_ids::carrying;
use crate::test_ids::class_id;
use crate::test_ids::class_of;
use crate::test_ids::costs;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::opt_master;
use crate::test_ids::order_kind;
use crate::test_ids::owner;
use crate::test_ids::player_id;
use crate::test_ids::task_id;
use crate::test_ids::tech_id;
use crate::test_ids::unit_list;
use crate::test_ids::{adds as test_ids_adds, amounts, attributes};
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::ids::VILLAGER_FEMALE;
use crate::testing::ids::VILLAGER_MALE;
use crate::testing::roles;
use crate::testing::scene::{move_by, moving_master, walking};
use crate::testing::slots;
use crate::testing::static_roles;
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiCommand;
use crate::unit_ai::AiLayer;
use crate::unit_ai::Answer;
use crate::unit_ai::Attackers;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderOutcome;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::Pace;
use crate::unit_ai::StopOrder;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::DropSite;
use crate::unit_ai::task_finder::DropTarget;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::{
    SideWorld, TaskWorld, task_search, task_target_with, wait_progress,
};
use crate::unit_ai::task_finder::{Actor, Diplomacy};
use crate::unit_ai::world_view::Helper;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::world_view::{DropWorld, Foreign, Wait, patrol_information};
use crate::unit_ai::{
    AiMove, Attacked, Attacker, NO_POINT, NewObject, OrderPriority, OrderRecord, PatrolPath,
    QueuedOrder, helper_answers,
};
use crate::unit_ai::{Effect, Notify, UnitAi};
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::ghost::{Cell, Watched};
use crate::vision::registry::UnitUpdate;
use crate::vision::registry::{Record, Registry};
use crate::vision::visibility::{Seer, capture_kind};
use crate::vision::visibility::{Visibility as VisionVisibility, VisibleMap};
use crate::world::AutoQueues;
use crate::world::checkpoint::TopAction;
use crate::world::layer::ClassRoles;
use crate::world::others::OthersRef;
use crate::world::parts::{Object, Part, PartTable, no_own_masters};
use crate::world::player::Player;
use crate::world::settings::FarmMode;
use crate::world::settings::OptionFlags;
use crate::world::world_update::ListCount;
use crate::{GenieError, GenieResult};
use ids::FISH_TRAP;
use std::iter::{once, repeat_n};
use std::sync::Arc;

/// Gives every player a default master for each master id its units name (the table the game
/// always has, every master with its dead-object values, as the `.dat` gives them), the ones the
/// test set already has kept.
pub(crate) fn stock_masters(world: &mut World) {
    let tables: Vec<Arc<Masters>> = world
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let held = world.model.masters.get(index);
            let mut all: Vec<Master> = held
                .map(|masters| masters.iter().cloned().collect())
                .unwrap_or_default();
            for unit in player.objects.iter().chain(&player.sleeping) {
                if !all.iter().any(|master| master.id == unit.life.master_id) {
                    all.push(Master {
                        id: unit.life.master_id,
                        life: Some(MasterData::default()),
                        ..Master::default()
                    });
                }
            }
            Arc::new(Masters::new(all))
        })
        .collect();
    world.model.masters = tables;
}

/// A player of objects `ids` (their unit AIs the world's, [`World::with_ais`]).
fn player(ids: &[i32]) -> Player {
    Player {
        objects: ids
            .iter()
            .map(|&id| Unit {
                id: object_id(id),
                ..Unit::default()
            })
            .collect(),
        ..Player::default()
    }
}

fn building() -> UnitAi {
    UnitAi {
        ai_kind: UnitAiKind::Building,
        current_order: None,
        current_action: None,
        current_target: None,
        adjusted_idle_timeout: Millis::new(3000),
        look_around_timeout: Millis::new(1000),
        retarget_timer: Millis::new(1),
        ..UnitAi::default()
    }
}

/// Object `id`'s construction, to change.
fn site_mut(world: &mut World, id: ObjectId) -> &mut Construction {
    world
        .parts
        .building_mut(id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
}

/// Object `unit`'s repairers ([`crate::world::parts::Parts::repairers_of`]).
fn repairers_of(world: &World, unit: &Unit) -> u8 {
    world.parts.repairers_of(unit.id).get()
}

#[test]
fn the_turn_rotates_and_only_the_turn_player_updates() {
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![player(&[1]), player(&[2]), player(&[])],
    )
    .with_ais([(object_id(1), building()), (object_id(2), building())]);
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    assert_eq!(world.player_turn, PlayerId::new(2));
    assert_eq!(world.player_time, [10, 0, 10].map(Millis::new));
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .unwrap()
            .last_retarget_time,
        WorldTime::ZERO
    );
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .last_retarget_time,
        WorldTime::from_millis(10)
    );
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    // turns 2 (no objects), then 0; back at player 1
    assert_eq!(world.player_turn, PlayerId::new(1));
    assert_eq!(world.player_time, [0, 20, 10].map(Millis::new));
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .unwrap()
            .last_retarget_time,
        WorldTime::from_millis(30)
    );
}

#[test]
fn the_frame_seed_chains_through_two_world_draws() {
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(&[])],
    );
    world.tick(Millis::new(10));
    // SeedAll(29910); s1 = World.rand15 = 19550 (see rng); SeedAll(19550); World.rand15
    let mut streams = Streams::default();
    streams.seed_all(19550);
    assert_eq!(
        world.frame_seed,
        FrameSeed::of_data(streams.get_mut(Stream::World).rand15())
    );
}

#[test]
fn artifacts_of_every_player_start_updating_in_the_second_tick() {
    // an animal of each player (ids 1 and 2) with its Artifact, the table's
    let animal = |id| Unit {
        id: object_id(id),
        ..Unit::default()
    };
    let layer = |owner| Action {
        list: ActionList::default().with_artifact(Some(Artifact::new(owner, None))),
        ..Default::default()
    };
    let players = vec![
        Player {
            objects: vec![animal(1)].into(),
            ..Player::default()
        },
        Player {
            objects: vec![animal(2)].into(),
            ..Player::default()
        },
    ];
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        players,
    )
    .with_actions([
        (object_id(1), layer(PlayerId::GAIA)),
        (object_id(2), layer(PlayerId::new(1))),
    ]);
    let state = |world: &World, player: usize| {
        world
            .parts
            .actions_of(world.players[player].objects[ListIndex::new(0)].id)
            .list
            .artifact()
            .copied()
            .unwrap()
            .state
    };
    stock_masters(&mut game);
    game.tick(Millis::new(50));
    assert_eq!(
        (state(&game, 0), state(&game, 1)),
        (ArtifactState::Check, ArtifactState::Check)
    );
    stock_masters(&mut game);
    game.tick(Millis::new(50));
    assert_eq!(
        (state(&game, 0), state(&game, 1)),
        (ArtifactState::Wait, ArtifactState::Wait)
    );
}

#[test]
fn an_unsupported_unit_stops_and_is_recorded() {
    let mut odd = building();
    // a unit-AI type the slice does not model: the ranged unit's, which only a load makes
    odd.ai_kind = UnitAiKind::Ranged;
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(&[1])],
    )
    .with_ais([(object_id(1), odd)]);
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    assert_eq!(
        world.players[0].unsupported,
        Some((
            Tick::new(1),
            ListIndex::new(0),
            GenieError::unsupported("unit-AI type not modelled")
        ))
    );
}

/// A dead building (object 1, its unit AI [`building`]).
fn dead_building() -> Unit {
    Unit {
        id: object_id(1),
        life: Lifecycle {
            class: Class::Animated,
            hp: HitPoints::ZERO,
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// A live building (its unit AI [`live_ai`]).
fn live(id: i32) -> Unit {
    Unit {
        id: object_id(id),
        ..Unit::default()
    }
}

/// A live building's unit AI, whose retarget timer falls due in the fourth tick of 10 ms (world
/// time 40).
fn live_ai() -> UnitAi {
    UnitAi {
        retarget_timer: Millis::new(40),
        ..building()
    }
}

/// The unit AIs of the live buildings `ids` ([`live_ai`]).
fn live_ais(ids: &[i32]) -> Vec<(ObjectId, UnitAi)> {
    ids.iter().map(|&id| (object_id(id), live_ai())).collect()
}

fn ids(world: &World) -> Vec<i32> {
    world.players[0]
        .objects
        .iter()
        .map(|unit| unit.id.get())
        .collect()
}

fn retarget(world: &World) -> Vec<u32> {
    world.players[0]
        .objects
        .iter()
        .map(|unit| {
            world
                .parts
                .unit_ai
                .get(unit.id)
                .unwrap()
                .retarget_timer
                .get()
        })
        .collect()
}

#[test]
fn removed_object_is_replaced_by_the_last_one_which_updates_in_the_same_tick() {
    // RGE_Object_List::update: the dead building leaves in its fourth update; the last object
    // takes its slot and updates there, so it draws before the object behind the slot
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![dead_building(), live(2), live(3)].into(),
            ..Player::default()
        }],
    )
    .with_ais(live_ais(&[2, 3]))
    .with_ais([(object_id(1), building())]);
    for _ in 0..4 {
        stock_masters(&mut world);
        world.tick(Millis::new(10));
    }
    assert_eq!(ids(&world), [3, 2]);
    // the same ticks with the two live buildings in that order from the start: their timers
    // fall due in the fourth tick, so its draws depend only on the order
    let mut reordered = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![live(3), live(2)].into(),
            ..Player::default()
        }],
    )
    .with_ais(live_ais(&[3, 2]));
    for _ in 0..4 {
        stock_masters(&mut reordered);
        reordered.tick(Millis::new(10));
    }
    assert_eq!(retarget(&world), retarget(&reordered));
}

#[test]
fn dead_objects_unit_ai_draws_nothing() {
    // the dead building's unit AI is the building's, the live ones' theirs
    let one = |objects: Vec<Unit>| {
        let ais: Vec<_> = objects
            .iter()
            .map(|unit| {
                let ai = if unit.id == object_id(1) {
                    building()
                } else {
                    live_ai()
                };
                (unit.id, ai)
            })
            .collect();
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(29910),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: objects.into(),
                ..Player::default()
            }],
        )
        .with_ais(ais);
        stock_masters(&mut world);
        world.tick(Millis::new(10));
        world
    };
    let with_dead = one(vec![dead_building(), live(2)]);
    let alone = one(vec![live(2)]);
    assert_eq!(
        with_dead.players[0].objects[ListIndex::new(0)].life.state,
        ObjectState::Dead
    );
    // its full update adds the world's dt to the time since the last enemy sighting, and
    // returns before anything else
    let ai = building();
    let dead_ai = UnitAi {
        time_since_enemy: ai.time_since_enemy + Seconds::new(0.01),
        ..ai
    };
    assert_eq!(
        with_dead.parts.unit_ai.get(object_id(1)),
        Some(&dead_ai),
        "the dead building's timers stay"
    );
    assert_eq!(
        with_dead.parts.unit_ai.get(object_id(2)),
        alone.parts.unit_ai.get(object_id(2))
    );
}

/// The masters of MarvinBlue/426048731's player 2 that its BUILD at tick 679 uses, shortened
/// from build 158041's save and `.dat`.
pub(crate) fn builder_masters() -> Masters {
    use crate::objects::masters::{AnimatedPart, Master, Masters, Task};
    let task = |action_kind, object_group| Task {
        action_kind,
        object_group,
        ..Task::default()
    };
    let villager = |id, group, tasks: Vec<Task>| Master {
        id,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        obstruction_class: ObstructionClass::NONE,
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::NONE,
        hit_points: MasterHitPoints::of_data(25),
        // a villager moves (its master's slot 0xe8, `+0x740`), which BUILD's last loop and the
        // work command ask
        animated: Some(AnimatedPart {
            max_speed: Speed::new(0.8),
        }),
        move_stats: Some(MoveStats {
            fly_mode: FlyMode::GROUND,
            speed: Speed::new(0.8),
            base: Speed::new(0.8),
            turn_rate: TurnRate::MAX,
            turn_radius: Distance::ZERO,
            turn_radius_speed: Speed::MAX,
            radius: Radii::new(0.2, 0.2, 0.5),
            restriction: RestrictionId::named(7),
            restriction_mask: TerrainMask::of_bits(6),
            turn_time: Seconds::ZERO,
            move_algorithm: MoveAlgorithm::NONE,
            min_collision: 1.0,
            fast_turn: false,
            facets: None,
            weapon_range: Distance::ZERO,
        }),
        // no sprites (the world's sprites' table is empty)
        standing_sprite: None,
        creatable: Some(CreatablePart {
            cost: costs(&[]),
            build_time: WholeSeconds::ZERO,
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            task_by_group: group,
            tasks,
            work_rate: WorkRate::new(1.0),
            ..ActionPart::default()
        }),
        moving: Some(MovingPart {
            move_sprite: None,
            run_sprite: None,
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    let base = || {
        vec![
            task(ActionKind::COMBAT, None),
            task(ENTER, Some(class_id(3))),
        ]
    };
    let builder = || {
        vec![
            task(ActionKind::BUILD, None),
            task(ENTER, Some(class_id(3))),
        ]
    };
    Masters::new([
        villager(VILLAGER_MALE, TaskGroup::MALE_VILLAGERS, base()),
        villager(ids::BUILDER, TaskGroup::MALE_VILLAGERS, builder()),
        villager(master_id(212), TaskGroup::FEMALE_VILLAGERS, builder()),
        villager(VILLAGER_FEMALE, TaskGroup::FEMALE_VILLAGERS, base()),
        Master {
            id: ids::HOUSE,
            terrain_restriction: Some(RestrictionId::named(0)),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(1.0, 1.0),
            height: Distance::new(1.0),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            obstruction_class: ObstructionClass::UNIT,
            object_flags: ObjectFlags::NONE,
            combat_level: CombatLevel::NONE,
            hit_points: MasterHitPoints::of_data(550),
            // a house's clearance, which BUILD's placement test reads
            clearance: Footprint::new(1.0, 1.0),
            // a house's building part: no annexes, no stack unit, not an annex; its destruction
            // sprite (392 in build 158041's .dat), which it dies on unbuilt
            building: Some(BuildingMaster {
                stack_unit: None,
                foundation_terrain: None,
                tech: None,
                annexes: [None; 4],
                head_unit: None,
                transform_unit: None,
                destruction_sprite: Some(SpriteId::named(392)),
                ..BuildingMaster::default()
            }),
            // its dying sprite (2196), which it dies on built
            death_sprite: Some(SpriteId::named(2196)),
            // built by the villagers (the builders' test, FUN_141262270)
            creatable: Some(CreatablePart {
                cost: costs(&[(attribute::WOOD, 25, true)]),
                build_time: WholeSeconds::of_data(25),
                trained_at: vec![ids::BUILDER],
                ..CreatablePart::default()
            }),
            action: Some(ActionPart {
                task_by_group: TaskGroup::NONE,
                tasks: vec![],
                work_rate: WorkRate::ZERO,
                ..ActionPart::default()
            }),
            ..Master::default()
        },
    ])
}

/// A flat map of terrain 0 that the first terrain restriction lets anything stand on, every
/// tile seen by every player: BUILD's placement test passes anywhere free.
pub(crate) fn placeable(world: &mut World) {
    placeable_on(world, GridSize::new(120, 120));
}

/// [`placeable`]'s terrain and sight on a map of `width` × `height` tiles.
pub(crate) fn placeable_on(world: &mut World, map: GridSize) {
    let (width, height) = (map.width(), map.height());
    let tiles = (width * height) as usize;
    world.spatial.map = GridSize::new(width, height);
    let mut terrain = Terrain::new(
        GridSize::new(width, height),
        vec![TileTerrain::of_save(0, 255, -1); tiles],
        vec![
            TerrainKind {
                shown: true,
                flags: TerrainFlags::NONE,
                forest: false,
            };
            1
        ],
        vec![Restriction::new(vec![1.0])],
    );
    terrain.set_elevations(vec![0; tiles]);
    world.spatial.terrain = Some(terrain);
    let players = world.players.len();
    let seen = (0..players).fold(0u64, |bits, player| {
        bits | (1 << player) | (1 << (32 + player))
    });
    world.spatial.sight = Some(VisionVisibility::new(
        GridSize::new(width, height),
        (0..players)
            .map(|_| VisibleMap {
                counts: vec![0; tiles],
                explored: TileCount::ZERO,
            })
            .collect(),
        vec![VisionBits::from_bits_retain(seen); tiles],
        Vec::new(),
        0,
        vec![PlayerKind::Human; players],
    ));
}

/// `FUN_140c42080` after a notify of a new action: a unit AI with a target the world holds
/// remembers the order with the world's time, both places as slot 0x520 gives them (a
/// [`BVector`]; these hold no movement state) and the target's attack category by its master
/// (an archer, class 0: 3); one whose target is gone, or on a point, remembers none.
///
/// [`BVector`]: crate::math::fmath::BVector
#[test]
fn an_order_on_target_joins_the_unit_ai_history() {
    use crate::math::fmath::BVector;
    let master = |id, class| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        ..Master::default()
    };
    let unit = |id, position| Unit {
        id,
        place: Static::at(position),
        life: Lifecycle {
            master_id: if id == object_id(1) {
                master_id(74)
            } else {
                master_id(4)
            },
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into()),
            player(
                vec![
                    unit(object_id(1), Position::new(10.0, 11.0, 0.0)),
                    unit(object_id(3), Position::new(1.0, 1.0, 0.0)),
                ]
                .into(),
            ),
            player(vec![unit(object_id(2), Position::new(20.0, 21.0, 1.0))].into()),
        ],
    )
    .with_ais(
        [(1, Some(2)), (3, Some(99)), (2, None)].map(|(id, target)| {
            let ai = UnitAi {
                current_target: target.map(object_id),
                ..UnitAi::default()
            };
            (object_id(id), ai)
        }),
    );
    let table = Masters::new([
        master(master_id(74), class::CLASS_6),
        master(master_id(4), class::CLASS_0),
    ]);
    world.model.masters = vec![
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table),
    ];
    world.world_time = WorldTime::from_millis(4321);
    world
        .remember_order(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(OrderKind::Attack), Some(AiAction::Attack)),
        )
        .unwrap();
    world
        .remember_order(
            PlayerId::new(1),
            ListIndex::new(1),
            (Some(OrderKind::Attack), Some(AiAction::Attack)),
        )
        .unwrap();
    let history = |list_index: ListIndex| {
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[list_index].id)
            .unwrap()
            .order_history
            .clone()
    };
    assert_eq!(
        history(ListIndex::new(0)),
        [OrderRecord {
            order: Some(OrderKind::Attack),
            action: Some(AiAction::Attack),
            time: WorldTime::from_millis(4321),
            position: BVector::new(10.0, 0.0, 11.0),
            target: Some(object_id(2)),
            category: attack_category(3),
            target_position: BVector::new(20.0, 1.0, 21.0),
        }]
    );
    assert_eq!(history(ListIndex::new(1)), []);
}

/// FLARE (`0x140e7bfd0`) at a point without a target: each player's flare comes at the point
/// with height 0, which the static `new_position` (slot 0x58, slot 0x60 `FUN_141193a10`) puts
/// on the ground, as 426048706's flare of update 2486 stands at 4 on raised ground.
#[test]
fn flare_at_point_stands_on_the_ground() {
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![player(), player(), player()],
    );
    placeable(&mut world);
    if let Some(terrain) = world.spatial.terrain.as_mut() {
        terrain.set_elevations(vec![4; 120 * 120]);
    }
    let flare = Master {
        id: master_id(274),
        kind: MasterKind::ANIMATED,
        standing_sprite: None,
        ..Master::default()
    };
    world.model.masters = (0..3)
        .map(|_| Masters::new([flare.clone()]))
        .map(Arc::new)
        .collect();
    world.next_object_id = object_id(100);
    world
        .flare(None, Point::new(19.114_586, 29.197_92), 3)
        .unwrap();
    for index in 1..3 {
        let at: Vec<Position> = world.players[index]
            .objects
            .iter()
            .map(Unit::position)
            .collect();
        assert_eq!(at, [Position::new(19.114_586, 29.197_92, 4.0)]);
    }
}

/// A hit on player 2's object makes player 2 a flare (master 112) at the attacker's place
/// (`FUN_14120c0a0`'s slot 0x1c0), whose amount (`+0x310`, its time) a trebuchet's hit doubles.
#[test]
fn hit_flare_lasts_twice_as_long_from_trebuchet() {
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![player(), player(), player()],
    );
    placeable(&mut world);
    let flare = Master {
        id: master_id(112),
        kind: MasterKind::ANIMATED,
        standing_sprite: None,
        storage: vec![(Some(AttributeId::named(12)), Amount::new(1.0), 0)],
        ..Master::default()
    };
    world.model.masters = (0..3)
        .map(|_| Masters::new([flare.clone()]))
        .map(Arc::new)
        .collect();
    world.next_object_id = object_id(100);
    world
        .make_flare(PlayerId::new(2), Position::new(10.5, 12.5, 0.0), false)
        .unwrap();
    world
        .make_flare(PlayerId::new(2), Position::new(10.5, 12.5, 0.0), true)
        .unwrap();
    let flares: Vec<(i16, f32)> = world.players[2]
        .objects
        .iter()
        .map(|unit| (unit.life.master_id.get(), unit.life.amount.get()))
        .collect();
    assert_eq!(flares, [(112, 1.0), (112, 2.0)]);
}

/// Player 1 with 300 attributes, `livestock` of them at a pasture 50 (master 1897, radius 2) at
/// `at` and 300 food, on a 120-tile map, its masters `livestock_masters` of the two livestock
/// (moving, one hit point, a turning idle sprite of 16 facets with a random first one, 100 food
/// held).
fn pasture_game(at: Position, livestock: f32, livestock_masters: &[MasterId]) -> World {
    use crate::objects::masters::MasterKind;
    use crate::objects::sprite::Graphic;
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![player(), player()],
    );
    world.spatial = Spatial::new(GridSize::new(120, 120));
    placeable(&mut world);
    world.players[1].attributes[289] = Amount::new(livestock);
    world.players[1].attributes[288] = Amount::new(300.0);
    let idle = |id| Graphic {
        id,
        frames: Frame::of_data(60),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ZERO,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: (SpriteFlags::ANIMATED | SpriteFlags::TURNS | SpriteFlags::RANDOM_FIRST_FRAME),
    };
    world.model.graphics = Arc::new(Graphics::from([
        (SpriteId::named(13124), idle(SpriteId::named(13124))),
        (SpriteId::named(13170), idle(SpriteId::named(13170))),
    ]));
    let pasture = Master {
        id: ids::PASTURE,
        kind: MasterKind::BUILDING,
        class: class::FARM,
        radius: Footprint::new(2.0, 2.0),
        ..Master::default()
    };
    let animal = |id: MasterId, standing_sprite| Master {
        id,
        kind: MasterKind::MOVING,
        class: class::FARM,
        hit_points: MasterHitPoints::of_data(1),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite,
        storage: vec![(Some(attribute::FOOD), Amount::new(100.0), 0)],
        ..moving_master(walker().stats)
    };
    let mut masters = vec![pasture];
    for &master_id in livestock_masters {
        masters.push(animal(
            master_id,
            if master_id.get() == 1899 {
                Some(SpriteId::named(13124))
            } else {
                Some(SpriteId::named(13170))
            },
        ));
    }
    world.model.masters = (0..2)
        .map(|_| Masters::new(masters.clone()))
        .map(Arc::new)
        .collect();
    let _ = world
        .push_live(
            PlayerId::new(1),
            Unit {
                id: object_id(50),
                place: Static::at(at),
                life: Lifecycle {
                    master_id: ids::PASTURE,
                    ..Lifecycle::default()
                },
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
        )
        .unwrap();
    world.next_object_id = object_id(100);
    world
}

/// `FUN_1411c8410` (429381190's pasture 15342 of update 1385 made three): per animal two draws of
/// the Object stream pick its master and its point along the edges, a point on a tile that holds
/// one of the pasture's livestock goes and another is drawn; the new object's setups draw its
/// facet and its angle, then the routine its first facet. Each is the pasture's, holds its share
/// of the food, no unit targets it, and the pasture lists it.
#[test]
fn completed_pasture_puts_its_livestock_on_its_edges() {
    use crate::gathering::pasture::{edge_points, livestock};
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        3.0,
        &[master_id(1899), master_id(1900)],
    );
    world.streams.get_mut(Stream::Object).set_seed(1385);
    let mut rng = world.streams.get_mut(Stream::Object).clone();
    let mut points = edge_points(
        Position::new(20.0, 20.0, 0.0),
        Distance::new(2.0),
        &tuning().pasture,
    );
    let mut tiles = Vec::new();
    let mut expected = Vec::new();
    for _ in 0..3 {
        let kind = livestock(slots().master)[rng.rand15() as usize % 2];
        let mut pick = rng.rand15() as usize % points.len();
        let tile = |point: Point| (point.x() as i32, point.y() as i32);
        while tiles.contains(&tile(points[pick])) {
            let _ = points.remove(pick);
            pick = rng.rand15() as usize % points.len();
        }
        tiles.push(tile(points[pick]));
        expected.push((kind, points.remove(pick)));
        // the static setup's facet, the moving setup's angle, the routine's facet
        for _ in 0..3 {
            let _ = rng.rand15();
        }
    }
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    let made: Vec<(MasterId, Point)> = world.players[1].objects.as_slice()[1..]
        .iter()
        .map(|unit| (unit.life.master_id, unit.position().ground()))
        .collect();
    assert_eq!(made, expected);
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .livestock,
        [object_id(100), object_id(101), object_id(102)]
    );
    for unit in &world.players[1].objects.as_slice()[1..] {
        assert_eq!(
            (
                unit.pasture,
                unit.life.amount,
                unit.life.attribute_kind,
                unit.flags.contains(UnitFlags::UNTARGETABLE)
            ),
            (
                Some(PastureId::new(object_id(50))),
                Amount::new(100.0),
                Some(attribute::FOOD),
                true
            )
        );
        assert_eq!(unit.life.state, ObjectState::Alive);
        assert!(unit.sprite.is_some() && world.parts.mover_of(unit.id).is_some());
    }
    assert_eq!(*world.streams.get_mut(Stream::Object), rng);
}

/// A livestock sprite of one facet that draws its first facet at random: the game's `IDIV` by
/// 0 stops it, and genie refuses the data (`invalid_input`).
#[test]
fn pasture_livestock_sprite_of_one_facet_is_refused() {
    use crate::ErrorKind;
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        1.0,
        &[master_id(1899), master_id(1900)],
    );
    for id in [SpriteId::named(13124), SpriteId::named(13170)] {
        let mut graphic = *world.model.graphics.get(id).unwrap();
        graphic.facets = FacetCount::of_data(1);
        let _ = Arc::make_mut(&mut world.model.graphics).insert(id, graphic);
    }
    assert_eq!(
        world
            .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
            .map_err(|error| error.kind()),
        Err(ErrorKind::InvalidInput)
    );
}

/// One livestock a tile: a pasture at (20, 20) has twelve tiles under its sixteen points, so
/// sixteen asked give twelve and the points run out; at the map's left edge the points off it
/// (x below 0 after CVTTSS2SI: −1.5, not −0.6) go too, leaving six tiles.
#[test]
fn pasture_puts_one_animal_tile_on_the_map() {
    let mut game = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        16.0,
        &[master_id(1899), master_id(1900)],
    );
    game.setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    let tiles = |world: &World| {
        let mut players: Vec<(i32, i32)> = world.players[1].objects.as_slice()[1..]
            .iter()
            .map(|unit| (unit.position().x() as i32, unit.position().y() as i32))
            .collect();
        players.sort_unstable();
        players
    };
    let mut twelve = tiles(&game);
    assert_eq!(twelve.len(), 12);
    twelve.dedup();
    assert_eq!(twelve.len(), 12);
    let mut world = pasture_game(
        Position::new(0.0, 10.0, 0.0),
        16.0,
        &[master_id(1899), master_id(1900)],
    );
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    assert_eq!(
        tiles(&world),
        [(0, 8), (0, 11), (1, 8), (1, 9), (1, 10), (1, 11)]
    );
}

/// A master the owner has not got makes nothing but counts as an attempt, its point staying; no
/// livestock asked draws nothing; the food is the pasture's over the count (DIVSS).
#[test]
fn pasture_counts_each_attempt_and_shares_its_food() {
    use crate::gathering::pasture::{edge_points, livestock};
    let mut world = pasture_game(Position::new(20.0, 20.0, 0.0), 3.0, &[master_id(1899)]);
    world.streams.get_mut(Stream::Object).set_seed(7);
    let mut rng = world.streams.get_mut(Stream::Object).clone();
    let mut points = edge_points(
        Position::new(20.0, 20.0, 0.0),
        Distance::new(2.0),
        &tuning().pasture,
    );
    let mut expected = Vec::new();
    for _ in 0..3 {
        let kind = livestock(slots().master)[rng.rand15() as usize % 2];
        let pick = rng.rand15() as usize % points.len();
        let tile = |point: Point| (point.x() as i32, point.y() as i32);
        assert!(
            !expected
                .iter()
                .any(|&point| tile(point) == tile(points[pick])),
            "the seed's points"
        );
        if kind == master_id(1899) {
            expected.push(points.remove(pick));
            for _ in 0..3 {
                let _ = rng.rand15();
            }
        }
    }
    assert!(expected.len() < 3, "the seed draws an Ibex");
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    let made: Vec<Point> = world.players[1].objects.as_slice()[1..]
        .iter()
        .map(|unit| unit.position().ground())
        .collect();
    assert_eq!(made, expected);
    assert_eq!(*world.streams.get_mut(Stream::Object), rng);
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        0.0,
        &[master_id(1899), master_id(1900)],
    );
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    assert_eq!(
        (
            world.players[1].objects.len(),
            world.streams.get_mut(Stream::Object).draws
        ),
        (1, 0)
    );
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        7.0,
        &[master_id(1899), master_id(1900)],
    );
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    let food = 300.0f32 / 7.0;
    assert!(
        world.players[1].objects.as_slice()[1..]
            .iter()
            .all(|unit| unit.life.amount == Amount::new(food))
    );
}

/// A moving object of type 30 that dies (429381190's Ibex 15371 in update 1426) shows its
/// master's dying sprite (slot 0x7a0, `FUN_1411887e0`, through slot 0x68) and goes on to state 4.
/// Its top speed (slot 65, `FUN_14114d1d0`) follows the sprite: the master's speed, times the
/// sprite's speed factor above 0.
#[test]
fn dying_livestock_shows_its_dying_sprite() {
    for (factor, speed) in [(0.0, 0.737), (2.0, 1.474)] {
        let object = dying_animal(MasterKind::MOVING, factor);
        let unit = &object.unit;
        assert_eq!(
            unit.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(13171))
        );
        assert_eq!(unit.life.state, ObjectState::DyingSprite);
        assert_eq!(
            object.parts.mover().map(|mover| mover.stats.speed),
            Some(Speed::new(speed))
        );
    }
}

/// A livestock of [`pasture_game`] of master type `kind` (speed 0.737, the dying sprite 13171 of
/// speed factor `factor`) running at 1.4003 when it dies, two updates later (the deer 9655 of
/// MarvinBlue/426048731, a combat object, likewise in update 3147: the Gaia pin of
/// `oracle_snapshots`).
fn dying_animal(kind: MasterKind, factor: f32) -> Object {
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        1.0,
        &[master_id(1899), master_id(1900)],
    );
    let dying = Graphic {
        id: SpriteId::named(13171),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::of_data(factor),
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: (SpriteFlags::ANIMATED | SpriteFlags::TURNS | SpriteFlags::RANDOM_FIRST_FRAME),
    };
    let _ = Arc::make_mut(&mut world.model.graphics).insert(SpriteId::named(13171), dying);
    for masters in &mut world.model.masters {
        let mut all: Vec<Master> = masters.iter().cloned().collect();
        for master in all
            .iter_mut()
            .filter(|master| master.kind == MasterKind::MOVING)
        {
            master.kind = kind;
            master.death_sprite = Some(SpriteId::named(13171));
            master
                .animated
                .get_or_insert_with(Default::default)
                .max_speed = Speed::new(0.737);
        }
        *masters = Arc::new(Masters::new(all));
    }
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    let animal = world.players[1].objects[ListIndex::new(1)].id;
    world.players[1].objects[ListIndex::new(1)].life.hp = HitPoints::ZERO;
    if let Some(mover) = world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(1)].id)
    {
        mover.stats.base = Speed::new(0.737);
        mover.stats.speed = Speed::new(1.4003);
    }
    for _ in 0..2 {
        world.tick(Millis::new(50));
    }
    world.object_of(world.object(animal).unwrap().1)
}

/// [`pasture_game`] with its three livestock made and player 1's villager 60 (class 4, radius
/// 0.2) at `at`, the owner's [`crate::gathering::pasture::PASTURE_WORKERS`] `room`.
fn tended_pasture(at: Position, room: f32) -> World {
    let mut world = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        3.0,
        &[master_id(1899), master_id(1900)],
    );
    world
        .setup_pasture(PlayerId::new(1), PastureId::new(object_id(50)))
        .unwrap();
    world.players[1].attributes[290] = Amount::new(room);
    let villager = Master {
        id: VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    for masters in &mut world.model.masters {
        let mut all: Vec<Master> = masters.iter().cloned().collect();
        all.push(villager.clone());
        *masters = Arc::new(Masters::new(all));
    }
    let villager = Unit {
        id: object_id(60),
        place: Static::at(at),
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let _ = world
        .push_live(
            PlayerId::new(1),
            Object::with_ai(villager, UnitAi::default()),
        )
        .unwrap();
    world
}

/// `FUN_1411c7670`: the pasture's livestock sorted by their footprints' gap to the villager;
/// the first without workers, else the first whose workers are below the owner's attribute
/// 290, else none; a unit of another player gets none. Slot 0x780 sums the livestock's workers.
#[test]
fn villager_tends_the_nearest_livestock_with_room() {
    let livestock = |world: &World| {
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .livestock
            .clone()
    };
    let place = |world: &World, id: ObjectId| world.object(id).unwrap().1.position();
    let game = tended_pasture(Position::new(20.0, 20.0, 0.0), 2.0);
    let ids = livestock(&game);
    let near = place(&game, ids[0]);
    let mut game = tended_pasture(near.with_z(0.0), 2.0);
    let villager = game.players[1].objects.last_place().unwrap();
    assert_eq!(
        game.pasture_pick(PastureId::new(object_id(50)), PlayerId::new(1), villager)
            .map(|picked| picked.map(|(id, _)| id)),
        Ok(Some(ids[0]))
    );
    let mut gaps: Vec<(f32, ObjectId)> = ids
        .iter()
        .map(|&id| {
            let position = place(&game, id);
            let distance_squared = footprint_gap_squared(
                position,
                Footprint::new(0.2, 0.2),
                near,
                Footprint::new(0.2, 0.2),
            );
            (distance_squared.get(), id)
        })
        .collect();
    gaps.sort_by(|left, right| left.0.total_cmp(&right.0));
    let by_gap: Vec<ObjectId> = gaps.iter().map(|gap| gap.1).collect();
    let worked = |world: &mut World, id: ObjectId, workers: i32| {
        world.object_mut(id).unwrap().1.workers = WorkerCount::of_data(workers);
    };
    worked(&mut game, by_gap[0], 1);
    assert_eq!(
        game.pasture_pick(PastureId::new(object_id(50)), PlayerId::new(1), villager)
            .map(|picked| picked.map(|(id, _)| id)),
        Ok(Some(by_gap[1]))
    );
    worked(&mut game, by_gap[1], 1);
    worked(&mut game, by_gap[2], 1);
    assert_eq!(
        game.objects()
            .livestock_workers(PastureId::new(object_id(50))),
        WorkerCount::of_data(3)
    );
    // all worked: the nearest below the room
    assert_eq!(
        game.pasture_pick(PastureId::new(object_id(50)), PlayerId::new(1), villager)
            .map(|picked| picked.map(|(id, _)| id)),
        Ok(Some(by_gap[0]))
    );
    worked(&mut game, by_gap[0], 2);
    assert_eq!(
        game.pasture_pick(PastureId::new(object_id(50)), PlayerId::new(1), villager)
            .map(|picked| picked.map(|(id, _)| id)),
        Ok(Some(by_gap[1]))
    );
    for &id in &by_gap {
        worked(&mut game, id, 2);
    }
    assert_eq!(
        game.pasture_pick(PastureId::new(object_id(50)), PlayerId::new(1), villager)
            .map(|picked| picked.map(|(id, _)| id)),
        Ok(None)
    );
    let mut other = tended_pasture(near, 2.0);
    let unit = other.players[1].objects.pop().unwrap();
    other.players.push(Player {
        attributes: vec![Amount::ZERO; 300],
        objects: vec![unit].into(),
        ..Player::default()
    });
    other.reindex();
    other.model.masters.push(other.model.masters[1].clone());
    assert_eq!(
        other.pasture_pick(
            PastureId::new(object_id(50)),
            PlayerId::new(2),
            ListIndex::new(0)
        ),
        Ok(None)
    );
}

/// Slot 0x7e8 of a villager on a pasture: with room (the livestock's workers below attribute
/// 290) it works the livestock `FUN_1411c7670` picks, at its place, and tends the pasture; a
/// full one has its unit AI take a search (0x202 with the hunt action, class 49, −1) and it
/// tends none.
#[test]
fn villager_ordered_on_pasture_works_its_livestock() {
    let order = |target| Work {
        target,
        point: Position::new(20.0, 20.0, 0.0),
        disable_retargeting: true,
        filter: None,
    };
    let mut world = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let players = world.players[1].objects.last_place().unwrap();
    world
        .order_work(PlayerId::new(1), players, &order(Some(object_id(50))))
        .unwrap();
    assert_eq!(
        world.players[1].objects[players].my_pasture,
        Some(PastureId::new(object_id(50)))
    );
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .pasture_workers,
        [object_id(60)]
    );
    let mut full = tended_pasture(Position::new(18.0, 18.0, 0.0), 0.0);
    let list_index = full.players[1].objects.last_place().unwrap();
    full.players[1].objects[list_index].my_pasture = Some(PastureId::new(object_id(50)));
    full.parts
        .building_mut(full.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .pasture_workers = vec![object_id(60)];
    full.order_work(PlayerId::new(1), list_index, &order(Some(object_id(50))))
        .unwrap();
    let unit = &full.players[1].objects[list_index];
    assert_eq!(
        full.parts.unit_ai.get(unit.id).unwrap().notifies,
        [Notify::own(
            object_id(60),
            NotifyKind::REQUIRES_SEARCH,
            [0x265, 49, -1]
        )]
    );
    assert_eq!(unit.my_pasture, None);
    assert_eq!(
        full.parts
            .building_of(full.players[1].objects[ListIndex::new(0)].id)
            .pasture_workers,
        Vec::new()
    );
}

/// Slot 0x7e8 of a unit in state 3 or later ignores the order: nothing of the pasture's or the
/// unit's changes.
#[test]
fn unit_past_state_2_ignores_work_order() {
    let order = Work {
        target: Some(object_id(50)),
        point: Position::new(20.0, 20.0, 0.0),
        disable_retargeting: true,
        filter: None,
    };
    let mut world = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let list_index = world.players[1].objects.last_place().unwrap();
    world.players[1].objects[list_index].life.state = ObjectState::Dying;
    let before = world.players[1].objects.clone();
    world
        .order_work(PlayerId::new(1), list_index, &order)
        .unwrap();
    assert_eq!(world.players[1].objects, before);
}

/// Slot 0x7e8 of a villager on a pasture with room but no livestock: `FUN_1411c7670` gives
/// none and the order ends there, before `FUN_141189ca0`: the pasture the villager tended stays
/// its own.
#[test]
fn villager_ordered_on_pasture_without_livestock_tends_on() {
    let order = Work {
        target: Some(object_id(50)),
        point: Position::new(20.0, 20.0, 0.0),
        disable_retargeting: true,
        filter: None,
    };
    let mut world = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let list_index = world.players[1].objects.last_place().unwrap();
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .livestock
        .clear();
    world.players[1].objects[list_index].my_pasture = Some(PastureId::new(object_id(50)));
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .pasture_workers = vec![object_id(60)];
    world
        .order_work(PlayerId::new(1), list_index, &order)
        .unwrap();
    assert_eq!(
        world.players[1].objects[list_index].my_pasture,
        Some(PastureId::new(object_id(50)))
    );
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .pasture_workers,
        [object_id(60)]
    );
}

/// `FUN_141189ca0`: a unit tending a pasture joins its workers once; tending none drops it
/// from the pasture it tended (`FUN_1411cafa0`); an object that is no building keeps no list.
#[test]
fn unit_joins_and_leaves_pasture_workers() {
    let mut world = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let list_index = world.players[1].objects.last_place().unwrap();
    world
        .tend_pasture(
            PlayerId::new(1),
            list_index,
            Some(PastureId::new(object_id(50))),
        )
        .unwrap();
    world
        .tend_pasture(
            PlayerId::new(1),
            list_index,
            Some(PastureId::new(object_id(50))),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .pasture_workers,
        [object_id(60)]
    );
    assert_eq!(
        world.players[1].objects[list_index].my_pasture,
        Some(PastureId::new(object_id(50)))
    );
    world
        .tend_pasture(PlayerId::new(1), list_index, None)
        .unwrap();
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .pasture_workers,
        Vec::new()
    );
    assert_eq!(world.players[1].objects[list_index].my_pasture, None);
    let animal = world.players[1].objects[ListIndex::new(1)].id;
    world
        .tend_pasture(
            PlayerId::new(1),
            list_index,
            ObjectId::of_field(animal.get()).map(PastureId::new),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(1)].id)
            .pasture_workers,
        Vec::new()
    );
    assert_eq!(
        world.players[1].objects[list_index].my_pasture,
        ObjectId::of_field(animal.get()).map(PastureId::new)
    );
}

/// `Process_New_Tiles`: player 1's newly explored tiles, in order, give the objects of its
/// masters that gather by themselves to its lists by group, at their places.
#[test]
fn player_new_tiles_give_their_resources_to_its_lists() {
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::{VisibleResource, VisibleResources};
    let object = |id, master, x, y| Unit {
        id,
        place: Static::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Static,
            state: ObjectState::Alive,
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                sleeping: vec![
                    object(object_id(501), master_id(411), 10.5, 10.25),
                    object(object_id(502), master_id(412), 10.5, 10.5),
                    object(object_id(503), master_id(413), 11.5, 10.75),
                ]
                .into(),
                ..Player::default()
            },
            Player::default(),
        ],
    );
    let master = |id, auto_gather, gather_group| Master {
        id,
        kind: MasterKind::STATIC,
        auto_gather,
        gather_group,
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([
            master(master_id(411), AutoGatherMode::ON, GatherGroup::WOOD),
            master(master_id(412), AutoGatherMode::OFF, GatherGroup::WOOD),
            master(master_id(413), AutoGatherMode::ON, GatherGroup::GOLD),
        ])),
        Arc::new(Masters::default()),
    ];
    world.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        Tiles::new(GridSize::new(120, 120), vec![0; 120 * 120]),
    );
    placeable(&mut world);
    for unit in &world.players[0].sleeping {
        world.spatial.insert(
            unit.id,
            Placed {
                position: unit.position(),
                radius: Footprint::new(0.5, 0.5),
                class: class::TREE,
                player: PlayerId::new(0),
                master: unit.life.master_id,
                obstruction_kind: ObstructionKind::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    let visibility = world.spatial.sight.as_mut().unwrap();
    visibility.resources[1] = VisibleResources {
        capacities: vec![32; 5],
        lists: vec![Vec::new(); 5],
    };
    visibility.new_tiles[1] = vec![Tile::new(11, 10), Tile::new(10, 10)];
    world.process_new_tiles(PlayerId::new(1)).unwrap();
    let rec = |id, x, y| VisibleResource {
        id,
        at: Point::new(x, y),
        ..VisibleResource::default()
    };
    let visibility = world.spatial.sight.as_ref().unwrap();
    assert_eq!(
        visibility.resources[1].lists[0],
        [rec(object_id(501), 10.5, 10.25)]
    );
    assert_eq!(
        visibility.resources[1].lists[4],
        [rec(object_id(503), 11.5, 10.75)]
    );
    assert_eq!(visibility.resources[0], VisibleResources::default());
}

/// `FUN_1411980b0`: a static resource that runs out (state 3 to 7) leaves the players'
/// visible-resource lists at once; an animated one (state 5 to 7, not below 4) only when it
/// is deleted (state 8 leaving its list, `FUN_14116b5e0`).
#[test]
fn resource_leaves_the_lists_when_it_runs_out_or_is_deleted() {
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::VisibleResources;
    let dead = |id, class, state, master| Unit {
        id,
        place: Static::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            class,
            state,
            master_id: master,
            attribute_kind: None,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![
                    dead(
                        object_id(501),
                        Class::Static,
                        ObjectState::Dying,
                        master_id(411),
                    ),
                    dead(
                        object_id(502),
                        Class::Animated,
                        ObjectState::Dead,
                        master_id(412),
                    ),
                ]
                .into(),
                ..Player::default()
            },
            Player::default(),
        ],
    );
    let master = |id, gather_group| Master {
        id,
        kind: MasterKind::STATIC,
        auto_gather: AutoGatherMode::ON,
        gather_group,
        life: Some(MasterData::default()),
        ..Master::default()
    };
    game.model.masters = vec![
        Arc::new(Masters::new([
            master(master_id(411), GatherGroup::WOOD),
            master(master_id(412), GatherGroup::FORAGE),
        ])),
        Arc::new(Masters::default()),
    ];
    placeable(&mut game);
    let sight = game.spatial.sight.as_mut().unwrap();
    for index in 0..2 {
        sight.resources[index] = VisibleResources {
            capacities: vec![32; 5],
            lists: vec![Vec::new(); 5],
        };
        sight.resources[index].add(object_id(501), GatherGroup::WOOD, Point::new(10.5, 10.5));
        sight.resources[index].add(object_id(502), GatherGroup::FORAGE, Point::new(10.5, 10.5));
    }
    let held = |world: &World| -> Vec<(usize, usize)> {
        let visibility = world.spatial.sight.as_ref().unwrap();
        (0..2)
            .map(|player| {
                (
                    visibility.resources[player].lists[0].len(),
                    visibility.resources[player].lists[1].len(),
                )
            })
            .collect()
    };
    game.tick(Millis::new(10));
    assert_eq!(
        game.players[0].objects[ListIndex::new(0)].life.state,
        ObjectState::Removed
    );
    assert_eq!(held(&game), [(0, 1), (0, 1)]);
    game.tick(Millis::new(10));
    assert_eq!(held(&game), [(0, 1), (0, 1)]);
    game.tick(Millis::new(10));
    assert_eq!(game.players[0].objects, []);
    assert_eq!(held(&game), [(0, 0), (0, 0)]);
}

/// `FUN_14162ce00`: a new object of a master that gathers by itself joins the lists of the
/// players that explored its tile; one of another master joins none.
#[test]
fn new_resource_joins_the_lists_of_the_players_that_explored_its_tile() {
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::{VisibleResource, VisibleResources};
    let made = |id, master| Unit {
        id,
        place: Static::at(Position::new(20.75, 30.25, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![Player::default(), Player::default(), Player::default()],
    );
    let master = |id, auto_gather| Master {
        id,
        kind: MasterKind::MOVING,
        auto_gather,
        gather_group: GatherGroup::FISH,
        ..Master::default()
    };
    world.model.masters = vec![
        Masters::new([
            master(master_id(53), AutoGatherMode::ON),
            master(master_id(54), AutoGatherMode::OFF)
        ]);
        3
    ]
    .into_iter()
    .map(Arc::new)
    .collect();
    placeable(&mut world);
    let visibility = world.spatial.sight.as_mut().unwrap();
    // players 0 and 2 explored the tile, player 1 did not
    visibility.unified[30 * 120 + 20] = VisionBits::from_bits_retain(1 << 32 | 1 << 34);
    for visible_resources in &mut visibility.resources {
        *visible_resources = VisibleResources {
            capacities: vec![32; 5],
            lists: vec![Vec::new(); 5],
        };
    }
    let _ = world
        .push_live(PlayerId::new(0), made(object_id(700), master_id(53)))
        .unwrap();
    world.made_new_object(PlayerId::new(0)).unwrap();
    let _ = world
        .push_live(PlayerId::new(0), made(object_id(701), master_id(54)))
        .unwrap();
    world.made_new_object(PlayerId::new(0)).unwrap();
    let fish = VisibleResource {
        id: object_id(700),
        at: Point::new(20.75, 30.25),
        ..VisibleResource::default()
    };
    let visibility = world.spatial.sight.as_ref().unwrap();
    let lists: Vec<_> = visibility
        .resources
        .iter()
        .map(|visible_resources| visible_resources.lists[2].clone())
        .collect();
    assert_eq!(lists, [vec![fish], vec![], vec![fish]]);
}

/// `FUN_14162ce00` after a master's slot 0x40: an object [`World::make_new_object`] makes of a
/// master that gathers by itself joins the lists of the players that explored its tile, a plain
/// object and a building alike; one of a master the player has not got is none.
#[test]
fn made_object_that_gathers_by_itself_joins_the_lists_of_the_explorers() {
    use crate::construction::build::world_side::MakeMode;
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::VisibleResources;
    let made = |kind| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    attributes: vec![Amount::ZERO; 300],
                    ..Player::default()
                };
                3
            ],
        );
        let master = Master {
            id: master_id(53),
            kind,
            auto_gather: AutoGatherMode::ON,
            gather_group: GatherGroup::FISH,
            building: (kind == MasterKind::BUILDING).then(BuildingMaster::default),
            ..Master::default()
        };
        world.model.masters = vec![Masters::new([master]); 3]
            .into_iter()
            .map(Arc::new)
            .collect();
        placeable(&mut world);
        let sight = world.spatial.sight.as_mut().unwrap();
        // players 0 and 2 explored the tile, player 1 did not
        sight.unified[30 * 120 + 20] = VisionBits::from_bits_retain(1 << 32 | 1 << 34);
        for resources in &mut sight.resources {
            *resources = VisibleResources {
                capacities: vec![32; 5],
                lists: vec![Vec::new(); 5],
            };
        }
        let id = world.make_new_object(
            PlayerId::new(0),
            master_id(53),
            Position::new(20.75, 30.25, 0.0),
            MakeMode::Plain,
        );
        (world, id)
    };
    for kind in [MasterKind::STATIC, MasterKind::BUILDING] {
        let (world, id) = made(kind);
        let id = id.unwrap().unwrap();
        let listed: Vec<bool> = world
            .spatial
            .sight
            .as_ref()
            .unwrap()
            .resources
            .iter()
            .map(|resources| resources.lists[2].iter().any(|resource| resource.id == id))
            .collect();
        assert_eq!(listed, [true, false, true], "{kind:?}");
    }
    let (mut world, _) = made(MasterKind::STATIC);
    assert_eq!(
        world
            .make_new_object(
                PlayerId::new(0),
                master_id(99),
                Position::new(20.75, 30.25, 0.0),
                MakeMode::Plain,
            )
            .unwrap(),
        None
    );
}

/// `FUN_141226060`: the class counters by class and master.
#[test]
fn combat_object_counts_in_its_player_class_attributes() {
    use crate::objects::masters::Master;
    let counted = |class, id, task_by_group| {
        let mut amounts = vec![Amount::ZERO; 240];
        let master = Master {
            id,
            class,
            action: Some(ActionPart {
                task_by_group,
                ..ActionPart::default()
            }),
            ..Master::default()
        };
        count_class(&mut amounts, &master, 1.0, &roles());
        count_class(&mut amounts, &master, 1.0, &roles());
        [amounts[37], amounts[38], amounts[40], amounts[235]].map(Amount::get)
    };
    assert_eq!(
        counted(class::CLASS_6, master_id(4), TaskGroup::NONE),
        [0.0, 0.0, 2.0, 0.0]
    );
    assert_eq!(
        counted(class::SIEGE, ids::ID_1663, TaskGroup::NONE),
        [0.0, 0.0, 2.0, 2.0]
    );
    assert_eq!(
        counted(class::CLASS_57, ids::ID_1697, TaskGroup::NONE),
        [0.0, 0.0, 2.0, 2.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, TaskGroup::MALE_VILLAGERS),
        [2.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, TaskGroup::FEMALE_VILLAGERS),
        [2.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, TaskGroup::FISHING_SHIPS),
        [2.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::TRADE_CART, master_id(128), TaskGroup::NONE),
        [2.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::LIVESTOCK, ids::ID_1663, TaskGroup::MALE_VILLAGERS),
        [0.0; 4]
    );
    assert_eq!(
        counted(class::BUILDING, ids::TOWN_CENTRE, TaskGroup::NONE),
        [0.0; 4]
    );
    let mut amounts = vec![Amount::new(5.0); 41];
    count_class(
        &mut amounts,
        &Master {
            class: class::CLASS_6,
            ..Master::default()
        },
        -1.0,
        &roles(),
    );
    assert_eq!(amounts[40], Amount::new(4.0));
}

/// `0x141071880`'s idle swap: a standing sprite that looped draws on the Sprite stream and,
/// above 0x3fff, swaps to the other standing sprite at its first frame; a sprite that did not
/// loop, another sprite or a master without a second standing sprite draws nothing.
#[test]
fn an_idle_object_swaps_its_standing_sprite_on_high_draw() {
    use crate::objects::sprite::{ActiveSprite, Graphic};
    let graphic_of = |id| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics: Graphics = [
        (SpriteId::named(100), graphic_of(SpriteId::named(100))),
        (SpriteId::named(101), graphic_of(SpriteId::named(101))),
    ]
    .into_iter()
    .collect();
    let master = Master {
        standing_sprite: Some(SpriteId::named(100)),
        standing_sprite_2: Some(SpriteId::named(101)),
        ..Master::default()
    };
    let shown = |id, looped| ActiveSprite {
        frame: Frame::of_data(4),
        looped,
        ..ActiveSprite::at_rest(graphic_of(id), Frame::of_data(4))
    };
    // a seed whose first draw is above 0x3fff, another whose first is not
    let rng = |seed| {
        let mut rng = Well512::default();
        rng.set_seed(seed);
        rng
    };
    let high = (0..100u32)
        .find(|&seed| 0x3fff < rng(seed).rand15())
        .expect("a high draw");
    let low = (0..100u32)
        .find(|&seed| rng(seed).rand15() <= 0x3fff)
        .expect("a low draw");
    let mut shown_unit = Unit {
        sprite: Some(shown(SpriteId::named(100), true)),
        ..Unit::default()
    };
    let mut generator = rng(high);
    shown_unit.idle_swap(
        MotionMut::new(None),
        master.standing_sprite,
        master.standing_sprite_2,
        &graphics,
        &mut generator,
        tuning().sprite,
    );
    assert_eq!(shown_unit.idle_variant, 1);
    let active_sprite = shown_unit.sprite.unwrap();
    assert_eq!(
        (active_sprite.graphic.id, active_sprite.frame),
        (SpriteId::named(101), Frame::of_data(0))
    );
    assert_eq!(generator.draws, 1 + rng(high).draws);
    // a low draw keeps the sprite
    let mut idle_unit = Unit {
        sprite: Some(shown(SpriteId::named(100), true)),
        ..Unit::default()
    };
    idle_unit.idle_swap(
        MotionMut::new(None),
        master.standing_sprite,
        master.standing_sprite_2,
        &graphics,
        &mut rng(low),
        tuning().sprite,
    );
    assert_eq!(
        (idle_unit.idle_variant, idle_unit.sprite.unwrap().graphic.id),
        (0, SpriteId::named(100))
    );
    // no loop, another sprite, no second standing sprite: no draw
    for (sprite, master) in [
        (shown(SpriteId::named(100), false), master.clone()),
        (shown(SpriteId::named(7), true), master.clone()),
        (
            shown(SpriteId::named(100), true),
            Master {
                standing_sprite_2: None,
                ..master
            },
        ),
    ] {
        let mut unit = Unit {
            sprite: Some(sprite),
            ..Unit::default()
        };
        let mut high_rng = rng(high);
        unit.idle_swap(
            MotionMut::new(None),
            master.standing_sprite,
            master.standing_sprite_2,
            &graphics,
            &mut high_rng,
            tuning().sprite,
        );
        assert_eq!(high_rng.draws, rng(high).draws);
    }
}

/// A villager's unit AI (type 8) without an order or action.
fn villager_ai() -> UnitAi {
    UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_order: None,
        current_action: None,
        current_target: None,
        ..UnitAi::default()
    }
}

/// Player 1's two idle villagers (9175 at the house's place, 9181 away), its resources and
/// the builders' masters, the house placeable: the BUILD tests' world.
fn build_world() -> World {
    let villager = |id, master, x, y| Unit {
        id,
        place: Static::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    villager(object_id(9175), VILLAGER_MALE, 30.0, 89.0),
                    villager(object_id(9181), VILLAGER_FEMALE, 29.0, 99.0),
                ]
                .into(),
                attributes: attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    )
    .with_ais([
        (object_id(9175), villager_ai()),
        (object_id(9181), villager_ai()),
    ]);
    world.next_object_id = object_id(10028);
    world.model.masters = vec![Arc::new(Masters::default()), Arc::new(builder_masters())];
    placeable(&mut world);
    world
}

/// BUILD's handler (`FUN_140e7a400`) does nothing for a human's packet with a target (`+0x14`);
/// a house of civ 12 keeps the packet's sprite override (`+0x18`, 0x2c9 to 0x2ce), any other
/// none.
#[test]
fn human_build_with_target_places_nothing_and_house_keeps_its_sprite_override() {
    let build = |target, sprite_override| {
        Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            queue_id: target,
            sprite_override,
            builders: unit_list(object_ids([9175, 9181])),
            queued: false,
            commits: false,
            frame: Frame::of_data(0),
        })
    };
    let mut game = build_world();
    let before = game.players[1].objects.len();
    game.run(&build(QueueId::of_data(3), SpriteOverride::NONE));
    assert_eq!(game.players[1].objects.len(), before);
    let overridden = |civ, sprite_override| {
        let mut world = build_world();
        world.players[1].civ = civ;
        world.run(&build(QueueId::of_data(-1), sprite_override));
        let foundation = world.players[1].objects.last().unwrap();
        world
            .parts
            .building_of(foundation.id)
            .construction
            .as_ref()
            .unwrap()
            .sprite_override
    };
    assert!(overridden(CivId::new(12), SpriteOverride::of_data(0x2c9)));
    assert!(!overridden(CivId::new(12), SpriteOverride::of_data(0x2cf)));
    assert!(!overridden(CivId::new(11), SpriteOverride::of_data(0x2c9)));
    assert!(!overridden(CivId::new(12), SpriteOverride::of_data(-1)));
}

#[test]
fn build_pays_the_cost_and_tasks_the_builders_of_its_own_player() {
    let mut world = build_world();
    world.tick_with(
        Millis::new(50),
        &[Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            queue_id: QueueId::of_data(-1),
            sprite_override: SpriteOverride::of_data(-1),
            builders: unit_list(object_ids([9175, 9181])),
            queued: false,
            commits: false,
            frame: Frame::of_data(0),
        })],
    );
    let player = &world.players[1];
    assert_eq!(player.resources(), amounts([200.0, 75.0, 200.0, 100.0]));
    // the house's paid cost, as a building with a train location, in attribute 247
    assert_eq!(player.attributes[247], Amount::new(25.0));
    let foundation = player.objects.last().unwrap();
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.master_id.get(),
            foundation.position()
        ),
        (10028, 70, Position::new(30.0, 88.0, 0.0))
    );
    // an animated object's constructor (FUN_141070cf0) sets its animation speed (+0x7b8) to 0:
    // its sprite's clock runs from its first update (a dying sprite plays out)
    assert_eq!(world.parts.anim_speed_of(foundation.id), Some(Speed::ZERO));
    let builder = |list_index: ListIndex| {
        (
            player.objects[list_index].life.master_id.get(),
            world
                .parts
                .actions_of(player.objects[list_index].id)
                .list
                .build()
                .map(|build_action| build_action.state.bare()),
        )
    };
    // the builder at the foundation works in its first update; the other's MoveTo fails at once
    // (FUN_140aa2100: these villagers lack their movement state, so their speed), and its
    // failed walk's path test needs that state
    assert_eq!(
        (builder(ListIndex::new(0)), builder(ListIndex::new(1))),
        (
            (118, Some(BuildState::Work)),
            (212, Some(BuildState::Move { move_to: None }))
        )
    );
    // each idle villager's town-bell record: no work, at its own place
    assert_eq!(
        world
            .parts
            .town_bell_of(player.objects[ListIndex::new(1)].id)
            .work,
        BackToWork {
            point: Point::new(29.0, 99.0),
            ..BackToWork::NONE
        }
    );
    assert_eq!(
        player.unsupported,
        Some((Tick::new(1), ListIndex::new(1), BUILDER_UNMOVING))
    );
}

/// BUILD's direct path is the builder's slot 0x128 with 1 (`FUN_141211b80`, a combat object's
/// and a building's): its unit AI's order queue empties (`FUN_140c5e010`) before the work goes
/// on. 430020666's villager 6676 drops the move it queued in update 40 when BUILD sends it to
/// another house in update 119.
#[test]
fn direct_build_empties_its_builder_order_queue() {
    let mut world = build_world();
    let queued_move = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(19.760_416, 101.677_086, 0.0),
        range: Distance::ZERO,
        shown: Point::new(19.760_416, 102.177_086),
        ready: true,
        target_kind: None,
    };
    for unit in &world.players[1].objects {
        world
            .parts
            .unit_ai
            .get_mut(unit.id)
            .unwrap()
            .orders
            .push(queued_move);
    }
    // the command phase alone (the units' updates would run the orders)
    world.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        queue_id: QueueId::of_data(-1),
        sprite_override: SpriteOverride::of_data(-1),
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: false,
        frame: Frame::of_data(0),
    }));
    let unit = |id: ObjectId| world.object(id).map(|(_, unit)| unit).unwrap();
    let orders = |id: i32| {
        world
            .parts
            .unit_ai
            .get(unit(object_id(id)).id)
            .map_or(0, |ai| ai.orders.len())
    };
    assert!(
        world
            .parts
            .actions_of(object_id(9175))
            .list
            .build()
            .is_some()
    );
    // the builder's queue empties; the villager BUILD did not name keeps its own
    assert_eq!((orders(9175), orders(9181)), (0, 1));
}

/// census v5's 257 stops "a builder of another player or a sleeping one": BUILD's last
/// loop (`FUN_140e7a400`) takes the packet's units the world holds whatever their player: a
/// queued BUILD puts the build order into another player's villager's queue, issued by its
/// own player and naming the BUILD's player as the target's; a unit of a master that does
/// not move is passed over unless the BUILD is queued. Another player's unit reaches the
/// handler only while players may order each other's (`do_command`'s check, the setting
/// `+0x1074`).
#[test]
fn queued_build_queues_its_order_for_another_player_villager() {
    let villager = |id, master, x, y| Unit {
        id,
        place: Static::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![villager(object_id(9175), VILLAGER_MALE, 30.0, 89.0)].into(),
                attributes: attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
            Player {
                objects: vec![villager(object_id(9618), VILLAGER_FEMALE, 29.0, 99.0)].into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    )
    .with_ais([
        (object_id(9175), villager_ai()),
        (object_id(9618), villager_ai()),
    ]);
    world.next_object_id = object_id(10028);
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(builder_masters()),
        Arc::new(builder_masters()),
    ];
    placeable(&mut world);
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    // the command phase alone (the units' updates would run the orders)
    world.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        queue_id: QueueId::of_data(-1),
        sprite_override: SpriteOverride::of_data(-1),
        builders: unit_list(object_ids([9618, 9175])),
        queued: true,
        commits: false,
        frame: Frame::of_data(2),
    }));
    // the foundation keeps the command's frame (+0xc54)
    assert_eq!(
        world
            .object(object_id(10028))
            .map(|(_, unit)| world.parts.building_of(unit.id).build_frame),
        Some(Frame::of_data(2))
    );
    let order = |player_id: PlayerId| {
        world
            .parts
            .unit_ai
            .get(world.players[player_id.index()].objects[ListIndex::new(0)].id)
            .and_then(|ai| ai.orders.first().copied())
            .map(|queued_order| {
                (
                    queued_order.issuer,
                    queued_order.order,
                    queued_order.target,
                    queued_order.target_owner,
                )
            })
    };
    assert_eq!(
        order(PlayerId::new(2)),
        Some((
            Some(Issuer::Player(player_id(2))),
            OrderKind::Build,
            Some(object_id(10028)),
            Some(player_id(1))
        ))
    );
    assert_eq!(
        order(PlayerId::new(1)),
        Some((
            Some(Issuer::Player(player_id(1))),
            OrderKind::Build,
            Some(object_id(10028)),
            Some(player_id(1))
        ))
    );
}

/// A world of player 1's villager 9175 (master 83, which has no Build task) beside its
/// house's foundation 10028 (master 70, state 0, no build points yet), with the unit AI's
/// queue `orders`.
pub(crate) fn sent_off_world(orders: Vec<QueuedOrder>) -> World {
    let villager = Unit {
        id: object_id(9175),
        place: Static::at(Position::new(29.5, 89.5, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let ai = UnitAi {
        last_target: None,
        orders,
        ..villager_ai()
    };
    let foundation = Object::with_building(
        Unit {
            id: object_id(10028),
            place: Static::at(Position::new(30.0, 88.0, 0.0)),
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Foundation,
                hp: HitPoints::new(1.0),
                master_id: ids::HOUSE,
                ..Lifecycle::default()
            },
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::ZERO,
                desolid: true,
                builders: BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![villager, foundation.unit].into(),
                attributes: vec![Amount::ZERO; 250],
                relations: relation_row([1, 0]),
                ..Player::default()
            },
        ],
    )
    .with_buildings([(object_id(10028), foundation.parts.building)])
    .with_ais([(object_id(9175), ai)]);
    world.model.masters = vec![Arc::new(builder_masters()), Arc::new(builder_masters())];
    world
}

fn task(action_kind: ActionKind) -> Task {
    Task {
        action_kind,
        object_group: None,
        ..Task::default()
    }
}

/// Slot 0x820's search (`FUN_14106c6a0`): the own master's list first; without a task there
/// the player's masters by id of the unit's class, type and swap group, with any filter,
/// the first with a task giving it with the master to switch to; none without a group.
#[test]
fn slot_0x820_search_takes_the_own_task_else_the_first_master_of_the_swap_group() {
    let sent_off_world = sent_off_world(vec![]);
    let masters = &sent_off_world.model.masters[1];
    let find = |id: ObjectId| sent_off_world.object(id);
    let site = task_target_with(
        object_id(10028),
        1,
        &find,
        &sent_off_world.parts,
        sent_off_world.master_tables(),
    )
    .unwrap()
    .unwrap();
    let world = TaskWorld {
        players: &sent_off_world.players,
        drop_sites: &[],
    };
    let search = |own: &Master| {
        let actor = Actor {
            id: object_id(9175),
            master: own,
            player: PlayerId::new(1),
            held: carrying(None, 0.0),
            attribute_95: Amount::ZERO,
            civ: CivId::new(0),
        };
        task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&site),
                general: Some(&site),
                terrain: None,
            },
            None,
            &world,
            slots(),
            &roles(),
        )
        .unwrap()
        .map(|(task, switch)| (task.action_kind, switch.map(|master| master.id.get())))
    };
    // a villager of master 83 builds as the builder 118, the first of its group with a task
    assert_eq!(
        search(masters.get(VILLAGER_MALE).unwrap()),
        Some((ActionKind::BUILD, Some(118)))
    );
    // the builders find it in their own lists
    assert_eq!(
        search(masters.get(ids::BUILDER).unwrap()),
        Some((ActionKind::BUILD, None))
    );
    assert_eq!(
        search(masters.get(master_id(212)).unwrap()),
        Some((ActionKind::BUILD, None))
    );
    // without a swap group there is no other list to try
    let alone = Master {
        action: Some(ActionPart {
            task_by_group: TaskGroup::NONE,
            ..masters
                .get(VILLAGER_MALE)
                .unwrap()
                .action
                .clone()
                .unwrap_or_default()
        }),
        ..masters.get(VILLAGER_MALE).unwrap().clone()
    };
    assert_eq!(search(&alone), None);
}

/// The task finder's target (`World::task_target`'s reading): an annex stands for its main
/// building, once per step; an object not held is none.
#[test]
fn task_target_reads_an_annex_as_its_main_building() {
    let mut world = sent_off_world(vec![]);
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .annex_of = Some(MainBuildingId::new(object_id(10028)));
    let find = |id: ObjectId| world.object(id);
    let at = |steps| {
        task_target_with(
            object_id(9175),
            steps,
            &find,
            &world.parts,
            world.master_tables(),
        )
        .unwrap()
        .map(|x| (x.id.get(), x.master.id.get(), x.state))
    };
    assert_eq!(at(0), Some((9175, 83, ObjectState::Alive)));
    assert_eq!(at(1), Some((10028, 70, ObjectState::Foundation)));
    assert_eq!(at(2), Some((10028, 70, ObjectState::Foundation)));
    assert!(
        task_target_with(object_id(77), 1, &find, &world.parts, world.master_tables(),)
            .unwrap()
            .is_none()
    );
}

/// The unit's slot 0x388 after its unit AI's update (`FUN_141226fc0`): the switch to the
/// master slot 0x820's search gave (slot 0x90), the actions cleared and the wait (type 0x9a,
/// state 10) the only one, its target −1 (the foundation is at its `+0x68`).
#[test]
fn wait_for_foundation_switches_the_villager_and_holds_the_task() {
    let mut world = sent_off_world(vec![]);
    let wait = Wait {
        target: FoundationId::new(object_id(10028)),
        task: task(ActionKind::BUILD),
        switch: Some(ids::BUILDER),
    };
    world
        .wait_for_foundation(PlayerId::new(1), ListIndex::new(0), wait)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, ids::BUILDER);
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .wait_solidify()
            .copied(),
        Some(WaitSolidify::new(
            task(ActionKind::BUILD),
            FoundationId::new(object_id(10028))
        ))
    );
    assert_eq!(
        world.parts.actions_of(unit.id).top_action(),
        Some(TopAction {
            kind: WAIT_SOLIDIFY,
            state: ActionState::new(10)
        })
    );
    // the wait names its foundation apart: its action has no target
    assert_eq!(world.parts.actions_of(unit.id).top_target(), None);
    // without a switch the master stays
    let mut kept = sent_off_world(vec![]);
    kept.wait_for_foundation(
        PlayerId::new(1),
        ListIndex::new(0),
        Wait {
            switch: None,
            ..wait
        },
    )
    .unwrap();
    assert_eq!(
        kept.players[1].objects[ListIndex::new(0)].life.master_id,
        VILLAGER_MALE
    );
}

/// While a player's list is out for its unit's update, the others are read around it, and
/// the task finder reads the owner's diplomacy from the update and another player's from its
/// own record.
#[test]
fn unit_side_reads_the_other_players_relations_around_its_own() {
    let players: Vec<Player> = (0..3)
        .map(|relation| Player {
            relations: relation_row(vec![relation as u8; 3]),
            ..Player::default()
        })
        .collect();
    let foreign = Foreign {
        before: &players[..1],
        after: &players[2..],
        left: None,
    };
    assert_eq!(
        foreign
            .player(PlayerId::new(0))
            .and_then(|player| player.relations.get(PlayerId::GAIA).copied()),
        Some(Relation::Ally)
    );
    assert!(foreign.player(PlayerId::new(1)).is_none());
    assert_eq!(
        foreign
            .player(PlayerId::new(2))
            .and_then(|player| player.relations.get(PlayerId::GAIA).copied()),
        Some(Relation::from(2))
    );
    assert!(foreign.player(PlayerId::new(3)).is_none());
    // the owner (1) counts player 2 as an ally (0), player 2 counts the owner an enemy (2)
    let world = SideWorld {
        player: PlayerId::new(1),
        relations: &ByPlayer::new(vec![Relation::Neutral, Relation::Neutral, Relation::Ally]),
        foreign,
        drop_sites: &[],
    };
    assert!(world.allied(PlayerId::new(1), PlayerId::new(2), AllyTest::AlliesOnly));
    assert!(!world.allied(PlayerId::new(2), PlayerId::new(1), AllyTest::AlliesOnly));
    assert!(world.allied(PlayerId::new(1), PlayerId::new(1), AllyTest::AlliesOnly));
}

/// The wait's foundation as its update reads it: its build points among the owner's other
/// objects or the other players', none when the world does not hold it.
#[test]
fn wait_reads_its_foundation_build_points_wherever_it_is() {
    let world = sent_off_world(vec![]);
    let (none, players) = (&world.players[..0], &world.players[..]);
    let own = Foreign {
        before: none,
        after: none,
        left: None,
    };
    let own = OthersRef::new(
        PlayerId::new(1),
        &world.players[1].objects,
        own,
        index::ObjectIndex::scanning(),
    );
    let sites = &world.parts.building;
    assert_eq!(
        wait_progress(FoundationId::new(object_id(10028)), own, sites),
        Some(Seconds::ZERO)
    );
    assert_eq!(
        wait_progress(FoundationId::new(object_id(77)), own, sites),
        None
    );
    // another player's (player 0 updating, player 1 after it)
    let other = Foreign {
        before: none,
        after: &players[1..],
        left: None,
    };
    assert_eq!(
        wait_progress(
            FoundationId::new(object_id(10028)),
            OthersRef::new(
                PlayerId::new(0),
                &ObjectList::default(),
                other,
                index::ObjectIndex::scanning()
            ),
            sites
        ),
        Some(Seconds::ZERO)
    );
}

/// `FUN_1411cc430` on an annex whose main building is in state 0 with a hit point (the
/// builder's work deferred to the caller, [`crate::construction::build::Worked::Main`]): the main
/// building's annexes start first (the annex itself among them, desolid), then the main
/// building starts and takes the work; the annex takes none.
#[test]
fn the_work_on_an_annex_goes_to_its_main_building_after_its_annexes_start() {
    let mut world = sent_off_world(vec![]);
    let house = &world.players[1].objects[ListIndex::new(1)];
    let annex = Object::with_building(
        Unit {
            id: object_id(10031),
            ..house.clone()
        },
        Building {
            annex_of: Some(MainBuildingId::new(object_id(10028))),
            ..world.parts.building_of(house.id).clone()
        },
    );
    let _ = world.push_live(PlayerId::new(1), annex).unwrap();
    world.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        Tiles::new(GridSize::new(120, 120), vec![4; 14400]),
    );
    for (id, x) in [(10028, 30.0), (10031, 32.0)] {
        world.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(x, 88.0, 0.0),
                radius: Footprint::new(1.0, 1.0),
                class: class::BUILDING,
                player: PlayerId::new(1),
                master: ids::HOUSE,
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    world.link_test_annex(object_id(10031), object_id(10028));
    assert_eq!(
        world.work_building(FoundationId::new(object_id(10031)), Seconds::new(0.021)),
        Ok(BuildProgress::Ongoing)
    );
    let construction_of = |id: ObjectId| world.parts.building_of(id).construction.unwrap();
    let started = f32::from_bits(0x2edb_e6ff);
    assert!(!construction_of(object_id(10031)).desolid);
    assert_eq!(
        construction_of(object_id(10031)).progress,
        Seconds::new(started)
    );
    assert!(!construction_of(object_id(10028)).desolid);
    assert_eq!(
        construction_of(object_id(10028)).progress,
        Seconds::new(started + 0.021)
    );
    assert_eq!(
        world.object(object_id(10031)).unwrap().1.life.hp,
        HitPoints::new(1.0)
    );
    assert!(HitPoints::new(1.0) < world.object(object_id(10028)).unwrap().1.life.hp);
}

/// The world of [`sent_off_world`] with the house built and damaged (300 of its 550 hit
/// points), the villager a builder (118) with a Repair task on any object (work value 0.5) and
/// a weapon range of 0.5.
fn repair_world() -> World {
    let mut world = sent_off_world(vec![]);
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.life.master_id = ids::BUILDER;
    let house = &mut world.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(300.0);
    let construction = world
        .parts
        .building_mut(object_id(10028))
        .unwrap()
        .construction
        .as_mut()
        .unwrap();
    (construction.progress, construction.desolid) = (Seconds::new(1.0), false);
    for masters in &mut world.model.masters {
        let builder = Arc::make_mut(masters).get_mut(ids::BUILDER).unwrap();
        builder
            .combat
            .get_or_insert_with(Default::default)
            .weapon_range = Distance::new(0.5);
        builder
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .push(Task {
                id: task_id(4),
                work_val1: WorkValue::new(0.5),
                ..task(REPAIR)
            });
    }
    world
}

/// The work command on the player's damaged house (`FUN_14106fd00`, then the Repair's start):
/// the task finder's Repair task (106), the target taken, the unit AI told order 0x2ce with
/// action 0x26a and the house's class, the Repair walking (state 4) with its master's weapon
/// range. The same command again while it faces the house (state 6) keeps the action; a
/// computer player's repairer walking to it keeps it too.
#[test]
fn work_command_on_damaged_building_starts_repair() {
    let mut world = repair_world();
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(unit.id).top_action_kind(),
        Some(REPAIR)
    );
    let repair_action = world
        .parts
        .actions_of(unit.id)
        .list
        .repair()
        .copied()
        .unwrap();
    assert_eq!(
        (
            repair_action.target,
            repair_action.state.bare(),
            repair_action.range,
            repair_action.task.id,
            repair_action.task.work_value
        ),
        (
            Some(RepairTargetId::new(object_id(10028))),
            RepairState::Move { move_to: None },
            Distance::new(0.5),
            task_id(4),
            WorkValue::new(0.5)
        )
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Repair),
            Some(AiAction::Repair),
            Some(object_id(10028))
        )
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .state = RepairState::Face;
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .repair()
            .copied()
            .unwrap()
            .state,
        RepairState::Face
    );
}

/// A work command on another damaged house while repairing (`FUN_140adb480`'s slot 0x68): a
/// repairer counted among the first house's repairers leaves them, takes the new one and walks
/// to it again (state 4, not counted).
#[test]
fn work_command_on_another_building_moves_the_repair_there() {
    let mut world = repair_world();
    let other = Unit {
        id: object_id(10031),
        place: Static::at(Position::new(34.0, 88.0, 0.0)),
        ..world.players[1].objects[ListIndex::new(1)].clone()
    };
    let layer = world
        .parts
        .building_of(world.players[1].objects[ListIndex::new(1)].id)
        .clone();
    let _ = world
        .push_live(PlayerId::new(1), Object::with_building(other, layer))
        .unwrap();
    let work = |target| Work {
        target,
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(10028))),
        )
        .unwrap();
    {
        let repair_action = world
            .parts
            .actions
            .of_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .repair_mut()
            .unwrap();
        (repair_action.state, repair_action.counted) = (RepairState::Work, true);
    }
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(10031))),
        )
        .unwrap();
    let repair_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .repair()
        .copied()
        .unwrap();
    assert_eq!(
        (
            repair_action.target,
            repair_action.state.bare(),
            repair_action.counted,
            repair_action.point
        ),
        (
            Some(RepairTargetId::new(object_id(10031))),
            RepairState::Move { move_to: None },
            false,
            Position::new(34.0, 88.0, 0.0)
        )
    );
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(1)].id)
            .construction
            .unwrap()
            .builders,
        BuilderCount::of_data(0)
    );
}

/// The work command on the player's foundation (`FUN_14106ff70`, the factory's case 0x65, then
/// `FUN_140aadd80`): the villager (83) becomes the builder 118 of its swap group, takes the
/// foundation (one worker), tells its unit AI order 702 with action 602 on it and walks to it
/// (state 4, its `MoveTo` starting: the footprints lie 0.3 apart) with the task's range, work
/// value and the builder's work rate.
#[test]
fn work_command_on_foundation_starts_build_as_its_builder() {
    let mut world = sent_off_world(vec![]);
    // a villager moves: its MoveTo starts with speed (FUN_140aa2100)
    world.parts.put_mover(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(walker()),
    );
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, ids::BUILDER);
    assert_eq!(
        world.parts.actions_of(unit.id).top_action_kind(),
        Some(ActionKind::BUILD)
    );
    let build_action = world
        .parts
        .actions_of(unit.id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            build_action.target,
            build_action.state.bare(),
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| ObjectId::field(move_to_action.target)),
            build_action.point,
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            build_action.added
        ),
        (
            Some(FoundationId::new(object_id(10028))),
            BuildState::Move { move_to: None },
            Some(10028),
            Position::new(30.0, 88.0, 0.0),
            Some(MoveTo::Start),
            false
        )
    );
    assert_eq!(
        build_action.task,
        BuildTask {
            range: Distance::ZERO,
            work_value: WorkValue::new(0.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: task_id(0),
            moving: None,
        }
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Build),
            Some(AiAction::Build),
            Some(object_id(10028))
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(1)].workers,
        WorkerCount::of_data(1)
    );
}

/// A builder with a master of its own (a captured villager's copy) builds as that master: its
/// Build's `MoveTo` starts with the own master's walking sprite (`FUN_140aa0df0`'s default),
/// not the one of its player's master of its id, which has none.
#[test]
fn builder_with_master_of_its_own_walks_with_its_own_master_sprite() {
    use crate::map::spatial::Placed;
    use crate::objects::sprite::Graphic;
    let mut sent_off_world = sent_off_world(vec![]);
    open_map(&mut sent_off_world);
    sent_off_world.spatial.relations =
        ByPlayer::new(vec![relation_row([0, 3]), relation_row([3, 0])]);
    let placed = |position, class, master, radius| Placed {
        position,
        radius,
        class,
        player: PlayerId::new(1),
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let at = Position::new(26.5, 89.5, 0.0);
    sent_off_world.spatial.insert(
        object_id(9175),
        placed(at, class::VILLAGER, VILLAGER_MALE, Footprint::new(0.2, 0.2)),
    );
    sent_off_world.spatial.insert(
        object_id(10028),
        placed(
            Position::new(30.0, 88.0, 0.0),
            class::BUILDING,
            ids::HOUSE,
            Footprint::new(1.0, 1.0),
        ),
    );
    let players = &mut sent_off_world.players[1].objects[ListIndex::new(0)];
    players.place_for_test(at);
    let mut mover = walker();
    mover.moved_at = at;
    sent_off_world.parts.put_mover(players.id, Some(mover));
    // a walker of sixteen facets (its yaw reads them)
    for table in &mut sent_off_world.model.masters {
        if let Some(move_stats) = Arc::make_mut(table)
            .get_mut(ids::BUILDER)
            .and_then(|master| master.move_stats.as_mut())
        {
            move_stats.facets = Some(SpriteFacets {
                count: FacetCount::of_data(16),
                turns: true,
            });
        }
    }
    sent_off_world.build_mesh().unwrap();
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    let walking = Graphic {
        id: SpriteId::named(7),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let _ = Arc::make_mut(&mut sent_off_world.model.graphics).insert(SpriteId::named(7), walking);
    let shown_after_update = |world: &mut World| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let me = Me {
                    human_order: WorldTime::ZERO,
                    id: unit.id,
                    group: unit.group.id,
                    action_state: actions
                        .top_action()
                        .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                    top_speed: TopSpeed::Own,
                };
                let _ = unit.build_part(actions, step, me)?;
                Ok(unit.sprite.map(|active_sprite| active_sprite.graphic.id))
            },
        )
        .unwrap()
    };
    // the player's master 118 has no sprites: the MoveTo shows none
    let mut table = sent_off_world.clone();
    table
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(shown_after_update(&mut table), None);
    // the MoveTo's constructor (FUN_140aa0df0) reads the builder's own master for its sprite
    let own = Master {
        moving: Some(MovingPart {
            move_sprite: Some(SpriteId::named(7)),
            ..sent_off_world.model.masters[1]
                .get(ids::BUILDER)
                .unwrap()
                .moving
                .unwrap_or_default()
        }),
        ..sent_off_world.model.masters[1]
            .get(ids::BUILDER)
            .unwrap()
            .clone()
    };
    let id = sent_off_world.players[1].objects[ListIndex::new(0)].id;
    sent_off_world
        .set_part(id, Part::OwnMaster(Some(Box::new(own))))
        .unwrap();
    sent_off_world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        shown_after_update(&mut sent_off_world),
        Some(SpriteId::named(7))
    );
}

/// The work command again on the Build's own foundation (`FUN_140ab1b30`): a Build at work
/// (state 6) keeps on; one counted among the builders that waits (state 222) leaves them and
/// walks again (state 4), its `MoveTo` arrived where the footprints touch.
#[test]
fn work_command_on_the_build_own_foundation_keeps_the_work_or_walks_again() {
    let mut world = sent_off_world(vec![]);
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .build_mut()
        .unwrap()
        .state = BuildState::Work;
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .map(|build_action| build_action.state.bare()),
        Some(BuildState::Work)
    );
    {
        let build_action = world
            .parts
            .actions
            .of_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .build_mut()
            .unwrap();
        (build_action.state, build_action.added) = (BuildState::Wait, true);
    }
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    world.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(29.0, 88.0, 0.0));
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let build_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            build_action.state.bare(),
            build_action.added,
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            build_action.target
        ),
        (
            BuildState::Move { move_to: None },
            false,
            Some(MoveTo::Arrived),
            Some(FoundationId::new(object_id(10028)))
        )
    );
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(1)].id)
            .construction
            .map(|construction| construction.builders.get()),
        Some(0)
    );
    // the same unit's work command on itself does nothing
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &Work {
                target: Some(object_id(9175)),
                ..work
            },
        )
        .unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
}

/// The work command on another foundation of the player's while building (`FUN_140ab0300`):
/// a counted builder leaves the first foundation's builders, lets it go and takes the new
/// one, walking to it again (state 4).
#[test]
fn work_command_on_another_foundation_moves_the_build_there() {
    let mut world = sent_off_world(vec![]);
    let other = Unit {
        id: object_id(10031),
        place: Static::at(Position::new(34.0, 88.0, 0.0)),
        ..world.players[1].objects[ListIndex::new(1)].clone()
    };
    let layer = world
        .parts
        .building_of(world.players[1].objects[ListIndex::new(1)].id)
        .clone();
    let _ = world
        .push_live(PlayerId::new(1), Object::with_building(other, layer))
        .unwrap();
    let work = |target| Work {
        target,
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(10028))),
        )
        .unwrap();
    {
        let build_action = world
            .parts
            .actions
            .of_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .build_mut()
            .unwrap();
        (build_action.state, build_action.added) = (BuildState::Wait, true);
    }
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(10031))),
        )
        .unwrap();
    let build_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            build_action.target,
            build_action.state.bare(),
            build_action.added,
            build_action.point,
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| ObjectId::field(move_to_action.target))
        ),
        (
            Some(FoundationId::new(object_id(10031))),
            BuildState::Move { move_to: None },
            false,
            Position::new(34.0, 88.0, 0.0),
            Some(10031)
        )
    );
    let site = |list_index: ListIndex| {
        let unit = &world.players[1].objects[list_index];
        (
            unit.workers.get(),
            world
                .parts
                .building_of(unit.id)
                .construction
                .map(|construction| construction.builders.get()),
        )
    };
    assert_eq!(
        (site(ListIndex::new(1)), site(ListIndex::new(2))),
        ((0, Some(0)), (1, Some(0)))
    );
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .map(|ai| ai.current_target),
        Some(Some(object_id(10031)))
    );
}

/// The unit AI's queued order on a target (`FUN_140c5c170` case `0x2db`): the unit's work
/// command on its foundation at the foundation's place (the villager becomes the builder and
/// starts the Build), then the tail with the order the work gave the AI (the build order) and
/// the queued order's priority. An order on the unit itself works on nothing but still takes
/// the tail; one on a target that is gone, or for a unit without a unit AI, is not modelled.
#[test]
fn queued_order_on_target_works_on_it_with_the_order_priority() {
    use crate::unit_ai::QueuedOrder;
    let order = |target| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: OrderPriority::of_data(3),
        target,
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_kind: order_kind(80),
    };
    let mut world = sent_off_world(vec![]);
    world
        .order_on_target(
            PlayerId::new(1),
            ListIndex::new(0),
            order(Some(object_id(10028))),
        )
        .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(players.life.master_id, ids::BUILDER);
    assert_eq!(
        world
            .parts
            .actions_of(players.id)
            .list
            .build()
            .map(|build_action| (
                build_action.target.map(FoundationId::id),
                build_action.state.bare()
            )),
        Some((Some(object_id(10028)), BuildState::Move { move_to: None }))
    );
    let ai = world.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.order_priority.get(), ai.current_target),
        (Some(OrderKind::Build), 3, Some(object_id(10028)))
    );
    let mut world = sent_off_world(vec![]);
    world
        .order_on_target(
            PlayerId::new(1),
            ListIndex::new(0),
            order(Some(object_id(9175))),
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.actions_of(unit.id).list.build().copied(), None);
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(unit.id)
            .map(|ai| (ai.current_order, ai.order_priority.get())),
        Some((None, 3))
    );
    assert!(
        world
            .order_on_target(
                PlayerId::new(1),
                ListIndex::new(0),
                order(Some(object_id(4444)))
            )
            .is_err()
    );
    world
        .parts
        .set_unit_ai(world.players[1].objects[ListIndex::new(0)].id, None);
    assert!(
        world
            .order_on_target(
                PlayerId::new(1),
                ListIndex::new(0),
                order(Some(object_id(9175)))
            )
            .is_err()
    );
}

/// [`placeable`]'s map with its tiles (class 4, open), on which the walker's restriction 7
/// lets a unit stand anywhere: the spatial table starts empty.
pub(crate) fn open_map(world: &mut World) {
    world.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        Tiles::new(GridSize::new(120, 120), vec![4; 120 * 120]),
    );
    placeable(world);
    if let Some(terrain) = world.spatial.terrain.as_mut() {
        terrain.restrictions = vec![Restriction::new(vec![1.0]); 8];
    }
}

/// [`sent_off_world`] with its villager the forager 120 (a Gather task on class 7 for food,
/// dropping at the town centre 109, as the drop-site table has it), Gaia's berry bushes 600 and
/// 601 (master 59, class 7) and player 1's town centre 700, all placed on [`open_map`]; the
/// villager can move.
pub(crate) fn forage_world() -> World {
    let mut world = sent_off_world(vec![]);
    for masters in &mut world.model.masters {
        let forager = Master {
            id: master_id(120),
            capacity: CarryCapacity::of_data(10),
            action: Some(ActionPart {
                drop_sites: vec![ids::TOWN_CENTRE],
                tasks: vec![Task {
                    id: task_id(2),
                    action_kind: GATHER,
                    object_group: Some(class_id(7)),
                    attribute_kind: Some(AttributeId::named(17)),
                    work_range: Distance::new(0.25),
                    work_val1: WorkValue::new(0.31),
                    ..Task::default()
                }],
                ..masters
                    .get(VILLAGER_MALE)
                    .unwrap()
                    .clone()
                    .action
                    .clone()
                    .unwrap_or_default()
            }),
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let bush = Master {
            id: master_id(59),
            kind: MasterKind::MOVING,
            class: class::CLASS_7,
            radius: Footprint::new(0.5, 0.5),
            ..Master::default()
        };
        let centre = Master {
            id: ids::TOWN_CENTRE,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(
            masters.iter().cloned().chain([forager, bush, centre]),
        ));
    }
    let resource = |id, x| Unit {
        id,
        place: Static::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(1.0),
            master_id: master_id(59),
            attribute_kind: Some(AttributeId::named(17)),
            amount: Amount::new(125.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    world.players[0].objects = vec![
        resource(object_id(600), 32.5),
        resource(object_id(601), 36.5),
    ]
    .into();
    world.reindex();
    let _ = world
        .push_live(
            PlayerId::new(1),
            Unit {
                id: object_id(700),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(2400.0),
                    master_id: ids::TOWN_CENTRE,
                    ..Lifecycle::default()
                },
                place: Static::at(Position::new(20.0, 89.5, 0.0)),
                ..Unit::default()
            },
        )
        .unwrap();
    // a building is an animated object (type 80)
    world.set_motion(object_id(700), Some(Motion::Animated(Animated::default())));
    world.parts.put_mover(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(walker()),
    );
    world.players[1].objects[ListIndex::new(0)].life.master_id = master_id(120);
    open_map(&mut world);
    let placed = |x: f32, class, player, master, radius| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius,
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(
        object_id(9175),
        placed(
            29.5,
            class::VILLAGER,
            PlayerId::new(1),
            VILLAGER_MALE,
            Footprint::new(0.2, 0.2),
        ),
    );
    world.spatial.insert(
        object_id(600),
        placed(
            32.5,
            class::CLASS_7,
            PlayerId::new(0),
            master_id(59),
            Footprint::new(0.5, 0.5),
        ),
    );
    world.spatial.insert(
        object_id(601),
        placed(
            36.5,
            class::CLASS_7,
            PlayerId::new(0),
            master_id(59),
            Footprint::new(0.5, 0.5),
        ),
    );
    world.spatial.insert(
        object_id(700),
        placed(
            20.0,
            class::BUILDING,
            PlayerId::new(1),
            ids::TOWN_CENTRE,
            Footprint::new(2.0, 2.0),
        ),
    );
    // the drop-site table: villagers drop berries at the town centre
    world.model.drop_sites = Arc::new(vec![DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        targets: vec![DropTarget {
            class: class::CLASS_7,
            workers: WorkerCount::of_data(-1),
            state: None,
            attribute: Some(AttributeId::named(17)),
        }],
        ..DropSite::default()
    }]);
    world
}

/// The work command on Gaia's berry bush (the factory's case 5, `FUN_140a97e00`): the forager's
/// Gather takes the bush (one worker), the unit
/// AI hears order 0x2c5 with action 0x261 on it and the Gather walks to it (state 4) with the
/// task's range. The command on the other bush while it walks (slot 0x48, `0x140a903a0`) moves
/// the Gather there: the first bush is let go, the second taken.
#[test]
fn work_command_on_bush_starts_gather_and_another_moves_it() {
    let mut forage_world = forage_world();
    let work = |target| Work {
        target,
        point: Position::new(32.5, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    forage_world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(600))),
        )
        .unwrap();
    let unit = &forage_world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, master_id(120));
    assert_eq!(
        forage_world.parts.actions_of(unit.id).top_action_kind(),
        Some(GATHER)
    );
    let gather = forage_world
        .parts
        .actions_of(unit.id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            gather.target,
            gather.carry_to,
            gather.state.bare(),
            gather.point,
            gather
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.range),
            gather.task.id
        ),
        (
            Some(GatherTargetId::new(object_id(600))),
            None,
            GatherState::Walk { move_to: None },
            Position::new(32.5, 89.5, 0.0),
            Some(Distance::new(0.25)),
            task_id(2)
        )
    );
    let ai = forage_world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Gather),
            Some(AiAction::Gather),
            Some(object_id(600))
        )
    );
    let workers = |world: &World| {
        (
            world.players[0].objects[ListIndex::new(0)].workers,
            world.players[0].objects[ListIndex::new(1)].workers,
        )
    };
    assert_eq!(
        workers(&forage_world),
        (WorkerCount::of_data(1), WorkerCount::of_data(0))
    );
    forage_world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(601))),
        )
        .unwrap();
    let gather_action = forage_world
        .parts
        .actions_of(forage_world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.state.bare(),
            gather_action.point
        ),
        (
            Some(GatherTargetId::new(object_id(601))),
            GatherState::Walk { move_to: None },
            Position::new(36.5, 89.5, 0.0)
        )
    );
    assert_eq!(
        workers(&forage_world),
        (WorkerCount::of_data(0), WorkerCount::of_data(1))
    );
    assert_eq!(
        forage_world
            .parts
            .unit_ai
            .get(forage_world.players[1].objects[ListIndex::new(0)].id)
            .map(|ai| ai.current_target),
        Some(Some(object_id(601)))
    );
    // at work on it (state 7) the same command keeps the Gather as it is
    forage_world
        .parts
        .actions
        .of_mut(forage_world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    let before = forage_world
        .parts
        .actions_of(forage_world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied();
    forage_world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(601))),
        )
        .unwrap();
    assert_eq!(
        forage_world
            .parts
            .actions_of(forage_world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .copied(),
        before
    );
    assert_eq!(
        workers(&forage_world),
        (WorkerCount::of_data(0), WorkerCount::of_data(1))
    );
}

/// The unit AI's gather command (`FUN_141085030`, forced): a new Gather on the bush replaces
/// the forager's actions without the unit AI's notify (the AI set its order itself); the command on the other bush while the Gather on the same task is on
/// top retargets it (slot 0x48): the first bush let go, the second taken.
#[test]
fn the_unit_ai_gather_command_starts_gather_or_retargets_its_own() {
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, master_id(120));
    let gather = world
        .parts
        .actions_of(unit.id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (gather.target, gather.state.bare(), gather.task.id),
        (
            Some(GatherTargetId::new(object_id(600))),
            GatherState::Walk { move_to: None },
            task_id(2)
        )
    );
    assert_eq!(
        world.parts.unit_ai.get(unit.id).map(|ai| ai.current_order),
        Some(None)
    );
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(601))
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.target),
        Some(Some(GatherTargetId::new(object_id(601))))
    );
    assert_eq!(
        (
            world.players[0].objects[ListIndex::new(0)].workers,
            world.players[0].objects[ListIndex::new(1)].workers
        ),
        (WorkerCount::of_data(0), WorkerCount::of_data(1))
    );
    // an object the world does not hold fails the command
    assert!(
        world
            .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(4444))
            .is_err()
    );
}

/// A loaded forager's work command on its own town centre, a drop site of its master: the task
/// finder's drop-off gives the Gather task for what it carries, and the Gather started on the
/// centre carries the load there (`0x140a8b3a0`: state 8, the centre its second target, no
/// target or point); the centre, taken as the target and again as the second target, then let
/// go as the first, counts one worker. A Gather at work on its bush sent there (slot 0x48 on a
/// building that is no farm answers 0) starts the same new Gather.
#[test]
fn loaded_forager_sent_to_its_town_centre_carries_its_load_there() {
    let work = Work {
        target: Some(object_id(700)),
        point: Position::new(20.0, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    let loaded = || {
        let mut forage_world = forage_world();
        let players = &mut forage_world.players[1].objects[ListIndex::new(0)];
        (players.life.attribute_kind, players.life.amount) =
            (Some(AttributeId::named(17)), Amount::new(6.0));
        forage_world
    };
    let carries = |world: &World| {
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let gather_action = world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .copied()
            .unwrap();
        (
            (
                gather_action.target,
                gather_action.carry_to,
                gather_action.state.bare(),
                gather_action.point,
            ),
            unit.life.held(),
            world.players[1].objects[ListIndex::new(2)].workers,
        )
    };
    let loaded_to_700 = (
        (
            None,
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None },
            Position::from([-1.0; 3]),
        ),
        carrying(Some(AttributeId::named(17)), 6.0),
        WorkerCount::ONE,
    );
    let mut world = loaded();
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(carries(&world), loaded_to_700);
    let mut world = loaded();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(carries(&world), loaded_to_700);
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
}

/// The work command without a task (`FUN_14106ff70`'s move): the forager sent to its town
/// centre empty-handed (no task on class 3) is told of the move (order 0x2c6, the centre with
/// its class) and walks to the centre; sent to a point it walks there; in state 1 it is told of
/// no order (−1) and stays; a master of speed 0 (its slot 0xe8) does nothing.
#[test]
fn work_command_without_task_moves_the_unit_there() {
    let work = |target| Work {
        target,
        point: Position::new(20.0, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    let mut world = forage_world();
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(700))),
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert!(world.parts.actions_of(unit.id).list.move_to().is_some());
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Move),
            Some(AiAction::Move),
            Some(object_id(700))
        )
    );
    let mut world = forage_world();
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work(None))
        .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert!(world.parts.actions_of(players.id).list.move_to().is_some());
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(players.id)
            .map(|ai| ai.current_order),
        Some(Some(OrderKind::Move))
    );
    let mut world = forage_world();
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Unfinished;
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(700))),
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(unit.id).list.move_to().copied(),
        None
    );
    assert_eq!(
        world.parts.unit_ai.get(unit.id).map(|ai| ai.current_order),
        Some(None)
    );
    let mut world = forage_world();
    move_by(
        Arc::make_mut(&mut world.model.masters[1])
            .get_mut(master_id(120))
            .unwrap(),
        walking(Speed::ZERO),
    );
    let before = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .clone();
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &work(Some(object_id(700))),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .move_to()
            .copied(),
        before.list.move_to().copied()
    );
}

/// A move toward an object while gathering (`FUN_140a97730`, the Gather's slot 0x40,
/// `0x140a8b740`): the Gather takes the move; toward the other bush it walks to the move's point
/// (state 15) without a target, the first bush let go. Loaded and moved toward its town centre,
/// a drop site, it carries the load there (state 8, the centre its second target, taken).
#[test]
fn gather_takes_move_toward_an_object_as_its_point_or_drop_site() {
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    let point = Position::new(36.0, 89.0, 0.0);
    assert_eq!(
        world.move_to_object(PlayerId::new(1), ListIndex::new(0), object_id(601), point,),
        Ok(Answer::Taken)
    );
    let gather_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.carry_to,
            gather_action.state.bare(),
            gather_action.point,
            gather_action
                .state
                .move_to()
                .copied()
                .and_then(|move_to_action| move_to_action.goal())
        ),
        (
            None,
            None,
            GatherState::Moved { move_to: None },
            point,
            Some(point)
        )
    );
    assert_eq!(
        (
            world.players[0].objects[ListIndex::new(0)].workers,
            world.players[0].objects[ListIndex::new(1)].workers
        ),
        (WorkerCount::of_data(0), WorkerCount::of_data(0))
    );
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    (unit.life.attribute_kind, unit.life.amount) = (Some(AttributeId::named(17)), Amount::new(6.0));
    assert_eq!(
        world.move_to_object(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
        ),
        Ok(Answer::Taken)
    );
    let gather_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.carry_to,
            gather_action.state.bare(),
            gather_action.point
        ),
        (
            None,
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None },
            Position::from([-1.0; 3])
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    // a move toward the drop site it already carries to changes nothing
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        world.move_to_object(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
}

/// `UNIT_ORDER`'s drop-off 6 of a loaded forager without an action (`FUN_14161e370`, then
/// [`World::drop_off_unasked`]): its master drops at the town centre, so it seeks its drop site
/// from itself (`FUN_1411671e0`: its zone, the planner; by the squared distance) and works on the
/// nearer of the player's two centres in state 2, its Gather carrying the load there. A master
/// dropping at the dock (45) too searches without a seeker; with no drop site found it does
/// nothing.
#[test]
fn drop_off_order_takes_forager_to_its_nearest_drop_site() {
    let mut forage_world = forage_world();
    let far = Unit {
        id: object_id(701),
        place: Static::at(Position::new(60.0, 89.5, 0.0)),
        ..forage_world.players[1].objects[ListIndex::new(2)].clone()
    };
    let layer = forage_world
        .parts
        .building_of(forage_world.players[1].objects[ListIndex::new(2)].id)
        .clone();
    let _ = forage_world
        .push_live(PlayerId::new(1), Object::with_building(far, layer))
        .unwrap();
    let players = &mut forage_world.players[1].objects[ListIndex::new(0)];
    (players.life.attribute_kind, players.life.amount) =
        (Some(AttributeId::named(17)), Amount::new(6.0));
    let world = forage_world.clone();
    forage_world
        .unit_order(
            player_id(1),
            &object_ids([9175]),
            UnitOrderAction::DropOffSearch,
        )
        .unwrap();
    let unit = &forage_world.players[1].objects[ListIndex::new(0)];
    let gather = forage_world
        .parts
        .actions_of(unit.id)
        .list
        .gather()
        .copied()
        .expect("a Gather to the drop site");
    assert_eq!(
        (gather.carry_to, gather.state.bare()),
        (
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None }
        )
    );
    assert_eq!(
        forage_world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    // dropping at the dock too: the search without a seeker finds the same centre
    let mut game = world.clone();
    for masters in &mut game.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .action
            .get_or_insert_with(Default::default)
            .drop_sites = vec![ids::ID_45, ids::TOWN_CENTRE];
    }
    game.unit_order(
        player_id(1),
        &object_ids([9175]),
        UnitOrderAction::DropOffSearch,
    )
    .unwrap();
    assert_eq!(
        game.parts
            .actions_of(game.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| (gather_action.carry_to, gather_action.state.bare())),
        Some((
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None }
        ))
    );
    // no centre standing (state 2): no drop site, no table entry for berries by class
    let mut world = world;
    for index in [2, 3] {
        world.players[1].objects[ListIndex::new(index)].life.state = ObjectState::Unfinished;
    }
    world
        .unit_order(
            player_id(1),
            &object_ids([9175]),
            UnitOrderAction::DropOffSearch,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .copied(),
        None
    );
}

/// The Empire Wars start (`BaseWorld::update`'s first update) under the farm setting: the idle
/// forager standing on its player's farm 800 works it (the first of class 49 on its tile), its
/// Gather on the farm at the farm's place; the foundation, the town centre and the farm take
/// nothing. A villager with orders waits; a fishing ship without a fish trap under it, and the
/// pass without the farm setting, are not modelled.
#[test]
fn the_empire_wars_start_puts_villager_on_its_farm_to_work() {
    let mut forage_world = forage_world();
    forage_world.settings.farm_mode = FarmMode::Walkable;
    for masters in &mut forage_world.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .push(Task {
                id: task_id(3),
                action_kind: GATHER,
                object_group: Some(class_id(49)),
                attribute_kind: Some(attribute::FOOD),
                work_range: Distance::new(0.25),
                ..Task::default()
            });
        let farm = Master {
            id: ids::FARM,
            kind: MasterKind::BUILDING,
            class: class::FARM,
            radius: Footprint::new(1.5, 1.5),
            ..Master::default()
        };
        let fishing = Master {
            id: master_id(13),
            class: class::FISHING_SHIP,
            ..masters.get(master_id(120)).unwrap().clone()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([farm, fishing])));
    }
    let _ = forage_world
        .push_live(
            PlayerId::new(1),
            Unit {
                id: object_id(800),
                place: Static::at(Position::new(29.5, 89.5, 0.0)),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(480.0),
                    master_id: ids::FARM,
                    attribute_kind: Some(attribute::FOOD),
                    amount: Amount::new(175.0),
                    ..Lifecycle::default()
                },
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
        )
        .unwrap();
    forage_world.spatial.insert(
        object_id(800),
        Placed {
            position: Position::new(29.5, 89.5, 0.0),
            radius: Footprint::new(1.5, 1.5),
            class: class::FARM,
            player: PlayerId::new(1),
            master: ids::FARM,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let world = forage_world.clone();
    forage_world.start_work(PlayerId::new(1)).unwrap();
    let gather_action = forage_world
        .parts
        .actions_of(forage_world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .expect("the farm's Gather");
    assert_eq!(
        (
            gather_action.target,
            gather_action.task.id,
            gather_action.point
        ),
        (
            Some(GatherTargetId::new(object_id(800))),
            task_id(3),
            Position::new(29.5, 89.5, 0.0)
        )
    );
    assert_eq!(
        forage_world.players[1].objects[ListIndex::new(3)].workers,
        WorkerCount::of_data(1)
    );
    let others = |world: &World| {
        world.players[1].objects.as_slice()[1..]
            .iter()
            .map(|unit| world.parts.actions_of(unit.id).top_action_kind())
            .collect::<Vec<_>>()
    };
    assert_eq!(others(&forage_world), [None, None, None]);
    // a villager with an order in its queue waits for it
    let mut game = world.clone();
    game.parts
        .unit_ai
        .get_mut(game.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .orders
        .push(QueuedOrder {
            issuer: Some(Issuer::Player(player_id(1))),
            order: OrderKind::Move,
            priority: OrderPriority::of_data(100),
            target: None,
            target_owner: None,
            point: Position::new(10.0, 10.0, 0.0),
            range: Distance::ZERO,
            shown: Point::new(10.0, 10.0),
            ready: true,
            target_kind: None,
        });
    game.start_work(PlayerId::new(1)).unwrap();
    assert_eq!(
        game.parts
            .actions_of(game.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .copied(),
        None
    );
    // a fishing ship finds no fish trap on its tile
    let mut game = world.clone();
    game.players[1].objects[ListIndex::new(0)].life.master_id = master_id(13);
    assert!(game.start_work(PlayerId::new(1)).is_err());
    let mut world = world;
    world.settings.farm_mode = FarmMode::Fixed;
    assert!(world.start_work(PlayerId::new(1)).is_err());
}

/// The forager's unit AI run outside its update making the calls `effects` on its unit
/// ([`World::run_unit_ai`]): each at once, in order
/// ([`crate::unit_ai::world_view::UnitView::effect_now`]).
fn with_effects(world: &mut World, effects: Vec<Effect>) -> GenieResult<()> {
    world.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        move |ai, _, side, _, rng| {
            effects
                .into_iter()
                .try_for_each(|effect| side.effect_now(effect, ai, rng))
        },
    )
}

/// The unit AI's explore (slot 0x398, `FUN_141084db0`) of the idle forager: a new Explore to
/// the point at its height walks there (state 11, its `MoveTo` starting); a point off the map's
/// quarter for the Explore on top makes it wait (state 3), its walk stopped, and ask its unit
/// AI for a point (notify 0x202 with the explore's action). An Explore replacing a Gather is
/// not modelled.
#[test]
fn the_unit_ai_explore_walks_to_its_point_or_asks_for_one() {
    use crate::movement::explore::ExploreState;
    use crate::unit_ai::Effect;
    let mut world = forage_world();
    with_effects(&mut world, vec![Effect::Explore(Point::new(40.0, 90.0))]).unwrap();
    let explore_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .explore()
        .copied()
        .unwrap();
    assert!(matches!(
        explore_action.state,
        ExploreState::Walk { move_to: Some(_) }
    ));
    assert_eq!(explore_action.point, Position::new(40.0, 90.0, 0.0));
    with_effects(&mut world, vec![Effect::Explore(Point::new(-1.0, 90.0))]).unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let explore_action = world
        .parts
        .actions_of(unit.id)
        .list
        .explore()
        .copied()
        .unwrap();
    assert_eq!(explore_action.state, ExploreState::Wait);
    let notifies = &world.parts.unit_ai.get(unit.id).unwrap().notifies;
    assert_eq!(
        notifies.last().map(|notify| (notify.kind, notify.params)),
        Some((
            NotifyKind::REQUIRES_SEARCH,
            [AiAction::Explore.value(), 0, 0]
        ))
    );
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    assert!(with_effects(&mut world, vec![Effect::Explore(Point::new(40.0, 90.0))]).is_err());
}

/// The unit AI's move to an object (slot 0x348, `FUN_141086440`): the forager's Gather goes (its
/// bush let go), a `MoveTo` to the other bush replaces it within the range given; the unit AI's
/// cap of its last human order (slot 0x300) and its works follow in order: a work on the bush
/// (slot 0x7e8 with the retargeting byte 0) starts a Gather there.
#[test]
fn the_unit_ai_moves_and_works_apply_in_order() {
    use crate::unit_ai::Effect;
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    world.players[1].objects[ListIndex::new(0)].last_human_order = WorldTime::from_millis(900);
    with_effects(
        &mut world,
        vec![
            Effect::CapHumanOrder(WorldTime::from_millis(300)),
            Effect::MoveToObject {
                target: object_id(601),
                state: ObjectState::Alive,
                range: Distance::new(0.5),
            },
        ],
    )
    .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            world.parts.actions_of(players.id).list.gather().copied(),
            players.last_human_order
        ),
        (None, WorldTime::from_millis(300))
    );
    assert!(world.parts.actions_of(players.id).list.move_to().is_some());
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
    with_effects(
        &mut world,
        vec![Effect::Command(AiCommand::WorkOn {
            target: object_id(601),
            point: Position::new(36.5, 89.5, 0.0),
        })],
    )
    .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .map(|gather_action| (gather_action.target, gather_action.state.bare())),
        Some((
            Some(GatherTargetId::new(object_id(601))),
            GatherState::Walk { move_to: None }
        ))
    );
    assert_eq!(
        world.parts.actions_of(unit.id).list.move_to().copied(),
        None
    );
    // a unit that does not move cannot be moved to an object
    world
        .parts
        .put_mover(world.players[1].objects[ListIndex::new(0)].id, None);
    assert!(
        with_effects(
            &mut world,
            vec![Effect::MoveToObject {
                target: object_id(600),
                state: ObjectState::Alive,
                range: Distance::new(0.5),
            }],
        )
        .is_err()
    );
}

/// The unit AI's hunt and build commands and its order on a target, as its effects give them
/// after the run: `HuntObject` hunts the deer (the hunt command, `FUN_1412151f0`); `BuildObject`
/// sends the villager to build its foundation (the build command, slot 0x380, as the builder);
/// `OrderOnTarget` works on the foundation with the order's priority, at once, a second one
/// again. An AI move of a master the slice does not have is not modelled; a restored patrol
/// without one saved does nothing, and the stop stops.
#[test]
fn the_unit_ai_hunt_build_and_target_orders_apply_after_its_run() {
    use crate::unit_ai::{AiMove, Effect, QueuedOrder};
    let mut hunt_world = hunt_world();
    with_effects(
        &mut hunt_world,
        vec![Effect::Command(AiCommand::HuntObject(object_id(500)))],
    )
    .unwrap();
    assert_eq!(
        hunt_world
            .parts
            .actions_of(hunt_world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.target),
        Some(Some(HuntTargetId::new(object_id(500))))
    );
    with_effects(&mut hunt_world, vec![Effect::Stop]).unwrap();
    assert_eq!(
        hunt_world
            .parts
            .actions_of(hunt_world.players[1].objects[ListIndex::new(0)].id)
            .top_action_kind(),
        None
    );
    let mut world = sent_off_world(vec![]);
    with_effects(
        &mut world,
        vec![Effect::Command(AiCommand::BuildObject(FoundationId::new(
            object_id(10028),
        )))],
    )
    .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, ids::BUILDER);
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .build()
            .map(|build_action| build_action.target.map(FoundationId::id)),
        Some(Some(object_id(10028)))
    );
    let mut world = sent_off_world(vec![]);
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: OrderPriority::of_data(3),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_kind: order_kind(80),
    };
    with_effects(&mut world, vec![Effect::OrderOnTarget(order)]).unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(players.id)
            .list
            .build()
            .map(|build_action| build_action.target.map(FoundationId::id)),
        Some(Some(object_id(10028)))
    );
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(players.id)
            .map(|ai| ai.order_priority.get()),
        Some(3)
    );
    let mut world = sent_off_world(vec![]);
    let twice = vec![Effect::OrderOnTarget(order), Effect::OrderOnTarget(order)];
    assert_eq!(with_effects(&mut world, twice), Ok(()));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .build()
            .map(|build_action| build_action.target.map(FoundationId::id)),
        Some(Some(object_id(10028)))
    );
    let mut world = sent_off_world(vec![]);
    world.players[1].objects[ListIndex::new(0)].life.master_id = master_id(4444);
    let walk = AiMove {
        point: Position::new(20.0, 20.0, 0.0),
        range: Distance::ZERO,
        force: OrderForce::Unforced,
        pace: Pace::Run,
    };
    assert!(with_effects(&mut world, vec![Effect::Move(walk)]).is_err());
    let mut world = sent_off_world(vec![]);
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    with_effects(&mut world, vec![Effect::RestorePatrol]).unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
}

/// A full update whose unit AI takes an order on a target (`0x2db`) runs the order and the
/// update's rest after it inside the one update (slot 0x230): the villager builds the
/// foundation with the order's priority within its update, the rest after the order runs (its
/// look-around check redraws the timeout past it), and the regeneration after the unit AI runs
/// once, as in an update without the order.
#[test]
fn an_order_on_target_and_the_rest_after_it_run_inside_the_one_update() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: OrderPriority::of_data(3),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_kind: order_kind(80),
    };
    let run = |orders| {
        let mut world = forage_world();
        let placed = *world.spatial.get(object_id(700)).unwrap();
        let site = Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: ClassId::of_master(80),
            master: ids::HOUSE,
            ..placed
        };
        world.spatial.insert(object_id(10028), site);
        world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .orders = orders;
        // the villager, the forager and the builder it becomes regenerate 20 a minute
        for masters in &mut world.model.masters {
            for id in [83, 118, 120] {
                if let Some(master) = Arc::make_mut(masters).get_mut(master_id(id)) {
                    (master.kind, master.hit_points) =
                        (MasterKind::COMBAT, MasterHitPoints::of_data(25));
                    master
                        .creatable
                        .get_or_insert_with(Default::default)
                        .regeneration = Regeneration::new(20.0);
                }
            }
        }
        let players = &mut world.players[1].objects[ListIndex::new(0)];
        players.life.hp = HitPoints::new(20.0);
        let ai = world.parts.unit_ai.get_mut(players.id).unwrap();
        (ai.idle_timer, ai.look_around_timer) = (Millis::new(5000), Millis::new(2000));
        world
            .update_objects(PlayerId::new(1), (Millis::new(50), Some(Millis::new(50))))
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let ai = world.parts.unit_ai.get(unit.id).unwrap();
        (
            world.parts.combat_of(unit.id).regen_timer,
            world
                .parts
                .actions_of(unit.id)
                .list
                .build()
                .map(|build_action| build_action.target),
            (
                ai.order_priority.get(),
                ai.orders.len(),
                ai.look_around_timer,
            ),
        )
    };
    let (idle_regen, idle_build, _) = run(vec![]);
    assert_eq!(idle_build, None);
    let (regen, build, (priority, queued, look)) = run(vec![order]);
    assert_eq!(
        (build, priority, queued),
        (Some(Some(FoundationId::new(object_id(10028)))), 3, 0)
    );
    assert_eq!(regen, idle_regen);
    assert!(
        look < Millis::new(2000),
        "the rest after the order ran its look-around check"
    );
}

/// The base handler's case 0x216 from the forager's side ([`UnitView::off_footprint`],
/// `FUN_1411566b0` with 0): with the forager's height and class, its player, the foundation by
/// its master and the Gather's bush as the action's target; without the foundation placed no
/// spot, placed at (30, 88) the flood's first empty tile beside its footprint, (30, 89).
///
/// [`UnitView::off_footprint`]: crate::unit_ai::world_view::UnitView::off_footprint
#[test]
fn unit_sent_off_foundation_finds_the_spot_beside_it() {
    let mut world = forage_world();
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let mut found = None;
    world
        .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            found = Some(side.off_footprint(
                FoundationId::new(object_id(10028)),
                Footprint::new(1.0, 1.0),
            )?);
            Ok(())
        })
        .unwrap();
    let off = found.unwrap();
    assert_eq!(
        (
            off.spot,
            off.class,
            off.player,
            off.player_kind.is_computer()
        ),
        (None, class::VILLAGER, player_id(1), false)
    );
    assert_eq!(
        off.site.map(|named| (named.id, named.master, named.owner)),
        Some((object_id(10028), master_id(70), player_id(1)))
    );
    assert_eq!(
        off.action_target
            .map(|named| (named.id, named.master, named.owner)),
        Some((object_id(600), master_id(59), player_id(0)))
    );
    world.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let mut found = None;
    world
        .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            found = Some(side.off_footprint(
                FoundationId::new(object_id(10028)),
                Footprint::new(1.0, 1.0),
            )?);
            Ok(())
        })
        .unwrap();
    assert_eq!(found.unwrap().spot, Some(Point::new(30.5, 89.5)));
}

/// The auto scout's search from the forager's side ([`UnitView::seek_unexplored`]): on a map
/// its player explored whole, neither the area about its town centre nor the unit AI's own
/// search (slot 0x90) finds a tile.
///
/// [`UnitView::seek_unexplored`]: crate::unit_ai::world_view::UnitView::seek_unexplored
#[test]
fn an_explored_map_leaves_the_auto_scout_nothing_to_seek() {
    let mut world = forage_world();
    let mut found = Some(None);
    world
        .run_unit_ai(
            (PlayerId::new(1), ListIndex::new(0)),
            |ai, _, side, _, rng| {
                found = Some(side.seek_unexplored(ai, rng)?);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(found, Some(None));
}

/// The unit AI's `ClosestResourceObject` from the forager's side (`FUN_140c49760`,
/// [`UnitView::closest_resource`]): of the bushes (gather group 2, gathering by themselves) the
/// nearer, 600; passing over its current target the other; from its town centre as the drop
/// site the one nearer the centre; none below the state asked for.
///
/// [`UnitView::closest_resource`]: crate::unit_ai::world_view::UnitView::closest_resource
#[test]
fn the_unit_ai_resource_search_finds_the_nearest_bush_it_reaches() {
    use crate::unit_ai::{NO_POINT, ResourceSearch};
    let mut forage_world = forage_world();
    for masters in &mut forage_world.model.masters {
        let bush = Arc::make_mut(masters).get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FISH);
        // the walk reaches the forager's line of sight plus 5 tiles: 601 lies 7 away
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .line_of_sight = Distance::new(4.0);
    }
    let search = |world: &mut World, resource_search: ResourceSearch| {
        let mut found = None;
        world
            .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
                found = Some(side.closest_resource(resource_search)?);
                Ok(())
            })
            .unwrap();
        found
            .unwrap()
            .map(|gatherable| (gatherable.id, gatherable.class))
    };
    let any = ResourceSearch {
        kind: GatherGroup::FISH,
        cap: WorkerCount::of_data(-1),
        site: None,
        current: None,
        point: NO_POINT,
        min_state: ObjectState::Alive,
    };
    assert_eq!(
        search(&mut forage_world, any),
        Some((object_id(600), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut forage_world,
            ResourceSearch {
                current: Some(object_id(600)),
                ..any
            }
        ),
        Some((object_id(601), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut forage_world,
            ResourceSearch {
                site: Some(DropSiteId::new(object_id(700))),
                ..any
            }
        ),
        Some((object_id(600), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut forage_world,
            ResourceSearch {
                site: Some(DropSiteId::new(object_id(4444))),
                ..any
            }
        ),
        Some((object_id(600), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut forage_world,
            ResourceSearch {
                min_state: ObjectState::Dying,
                ..any
            }
        ),
        None
    );
}

/// [`forage_world`] with player 1's mule cart 750 (master 1808: a building of a task of action
/// type 0x99 on class 7, berries) at (24.5, 89.5), moving and with a unit AI; the bushes gather
/// by themselves as group 1.
fn cart_world() -> World {
    let mut world = forage_world();
    for masters in &mut world.model.masters {
        let bush = Arc::make_mut(masters).get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FORAGE);
        let cart = Master {
            id: ids::MASTER_1808,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(0.5, 0.5),
            line_of_sight: Distance::new(4.0),
            action: Some(ActionPart {
                tasks: vec![Task {
                    action_kind: CART_TASK,
                    object_group: Some(class_id(7)),
                    ..Task::default()
                }],
                ..ActionPart::default()
            }),
            ..moving_master(walker().stats)
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([cart])));
    }
    let _ = world
        .push_live(
            PlayerId::new(1),
            Object::with_ai(
                Unit {
                    id: object_id(750),
                    life: Lifecycle {
                        state: ObjectState::Alive,
                        hp: HitPoints::new(100.0),
                        master_id: ids::MASTER_1808,
                        ..Lifecycle::default()
                    },
                    place: Static::at(Position::new(24.5, 89.5, 0.0)),
                    pasture: None,
                    my_pasture: None,
                    ..Unit::default()
                },
                UnitAi {
                    current_order: None,
                    current_action: None,
                    current_target: None,
                    ..UnitAi::default()
                },
            ),
        )
        .unwrap();
    world.set_motion(object_id(750), Some(Motion::Moving(walker())));
    world.spatial.insert(
        object_id(750),
        Placed {
            position: Position::new(24.5, 89.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::MASTER_1808,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world
}

/// The mule cart's own search (slot 0x1e8, `FUN_140c16240`): its task's class 7 names gather
/// group 1, searched from the cart as the drop site; the nearer bush by the footprints' gap
/// with its place. Without its movement state the search is not modelled; without a task of
/// a known class it finds none (its own place).
#[test]
fn mule_cart_finds_the_nearest_resource_of_its_task_class() {
    let mut world = cart_world();
    assert_eq!(
        world.cart_find(PlayerId::new(1), ListIndex::new(3)),
        Ok((Some(object_id(600)), Position::new(32.5, 89.5, 0.0)))
    );
    let mut world = cart_world();
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(ids::MASTER_1808)
        .unwrap()
        .action
        .get_or_insert_with(Default::default)
        .tasks[0]
        .object_group = Some(class_id(99));
    assert_eq!(
        world.cart_find(PlayerId::new(1), ListIndex::new(3)),
        Ok((None, Position::new(24.5, 89.5, 0.0)))
    );
    let mut world = cart_world();
    world
        .parts
        .put_mover(world.players[1].objects[ListIndex::new(3)].id, None);
    assert!(
        world
            .cart_find(PlayerId::new(1), ListIndex::new(3))
            .is_err()
    );
}

/// [`forage_world`] with the house's foundation 10028 placed (radii 1) and the forager standing
/// on its footprint at (30, 88.5).
fn footprint_world() -> World {
    let mut world = forage_world();
    world.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(30.0, 88.5, 0.0));
    world
        .spatial
        .set_position(
            object_id(9175),
            Position::new(30.0, 88.5, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    world
}

/// `FUN_14123bb20`'s sending off of the units on a new foundation's footprint: the idle forager
/// standing there hears notify 0x216 with the foundation master's radii in hundredths and the
/// foundation; a bush (without a unit AI), an object the world does not hold, a unit at work
/// on the foundation, one walking and one moving off already are passed over.
#[test]
fn the_units_on_new_foundation_are_sent_off_it() {
    let sent = |world: &World| {
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .notifies
            .iter()
            .filter(|notify| notify.kind == NotifyKind::SENT_OFF)
            .map(|notify| notify.params)
            .collect::<Vec<_>>()
    };
    let mut world = footprint_world();
    world
        .evacuate(
            FoundationId::new(object_id(10028)),
            &[object_id(9175), object_id(600), object_id(4444)],
        )
        .unwrap();
    assert_eq!(sent(&world), [[100, 100, 10028]]);
    let mut world = footprint_world();
    world.parts.set_anim_speed(
        world.players[1].objects[ListIndex::new(0)].id,
        Speed::new(1.0),
    );
    world
        .evacuate(FoundationId::new(object_id(10028)), &[object_id(9175)])
        .unwrap();
    assert_eq!(sent(&world), Vec::<[i32; 3]>::new());
    let mut world = footprint_world();
    world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .current_action = Some(AiAction::MoveAside);
    world
        .evacuate(FoundationId::new(object_id(10028)), &[object_id(9175)])
        .unwrap();
    assert_eq!(sent(&world), Vec::<[i32; 3]>::new());
    let mut world = footprint_world();
    world
        .order_work(
            PlayerId::new(1),
            ListIndex::new(0),
            &Work {
                target: Some(object_id(10028)),
                point: Position::new(30.0, 88.0, 0.0),
                disable_retargeting: false,
                filter: None,
            },
        )
        .unwrap();
    world
        .evacuate(FoundationId::new(object_id(10028)), &[object_id(9175)])
        .unwrap();
    assert_eq!(sent(&world), Vec::<[i32; 3]>::new());
    assert!(
        world
            .evacuate(FoundationId::new(object_id(4444)), &[object_id(9175)])
            .is_err()
    );
}

/// `FUN_14123bb20`'s owner's units of a formation category on the footprint go together
/// (`FUN_1411566b0` with 1, then `FUN_1416123a0`): the farthest free spot beside the footprint
/// from the first, and the forager the splitter leaves loose takes the action of moving off a
/// footprint (`0x2bf`) and moves there, so it hears no notify `0x21b`.
#[test]
fn formation_units_on_new_foundation_move_off_together() {
    let mut world = footprint_world();
    for masters in &mut world.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .creatable
            .get_or_insert_with(Default::default)
            .formation_category = FormationClass::of_data(1);
    }
    world
        .evacuate(FoundationId::new(object_id(10028)), &[object_id(9175)])
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(ai.current_action, Some(AiAction::MoveAside));
    assert!(
        ai.notifies
            .iter()
            .all(|notify| notify.kind != NotifyKind::SENT_OFF_GROUP)
    );
    assert!(world.parts.actions_of(unit.id).list.move_to().is_some());
}

/// A built castle's death outside its update (`FUN_1411dbb70` to state 3 from 2,
/// [`World::set_state`]): the population headroom its storage gave leaves its owner (mode 4),
/// the castle count (attribute 134) falls by one, not below 0, and it goes to state 3. A
/// computer player's building, or one with units inside, is not modelled.
#[test]
fn built_castle_death_takes_its_headroom_and_its_count() {
    let castle = Master {
        id: ids::CASTLE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        hit_points: MasterHitPoints::of_data(4800),
        storage: vec![(Some(attribute::HEADROOM), Amount::new(20.0), 4)],
        building: Some(BuildingMaster {
            tech: None,
            foundation_terrain: None,
            stack_unit: None,
            head_unit: None,
            transform_unit: None,
            annexes: [None; 4],
            ..BuildingMaster::default()
        }),
        ..Master::default()
    };
    let world = |count: f32| {
        let mut attributes = vec![Amount::ZERO; 250];
        attributes[4] = Amount::new(25.0);
        attributes[134] = Amount::new(count);
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player::default(),
                Player {
                    objects: vec![Unit {
                        id: object_id(900),
                        place: Static::at(Position::new(50.0, 50.0, 0.0)),
                        life: Lifecycle {
                            state: ObjectState::Alive,
                            hp: HitPoints::new(4800.0),
                            master_id: ids::CASTLE,
                            ..Lifecycle::default()
                        },
                        pasture: None,
                        my_pasture: None,
                        ..Unit::default()
                    }]
                    .into(),
                    attributes,
                    ..Player::default()
                },
            ],
        );
        world.model.masters = vec![
            Arc::new(Masters::default()),
            Arc::new(Masters::new([castle.clone()])),
        ];
        world
    };
    let mut game = world(1.0);
    game.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
        .unwrap();
    let player = &game.players[1];
    assert_eq!(
        (player.attributes[4], player.attributes[134]),
        (Amount::new(5.0), Amount::ZERO)
    );
    assert_eq!(
        player.objects[ListIndex::new(0)].life.state,
        ObjectState::Dying
    );
    let mut game = world(0.0);
    game.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
        .unwrap();
    assert_eq!(game.players[1].attributes[134], Amount::ZERO);
    let mut game = world(1.0);
    game.players[1].kind = PlayerKind::Computer;
    assert!(
        game.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
            .is_err()
    );
    let mut world = world(1.0);
    world.players[1].objects[ListIndex::new(0)].garrison = vec![object_id(5)];
    assert!(
        world
            .set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
            .is_err()
    );
}

/// The load's shape mesh (`RGE_Static_Object::rehook` per object): a standing house of a tall
/// master with an obstruction kind and class takes its shape; a dead one (state 7), one inside
/// another, a desolid foundation, an object the spatial table lacks and one whose master lacks
/// a kind or class take none.
#[test]
fn the_load_mesh_takes_the_shapes_of_the_standing_tall_objects() {
    let house = |id, state, inside, desolid: bool, master| {
        Object::with_building(
            Unit {
                id,
                place: Static::at(Position::new(10.5, 10.5, 0.0)),
                life: Lifecycle {
                    state,
                    master_id: master,
                    inside,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                construction: desolid.then_some(Construction {
                    progress: Seconds::ZERO,
                    desolid: true,
                    builders: BuilderCount::of_data(0),
                    hit_points: MasterHitPoints::of_data(550),
                    build_time: WholeSeconds::of_data(25),
                    record: Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::UNIT,
                    },
                    snow: Snow::None,
                    sprite_override: false,
                }),
                ..Default::default()
            },
        )
    };
    let split = World::split_layers(vec![
        house(object_id(1), ObjectState::Alive, None, false, ids::HOUSE),
        house(object_id(2), ObjectState::Removed, None, false, ids::HOUSE),
        house(
            object_id(3),
            ObjectState::Alive,
            Some(ContainerId::new(object_id(1))),
            false,
            ids::HOUSE,
        ),
        house(
            object_id(4),
            ObjectState::Foundation,
            None,
            true,
            ids::HOUSE,
        ),
        house(object_id(5), ObjectState::Alive, None, false, ids::HOUSE),
        house(object_id(6), ObjectState::Alive, None, false, master_id(71)),
    ]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: split.0.into(),
            ..Player::default()
        }],
    )
    .with_parts(split.1);
    let master = |id, obstruction_class| Master {
        id,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(1.0),
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class,
        ..Master::default()
    };
    world.model.masters = vec![Arc::new(Masters::new([
        master(ids::HOUSE, ObstructionClass::UNIT),
        master(master_id(71), ObstructionClass::NONE),
    ]))];
    world.spatial = Spatial::new(GridSize::new(40, 40));
    for id in [1, 2, 3, 4, 6] {
        world.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(10.5, 10.5, 0.0),
                radius: Footprint::new(1.0, 1.0),
                class: class::BUILDING,
                player: PlayerId::new(0),
                master: ids::HOUSE,
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    world.build_mesh().unwrap();
    let shaped: Vec<i32> = (1..=6)
        .filter(|&id| world.pathing.shape(object_id(id)).is_some())
        .collect();
    assert_eq!(shaped, [1]);
}

/// [`sent_off_world`] with its villager the hunter 122 (task 110 on class 9 within 0.5,
/// dropping at the town centre 109), Gaia's deer 500 and 501 (master 65, class 9) and player 1's
/// town centre 700, all placed on [`open_map`]; the villager can move.
pub(crate) fn hunt_world() -> World {
    let mut world = sent_off_world(vec![]);
    for masters in &mut world.model.masters {
        let hunter = Master {
            id: master_id(122),
            action: Some(ActionPart {
                drop_sites: vec![ids::TOWN_CENTRE],
                tasks: vec![Task {
                    action_kind: HUNT,
                    object_group: Some(class_id(9)),
                    work_range: Distance::new(0.5),
                    ..Task::default()
                }],
                ..masters
                    .get(VILLAGER_MALE)
                    .unwrap()
                    .clone()
                    .action
                    .clone()
                    .unwrap_or_default()
            }),
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let deer = Master {
            id: master_id(65),
            kind: MasterKind::COMBAT,
            class: class::PREY,
            radius: Footprint::new(0.4, 0.4),
            ..Master::default()
        };
        let centre = Master {
            id: ids::TOWN_CENTRE,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(
            masters.iter().cloned().chain([hunter, deer, centre]),
        ));
    }
    let deer = |id, x| Unit {
        id,
        place: Static::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(5.0),
            master_id: master_id(65),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    world.players[0].objects = vec![deer(object_id(500), 32.5), deer(object_id(501), 36.5)].into();
    world.reindex();
    let _ = world
        .push_live(
            PlayerId::new(1),
            Unit {
                id: object_id(700),
                place: Static::at(Position::new(20.0, 89.5, 0.0)),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(2400.0),
                    master_id: ids::TOWN_CENTRE,
                    ..Lifecycle::default()
                },
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
        )
        .unwrap();
    open_map(&mut world);
    let placed = |x: f32, class, player, master, radius| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius,
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(
        object_id(9175),
        placed(
            29.5,
            class::VILLAGER,
            PlayerId::new(1),
            master_id(122),
            Footprint::new(0.2, 0.2),
        ),
    );
    world.spatial.insert(
        object_id(500),
        placed(
            32.5,
            class::PREY,
            PlayerId::new(0),
            master_id(65),
            Footprint::new(0.4, 0.4),
        ),
    );
    world.spatial.insert(
        object_id(501),
        placed(
            36.5,
            class::PREY,
            PlayerId::new(0),
            master_id(65),
            Footprint::new(0.4, 0.4),
        ),
    );
    world.spatial.insert(
        object_id(700),
        placed(
            20.0,
            class::BUILDING,
            PlayerId::new(1),
            ids::TOWN_CENTRE,
            Footprint::new(2.0, 2.0),
        ),
    );
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.life.master_id = master_id(122);
    world.parts.put_mover(unit.id, Some(walker()));
    world
}

/// The work command on the other deer while hunting the first on the same task (the Hunt's slot
/// 9, `0x140ac97c0`): the Hunt takes it and starts over on it (its Attack walking there), the
/// first deer let go and the second taken; the unit AI hears the hunt order on it.
#[test]
fn work_command_on_another_deer_moves_the_hunt_there() {
    let mut world = hunt_world();
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let work = Work {
        target: Some(object_id(501)),
        point: Position::new(36.5, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let hunt_action = world
        .parts
        .actions_of(unit.id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.target,
            hunt_action.carry_to,
            hunt_action.target_class
        ),
        (
            Some(HuntTargetId::new(object_id(501))),
            None,
            Some(class::PREY)
        )
    );
    assert_eq!(
        (
            hunt_action.state.bare(),
            hunt_action
                .state
                .attack()
                .copied()
                .map(|attack| attack.target)
        ),
        (
            HuntState::Attack { attack: None },
            Some(Some(AttackTargetId::new(object_id(501))))
        )
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Hunt),
            Some(AiAction::Hunt),
            Some(object_id(501))
        )
    );
    assert_eq!(
        (
            world.players[0].objects[ListIndex::new(0)].workers,
            world.players[0].objects[ListIndex::new(1)].workers
        ),
        (WorkerCount::of_data(0), WorkerCount::of_data(1))
    );
}

/// A move toward an object while hunting (the Hunt's slot 0x40, `FUN_140ac52c0`): toward the
/// other deer it walks to the move's point (state 15) and lets the first go; loaded and moved
/// toward its town centre, a drop site, it carries the load there (state 8, the centre its second
/// target, taken).
#[test]
fn hunt_takes_move_toward_an_object_as_its_point_or_drop_site() {
    let mut world = hunt_world();
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let point = Position::new(36.0, 89.0, 0.0);
    assert_eq!(
        world.move_to_object(PlayerId::new(1), ListIndex::new(0), object_id(501), point,),
        Ok(Answer::Taken)
    );
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.target,
            hunt_action.carry_to,
            hunt_action.state.bare(),
            hunt_action.point
        ),
        (None, None, HuntState::WalkPoint { move_to: None }, point)
    );
    assert_eq!(
        (
            world.players[0].objects[ListIndex::new(0)].workers,
            world.players[0].objects[ListIndex::new(1)].workers
        ),
        (WorkerCount::of_data(0), WorkerCount::of_data(0))
    );
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    (unit.life.attribute_kind, unit.life.amount) = (Some(attribute::MEAT), Amount::new(6.0));
    assert_eq!(
        world.move_to_object(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
        ),
        Ok(Answer::Taken)
    );
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.target,
            hunt_action.carry_to,
            hunt_action.state.bare(),
            hunt_action.point,
            hunt_action.target_class
        ),
        (
            None,
            Some(DropSiteId::new(object_id(700))),
            HuntState::Drop { move_to: None },
            Position::from([-1.0; 3]),
            Some(class::BUILDING)
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
}

/// Runs `work` on object `list_index` of player `player_id`, its action layer and its `Step` as its
/// update has them (dt `dt`, outside a turn), the unit staying in its list, then what the `Step`
/// gathered ([`World::step_object`]): `work`'s answer, or the first error.
pub(crate) fn in_update<R>(
    world: &mut World,
    (player_id, list_index): (PlayerId, ListIndex),
    dt: u32,
    work: impl FnOnce(&mut Unit, &mut Action, &mut step::Step<'_>) -> GenieResult<R>,
) -> GenieResult<R> {
    let mut answer = None;
    let _ = world.step_object(
        (player_id, list_index),
        (Millis::new(dt), None, false),
        |unit, actions, step| {
            answer = Some(work(unit, actions, step)?);
            Ok(Outcome::Stay)
        },
    )?;
    Ok(answer.unwrap())
}

/// [`in_update`] whose `work` also says how the step ends (its [`Outcome`]): the list's loop then
/// files the object by it ([`World::file_step`]: a captured unit goes to its new owner).
pub(crate) fn in_update_filed<R>(
    world: &mut World,
    (player_id, list_index): (PlayerId, ListIndex),
    dt: u32,
    work: impl FnOnce(&mut Unit, &mut Action, &mut step::Step<'_>) -> GenieResult<(R, Outcome)>,
) -> GenieResult<R> {
    let mut answer = None;
    let stepped = world.step_object(
        (player_id, list_index),
        (Millis::new(dt), None, false),
        |unit, actions, step| {
            let (value, outcome) = work(unit, actions, step)?;
            answer = Some(value);
            Ok(outcome)
        },
    )?;
    let _ = world.file_step(player_id, stepped.slot, stepped.outcome)?;
    answer.ok_or(GenieError::unreachable("a step without its answer"))
}

/// [`hunt_world`]'s hunter on deer 500 (the hunt command's), its hunt then set by `set`.
fn hunter_in(set: impl FnOnce(&mut HuntAction)) -> World {
    let mut world = hunt_world();
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    set(world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap());
    world
}

/// The hunter's notifies of `kind` (their parameters).
fn notified(world: &World, kind: NotifyKind) -> Vec<[i32; 3]> {
    world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .notifies
        .iter()
        .filter(|notify| notify.kind == kind)
        .map(|notify| notify.params)
        .collect()
}

/// The hunter's part of the update in its hunt's state ([`Unit::hunt_part`]) with `dt`, the
/// hunter as a combat object and its lost tree as the update reads them.
fn hunt_part_of(world: &mut World, dt: u32) -> GenieResult<()> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        dt,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let mut own = unit.own_victim(actions, step, &roles())?;
            let mut hunt = *actions.list.hunt().unwrap();
            let tree_lost = unit.tree_lost(&hunt, step, &roles());
            let code = unit.hunt_part(
                (&mut hunt, actions),
                step,
                me,
                (&mut own, tree_lost),
                &roles(),
            )?;
            unit.end_head_update(actions, step, code)
        },
    )
}

/// The drop-off's wait (state 8, its timer below 0, `0x140ac7a00` case 7): the timer runs by
/// dt; at its end the drop site goes and `set_state(8)` searches again, finding the town centre
/// and walking there.
#[test]
fn hunter_drop_off_waits_then_searches_its_drop_site_again() {
    let wait = |timer| {
        hunter_in(|hunt_action| {
            (
                hunt_action.state,
                hunt_action.timer,
                hunt_action.target,
                hunt_action.carry_to,
            ) = (
                HuntState::Drop { move_to: None },
                timer,
                None,
                Some(DropSiteId::new(object_id(700))),
            );
        })
    };
    let mut world = wait(Seconds::new(-0.5));
    world.players[1].objects[ListIndex::new(2)].workers = WorkerCount::of_data(1);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (hunt_action.state.bare(), hunt_action.carry_to),
        (
            HuntState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(700)))
        )
    );
    assert!((hunt_action.timer + Seconds::new(0.45)).get().abs() < 1e-6);
    let mut world = wait(Seconds::new(-0.01));
    world.players[1].objects[ListIndex::new(0)]
        .life
        .attribute_kind = Some(attribute::MEAT);
    world.players[1].objects[ListIndex::new(0)].life.amount = Amount::new(5.0);
    world.players[1].objects[ListIndex::new(2)].workers = WorkerCount::of_data(1);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.state.bare(),
            hunt_action.carry_to,
            hunt_action.timer
        ),
        (
            HuntState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(700))),
            Seconds::ZERO
        )
    );
    // the centre let go, then taken again as the new drop site
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
}

/// The drop-off's walk (state 8, case 7): arrived at the town centre within a tile, where the
/// drop-site table takes meat, the hunter's load goes to its player as food and it carries
/// nothing; the hunt goes back to its deer (`set_state(4)`: the live deer's chase, state 12).
/// A failed walk waits 3
/// seconds and stops the animation; a centre that does not take the load is not modelled.
#[test]
fn hunter_drops_its_meat_at_the_town_centre_and_goes_back() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::{MoveTo, MoveToAction};
    let dropping = |move_to| {
        let mut hunter_in = hunter_in(|hunt_action| {
            (hunt_action.state, hunt_action.timer, hunt_action.carry_to) = (
                HuntState::Drop {
                    move_to: Some(MoveToAction::sub_in(move_to)),
                },
                Seconds::ZERO,
                Some(DropSiteId::new(object_id(700))),
            );
        });
        let unit = &mut hunter_in.players[1].objects[ListIndex::new(0)];
        (unit.life.attribute_kind, unit.life.amount) = (Some(attribute::MEAT), Amount::new(5.0));
        unit.place_for_test(Position::new(22.3, 89.5, 0.0));
        hunter_in
            .spatial
            .set_position(
                object_id(9175),
                Position::new(22.3, 89.5, 0.0),
                &roles(),
                tuning().visibility,
            )
            .unwrap();
        hunter_in.model.drop_sites = Arc::new(vec![DropSite {
            building: ids::TOWN_CENTRE,
            worker_class: Some(class::VILLAGER),
            targets: vec![DropTarget {
                class: class::PREY,
                workers: WorkerCount::of_data(-1),
                state: None,
                attribute: Some(attribute::MEAT),
            }],
            ..DropSite::default()
        }]);
        hunter_in
    };
    let mut world = dropping(MoveTo::Arrived);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(players.life.amount, Amount::ZERO);
    assert_eq!(world.players[1].attributes[0], Amount::new(5.0));
    let action = world
        .parts
        .actions_of(players.id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (action.state.bare(), action.target, action.carry_to),
        (
            HuntState::Attack { attack: None },
            Some(HuntTargetId::new(object_id(500))),
            None
        )
    );
    let mut world = dropping(MoveTo::Failed);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.anim_speed_of(unit.id), Some(Speed::ZERO));
    assert!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .hunt()
            .is_some_and(|hunt_action| hunt_action.timer < Seconds::ZERO)
    );
    let mut world = dropping(MoveTo::Arrived);
    Arc::make_mut(&mut world.model.drop_sites).clear();
    assert!(hunt_part_of(&mut world, 50).is_err());
}

/// The failed hunt (state 13, case 0xc): notify 0x1f9 with the hunt action; the hunt goes and
/// its deer, another player's, is let go after the update (its state holds no sub-action:
/// setting it dropped the Attack); without a hunt the part does nothing.
#[test]
fn failed_hunt_tells_its_unit_ai_and_lets_its_target_go() {
    let mut world = hunter_in(|hunt_action| {
        hunt_action.state = HuntState::Failed;
    });
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(1)
    );
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .copied(),
        None
    );
    assert_eq!(
        notified(&world, NotifyKind::ACTION_FAILED),
        [[AiAction::Hunt.value(), 0, 0]]
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
}

/// The search (state 3, case 2): the timer runs by dt; at its end it starts again at −10 s and
/// the unit AI hears notify 0x202 with the hunt action, the hunt's class (9) and its drop site,
/// the town centre the search finds. Without its target a lead (state 10, case 9) searches at
/// once; a done hunt (state 2) waits, but of a tree's class it is not modelled.
#[test]
fn searching_hunt_asks_its_unit_ai_again_after_ten_seconds() {
    use crate::gathering::hunt::HuntState;
    let search = [AiAction::Hunt.value(), 9, 700];
    let mut world = hunter_in(|hunt_action| {
        (hunt_action.state, hunt_action.timer) = (HuntState::Search, Seconds::new(-0.5));
    });
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(notified(&world, NotifyKind::REQUIRES_SEARCH), [[0; 3]; 0]);
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .timer = Seconds::new(-0.01);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(notified(&world, NotifyKind::REQUIRES_SEARCH), [search]);
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.timer),
        Some(SEARCH_AGAIN)
    );
    let mut world = hunter_in(|hunt_action| {
        (hunt_action.state, hunt_action.target) = (HuntState::Lead, None);
    });
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state),
        Some(HuntState::Search)
    );
    assert_eq!(notified(&world, NotifyKind::REQUIRES_SEARCH), [search]);
    let mut world = hunter_in(|hunt_action| {
        hunt_action.state = HuntState::Idle;
    });
    let before = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied();
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .copied(),
        before
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .target_class = Some(class::TREE);
    assert!(hunt_part_of(&mut world, 50).is_err());
}

/// The walks to a point (states 15 and 11): arrived, state 15 is done (`set_state(2)`: the
/// target goes and is let go) and state 11 searches (`set_state(3)`: notify 0x202); a waiting
/// walk asks again after its delay. Waiting after a given-up `MoveTo` without its target (state
/// 4, its timer below 0) the timer runs, then the walk to the point (state 11).
#[test]
fn hunt_walk_to_point_ends_done_or_in_search() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
    let walked = |state| {
        hunter_in(|hunt_action| {
            hunt_action.state = state;
            *hunt_action.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Arrived));
        })
    };
    let mut world = walked(HuntState::WalkPoint { move_to: None });
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            action.state.bare(),
            action.target,
            action.state.move_to().copied()
        ),
        (HuntState::Idle, None, None)
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
    let mut walked = walked(HuntState::WalkSearch { move_to: None });
    assert_eq!(hunt_part_of(&mut walked, 50), Ok(()));
    assert_eq!(
        walked
            .parts
            .actions_of(walked.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state.bare()),
        Some(HuntState::Search)
    );
    assert_eq!(notified(&walked, NotifyKind::REQUIRES_SEARCH).len(), 1);
    let mut world = hunter_in(|hunt_action| {
        hunt_action.state = HuntState::WalkPoint {
            move_to: Some(MoveToAction {
                wait: MoveWait {
                    delay: Seconds::new(1.0),
                    ..MoveWait::NEW
                },
                ..MoveToAction::sub_in(MoveTo::Waiting)
            }),
        };
    });
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            action.state.bare()
        ),
        (
            Some(MoveTo::Waiting),
            HuntState::WalkPoint { move_to: None }
        )
    );
    let mut world = hunter_in(|hunt_action| {
        (hunt_action.state, hunt_action.target, hunt_action.timer) =
            (HuntState::Walk { move_to: None }, None, Seconds::new(-0.01));
        hunt_action.point = Position::new(36.0, 89.5, 0.0);
    });
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state.bare()),
        Some(HuntState::WalkSearch { move_to: None })
    );
}

/// The hunt's strike (`0x140ac7a00`'s states 12 and 7, [`Unit::hunt_strike`]): the hunt
/// command's Attack on the live deer walks to it (state 4, its `MoveTo` moving on a path).
/// Gathering the dead deer (work rate 1 by work value 1 a second) the hunter gains 0.05 in
/// 50 ms and the deer loses it; with less room than that it is full and carries its load to the
/// town centre (state 8) without taking it.
#[test]
fn hunter_strikes_its_live_deer_and_gathers_the_dead_one_until_full() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::MoveTo;
    let mut game = hunter_in(|_| {});
    assert_eq!(hunt_part_of(&mut game, 50), Ok(()));
    let players = &game.players[1].objects[ListIndex::new(0)];
    let action = game
        .parts
        .actions_of(players.id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            action.state.bare(),
            action.state.attack().copied().map(|attack| (
                attack.state.bare(),
                attack
                    .state
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state)
            ))
        ),
        (
            HuntState::Attack { attack: None },
            Some((AttackState::Move { move_to: None }, Some(MoveTo::Moving)))
        )
    );
    assert!(
        game.parts
            .mover_of(players.id)
            .is_some_and(|mover| !mover.paths.is_empty())
    );
    let gathering = |held: f32| {
        let mut world = hunter_in(|hunt_action| {
            hunt_action.state = HuntState::Gather;
            (hunt_action.task.work_value, hunt_action.task.resource) =
                (WorkValue::new(1.0), Some(attribute::MEAT));
        });
        for masters in &mut world.model.masters {
            let hunter = Arc::make_mut(masters).get_mut(master_id(122)).unwrap();
            (
                hunter.capacity,
                hunter.action.get_or_insert_with(Default::default).work_rate,
            ) = (CarryCapacity::of_data(10), WorkRate::new(1.0));
        }
        let deer = &mut world.players[0].objects[ListIndex::new(0)];
        (deer.life.state, deer.life.hp) = (ObjectState::Dying, HitPoints::ZERO);
        (deer.life.attribute_kind, deer.life.amount) = (Some(attribute::MEAT), Amount::new(100.0));
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        (unit.life.attribute_kind, unit.life.amount) = (Some(attribute::MEAT), Amount::new(held));
        unit.place_for_test(Position::new(31.9, 89.5, 0.0));
        world
            .spatial
            .set_position(
                object_id(9175),
                Position::new(31.9, 89.5, 0.0),
                &roles(),
                tuning().visibility,
            )
            .unwrap();
        world
    };
    let mut world = gathering(5.0);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let (gained, left) = (
        world.players[1].objects[ListIndex::new(0)].life.amount,
        world.players[0].objects[ListIndex::new(0)].life.amount,
    );
    assert!(
        (gained - Amount::new(5.05)).abs() < Amount::new(1e-5)
            && (left - Amount::new(99.95)).abs() < Amount::new(1e-4)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state.bare()),
        Some(HuntState::Gather)
    );
    let mut world = gathering(9.99);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (hunt_action.state.bare(), hunt_action.carry_to),
        (
            HuntState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(700)))
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].life.amount,
        Amount::new(9.99)
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].life.amount,
        Amount::new(100.0)
    );
}

/// A hunter's work on its dead deer reaches the deer where it ran (the hit's write-back,
/// [`crate::combat::victims::write_back`]): in the same update the deer holds 0.05 less.
#[test]
fn hunter_work_reaches_its_deer_in_the_same_update() {
    let mut world = hunter_in(|hunt_action| {
        hunt_action.state = HuntState::Gather;
        (hunt_action.task.work_value, hunt_action.task.resource) =
            (WorkValue::new(1.0), Some(attribute::MEAT));
    });
    for masters in &mut world.model.masters {
        let hunter = Arc::make_mut(masters).get_mut(master_id(122)).unwrap();
        (
            hunter.capacity,
            hunter.action.get_or_insert_with(Default::default).work_rate,
        ) = (CarryCapacity::of_data(10), WorkRate::new(1.0));
    }
    let deer = &mut world.players[0].objects[ListIndex::new(0)];
    (deer.life.state, deer.life.hp) = (ObjectState::Dying, HitPoints::ZERO);
    (deer.life.attribute_kind, deer.life.amount) = (Some(attribute::MEAT), Amount::new(100.0));
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    (players.life.attribute_kind, players.life.amount) = (Some(attribute::MEAT), Amount::new(5.0));
    players.place_for_test(Position::new(31.9, 89.5, 0.0));
    world
        .spatial
        .set_position(
            object_id(9175),
            Position::new(31.9, 89.5, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let mut own = unit.own_victim(actions, step, &roles())?;
            let mut hunt = *actions.list.hunt().unwrap();
            let tree_lost = unit.tree_lost(&hunt, step, &roles());
            let code = unit.hunt_part(
                (&mut hunt, actions),
                step,
                me,
                (&mut own, tree_lost),
                &roles(),
            )?;
            unit.end_head_update(actions, step, code)?;
            Ok(
                OthersRef::of(&step.world.players, &step.world.index, step.player)
                    .unwrap()
                    .find(object_id(500))
                    .map(|(_, deer)| deer.life.amount),
            )
        },
    );
    assert!(
        left.as_ref().is_ok_and(|load| load
            .is_some_and(|amount| (amount - Amount::new(99.95)).abs() < Amount::new(1e-4))),
        "{left:?}"
    );
}

/// A gathering hunt (state 7) whose deer left the world drops it at its update's start and
/// ends without a target in the same update: holding nothing, it searches (state 3), reading no
/// target.
#[test]
fn gathering_hunt_whose_target_left_searches_without_reading_one() {
    use crate::gathering::hunt::HuntState;
    let mut world = hunter_in(|hunt_action| hunt_action.state = HuntState::Gather);
    let _ = world
        .remove_live(PlayerId::new(0), ListIndex::new(0))
        .unwrap();
    let _ = world.spatial.leave(object_id(500));
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let mut hunt = *actions.list.hunt().unwrap();
            unit.hunt_targets(&mut hunt, step)?;
            actions.list.set_hunt(Some(hunt));
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let mut own = unit.own_victim(actions, step, &roles())?;
            let code =
                unit.hunt_part((&mut hunt, actions), step, me, (&mut own, false), &roles())?;
            unit.end_head_update(actions, step, code)
        },
    );
    assert_eq!(in_update, Ok(()));
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (hunt_action.state, hunt_action.target),
        (HuntState::Search, None)
    );
}

/// [`hunt_world`] with player 1's own sheep 510 (master 594, class 58, moving) at `x` beside
/// the hunter, its hunter's task on livestock, the drop-site table's town centre taking
/// livestock; the hunt command's hunt faces it (state 4, arrived).
fn shepherd_world(x: f32) -> World {
    use crate::movement::move_to::{MoveTo, MoveToAction};
    let mut world = hunt_world();
    for masters in &mut world.model.masters {
        let sheep = Master {
            id: master_id(594),
            kind: MasterKind::COMBAT,
            class: class::LIVESTOCK,
            radius: Footprint::new(0.2, 0.2),
            ..moving_master(walker().stats)
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([sheep])));
        Arc::make_mut(masters)
            .get_mut(master_id(122))
            .unwrap()
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .push(Task {
                action_kind: HUNT,
                object_group: Some(class_id(58)),
                work_range: Distance::new(0.5),
                ..Task::default()
            });
    }
    let _ = world
        .push_live(
            PlayerId::new(1),
            Unit {
                id: object_id(510),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(7.0),
                    master_id: master_id(594),
                    attribute_kind: Some(attribute::MEAT),
                    amount: Amount::new(100.0),
                    ..Lifecycle::default()
                },
                place: Static::at(Position::new(x, 89.5, 0.0)),
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
        )
        .unwrap();
    world.set_motion(object_id(510), Some(Motion::Moving(walker())));
    world.spatial.insert(
        object_id(510),
        Placed {
            position: Position::new(x, 89.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::LIVESTOCK,
            player: PlayerId::new(1),
            master: master_id(594),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world.model.drop_sites = Arc::new(vec![DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        livestock: true,
        ..DropSite::default()
    }]);
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(510))
        .unwrap();
    let hunt_action = world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap();
    *hunt_action.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Arrived));
    world
}

/// A hunt facing its own live sheep (`set_state(6)` on livestock with its herd,
/// [`Unit::herd_for`]): the sheep stands more than 3 tiles from the drop-site table's nearest
/// town centre, so the hunter leads it there (state 10, the centre its drop site) and the sheep
/// walks; the lead goes on in the next update. A sheep beside the centre is slaughtered at once
/// (state 12, the Attack).
#[test]
fn hunter_leads_its_far_sheep_to_the_town_centre_and_slaughters_near_one() {
    use crate::gathering::hunt::HuntState;
    let mut world = shepherd_world(30.2);
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (action.state.bare(), action.target, action.drop_site),
        (
            HuntState::Lead,
            Some(HuntTargetId::new(object_id(510))),
            Some(DropSiteId::new(object_id(700)))
        )
    );
    assert!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(3)].id)
            .list
            .move_to()
            .is_some()
    );
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state.bare()),
        Some(HuntState::Lead)
    );
    let mut world = shepherd_world(22.6);
    world.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(23.2, 89.5, 0.0));
    world
        .spatial
        .set_position(
            object_id(9175),
            Position::new(23.2, 89.5, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(hunt_part_of(&mut world, 50), Ok(()));
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.state.bare(),
            hunt_action
                .state
                .attack()
                .copied()
                .map(|attack| attack.target)
        ),
        (
            HuntState::Attack { attack: None },
            Some(Some(AttackTargetId::new(object_id(510))))
        )
    );
}

/// The lead is the Hunt's own call (`0x140ac60e0` case 6, `FUN_140ac4c60` before
/// `set_state(10)`): the far sheep has its `MoveTo` to the town centre while the hunter's
/// update still runs, not after it.
#[test]
fn hunter_sheep_walks_before_the_hunter_update_ends() {
    let mut world = shepherd_world(30.2);
    let walks = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let mut own = unit.own_victim(actions, step, &roles())?;
            let mut hunt = *actions.list.hunt().unwrap();
            let tree_lost = unit.tree_lost(&hunt, step, &roles());
            let _ = unit.hunt_part(
                (&mut hunt, actions),
                step,
                me,
                (&mut own, tree_lost),
                &roles(),
            )?;
            Ok(step
                .world
                .parts
                .actions_of(step.world.players[1].objects[ListIndex::new(3)].id)
                .list
                .move_to()
                .is_some())
        },
    );
    assert_eq!(walks, Ok(true));
}

/// [`hunt_world`] with Gaia's trees 520 and 521 (master 349, class 15, 100 wood, gathered by
/// themselves as group 0) at x `near` and 40, the hunter's task on trees, its unit AI targeting
/// class 15 and its hunt without a target at its own place.
fn lumber_world(near: f32) -> World {
    let mut world = hunt_world();
    for masters in &mut world.model.masters {
        let tree = Master {
            id: master_id(349),
            kind: MasterKind::STATIC,
            class: class::TREE,
            radius: Footprint::new(0.5, 0.5),
            auto_gather: AutoGatherMode::ON,
            gather_group: GatherGroup::WOOD,
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([tree])));
        let hunter = Arc::make_mut(masters).get_mut(master_id(122)).unwrap();
        hunter.line_of_sight = Distance::new(4.0);
        hunter
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .push(Task {
                action_kind: HUNT,
                object_group: Some(class_id(15)),
                work_range: Distance::new(0.5),
                ..Task::default()
            });
    }
    let tree = |id, x: f32| Unit {
        id,
        place: Static::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(20.0),
            master_id: master_id(349),
            attribute_kind: Some(attribute::WOOD),
            amount: Amount::new(100.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    world
        .extend_live(
            PlayerId::new(0),
            [tree(object_id(520), near), tree(object_id(521), 40.0)],
        )
        .unwrap();
    for (id, x) in [(520, near), (521, 40.0)] {
        world.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(x, 89.5, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::TREE,
                player: PlayerId::new(0),
                master: master_id(349),
                obstruction_kind: ObstructionKind::NONE,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    world.spatial.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([3, 0])]);
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    let here = unit.position();
    let hunt_action = world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap();
    (
        hunt_action.target,
        hunt_action.target_class,
        hunt_action.point,
    ) = (None, Some(class::TREE), here);
    // the attack's Attack goes, the state stays
    if let Some(attack) = hunt_action.state.attack_mut() {
        *attack = None;
    }
    world.parts.unit_ai.get_mut(unit.id).unwrap().target_class = Some(class::TREE);
    world
}

/// [`lumber_world`] with its tree 521 asleep and a Gaia foundation 530 (state 0) of a building
/// master with a stack unit (master 99, which the game never uses) that is an annex of tree
/// 520.
fn lets_world() -> World {
    let mut world = lumber_world(31.5);
    let asleep = world
        .swap_remove_live(PlayerId::new(0), ListIndex::new(3))
        .unwrap();
    let _ = world.push_asleep(PlayerId::new(0), asleep).unwrap();
    for masters in &mut world.model.masters {
        let annex = Master {
            id: master_id(99),
            kind: MasterKind::BUILDING,
            building: Some(BuildingMaster {
                stack_unit: Some(master_id(5)),
                ..BuildingMaster::default()
            }),
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([annex])));
    }
    let _ = world
        .push_live(
            PlayerId::new(0),
            Object::with_building(
                Unit {
                    id: object_id(530),
                    life: Lifecycle {
                        state: ObjectState::Foundation,
                        master_id: master_id(99),
                        ..Lifecycle::default()
                    },
                    pasture: None,
                    my_pasture: None,
                    ..Unit::default()
                },
                Building {
                    annex_of: Some(MainBuildingId::new(object_id(520))),
                    ..Default::default()
                },
            ),
        )
        .unwrap();
    world
}

/// An update's take through its view (slot 0x178, [`crate::world::lets::take`]): a live tree
/// counts its worker; a sleeping one counts its worker and wakes, last in its player's live
/// list and tracker; an unallied player's foundation with a stack unit lists its taker, and
/// Gaia's own taker is allied; one the world does not hold is not taken; the updating unit
/// itself counts its worker at once, within its update.
#[test]
fn an_update_take_counts_its_worker_and_wakes_sleeping_object() {
    use crate::world::lets::take;
    let mut lets_world = lets_world();
    let gaia = Releaser {
        id: object_id(500),
        class: Some(class::PREY),
        player: PlayerId::new(0),
    };
    let own = in_update(
        &mut lets_world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, _, step| {
            let by = unit.releaser(step)?;
            for id in [520, 521, 530, 9999] {
                take(step, unit, object_id(id), by, &roles()).unwrap();
            }
            let before = unit.workers.get();
            take(step, unit, unit.id, by, &roles()).unwrap();
            take(step, unit, object_id(530), gaia, &roles()).unwrap();
            Ok((before, unit.workers.get()))
        },
    );
    assert_eq!(own, Ok((0, 1)));
    let workers =
        |world: &World, id: ObjectId| world.object(id).map(|(_, unit)| unit.workers.get());
    assert_eq!(workers(&lets_world, object_id(520)), Some(1));
    assert_eq!(workers(&lets_world, object_id(521)), Some(1));
    assert_eq!(
        lets_world.players[0]
            .objects
            .iter()
            .last()
            .map(|unit| unit.id.get()),
        Some(521)
    );
    assert!(
        lets_world.players[0]
            .sleeping
            .by_id(object_id(521))
            .is_none()
    );
    let key = key(
        lets_world.model.masters.first().map(|masters| &**masters),
        &lets_world.model.lines,
        master_id(349),
        &roles(),
    );
    assert_eq!(
        lets_world.players[0]
            .tracker
            .list(ListKey::of_master(key.master))
            .last(),
        Some(&Some(object_id(521)))
    );
    let villager = lets_world.players[1].objects[ListIndex::new(0)].id;
    assert_eq!(workers(&lets_world, villager), Some(1));
    let annex = lets_world.object(object_id(530)).map(|(_, unit)| {
        (
            unit.workers.get(),
            lets_world
                .parts
                .building_of(unit.id)
                .attacking_units
                .clone(),
        )
    });
    assert_eq!(annex, Some((2, vec![villager])));
}

/// An update's release through its view (slot 0x180, [`crate::world::lets::release`]): a live
/// tree no one works on any more goes to sleep; a sleeping one counts a worker less and stays
/// asleep; one the world does not hold is not let go; the updating unit itself counts a worker
/// less at once, within its update. Then a counted builder's leave
/// ([`crate::world::lets::Lets::leave_builders`]): an object counts one builder less, an annex's
/// main building for the annex, the updating unit itself at once; the teardown's pair applies
/// leaves before releases ([`crate::world::lets::torn_down`]).
#[test]
fn an_update_release_and_builder_leave_change_the_objects_at_once() {
    use crate::world::lets::{leave_builders, release, torn_down};
    let mut lets_world = lets_world();
    for id in [520, 521] {
        let unit = lets_world.object_mut(object_id(id)).unwrap().1;
        unit.workers = WorkerCount::of_data(2);
        lets_world.parts.combat_mut(object_id(id)).unwrap().builders = BuilderCount::of_data(3);
    }
    let own = in_update(
        &mut lets_world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, _, step| {
            let by = unit.releaser(step)?;
            unit.workers = WorkerCount::ONE;
            *step.world.parts.repairers_mut(unit.id)? = BuilderCount::of_data(1);
            for id in [520, 521, 9999] {
                release(step, unit, object_id(id), by)?;
            }
            release(step, unit, unit.id, by)?;
            leave_builders(step, unit, object_id(520))?;
            leave_builders(step, unit, object_id(530))?;
            leave_builders(step, unit, unit.id)?;
            torn_down(
                step,
                unit,
                (vec![object_id(521)], vec![(object_id(520), by)]),
            )?;
            Ok((
                unit.workers.get(),
                step.world.parts.repairers_of(unit.id).get(),
            ))
        },
    );
    assert_eq!(own, Ok((0, 0)));
    let tree = |world: &World, id| {
        world.object(object_id(id)).map(|(_, unit)| {
            (
                unit.workers.get(),
                world.parts.combat_of(unit.id).builders.get(),
                unit.life.goto_sleep,
            )
        })
    };
    assert_eq!(tree(&lets_world, 520), Some((0, 1, true)));
    assert_eq!(tree(&lets_world, 521), Some((1, 2, false)));
    assert!(
        lets_world.players[0]
            .sleeping
            .by_id(object_id(521))
            .is_some()
    );
}

/// Outside an update a take, a release and a builder's leave run the update's calls on a view
/// of the whole world ([`World::with_lets`]): the sleeping tree wakes to the end of Gaia's live
/// list and tracker, a release lets it sleep again, and the annex's main building counts a
/// builder less.
#[test]
fn outside_an_update_the_lets_run_on_the_whole_world() {
    let mut world = lets_world();
    let by = Releaser {
        id: object_id(world.players[1].objects[ListIndex::new(0)].id.get()),
        class: Some(class::VILLAGER),
        player: PlayerId::new(1),
    };
    world.take(object_id(521), by).unwrap();
    assert_eq!(
        world.players[0]
            .objects
            .iter()
            .last()
            .map(|unit| unit.id.get()),
        Some(521)
    );
    let key = key(
        world.model.masters.first().map(|masters| &**masters),
        &world.model.lines,
        master_id(349),
        &roles(),
    );
    assert_eq!(
        world.players[0]
            .tracker
            .list(ListKey::of_master(key.master))
            .last(),
        Some(&Some(object_id(521)))
    );
    assert_eq!(
        world
            .object(object_id(521))
            .map(|(_, unit)| unit.workers.get()),
        Some(1)
    );
    world.release(object_id(521), by).unwrap();
    let tree = world
        .object(object_id(521))
        .map(|(_, unit)| (unit.workers.get(), unit.life.goto_sleep));
    assert_eq!(tree, Some((0, true)));
    world.parts.combat_mut(object_id(520)).unwrap().builders = BuilderCount::of_data(2);
    world.leave_builders(object_id(530)).unwrap();
    assert_eq!(
        world
            .object(object_id(520))
            .map(|(_, unit)| world.parts.combat_of(unit.id).builders.get()),
        Some(1)
    );
}

/// A player's live object out of its slot ([`World::take_out_own`]) is out of the world's
/// lookups as an updating one is, its stand-in in its place, until it goes back
/// ([`World::put_back_own`]); the sleeping object wakes once ([`World::wake_sleeper`]).
#[test]
fn an_own_object_taken_out_leaves_the_world_until_it_goes_back() {
    let mut world = lets_world();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    assert!(
        world.take_out_own(PlayerId::new(0), id).unwrap().is_none(),
        "another player's"
    );
    let mut taken_unit = world
        .take_out_own(PlayerId::new(1), id)
        .unwrap()
        .expect("the owner's");
    assert_eq!(taken_unit.id, id);
    assert!(world.object(id).is_none() && world.locate(id).is_none());
    assert_eq!(world.players[1].objects[ListIndex::new(0)].id, TAKEN_OUT);
    taken_unit.life.hp = HitPoints::new(8.0);
    assert_eq!(
        world.put_back_own(PlayerId::new(1), taken_unit).ok(),
        Some(ListIndex::new(0))
    );
    assert_eq!(
        world.locate(id).map(|(player_id, unit, sleeping)| (
            player_id,
            unit.life.hp.get(),
            sleeping
        )),
        Some((PlayerId::new(1), 8.0, false))
    );
    assert_eq!(
        world
            .locate(object_id(521))
            .map(|(player_id, _, sleeping)| (player_id, sleeping)),
        Some((PlayerId::new(0), true))
    );
    assert_eq!(world.wake_sleeper(object_id(521)), Ok(Wake::Woke));
    assert_eq!(
        world.wake_sleeper(object_id(521)),
        Ok(Wake::NotAsleep),
        "awake already"
    );
    assert_eq!(
        world
            .locate(object_id(521))
            .map(|(player_id, _, sleeping)| (player_id, sleeping)),
        Some((PlayerId::new(0), false))
    );
}

/// The unit AI's side's release of the unit itself ([`crate::world::lets::Lets::release`] on
/// the side's unit): the unit counts its worker less at once, and the notify 699 for its own AI,
/// which the side does not hold while it runs, reaches the AI as the side's call returns.
#[test]
fn side_own_release_tells_the_unit_ai_as_the_side_returns() {
    use crate::unit_ai::world_view::OwnerView;
    let mut world = hunt_world();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let ai = world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    (ai.attackers, ai.notifies) = (Attackers::of_saved(&[id.get()]), Vec::new());
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            unit.workers = WorkerCount::ONE;
            let by = unit.releaser(step)?;
            let view = OwnerView::of(step, None).unwrap();
            let attacker = unit.attacker_of(
                actions,
                UnitAiKind::Soldier,
                None,
                (PlayerId::new(1), PlayerKind::Human),
                GameMode::RandomMap,
                (
                    CaptureKind::None,
                    Firepower::ZERO,
                    step.world
                        .parts
                        .mover_of(unit.id)
                        .map_or(Speed::ZERO, |mover| mover.stats.speed),
                ),
            );
            let inside = unit.with_side(
                actions,
                step,
                &view,
                attacker,
                (PlayerId::new(1), PlayerKind::Human),
                |ai, side, _| {
                    side.release(by.id, by)?;
                    Ok((side.unit.workers.get(), ai.notifies.len()))
                },
            )?;
            let after = step
                .world
                .parts
                .unit_ai
                .get(unit.id)
                .map(|ai| ai.notifies.len());
            Ok((inside, after))
        },
    );
    assert_eq!(seen, Ok(((0, 0), Some(1))));
    assert_eq!(notified(&world, NotifyKind::NO_LONGER_ON_ME).len(), 1);
}

/// The alert object (`+0x1bc`) with the alert mode on: a soldier's slot 0x1d8
/// (`FUN_140c1c6e0`) names the last search's nearest enemy whose reload time (slot 0x218) is not
/// 0, so deer 500, whose powerup takes its 2 s off, gives way to deer 501; any other factory
/// type's names none (the base's −1); with the alert mode off it stays.
#[test]
fn alert_watch_names_the_nearest_armed_enemy() {
    use crate::players::ByRelation;
    use crate::unit_ai::world_view::OwnerView;
    use crate::unit_ai::{AlertMode, UnitAi, UnitAiKind};
    use crate::vision::registry::Record;
    let mut world = hunt_world();
    Arc::make_mut(&mut world.model.masters[0])
        .get_mut(master_id(65))
        .unwrap()
        .combat
        .get_or_insert_with(Default::default)
        .reload = Seconds::new(2.0);
    world
        .parts
        .combat_mut(object_id(500))
        .unwrap()
        .reload_powerup = Seconds::new(2.0);
    let enemy = |id: i32, x: f32| Record {
        id: object_id(id),
        distance: Distance::new(f32::MAX),
        owner: PlayerId::GAIA,
        at: Point::new(x, 89.5),
        radius: Footprint::new(0.4, 0.4),
    };
    world.spatial.set_searched(ByRelation::new([
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![enemy(500, 32.5), enemy(501, 36.5)],
    ]));
    let watched = |world: &mut World, kind: UnitAiKind, mode: AlertMode| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let view = OwnerView::of(step, None).unwrap();
                let attacker = unit.attacker_of(
                    actions,
                    UnitAiKind::Soldier,
                    None,
                    (PlayerId::new(1), PlayerKind::Human),
                    GameMode::RandomMap,
                    (CaptureKind::None, Firepower::ZERO, Speed::ZERO),
                );
                unit.with_side(
                    actions,
                    step,
                    &view,
                    attacker,
                    (PlayerId::new(1), PlayerKind::Human),
                    |ai: &mut UnitAi, side, _| {
                        (ai.ai_kind, ai.alert_mode, ai.alert_object) =
                            (kind, mode, Some(object_id(7)));
                        ai.watch_alert(side)?;
                        Ok(ai.alert_object)
                    },
                )
            },
        )
    };
    assert_eq!(
        watched(&mut world, UnitAiKind::Soldier, AlertMode::Alert),
        Ok(Some(object_id(501)))
    );
    assert_eq!(
        watched(&mut world, UnitAiKind::Siege, AlertMode::Alert),
        Ok(None)
    );
    assert_eq!(
        watched(&mut world, UnitAiKind::Soldier, AlertMode::Calm),
        Ok(Some(object_id(7)))
    );
}

/// Slot 0x3d0 of an object that does not move (types 10 to 25, `FUN_141071b50`) answers 0: the
/// unit AI's side of a unit without a mover reaches no object, the deer 3 tiles away included.
#[test]
fn side_without_mover_reaches_nothing() {
    use crate::unit_ai::world_view::OwnerView;
    let mut world = hunt_world();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    world.set_motion(id, None);
    let reached = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let view = OwnerView::of(step, None).unwrap();
            let attacker = unit.attacker_of(
                actions,
                UnitAiKind::Soldier,
                None,
                (PlayerId::new(1), PlayerKind::Human),
                GameMode::RandomMap,
                (CaptureKind::None, Firepower::ZERO, Speed::ZERO),
            );
            unit.with_side(
                actions,
                step,
                &view,
                attacker,
                (PlayerId::new(1), PlayerKind::Human),
                |_, side, _| side.reaches(object_id(500), Distance::new(4.0), None),
            )
        },
    );
    assert_eq!(reached, Ok(false));
}

/// A notify for the updating unit's own AI from a call of its step ([`Step::notify_own`]):
/// posted at once while the AI is in the world's table; held while the AI's side runs, the AI
/// out of the table, and posted as the side returns.
///
/// [`Step::notify_own`]: crate::world::step::Step::notify_own
#[test]
fn step_own_notify_waits_for_the_unit_ai_side_to_return() {
    use crate::unit_ai::Notify;
    use crate::unit_ai::world_view::OwnerView;
    let mut world = hunt_world();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let ai = world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    (ai.attackers, ai.notifies) = (Attackers::of_saved(&[id.get()]), Vec::new());
    let notify = Notify::own(id, NotifyKind::NO_LONGER_ON_ME, [1, 2, 3]);
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            step.notify_own(unit, notify)?;
            let at_once = step
                .world
                .parts
                .unit_ai
                .get(unit.id)
                .map(|ai| ai.notifies.len());
            let view = OwnerView::of(step, None).unwrap();
            let attacker = unit.attacker_of(
                actions,
                UnitAiKind::Soldier,
                None,
                (PlayerId::new(1), PlayerKind::Human),
                GameMode::RandomMap,
                (
                    CaptureKind::None,
                    Firepower::ZERO,
                    step.world
                        .parts
                        .mover_of(unit.id)
                        .map_or(Speed::ZERO, |mover| mover.stats.speed),
                ),
            );
            let inside = unit.with_side(
                actions,
                step,
                &view,
                attacker,
                (PlayerId::new(1), PlayerKind::Human),
                |ai, side, _| {
                    side.step.notify_own(side.unit, notify)?;
                    Ok(ai.notifies.len())
                },
            )?;
            let after = step
                .world
                .parts
                .unit_ai
                .get(unit.id)
                .map(|ai| ai.notifies.len());
            Ok((at_once, inside, after))
        },
    );
    assert_eq!(seen, Ok((Some(1), 1, Some(2))));
    assert_eq!(
        notified(&world, NotifyKind::NO_LONGER_ON_ME),
        vec![[1, 2, 3]; 2]
    );
}

/// The AI stream is the unit AI's while it runs on its side ([`Unit::with_side`]): its draws are
/// the world's stream after the run, and a world routine that draws from it meanwhile fails the
/// run.
#[test]
fn the_ai_stream_is_the_unit_ai_while_its_side_runs() {
    use crate::unit_ai::world_view::OwnerView;
    let mut hunt_world = hunt_world();
    let mut drawn = hunt_world.streams.get_mut(Stream::Ai).clone();
    let first = drawn.rand15();
    let run = |world: &mut World, world_draws: bool| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let view = OwnerView::of(step, None).unwrap();
                let attacker = unit.attacker_of(
                    actions,
                    UnitAiKind::Soldier,
                    None,
                    (PlayerId::new(1), PlayerKind::Human),
                    GameMode::RandomMap,
                    (
                        CaptureKind::None,
                        Firepower::ZERO,
                        step.world
                            .parts
                            .mover_of(unit.id)
                            .map_or(Speed::ZERO, |mover| mover.stats.speed),
                    ),
                );
                unit.with_side(
                    actions,
                    step,
                    &view,
                    attacker,
                    (PlayerId::new(1), PlayerKind::Human),
                    |_, side, rng| {
                        if world_draws {
                            let _ = side.step.world.streams.get_mut(Stream::Ai).rand15();
                        }
                        Ok(rng.rand15())
                    },
                )
            },
        )
    };
    assert_eq!(run(&mut hunt_world, false), Ok(first));
    assert_eq!(*hunt_world.streams.get_mut(Stream::Ai), drawn);
    let failed = run(&mut hunt_world, true).expect_err("a draw beside the unit AI's");
    assert!(failed.to_string().contains("AI stream"), "{failed}");
}

/// The unit AI's stop (`Effect::Stop`, [`Unit::stop_action`]): the forager's Gather goes and
/// its bush counts a worker less within the update, through the view, not after the step.
#[test]
fn unit_ai_stop_lets_its_target_go_within_the_update() {
    let mut world = forage_world();
    let work = Work {
        target: Some(object_id(600)),
        point: Position::new(32.5, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(1)
    );
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            unit.stop_action(actions, step)?;
            let bush = OthersRef::of(&step.world.players, &step.world.index, step.player)
                .unwrap()
                .find(object_id(600))
                .map(|(_, unit)| unit.workers.get());
            Ok((bush, actions.list.gather().is_some()))
        },
    );
    assert_eq!(seen, Ok((Some(0), false)));
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
}

/// The hunt's search for the next tree (`FUN_140ac4670`, [`Unit::hunt_tree_search`]): the
/// unit AI's `ClosestResourceObject` of group 0 names the nearer tree, which the hunt takes
/// (slot 0x48: its Attack on the tree) and the tree counts the hunter. A hunt farther than
/// `within` from its point, a tree farther than `near` from the unit, or no tree, find none.
#[test]
fn hunt_that_lost_its_tree_takes_the_next_one() {
    let search = |world: &mut World, within, near| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let mut hunt = *actions.list.hunt().unwrap();
                unit.hunt_tree_search((&mut hunt, actions), step, within, near, &roles())
            },
        )
    };
    let mut world = lumber_world(31.5);
    assert_eq!(
        search(&mut world, Some(Distance::new(8.0)), None),
        Ok(TreeSearch::Found)
    );
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            hunt_action.target,
            hunt_action
                .state
                .attack()
                .copied()
                .map(|attack| attack.target)
        ),
        (
            Some(HuntTargetId::new(object_id(520))),
            Some(Some(AttackTargetId::new(object_id(520))))
        )
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    let mut world = lumber_world(31.5);
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .point = Position::new(40.0, 89.5, 0.0);
    assert_eq!(
        search(&mut world, Some(Distance::new(8.0)), None),
        Ok(TreeSearch::NoneFound)
    );
    assert_eq!(
        search(&mut world, None, Some(Distance::new(1.5))),
        Ok(TreeSearch::NoneFound)
    );
    let mut world = lumber_world(31.5);
    for index in [2, 3] {
        world.players[0].objects[ListIndex::new(index)].life.amount = Amount::ZERO;
    }
    assert_eq!(search(&mut world, None, None), Ok(TreeSearch::NoneFound));
}

/// A hunt's end of an Attack that failed (`FUN_140ac4670` with -1 and 5,
/// [`Unit::hunt_attack_failed`]): a hunter whose unit AI targets trees takes the next tree
/// within 5 of it; with none so near, or a unit AI that targets another class, the hunt
/// searches (state 3, its timer at -10 s) with its target kept.
#[test]
fn hunt_whose_attack_failed_takes_near_tree_or_searches() {
    let failed = |world: &mut World| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| {
                let mut hunt = *action.list.hunt().unwrap();
                Unit::hunt_attack_failed(unit, &mut hunt, action, step, &roles())
            },
        )
    };
    let mut game = lumber_world(31.5);
    assert_eq!(failed(&mut game), Ok(()));
    let action = game
        .parts
        .actions_of(game.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (
            action.target,
            action.state.attack().copied().map(|attack| attack.target)
        ),
        (
            Some(HuntTargetId::new(object_id(520))),
            Some(Some(AttackTargetId::new(object_id(520))))
        )
    );
    for (near, target_class) in [(39.0, class::TREE), (31.5, class_id(99))] {
        let mut world = lumber_world(near);
        world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .target_class = Some(target_class);
        assert_eq!(failed(&mut world), Ok(()));
        let hunt_action = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .copied()
            .unwrap();
        assert_eq!(
            (hunt_action.state, hunt_action.timer, hunt_action.target),
            (HuntState::Search, SEARCH_AGAIN, None)
        );
    }
}

/// [`sent_off_world`] with the queue `orders`, the house built and damaged (300 hit points), and
/// with `repairer` the repairer master 156 (the villager's with the Repair task 4, range 0.5).
fn repairer_world(orders: Vec<QueuedOrder>, repairer: bool) -> World {
    let mut world = sent_off_world(orders);
    let house = &mut world.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(300.0);
    let construction = site_mut(&mut world, object_id(10028));
    (construction.progress, construction.desolid) = (Seconds::new(1.0), false);
    if repairer {
        for masters in &mut world.model.masters {
            let mut master = masters.get(VILLAGER_MALE).unwrap().clone();
            master.id = master_id(156);
            master
                .combat
                .get_or_insert_with(Default::default)
                .weapon_range = Distance::new(0.5);
            master
                .action
                .get_or_insert_with(Default::default)
                .tasks
                .push(Task {
                    id: task_id(4),
                    work_val1: WorkValue::new(0.5),
                    ..task(REPAIR)
                });
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([master])));
        }
    }
    world
}

/// The unit AI's repair order on the player's damaged house (`0x2ce`, slot 0x378 forced,
/// `FUN_141217c00`): the villager (83, task swap group 1) becomes its player's repairer (156,
/// slot 0x90), whose task list gives the Repair task; the Repair walks to the house (state 4)
/// and the AI holds the repair action on it. Without the repairer master the villager's own list
/// must give the task, which it does not: the order is not taken.
#[test]
fn the_unit_ai_repair_order_switches_villager_to_the_repairer() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Repair,
        priority: OrderPriority::of_data(100),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_kind: order_kind(80),
    };
    let world = |repairer: bool| repairer_world(vec![order], repairer);
    let mut game = world(true);
    game.tick(Millis::new(50));
    let players = &game.players[1].objects[ListIndex::new(0)];
    assert_eq!(players.life.master_id, master_id(156));
    let repair_action = game
        .parts
        .actions_of(players.id)
        .list
        .repair()
        .copied()
        .expect("the Repair");
    assert_eq!(
        (
            repair_action.target,
            repair_action.state.bare(),
            repair_action.range,
            repair_action.task.id
        ),
        (
            Some(RepairTargetId::new(object_id(10028))),
            RepairState::Move { move_to: None },
            Distance::new(0.5),
            task_id(4)
        )
    );
    let ai = game.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        (ai.current_action, ai.current_target),
        (Some(AiAction::Repair), Some(object_id(10028)))
    );
    let mut world = world(false);
    world.tick(Millis::new(50));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            unit.life.master_id,
            world.parts.actions_of(unit.id).list.repair().copied()
        ),
        (VILLAGER_MALE, None)
    );
}

/// The unit AI's side of player 1's villager in `world` as its update runs it
/// ([`Unit::with_side`]): what `work` gives and the villager's unit AI as the side returns.
fn villager_side<R>(
    world: &mut World,
    work: impl FnOnce(&mut UnitView<'_, '_>) -> GenieResult<R>,
) -> GenieResult<(R, Option<UnitAi>)> {
    use crate::unit_ai::world_view::OwnerView;
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, actions, step| {
            let view = OwnerView::of(step, None).unwrap();
            let attacker = unit.attacker_of(
                actions,
                UnitAiKind::Boar,
                None,
                (PlayerId::new(1), PlayerKind::Human),
                GameMode::RandomMap,
                (
                    CaptureKind::None,
                    Firepower::ZERO,
                    step.world
                        .parts
                        .mover_of(unit.id)
                        .map_or(Speed::ZERO, |mover| mover.stats.speed),
                ),
            );
            let answer = unit.with_side(
                actions,
                step,
                &view,
                attacker,
                (PlayerId::new(1), PlayerKind::Human),
                |_, side, _| work(side),
            )?;
            Ok((answer, step.world.parts.unit_ai.get(unit.id).cloned()))
        },
    )
}

/// The unit AI's repair command at once (slot 0x378 while the AI runs, the villager lent back to
/// its slot): within the run the villager is its player's repairer, holds the Repair, and its top
/// target is the house ([`crate::unit_ai::world_view::UnitView::top_target_now`]).
#[test]
fn the_unit_ai_repair_command_is_the_unit_within_its_run() {
    use crate::unit_ai::UnitCommand;
    let mut world = repairer_world(vec![], true);
    let house = Placed {
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: class::BUILDING,
        player: PlayerId::new(1),
        master: ids::HOUSE,
        obstruction_kind: ObstructionKind::KIND_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(object_id(10028), house);
    let seen = villager_side(&mut world, |side| {
        let repairs = side.command(UnitCommand::Repair(object_id(10028)))?;
        let role_id = side
            .actions
            .list
            .repair()
            .map(|repair_action| repair_action.target);
        Ok((
            repairs,
            side.unit.life.master_id.get(),
            role_id,
            side.top_target_now(),
        ))
    });
    assert_eq!(
        seen.map(|(answer, _)| answer),
        Ok((
            Answer::Taken,
            156,
            Some(Some(RepairTargetId::new(object_id(10028)))),
            Some(object_id(10028))
        ))
    );
}

/// A world routine the unit AI's side lends its unit to ([`Step::lend`]) finds a stand-in for
/// the AI, which is out of the unit while it runs: one that changes it fails the lend.
///
/// [`Step::lend`]: crate::world::step::Step::lend
#[test]
fn world_routine_that_reaches_the_running_unit_ai_fails_the_side_lend() {
    let mut repairer_world = repairer_world(vec![], false);
    let held = villager_side(&mut repairer_world, |side| {
        side.step.lend_at(
            (&mut *side.unit, &mut *side.actions),
            |world, player_id, list_index| {
                Ok(world
                    .parts
                    .unit_ai
                    .contains(world.players[player_id.index()].objects[list_index].id))
            },
        )
    });
    assert_eq!(
        held.map(|(lent, ai)| (lent, ai.is_some())),
        Ok((true, true))
    );
    let reached = villager_side(&mut repairer_world, |side| {
        side.step.lend_at(
            (&mut *side.unit, &mut *side.actions),
            |world, player_id, list_index| {
                world
                    .parts
                    .unit_ai
                    .get_mut(world.players[player_id.index()].objects[list_index].id)
                    .unwrap()
                    .current_target = Some(object_id(7));
                Ok(())
            },
        )
    });
    let error = reached.expect_err("the stand-in changed");
    assert!(
        error.to_string().contains("reached the unit's AI"),
        "{error}"
    );
    assert_eq!(
        repairer_world
            .parts
            .unit_ai
            .get(repairer_world.players[1].objects[ListIndex::new(0)].id)
            .map(|ai| ai.current_target),
        Some(None)
    );
}

/// The world of [`sent_off_world`] with the queue `orders`, the house built and whole, taking
/// villagers in with room for `room`, both placed and the villager moving.
fn enter_world(orders: Vec<QueuedOrder>, room: u8) -> World {
    let mut world = sent_off_world(orders);
    let house = &mut world.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(550.0);
    let construction = site_mut(&mut world, object_id(10028));
    (construction.progress, construction.desolid) = (Seconds::new(1.0), false);
    for masters in &mut world.model.masters {
        let house = Arc::make_mut(masters).get_mut(ids::HOUSE).unwrap();
        house.garrison = GarrisonCapacity::of_data(room);
        // it takes villagers in (bit 0)
        house.building.as_mut().unwrap().garrison_kind = GarrisonKinds::VILLAGERS;
    }
    world.parts.put_mover(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(walker()),
    );
    world.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        Tiles::new(GridSize::new(120, 120), vec![4; 14400]),
    );
    world.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world.spatial.insert(
        object_id(9175),
        Placed {
            position: Position::new(29.5, 89.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
    world
}

/// Slot 0x3a0 (`FUN_1410847e0`) itself: a target the world does not hold is 0; a task found
/// gives the unit its Enter (1); unforced, a unit entering the target already keeps its Enter
/// (1), forced it takes a new one.
#[test]
fn the_enter_command_enters_or_keeps_the_enter_on_its_target() {
    let mut world = enter_world(vec![], 5);
    // the villager out of reach, so that it walks
    world.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(20.0, 80.0, 0.0));
    world
        .spatial
        .set_position(
            object_id(9175),
            Position::new(20.0, 80.0, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(4242),
            OrderForce::Forced
        ),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .enter()
            .copied(),
        None
    );
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let first = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .enter()
        .copied()
        .expect("the Enter");
    assert_eq!(
        (first.target, first.state.bare()),
        (
            Some(container_id(10028)),
            EnterState::Walk { move_to: None }
        )
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .enter_mut()
        .unwrap()
        .timer = Seconds::new(7.0);
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Unforced
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .enter()
            .copied()
            .unwrap()
            .timer,
        Seconds::new(7.0)
    );
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .enter()
            .copied()
            .unwrap()
            .timer,
        first.timer
    );
}

/// The Enter's start (`FUN_140a86f20`) of a unit inside an object: `set_state(13)`, its
/// standing sprite and notify 0x1f9 with the enter's action value to its unit AI; the update
/// then returns 3 and the list removes it.
#[test]
fn an_enter_of_unit_inside_an_object_fails_at_its_start() {
    let mut world = enter_world(vec![], 5);
    world.players[1].objects[ListIndex::new(0)].life.inside =
        Some(ContainerId::new(object_id(4242)));
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .enter()
            .map(|enter_action| enter_action.state),
        Some(EnterState::Failed)
    );
    assert_eq!(
        world.parts.unit_ai.get(unit.id).unwrap().notifies,
        [Notify::own(
            object_id(9175),
            NotifyKind::ACTION_FAILED,
            [AiAction::Enter.value(), 0, 0]
        )]
    );
}

/// The unit AI's enter command at once (slot 0x3a0 while the AI runs) of a unit inside an
/// object: its Enter fails at the start, and the notify 0x1f9 for its own AI, which the side does
/// not hold, reaches the AI as the side returns.
#[test]
fn the_unit_ai_enter_from_inside_tells_its_ai_as_the_side_returns() {
    use crate::unit_ai::UnitCommand;
    let mut world = enter_world(vec![], 5);
    world.players[1].objects[ListIndex::new(0)].life.inside =
        Some(ContainerId::new(object_id(4242)));
    let seen = villager_side(&mut world, |side| {
        let enters = side.command(UnitCommand::Enter(object_id(10028)))?;
        Ok((
            enters,
            side.actions
                .list
                .enter()
                .map(|enter_action| enter_action.state),
        ))
    });
    let failed = Notify::own(
        object_id(9175),
        NotifyKind::ACTION_FAILED,
        [AiAction::Enter.value(), 0, 0],
    );
    assert_eq!(
        seen.map(|(answer, ai)| (answer, ai.map(|ai| ai.notifies))),
        Ok((
            (Answer::Taken, Some(EnterState::Failed)),
            Some(vec![failed])
        ))
    );
}

/// [`enter_world`] whose villager is a sheep under its Artifact, the house taking herdables in.
fn livestock_enter_world() -> World {
    let mut world = enter_world(vec![], 5);
    let sheep = Master {
        id: master_id(594),
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: None,
        action: Some(ActionPart {
            tasks: vec![Task {
                id: task_id(1),
                action_kind: ENTER,
                object_group: Some(class_id(3)),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        moving: Some(MovingPart {
            // no walking or standing sprite: its Enter's MoveTo takes none (the world's sprites'
            // table holds none)
            move_sprite: None,
            ..MovingPart::default()
        }),
        ..moving_master(walker().stats)
    };
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([sheep.clone()])));
        // the house takes herdables in (bit 4)
        Arc::make_mut(masters)
            .get_mut(ids::HOUSE)
            .unwrap()
            .building
            .as_mut()
            .unwrap()
            .garrison_kind = GarrisonKinds::HERDABLES;
    }
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.life.master_id = master_id(594);
    world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_artifact(Some(Artifact::new(PlayerId::new(1), None)));
    world
}

/// The enter command of livestock under its Artifact (`FUN_1410847e0`): the Enter becomes the
/// Artifact's sub-action (its list emptied, `FUN_140a98e20`, then `FUN_140a977e0`), the Artifact
/// staying the top action; the next update runs it through the Artifact's.
#[test]
fn livestock_enter_command_becomes_its_artifact_sub_action() {
    let mut world = livestock_enter_world();
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Unforced
        ),
        Ok(Answer::Taken)
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.actions_of(unit.id).list.enter().copied(), None);
    assert_eq!(
        world.parts.actions_of(unit.id).top_action_kind(),
        Some(ARTIFACT_KIND)
    );
    let sub = world
        .parts
        .actions_of(unit.id)
        .list
        .artifact()
        .copied()
        .unwrap()
        .enter()
        .copied();
    assert!(
        sub.is_some_and(
            |enter_action| enter_action.target == Some(ContainerId::new(object_id(10028)))
        ) || unit.life.inside == Some(ContainerId::new(object_id(10028))),
        "{unit:?}"
    );
}

/// The Enter's `MoveTo` shows its sprite as it starts (`FUN_140a87320` case 4 gives its
/// constructor the task's moving sprite, `0x140aa29e0`'s `set_state(4)` shows it): the task's,
/// else `FUN_140aa0df0`'s default toward a building, the master's walking sprite.
#[test]
fn an_enter_move_to_shows_its_sprite_as_it_starts() {
    for (moving, shown) in [
        (Some(SpriteId::named(30)), SpriteId::named(30)),
        (None, SpriteId::named(31)),
    ] {
        let mut world = enter_world(vec![], 5);
        world.pathing = Pathing::new(
            GridSize::new(120, 120),
            vec![TerrainId::named(0); 120 * 120],
            vec![Restriction::new(vec![1.0]); 8],
            0,
        );
        world.build_mesh().unwrap();
        for id in [30, 31] {
            let graphic = Graphic {
                id: SpriteId::named(id),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(16),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            };
            let _ = Arc::make_mut(&mut world.model.graphics).insert(SpriteId::named(id), graphic);
        }
        Arc::make_mut(&mut world.model.masters[1])
            .get_mut(VILLAGER_MALE)
            .unwrap()
            .moving
            .get_or_insert_with(Default::default)
            .move_sprite = Some(SpriteId::named(31));
        let players = &mut world.players[1].objects[ListIndex::new(0)];
        let here = players.position();
        world.parts.mover_mut(players.id).unwrap().moved_at = here;
        let task = enter_task(Distance::ZERO, class::BUILDING);
        // waiting at its end: the update's walk again makes the MoveTo with its sprite, the
        // next update starts it and shows the sprite
        world
            .parts
            .actions
            .of_mut(players.id)
            .unwrap()
            .list
            .set_enter(Some(EnterAction {
                state: EnterState::Wait,
                timer: Seconds::ZERO,
                moving,
                ..EnterAction::new(
                    ContainerId::new(object_id(10028)),
                    Position::new(30.0, 88.0, 0.0),
                    &task,
                )
            }));
        let _ = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update(unit, action, step, &roles(), slots()),
        )
        .unwrap();
        let made = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .enter()
            .and_then(|enter_action| enter_action.state.move_to().copied());
        assert_eq!(
            made.map(|move_to_action| (
                move_to_action.state,
                move_to_action.sprite.map(|graphic| graphic.id)
            )),
            Some((MoveTo::Start, Some(shown)))
        );
        let _ = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update(unit, action, step, &roles(), slots()),
        )
        .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            world
                .parts
                .actions_of(unit.id)
                .list
                .enter()
                .and_then(|enter_action| enter_action.state.move_to().copied())
                .map(|move_to_action| move_to_action.state),
            Some(MoveTo::Moving)
        );
        assert_eq!(
            unit.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(shown)
        );
    }
}

/// A second enter command of livestock walking under its Artifact (430061417's pig 7920 in
/// update 32, INTERACT on the transport again): the new Enter takes the house (slot 0x178), then
/// `FUN_140a98e20` pops the old one, whose teardown lets the house go (slot 0x180) and whose
/// `MoveTo`'s stops the unit: its speed and acceleration 0, so the update's velocity step does
/// not move it.
#[test]
fn livestock_second_enter_command_stops_the_walk_and_lets_the_old_enter_target_go() {
    let mut world = livestock_enter_world();
    let mut walking = EnterAction::new(
        ContainerId::new(object_id(10028)),
        Position::new(30.0, 88.0, 0.0),
        &enter_task(Distance::ZERO, class::BUILDING),
    );
    walking = walking.walking(MoveTo::Moving);
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .artifact_mut()
        .unwrap()
        .sub = Some(ArtifactSub::Enter { enter: walking });
    world.parts.mover_mut(players.id).unwrap().velocity = Some(Velocity {
        velocity: BVector::new(0.0, 0.0, 0.7),
        acceleration: BVector::new(0.0, 0.0, f32::MAX),
    });
    // the old Enter took the house
    world.players[1].objects[ListIndex::new(1)].workers = WorkerCount::of_data(1);
    assert_eq!(
        world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let velocity = world.parts.mover_of(unit.id).unwrap().velocity.unwrap();
    assert_eq!(
        (velocity.velocity.y(), velocity.acceleration.y()),
        (0.0, 0.0)
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(1)].workers,
        WorkerCount::of_data(1)
    );
}

/// The inside update (`FUN_140a98790`) of a villager inside the house deletes its top action
/// whatever its kind: a `MoveTo` left from before the enter goes (its destructor stops the
/// unit), and the update after has nothing left to delete (426027790-style villagers in a town
/// bell: 427445417's p3, 429184426's p4).
#[test]
fn an_inside_unit_update_deletes_its_top_action() {
    let mut world = enter_world(vec![], 5);
    world.players[1].objects[ListIndex::new(0)].life.inside =
        Some(ContainerId::new(object_id(10028)));
    world
        .spatial
        .go_inside(object_id(9175), Position::new(30.0, 88.0, 0.0));
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction::to_point(
            Position::new(40.0, 80.0, 0.0),
            Distance::ZERO,
            None,
            Speed::ZERO,
        )));
    world.tick(Millis::new(50));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(unit.id).list.move_to().copied(),
        None
    );
    assert_eq!(world.parts.actions_of(unit.id).top_action_kind(), None);
    assert_eq!(world.players[1].freezes, vec![]);
}

/// An Enter whose walk arrived in reach gets its unit in in its update (`FUN_140a867b0`): the
/// building adds the unit to its list in the same update, its own player's and an ally's
/// (player 2's) alike.
#[test]
fn an_enter_update_adds_its_unit_to_its_building_own_or_an_ally_s() {
    let entered = |ally: bool| {
        let mut world = enter_world(vec![], 5);
        if ally {
            let house = world
                .remove_live(PlayerId::new(1), ListIndex::new(1))
                .unwrap();
            let mut ally = Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            };
            ally.objects.push(house.unit);
            world.players.push(ally);
            world.reindex();
            world.model.masters.push(world.model.masters[1].clone());
            let mut placed = *world.spatial.get(object_id(10028)).unwrap();
            placed.player = PlayerId::new(2);
            world.spatial.insert(object_id(10028), placed);
            world.players[1].relations = relation_row([3, 0, 0]);
            world.players[2].relations = relation_row([3, 0, 0]);
            world.spatial.relations = ByPlayer::new(vec![
                relation_row([0, 3, 3]),
                relation_row([3, 0, 0]),
                relation_row([3, 0, 0]),
            ]);
        }
        let mut enter_action = EnterAction::new(
            ContainerId::new(object_id(10028)),
            Position::new(30.0, 88.0, 0.0),
            &enter_task(Distance::new(2.0), class::BUILDING),
        );
        enter_action = enter_action.walking(MoveTo::Arrived);
        let players = &mut world.players[1].objects[ListIndex::new(0)];
        players.place_for_test(Position::new(29.5, 89.5, 0.0));
        world
            .parts
            .actions
            .of_mut(players.id)
            .unwrap()
            .list
            .set_enter(Some(enter_action));
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let me = Me {
                    human_order: WorldTime::ZERO,
                    id: unit.id,
                    group: unit.group.id,
                    action_state: actions
                        .top_action()
                        .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                    top_speed: TopSpeed::Own,
                };
                let code = unit.enter_part(actions, step, me, &roles())?;
                unit.end_head_update(actions, step, code)?;
                let (_, house) = OthersRef::of(&step.world.players, &step.world.index, step.player)
                    .unwrap()
                    .find(object_id(10028))
                    .unwrap();
                Ok((unit.life.inside, house.garrison.clone()))
            },
        )
    };
    assert_eq!(
        entered(false),
        Ok((Some(container_id(10028)), vec![object_id(9175)]))
    );
    assert_eq!(
        entered(true),
        Ok((Some(container_id(10028)), vec![object_id(9175)]))
    );
}

/// The Enter's enter takes the unit out of its group in the update (removeUnit, `FUN_14102c160`
/// inside `FUN_140a867b0`): the group has lost it and the unit its link while the update still
/// runs.
#[test]
fn an_enter_unit_leaves_its_group_inside_the_update() {
    let mut world = enter_world(vec![], 5);
    let member = |id| Member {
        id,
        ..Member::default()
    };
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(PlayerId::new(1)),
        members: vec![member(object_id(9175)), member(object_id(401))],
        ..UnitGroup::default()
    });
    let mut enter_action = EnterAction::new(
        ContainerId::new(object_id(10028)),
        Position::new(30.0, 88.0, 0.0),
        &enter_task(Distance::new(2.0), class::BUILDING),
    );
    enter_action = enter_action.walking(MoveTo::Arrived);
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    players.place_for_test(Position::new(29.5, 89.5, 0.0));
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .set_enter(Some(enter_action));
    players.group.id = Some(group_id(7));
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let code = unit.enter_part(actions, step, me, &roles())?;
            unit.end_head_update(actions, step, code)?;
            let members: Vec<ObjectId> = step
                .world
                .groups
                .get(group_id(7))
                .map_or_else(Vec::new, |group| {
                    group.members.iter().map(|member| member.id).collect()
                });
            Ok((unit.life.inside, unit.group.id, members))
        },
    );
    assert_eq!(
        left,
        Ok((Some(container_id(10028)), None, vec![object_id(401)]))
    );
}

/// An Enter that got the unit in, the unit let out again before its next update: that update's
/// 1 has the action list remove it (its teardown lets the building go).
#[test]
fn an_entered_enter_of_unit_let_out_goes_at_its_update() {
    let mut world = enter_world(vec![], 5);
    let mut enter_action = EnterAction::new(
        ContainerId::new(object_id(10028)),
        Position::new(30.0, 88.0, 0.0),
        &enter_task(Distance::new(0.5), class::BUILDING),
    );
    enter_action.state = EnterState::Done;
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_enter(Some(enter_action));
    world.tick(Millis::new(50));
    world.tick(Millis::new(50));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .enter()
            .copied(),
        None
    );
    assert_eq!(world.players[1].freezes, vec![]);
}

/// The unit AI's enter order on the player's house (`0x2cb`, slot 0x3a0 forced,
/// `FUN_1410847e0`): its slot 0x820 with the garrison filter finds the villager's Enter task on
/// buildings (action 3, class 3); after the update the Enter is the unit's (or the unit inside
/// the house), and the AI holds the enter action on it. A house without room gives no task:
/// the order is not taken.
#[test]
fn the_unit_ai_enter_order_gives_the_unit_an_enter() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Enter,
        priority: OrderPriority::of_data(100),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_kind: order_kind(80),
    };
    let mut world = enter_world(vec![order], 5);
    world.tick(Millis::new(50));
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert!(
        world
            .parts
            .actions_of(players.id)
            .list
            .enter()
            .is_some_and(
                |enter_action| enter_action.target == Some(ContainerId::new(object_id(10028)))
            )
            || players.life.inside == Some(ContainerId::new(object_id(10028))),
        "{:?}",
        world.players[1].freezes
    );
    let ai = world.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        (ai.current_action, ai.current_target),
        (Some(AiAction::Enter), Some(object_id(10028)))
    );
    let mut world = enter_world(vec![order], 0);
    world.tick(Millis::new(50));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            world.players[1].freezes.len(),
            world.parts.actions_of(unit.id).list.enter().copied()
        ),
        (0, None)
    );
    assert_ne!(
        world.parts.unit_ai.get(unit.id).unwrap().current_action,
        Some(AiAction::Enter)
    );
}

/// The whole of it: a villager the foundation sent off (its queue the foundation's order)
/// runs the order on its turn (slot 0x108 forced: its slot 0x388 finds the builder's task by
/// the swap group), becomes the builder with the wait as its only action, and keeps waiting
/// while the foundation has no build points; once they pass 0.01 its next update drops the
/// wait, the unit AI's action value staying 0x27a.
#[test]
fn villager_sent_off_foundation_waits_for_it_as_builder() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Foundation,
        priority: OrderPriority::of_data(100),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 0.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 0.0),
        ready: true,
        target_kind: order_kind(70),
    };
    let mut world = sent_off_world(vec![order]);
    world.tick(Millis::new(50));
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert!(
        !players.stopped,
        "{:?}",
        world.players[1].objects[ListIndex::new(0)]
    );
    assert_eq!(players.life.master_id, ids::BUILDER);
    assert_eq!(
        world.parts.actions_of(players.id).top_action_kind(),
        Some(WAIT_SOLIDIFY)
    );
    let ai = world.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action, ai.current_order),
        (
            Some(AiAction::WaitSolidify),
            Some(AiAction::WaitSolidify),
            Some(OrderKind::Foundation)
        )
    );
    assert_eq!(ai.orders, vec![]);
    // player 0's update, then player 1's: the wait reads 0 build points and stays
    world.tick(Millis::new(50));
    world.tick(Millis::new(50));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .top_action_kind(),
        Some(WAIT_SOLIDIFY)
    );
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .progress = Seconds::new(0.5);
    world.tick(Millis::new(50));
    world.tick(Millis::new(50));
    assert_eq!(world.players[1].freezes, vec![]);
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .wait_solidify()
            .copied(),
        None
    );
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(unit.id)
            .and_then(|ai| ai.current_action),
        Some(AiAction::WaitSolidify)
    );
}

/// Slot 0x230's build order (`0x2be`): slot 0xf0 builds the foundation (the builder's task by
/// the swap group), then the tail makes the order the AI's with its priority, leaving a patrol
/// order: its secondary timer capped at 750 and an adjusted idle timeout of 750 redrawn.
#[test]
fn queued_build_order_builds_and_leaves_patrol_order() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Build,
        priority: OrderPriority::of_data(100),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_kind: order_kind(70),
    };
    let mut world = sent_off_world(vec![]);
    {
        let ai = world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        ai.current_order = Some(OrderKind::Patrol);
        ai.secondary_timer = Millis::new(900);
        ai.adjusted_idle_timeout = Millis::new(750);
        ai.idle_timeout = Millis::new(3000);
    }
    let draws = world.streams.get_mut(Stream::Ai).draws;
    // the unit AI runs the order (slot 0x230) and gives the build command, which runs at once
    world
        .run_unit_ai(
            (PlayerId::new(1), ListIndex::new(0)),
            |ai, at, side, _, rng| {
                let order_outcome = ai.process_order(
                    &order,
                    Some((at, PlayerKind::Human)),
                    None,
                    rng,
                    side,
                    AiLayer {
                        roles: &roles(),
                        slots: slots(),
                        unit_ai_tuning: &tuning().unit_ai,
                    },
                )?;
                assert_eq!(order_outcome, OrderOutcome::Taken);
                Ok(())
            },
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, ids::BUILDER);
    assert!(world.parts.actions_of(unit.id).list.build().is_some());
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (
            ai.current_order,
            ai.order_priority.get(),
            ai.current_action,
            ai.current_target
        ),
        (
            Some(OrderKind::Build),
            100,
            Some(AiAction::Build),
            Some(object_id(10028))
        )
    );
    assert_eq!(ai.secondary_timer, Millis::new(750));
    assert_ne!(ai.adjusted_idle_timeout, Millis::new(750));
    assert_eq!(world.streams.get_mut(Stream::Ai).draws, draws + 1);
}

/// Slot 0x380 (`FUN_14120d410`) answers 0 and does nothing for a foundation the world does not
/// hold and for a unit without a task on it (its own master's list and its swap group's); the
/// Gather's `set_state(6)` ignores that 0.
#[test]
fn build_command_without_its_foundation_or_task_does_nothing() {
    let without_build_tasks = |world: &mut World| {
        for id in [118, 212] {
            let master = Arc::make_mut(&mut world.model.masters[1])
                .get_mut(master_id(id))
                .unwrap();
            master
                .action
                .get_or_insert_with(Default::default)
                .tasks
                .retain(|task| task.action_kind != ActionKind::BUILD);
        }
    };
    let mut world = sent_off_world(vec![]);
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        world.build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(99_999))
        ),
        Ok(Answer::NotTaken)
    );
    without_build_tasks(&mut world);
    assert_eq!(
        world.build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(10028))
        ),
        Ok(Answer::NotTaken)
    );
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
    // with a task it builds as the builder 118
    let mut world = sent_off_world(vec![]);
    assert_eq!(
        world.build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(10028))
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].life.master_id,
        ids::BUILDER
    );
}

/// A world routine's unit AI run (a stance's, a game status's or a let-out's stop,
/// `World::run_ai_calls`): the AI's calls run on the unit at once, so the AI reads the action
/// list a stop left before it returns, and the unit is back in its slot after it.
#[test]
fn world_routine_unit_ai_calls_run_on_the_unit_at_once() {
    let mut world = sent_off_world(vec![]);
    let (explore, _) = ExploreAction::new(
        Position::new(10.0, 10.0, 0.0),
        &bare_walk(),
        &tuning().move_to,
    )
    .unwrap();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_explore(Some(explore));
    let seen = world
        .run_ai_calls((PlayerId::new(1), ListIndex::new(0)), |ai, side, rng| {
            let before = side.busy_now();
            ai.stop_with(StopOrder::Keep, None, side, rng, &roles())?;
            Ok((before, side.busy_now()))
        })
        .unwrap();
    assert_eq!(seen, (true, false));
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            unit.id.get(),
            world.parts.actions_of(unit.id).top_action().is_none()
        ),
        (9175, true)
    );
}

/// The unit AI reads its target where it runs (`FUN_1415417a0` on `+0x48`), not as the unit's
/// step began: a target the update changed (the Build's work order on a resource) is the new
/// one, and a unit that is its own target reads itself out of its slot.
#[test]
fn the_unit_ai_reads_the_target_its_unit_update_left() {
    use crate::unit_ai::world_view::OwnerView;
    let mut world = sent_off_world(vec![]);
    let mut read = Vec::new();
    world
        .run_on_step(
            PlayerId::new(1),
            ListIndex::new(0),
            |unit, actions, step| {
                for target in [10028, 9175, -1] {
                    step.world
                        .parts
                        .unit_ai
                        .get_mut(unit.id)
                        .unwrap()
                        .current_target = ObjectId::of_field(target);
                    let master = unit
                        .master(step.world.master_tables(), PlayerId::new(1))
                        .ok();
                    let at = unit.surroundings(
                        actions,
                        step,
                        &OwnerView::of(step, None).unwrap(),
                        (master, master.map(|master| master.class)),
                        (PlayerId::new(1), false, false),
                    )?;
                    read.push(
                        at.target
                            .map(|seen| (seen.state, seen.owner, seen.position)),
                    );
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        read,
        [
            Some((
                ObjectState::Foundation,
                PlayerId::new(1),
                Position::new(30.0, 88.0, 0.0)
            )),
            Some((
                ObjectState::Alive,
                PlayerId::new(1),
                Position::new(29.5, 89.5, 0.0)
            )),
            None
        ]
    );
}

/// The unit AI's view answers slot 0x380 before `BuildObject` takes the build
/// (`UnitView::builds`): 0 for a foundation the world does not hold or no task for it, 1 with a
/// task or a Build on top (its slot 0x48), whatever the master's lists then hold.
#[test]
fn the_unit_ai_view_answers_the_build_command_before_it_runs() {
    let answers = |world: &mut World, foundation| {
        let mut answer = None;
        world
            .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
                answer = Some(side.builds(foundation)?);
                Ok(())
            })
            .unwrap();
        answer.unwrap()
    };
    let mut world = sent_off_world(vec![]);
    assert!(answers(&mut world, FoundationId::new(object_id(10028))));
    assert!(!answers(&mut world, FoundationId::new(object_id(99_999))));
    // the build command on top: a Build answers 1 without a task in the lists
    assert_eq!(
        world.build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(10028))
        ),
        Ok(Answer::Taken)
    );
    for id in [83, 118, 212] {
        let master = Arc::make_mut(&mut world.model.masters[1])
            .get_mut(master_id(id))
            .unwrap();
        master
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .retain(|task| task.action_kind != ActionKind::BUILD);
    }
    assert!(answers(&mut world, FoundationId::new(object_id(10028))));
    let mut world = sent_off_world(vec![]);
    for id in [118, 212] {
        let master = Arc::make_mut(&mut world.model.masters[1])
            .get_mut(master_id(id))
            .unwrap();
        master
            .action
            .get_or_insert_with(Default::default)
            .tasks
            .retain(|task| task.action_kind != ActionKind::BUILD);
    }
    assert!(!answers(&mut world, FoundationId::new(object_id(10028))));
}

/// The build command at once from the unit AI's run (`UnitView::effect_now` with
/// `Effect::BuildObject`: a requeued build order's, which a following attack command replaces) does
/// what the build command after the update does: the switch to the builder (118), the Build on the
/// foundation, which it takes.
#[test]
fn the_build_command_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    let mut now = later.clone();
    let _ = later
        .build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(10028)),
        )
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| {
            side.effect_now(
                Effect::Command(AiCommand::BuildObject(FoundationId::new(object_id(10028)))),
                ai,
                rng,
            )
        },
    )
    .unwrap();
    let (first, second) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(second.life.master_id, ids::BUILDER);
    assert!(
        now.parts
            .actions_of(second.id)
            .list
            .build()
            .is_some_and(|build| build.target == Some(FoundationId::new(object_id(10028))))
    );
    assert_eq!(
        (
            second.life.master_id,
            now.parts.actions_of(second.id).list.build().copied(),
            &second.sprite
        ),
        (
            first.life.master_id,
            later.parts.actions_of(first.id).list.build().copied(),
            &first.sprite
        )
    );
    assert_eq!(now.players[1].attributes, later.players[1].attributes);
    assert_eq!(now.streams, later.streams);
    let taken = |world: &World| world.object(object_id(10028)).map(|(_, unit)| unit.clone());
    assert_eq!(taken(&now), taken(&later));
}

/// `FUN_141205ac0`, the way back after a drop-off: the order on a target `0x325` in front of the
/// unit AI's queue, on the town-bell record's object the world holds (its master the target
/// type, its owner the target's, its place the point), else on the record's target as recorded
/// (owner −1, the record's point); issued by the unit's owner, priority 100, ready.
#[test]
fn the_way_back_after_drop_off_queues_the_order_on_the_recorded_work() {
    use crate::orders::town_bell::BackToWork;
    let mut world = sent_off_world(vec![]);
    let work = BackToWork {
        target: Some(object_id(10028)),
        target_class: class_of(3),
        action: Some(AiAction::Build),
        target2: None,
        point: Point::new(1.0, 2.0),
    };
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let mut bell = world.parts.town_bell_of(id);
    bell.work = work;
    world.parts.set_town_bell(id, bell);
    world
        .queue_way_back(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        ai.orders,
        [QueuedOrder {
            issuer: Some(Issuer::Player(player_id(1))),
            order: OrderKind::OnTarget2,
            priority: OrderPriority::of_data(100),
            target: Some(object_id(10028)),
            target_owner: owner(1),
            point: Position::new(30.0, 88.0, 0.0),
            range: Distance::ZERO,
            shown: Point::new(30.0, 88.0),
            ready: true,
            target_kind: order_kind(70),
        }]
    );
    // a recorded target the world no longer holds: the record as it is, in front
    bell.work = BackToWork {
        target: Some(object_id(999)),
        ..work
    };
    world.parts.set_town_bell(id, bell);
    world
        .queue_way_back(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let first = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .orders[0];
    assert_eq!(
        (
            first.target,
            first.target_kind,
            first.target_owner,
            first.point
        ),
        (
            Some(object_id(999)),
            OrderTargetKind::of_class(class::BUILDING),
            None,
            Position::new(1.0, 2.0, 0.0)
        )
    );
}

/// `UNIT_ORDER`'s drop-off (action 8, `FUN_14161e370`) runs on any player's units its command
/// names: a carrying villager without a gather action, of a master without drop sites and a
/// class the drop-site table does not list, does nothing, whoever commands it.
#[test]
fn unit_order_drop_off_takes_any_player_unit() {
    let mut world = sent_off_world(vec![]);
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    (unit.life.attribute_kind, unit.life.amount) = (Some(attribute::WOOD), Amount::new(5.0));
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    world
        .unit_order(
            player_id(1),
            &object_ids([9175]),
            UnitOrderAction::DropOffAndBack,
        )
        .unwrap();
    world
        .unit_order(
            player_id(0),
            &object_ids([9175]),
            UnitOrderAction::DropOffAndBack,
        )
        .unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
}

/// The unit AI's move at once (`UnitView::effect_now` with `Effect::Move`, ahead of a stop and an
/// attack command in the same run) does what the move after the update does (slot 0x340: the
/// actions go, a `MoveTo` to the point).
#[test]
fn the_ai_move_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    later.parts.put_mover(
        later.players[1].objects[ListIndex::new(0)].id,
        Some(walker()),
    );
    let mut now = later.clone();
    let ai_move = AiMove {
        point: Position::new(35.0, 80.0, 0.0),
        range: Distance::ZERO,
        force: OrderForce::Forced,
        pace: Pace::Run,
    };
    later
        .move_aside(PlayerId::new(1), ListIndex::new(0), ai_move)
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| side.effect_now(Effect::Move(ai_move), ai, rng),
    )
    .unwrap();
    let (first, second) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(
        now.parts
            .actions_of(second.id)
            .list
            .move_to()
            .copied()
            .and_then(|mt| mt.goal()),
        Some(Position::new(35.0, 80.0, 0.0))
    );
    assert_eq!(
        (
            now.parts.actions_of(second.id).list.move_to().copied(),
            (&second.place, now.parts.motion.get(second.id)),
            &second.sprite
        ),
        (
            later.parts.actions_of(first.id).list.move_to().copied(),
            (&first.place, later.parts.motion.get(first.id)),
            &first.sprite
        )
    );
    assert_eq!(now.streams, later.streams);
}

/// The hunt command at once from the unit AI's run (`UnitView::effect_now` with
/// `Effect::HuntObject`: a requeued hunt order's, which a following attack command replaces) does
/// what `HuntObject`'s unit part after the update does: the stopAction's standing sprite, the
/// switch to the hunter (122) whose task hunts deer (class 9), the Hunt on the deer, which it
/// takes.
#[test]
fn the_hunt_command_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    for masters in &mut later.model.masters {
        let hunter = Master {
            id: master_id(122),
            action: Some(ActionPart {
                tasks: vec![Task {
                    action_kind: HUNT,
                    object_group: Some(class_id(9)),
                    work_range: Distance::new(0.5),
                    ..Task::default()
                }],
                ..masters
                    .get(VILLAGER_MALE)
                    .unwrap()
                    .clone()
                    .action
                    .clone()
                    .unwrap_or_default()
            }),
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let deer = Master {
            id: master_id(65),
            kind: MasterKind::COMBAT,
            class: class::PREY,
            radius: Footprint::new(0.4, 0.4),
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([hunter, deer])));
    }
    let _ = later
        .push_live(
            PlayerId::new(0),
            Unit {
                id: object_id(500),
                place: Static::at(Position::new(32.5, 89.5, 0.0)),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(5.0),
                    master_id: master_id(65),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
        )
        .unwrap();
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    later.spatial.insert(
        object_id(9175),
        placed(29.5, class::VILLAGER, PlayerId::new(1), VILLAGER_MALE),
    );
    later.spatial.insert(
        object_id(500),
        placed(32.5, class::PREY, PlayerId::new(0), master_id(65)),
    );
    later.parts.put_mover(
        later.players[1].objects[ListIndex::new(0)].id,
        Some(walker()),
    );
    let mut now = later.clone();
    later
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| {
            side.effect_now(
                Effect::Command(AiCommand::HuntObject(object_id(500))),
                ai,
                rng,
            )
        },
    )
    .unwrap();
    let (first, second) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(second.life.master_id, master_id(122));
    assert!(
        now.parts.actions_of(second.id).list.hunt().is_some_and(
            |hunt_action| hunt_action.target == Some(HuntTargetId::new(object_id(500)))
        )
    );
    assert_eq!(
        (
            second.life.master_id,
            now.parts.actions_of(second.id).list.hunt().copied(),
            &second.sprite,
            (&second.place, now.parts.motion.get(second.id))
        ),
        (
            first.life.master_id,
            later.parts.actions_of(first.id).list.hunt().copied(),
            &first.sprite,
            (&first.place, later.parts.motion.get(first.id))
        )
    );
    assert_eq!(now.players[1].attributes, later.players[1].attributes);
    assert_eq!(now.streams, later.streams);
    assert_eq!(now.players[0].objects, later.players[0].objects);
}

#[test]
fn build_creates_foundation_whose_unit_ai_draws_before_the_turn_players_units() {
    // MarvinBlue/426048731 tick 281: player 1's BUILD draws three AI values in the command
    // phase, so player 2's units, on their turn, get the draws after them (a computer's
    // BUILD here: it needs no builder that can build it)
    let world = |objects: Vec<Unit>| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(29910),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    attributes: attributes(250, [0.0, 100.0, 0.0, 0.0]),
                    kind: PlayerKind::Computer,
                    ..Player::default()
                },
                Player {
                    objects: objects.into(),
                    ..Player::default()
                },
            ],
        )
        .with_ais(live_ais(&[2]));
        world.next_object_id = object_id(100);
        world.model.masters = vec![Arc::new(builder_masters()), Arc::new(builder_masters())];
        placeable(&mut world);
        world
    };
    let mut built = world(vec![live(2)]);
    stock_masters(&mut built);
    built.tick_with(
        Millis::new(40),
        &[Command::Build(BuildCommand {
            player: player_id(0),
            master: master_id(70),
            position: Point::new(10.0, 10.0),
            queue_id: QueueId::of_data(-1),
            sprite_override: SpriteOverride::of_data(-1),
            builders: unit_list(vec![]),
            queued: false,
            commits: false,
            frame: Frame::of_data(0),
        })],
    );
    let foundation = &built.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.state,
            foundation.life.master_id.get()
        ),
        (100, ObjectState::Foundation, 70)
    );
    assert_eq!(built.next_object_id, object_id(101));
    // the same tick without the BUILD, with three AI draws taken by hand after the reseed
    let mut by_hand = world(vec![live(2)]);
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let ai = streams.get_mut(Stream::Ai);
    let new = NewObject {
        position: Position::new(10.0, 10.0, 0.0),
        ..NewObject::default()
    };
    let first = UnitAi::constructed(UnitAiKind::Building, &new, ai, &roles());
    assert_eq!(built.parts.unit_ai.get(foundation.id), Some(&first));
    let next = 750 + ai.rand15() % 500;
    stock_masters(&mut by_hand);
    by_hand.tick(Millis::new(40));
    assert_eq!(
        built
            .parts
            .unit_ai
            .get(built.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
    assert_ne!(
        by_hand
            .parts
            .unit_ai
            .get(by_hand.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
}

#[test]
fn an_order_resets_the_idle_timeouts_of_its_units_wherever_they_are() {
    // MOVE: one AI draw per listed unit with a unit AI, in list order, before the turn player's
    // units update; the ordering player's lane stops
    let ordered = |id| Unit {
        id,
        ..Unit::default()
    };
    let ai = || UnitAi {
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(3000),
        ..building()
    };
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![ordered(object_id(7)), ordered(object_id(8))].into(),
                ..Player::default()
            },
            Player {
                objects: vec![live(2)].into(),
                ..Player::default()
            },
        ],
    )
    .with_ais([(object_id(7), ai()), (object_id(8), ai())])
    .with_ais(live_ais(&[2]));
    stock_masters(&mut game);
    game.tick_with(
        Millis::new(40),
        &[Command::Order(OrderCommand {
            player: player_id(0),
            units: unit_list(object_ids([8, 99, 7])),
            order: Order::Retreat(Interact::default()),
        })],
    );
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let ai = streams.get_mut(Stream::Ai);
    let (eight, seven) = (ai.rand15() % 1500 + 2250, ai.rand15() % 1500 + 2250);
    let next = 750 + ai.rand15() % 500;
    let timeout = |world: &World, list_index: ListIndex| {
        world
            .parts
            .unit_ai
            .get(world.players[0].objects[list_index].id)
            .unwrap()
            .adjusted_idle_timeout
    };
    assert_eq!(
        (
            timeout(&game, ListIndex::new(1)),
            timeout(&game, ListIndex::new(0))
        ),
        (Millis::new(eight), Millis::new(seven))
    );
    assert_eq!(
        game.parts
            .unit_ai
            .get(game.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
    assert!(game.players[0].unsupported.is_some());
}

#[test]
fn an_unread_object_stops_its_player_unless_it_dies_first() {
    let task = GenieError::unsupported("a default task the slice does not model");
    let with_task = |life| {
        Object::unread(
            Unit {
                id: object_id(5),
                life,
                ..Unit::default()
            },
            task.clone(),
        )
    };
    let run = |unit| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player(&[])],
        );
        let _ = world.push_live(PlayerId::new(0), unit).unwrap();
        stock_masters(&mut world);
        world.tick(Millis::new(10));
        world.players[0].unsupported.clone()
    };
    assert_eq!(
        run(with_task(Lifecycle::default())),
        Some((Tick::new(1), ListIndex::new(0), task.clone()))
    );
    let dying = Lifecycle {
        class: Class::Animated,
        hp: HitPoints::ZERO,
        ..Lifecycle::default()
    };
    assert_eq!(run(with_task(dying)), None);
}

/// Build 158041's farm (master 50): its default task 0 is a Make of the farm food kind 16
/// at `work_val1` 1.0, its work rate 0.4 a second, its capacity 15.
fn farm_master() -> Master {
    Master {
        id: ids::FARM,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: MasterKind::BUILDING,
        class: class::FARM,
        capacity: CarryCapacity::of_data(15),
        action: Some(ActionPart {
            work_rate: WorkRate::new(0.4),
            default_task: TaskIndex::of_data(0),
            tasks: vec![Task {
                action_kind: MAKE,
                attribute_kind: Some(attribute::FARM_FOOD),
                attribute_kind2: Some(attribute::FOOD),
                work_val1: WorkValue::new(1.0),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// Livestock whose action list is empty in state 2 (out of a building, its Artifact gone)
/// starts its default task's Artifact again (slot 0x7f0), Gaia's waiting half as long; its first
/// update comes in the next tick, as a Fly's.
#[test]
fn livestock_without_actions_starts_its_artifact_again() {
    let sheep = Object::with_actions(
        Unit {
            id: object_id(40),
            life: Lifecycle {
                master_id: master_id(594),
                state: ObjectState::Alive,
                hp: HitPoints::new(7.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action {
            default_task: DefaultTask::Artifact,
            ..Default::default()
        },
    );
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![sheep.unit].into(),
                attributes: vec![Amount::ZERO; 100],
                ..Player::default()
            },
        ],
    )
    .with_actions([(object_id(40), sheep.parts.actions)]);
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::new([Master {
            id: master_id(594),
            kind: MasterKind::COMBAT,
            class: class::LIVESTOCK,
            ..Master::default()
        }])),
    ];
    world.tick(Millis::new(50));
    let artifact = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .artifact()
        .copied()
        .expect("the Artifact again");
    assert_eq!(artifact, Artifact::new(PlayerId::new(1), None));
    // its first update checks for a new owner, finds none and waits a second
    world.tick(Millis::new(50));
    let artifact = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(
        (artifact.state, artifact.timer),
        (ArtifactState::Wait, Seconds::new(1.0))
    );
}

/// [`forage_world`]'s villager 9175 (master 120) with `set` run on its master and on itself,
/// a master 99 for the remains, then `part` on it in an update: what the part left in the
/// Step, the attribute changes and the remains.
fn life_part(
    set: impl FnOnce(&mut Master, &mut Unit),
    part: impl FnOnce(&mut Unit, &mut Action, &mut step::Step<'_>) -> GenieResult<()>,
) -> GenieResult<LifeLeft> {
    let mut world = forage_world();
    let mut master = world.model.masters[1].get(master_id(120)).unwrap().clone();
    set(
        &mut master,
        &mut world.players[1].objects[ListIndex::new(0)],
    );
    // the remains and death spawn the tests name (99, a master the game does not have)
    let left = Master {
        id: master_id(99),
        kind: MasterKind::ANIMATED,
        ..Master::default()
    };
    let ms = &mut world.model.masters[1];
    *ms = Arc::new(Masters::new(
        ms.iter()
            .filter(|master| master.id != master_id(120))
            .cloned()
            .chain([master, left]),
    ));
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let before = attribute_values(step.world, 1);
            let ids: Vec<ObjectId> = step.world.players[1]
                .objects
                .iter()
                .map(|unit| unit.id)
                .collect();
            part(unit, actions, step)?;
            let own = step.world.players[1].objects.iter();
            let made = own
                .filter(|unit| !ids.contains(&unit.id))
                .map(|unit| (unit.life.master_id.get(), unit.position()));
            Ok((gained(step.world, 1, &before), made.collect()))
        },
    )
}

/// Player `player`'s food, wood, stone and gold, then its attributes from 4 on: every attribute's
/// value in attribute order.
fn attribute_values(world: &World, player: usize) -> Vec<f32> {
    let owner = &world.players[player];
    owner
        .resources()
        .iter()
        .chain(owner.attributes.iter().skip(4))
        .copied()
        .map(Amount::get)
        .collect()
}

/// What player `player`'s attributes changed by since `before` ([`attribute_values`]): each changed
/// attribute and its change, in attribute order.
fn gained(world: &World, player: usize, before: &[f32]) -> Vec<(i16, f32)> {
    let now = attribute_values(world, player);
    let changed = now
        .iter()
        .zip(before)
        .enumerate()
        .filter(|(_, (after_value, before_value))| after_value != before_value);
    changed
        .map(|(attribute, (after_value, before_value))| {
            (attribute as i16, after_value - before_value)
        })
        .collect()
}

/// What [`life_part`] gives within the update: the owner's attribute changes and the objects
/// it made (master, place).
type LifeLeft = (Vec<(i16, f32)>, Vec<(i16, Position)>);

/// The life cycle's `set_state(3)` storage (`FUN_1411a56c0` with 0): a live object below one
/// hit point gives back each storage entry of kind 2 or 16 with an attribute (the amount
/// negated); one at a hit point, or at a NaN one, gives back nothing.
#[test]
fn dying_object_gives_back_its_storage_of_kinds_2_and_16() {
    let storage = |hp: HitPoints| {
        life_part(
            |master, unit| {
                master.storage = vec![
                    (Some(attribute::HEADROOM), Amount::new(1.0), 2),
                    (Some(attribute::POPULATION), Amount::new(5.0), 16),
                    (Some(AttributeId::named(9)), Amount::new(1.0), 1),
                    (None, Amount::new(1.0), 2),
                ];
                (unit.life.state, unit.life.hp) = (ObjectState::Alive, hp);
            },
            |unit, _, step| unit.dying_storage_leaves(step),
        )
        .unwrap()
        .0
    };
    assert_eq!(storage(HitPoints::new(0.5)), [(4, -1.0), (11, -5.0)]);
    assert_eq!(storage(HitPoints::new(1.0)), []);
    assert_eq!(storage(HitPoints::new(f32::NAN)), []);
}

/// The dying sprite's end (`RGE_Animated_Object::update`, [`Unit::dying_sprite_ended`]): the
/// animation stops and a unit (type below 80) leaves its remains (here 99) at its place within
/// its update (the dead unit lent to the world); without remains (−1) or as a building none. A
/// dead object's death spawn ([`Unit::death_spawn_comes`]) comes the same way. A master the
/// player lacks fails.
#[test]
fn dead_object_leaves_its_remains_and_its_death_spawn() {
    let ended = |kind: MasterKind, remains: Option<MasterId>| {
        life_part(
            |master, _| {
                master.kind = kind;
                master.death_spawns[1] = remains;
            },
            Unit::dying_sprite_ended,
        )
    };
    let forage_world = forage_world().players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        ended(MasterKind::COMBAT, Some(master_id(99))).unwrap().1,
        [(99, forage_world.position())]
    );
    assert_eq!(ended(MasterKind::COMBAT, None).unwrap().1, []);
    assert_eq!(
        ended(MasterKind::BUILDING, Some(master_id(99))).unwrap().1,
        []
    );
    let spawned = life_part(
        |master, _| master.death_spawns = [Some(master_id(99)), None],
        Unit::death_spawn_comes,
    );
    assert_eq!(spawned.unwrap().1, [(99, forage_world.position())]);
    let lacking = life_part(
        |_, unit| unit.life.master_id = master_id(999),
        Unit::dying_sprite_ended,
    );
    assert_eq!(
        lacking.unwrap_err().reason(),
        "an object of a master the slice does not have"
    );
}

/// A building's death spawn (`FUN_1411e5cd0` after the base slot 0x160): a farm's kept snow
/// (2) goes to a spawn that is a building; a completion's (1) does not (the spawn holds the
/// constructor's none, off the snow), nor does a spawn without a construction (a static one).
#[test]
fn dead_farm_gives_its_death_spawn_its_kept_snow() {
    let spawn_snow = |snow: Snow, kind: MasterKind| {
        let mut world = forage_world();
        let id = world.players[1].objects[ListIndex::new(0)].id;
        let spawn = Master {
            id: master_id(98),
            kind,
            building: (kind == MasterKind::BUILDING).then(BuildingMaster::default),
            ..Master::default()
        };
        let ms = &mut world.model.masters[1];
        *ms = Arc::new(Masters::new(
            ms.iter()
                .cloned()
                .map(|mut master| {
                    if master.id == master_id(120) {
                        master.death_spawns = [Some(master_id(98)), None];
                    }
                    master
                })
                .chain([spawn]),
        ));
        let construction = Construction {
            progress: Seconds::new(15.0),
            desolid: false,
            builders: BuilderCount::of_data(0),
            hit_points: MasterHitPoints::of_data(480),
            build_time: WholeSeconds::of_data(15),
            record: Obstruction {
                kind: ObstructionKind::KIND_2,
                class: ObstructionClass::NONE,
            },
            snow,
            sprite_override: false,
        };
        world
            .set_part(
                id,
                Part::Building(Box::new(Building {
                    construction: Some(construction),
                    ..Building::default()
                })),
            )
            .unwrap();
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::death_spawn_comes,
        )
        .unwrap();
        let made = world.players[1].objects.iter().last().unwrap().id;
        assert_ne!(made, id);
        world
            .parts
            .building_of(made)
            .construction
            .as_ref()
            .map(|made_construction| made_construction.snow)
    };
    assert_eq!(
        spawn_snow(Snow::Kept, MasterKind::BUILDING),
        Some(Snow::Kept)
    );
    assert_eq!(
        spawn_snow(Snow::Completed, MasterKind::BUILDING),
        Some(Snow::None)
    );
    assert_eq!(spawn_snow(Snow::Kept, MasterKind::ANIMATED), None);
}

/// A new unit's default task is its master's (slot 0x7f0 starts it once its list is empty in
/// state 2, [`crate::objects::layers::default_start`]): a death spawn of a bird's master (its
/// default task a Fly) flies, where the spawn gave only livestock its Artifact.
#[test]
fn death_spawn_of_bird_master_takes_its_fly() {
    use crate::objects::layers::DefaultTask;
    let mut world = forage_world();
    let villager = world.model.masters[1].get(master_id(120)).unwrap().clone();
    let mut bird = Master {
        id: master_id(98),
        ..villager
    };
    let action = bird.action.get_or_insert_with(Default::default);
    action.tasks[0].action_kind = FLY;
    action.default_task = TaskIndex::of_data(0);
    // the new unit's standing sprite, which its making starts
    let standing = SpriteId::named(207);
    bird.standing_sprite = Some(standing);
    let _ = Arc::make_mut(&mut world.model.graphics).insert(
        standing,
        Graphic {
            id: standing,
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    );
    let ms = &mut world.model.masters[1];
    *ms = Arc::new(Masters::new(
        ms.iter()
            .cloned()
            .map(|mut master| {
                if master.id == master_id(120) {
                    master.death_spawns = [Some(master_id(98)), None];
                }
                master
            })
            .chain([bird]),
    ));
    let id = world.players[1].objects[ListIndex::new(0)].id;
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        Unit::death_spawn_comes,
    )
    .unwrap();
    let made = world.players[1].objects.iter().last().unwrap().id;
    assert_ne!(made, id);
    assert_eq!(world.parts.actions_of(made).default_task, DefaultTask::Fly);
}

/// The Artifact's handover to Gaia (state 100, [`Unit::artifact_handover`]): player 1's sheep
/// stops and changes hands to Gaia (slot 0x70), and its Artifact checks again (state 6). The
/// world's first update runs no Artifact (`0x140aad250` from the second on).
#[test]
fn an_artifact_in_state_100_hands_its_sheep_to_gaia() {
    let master = Master {
        id: master_id(594),
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        ..Master::default()
    };
    let sheep = Object::with_actions(
        Unit {
            id: object_id(40),
            life: Lifecycle {
                master_id: master_id(594),
                state: ObjectState::Alive,
                hp: HitPoints::new(7.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_artifact(Some(Artifact {
                state: ArtifactState::ToGaia,
                ..Artifact::new(PlayerId::new(1), None)
            })),
            ..Default::default()
        },
    );
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![player(vec![].into()), player(vec![sheep.unit].into())],
    )
    .with_actions([(object_id(40), sheep.parts.actions)]);
    world.model.masters = vec![
        Arc::new(Masters::new([master.clone()])),
        Arc::new(Masters::new([master])),
    ];
    placeable(&mut world);
    let placed = Placed {
        position: Position::new(10.5, 10.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        master: master_id(594),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(object_id(40), placed);
    world.tick(Millis::new(50));
    assert_eq!(world.players[1].objects.len(), 1);
    world.tick(Millis::new(50));
    assert_eq!(
        (
            world.players[0].unsupported.clone(),
            world.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(world.players[1].objects.len(), 0);
    let sheep = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(sheep.id, object_id(40));
    assert_eq!(
        world
            .parts
            .actions_of(sheep.id)
            .list
            .artifact()
            .map(|artifact| artifact.state),
        Some(ArtifactState::Check)
    );
}

#[test]
fn farm_starts_its_make_in_its_first_update_and_fills_from_the_next() {
    let farm = Object::with_actions(
        Unit {
            id: object_id(40),
            life: Lifecycle {
                master_id: ids::FARM,
                attribute_kind: Some(attribute::FOOD),
                amount: Amount::new(15.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action {
            default_task: DefaultTask::Make,
            ..Default::default()
        },
    );
    let mut attributes = vec![Amount::ZERO; 100];
    attributes[attribute::FARM_FOOD_AMOUNT.index()] = Amount::new(175.0);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![farm.unit].into(),
            attributes,
            ..Player::default()
        }],
    )
    .with_actions([(object_id(40), farm.parts.actions)]);
    world.model.masters = vec![Arc::new(Masters::new([farm_master()]))];
    world.tick(Millis::new(104));
    let players = &world.players[0].objects[ListIndex::new(0)];
    let make = world
        .parts
        .actions_of(players.id)
        .list
        .make()
        .copied()
        .unwrap();
    assert_eq!(
        (make.state, make.remaining, make.total),
        (MakeState::Making, Amount::new(175.0), Amount::new(175.0))
    );
    assert_eq!(
        players.life.held(),
        carrying(Some(attribute::FARM_FOOD), 0.0)
    );
    assert_eq!(
        world.parts.actions_of(players.id).top_action(),
        Some(TopAction {
            kind: MAKE,
            state: ActionState::new(6)
        })
    );
    world.tick(Millis::new(104));
    let unit = &world.players[0].objects[ListIndex::new(0)];
    let made = 0.4f32 * 1.0 * Millis::new(104).seconds();
    assert_eq!(
        (
            unit.life.amount,
            world
                .parts
                .actions_of(unit.id)
                .list
                .make()
                .copied()
                .unwrap()
                .remaining
        ),
        (Amount::new(made), Amount::new(175.0 - made))
    );
    assert!(world.players[0].unsupported.is_none());
}

/// What a new object's master's default task starts (slot 0x7f0, [`default_start`]): a farm's
/// Make, a bird's Fly, livestock's Artifact; another type the factory makes is reported.
///
/// [`default_start`]: crate::objects::layers::default_start
#[test]
fn new_object_default_task_starts_by_its_master_task_kind() {
    use crate::objects::layers::DefaultTask;
    use crate::objects::layers::default_start;
    use crate::ownership::artifact::ARTIFACT_KIND;
    assert_eq!(default_start(&farm_master()), DefaultTask::Make);
    let mut other = farm_master();
    let with_kind = |master: &mut Master, kind| {
        master.action.get_or_insert_with(Default::default).tasks[0].action_kind = kind;
        default_start(master)
    };
    assert_eq!(with_kind(&mut other, FLY), DefaultTask::Fly);
    assert_eq!(with_kind(&mut other, ARTIFACT_KIND), DefaultTask::Artifact);
    assert_eq!(
        with_kind(&mut other, GATHER),
        DefaultTask::Fails(Unstartable::Unported(GATHER))
    );
    // a type the factories make nothing of starts nothing (a wild animal's Explore, 6)
    other.action.get_or_insert_with(Default::default).tasks[0].action_kind = ActionKind::named(6);
    assert_eq!(default_start(&other), DefaultTask::Nothing);
    other
        .action
        .get_or_insert_with(Default::default)
        .default_task = None;
    assert_eq!(default_start(&other), DefaultTask::Nothing);
}

/// `TRIBE_Building_Object::update`'s whole seconds (`0x1411c95a2`): a built building outside
/// the walls' branch (a barracks) has its `damaged_lately_timer` (`+0x30d`) lose one in an
/// update that passes whole seconds, however many, and stay at 0.
#[test]
fn building_damaged_lately_timer_loses_one_an_update_with_whole_seconds() {
    let barracks = Unit {
        id: object_id(40),
        life: Lifecycle {
            master_id: master_id(12),
            hp: HitPoints::new(550.0),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        damaged_lately: DamagedTimer::of_data(2),
        ..Unit::default()
    };
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![barracks].into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        }],
    );
    game.model.masters = vec![Arc::new(Masters::new([Master {
        id: master_id(12),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(550),
        building: Some(BuildingMaster::default()),
        ..Master::default()
    }]))];
    let timer = |world: &World| world.players[0].objects[ListIndex::new(0)].damaged_lately;
    game.tick(Millis::new(500));
    assert_eq!(timer(&game), DamagedTimer::of_data(2));
    game.tick(Millis::new(2600));
    assert_eq!(timer(&game), DamagedTimer::of_data(1));
    game.tick(Millis::new(1000));
    assert_eq!(timer(&game), DamagedTimer::of_data(0));
    game.tick(Millis::new(1000));
    assert_eq!(timer(&game), DamagedTimer::of_data(0));
    assert!(game.players[0].unsupported.is_none());
}

fn town_centre(id: i32) -> Unit {
    Unit {
        id: object_id(id),
        ..Unit::default()
    }
}

fn queue(building: i32, unit: i16, amount: i16) -> Command {
    Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(vec![Some(object_id(building))]),
        building_master: master_id(109),
        unit: master_id(unit),
        amount: QueuedUnits::of_data(amount),
        button: QueueButton::NONE,
    })
}

/// A town centre of master 109 ([`one_player`]'s).
fn centre(id: i32) -> Unit {
    let mut tc = town_centre(id);
    tc.life.master_id = ids::TOWN_CENTRE;
    tc
}

#[test]
fn unit_inside_heals_up_to_its_hit_points() {
    // records/425906646's update 2191: villager 27260 inside TC 27150 heals by the TC's
    // garrison heal rate once a second
    let master = Master {
        id: master_id(592),
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(25),
        ..Master::default()
    };
    let mut unit = Unit {
        id: object_id(27260),
        life: Lifecycle {
            hp: HitPoints::new(11.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut combat = Combat::default();
    unit.heal(
        &mut combat,
        master.class,
        master.hit_points,
        HitPoints::new(0.1),
        &roles(),
    )
    .unwrap();
    assert_eq!(unit.life.hp, HitPoints::new(11.1));
    // a heal past the hit points stops at them, the powerup's room taking the rest
    unit.life.hp = HitPoints::new(24.95);
    combat.hp_powerup = HpPowerup {
        taken: HitPoints::new(1.0),
        max: HitPoints::new(2.0),
    };
    unit.heal(
        &mut combat,
        master.class,
        master.hit_points,
        HitPoints::new(0.1),
        &roles(),
    )
    .unwrap();
    assert_eq!(
        (unit.life.hp, combat.hp_powerup.taken, combat.hp_powerup.max),
        (
            HitPoints::new(24.95f32 + 0.1),
            HitPoints::new(1.0),
            HitPoints::new(2.0)
        )
    );
    // past them with the room: the overflow comes off the powerup, then a full unit holds
    // exactly its master's hit points
    unit.life.hp = HitPoints::new(26.95);
    unit.heal(
        &mut combat,
        master.class,
        master.hit_points,
        HitPoints::new(0.1),
        &roles(),
    )
    .unwrap();
    assert_eq!(unit.life.hp, HitPoints::new(25.0));
    assert_eq!(
        combat.hp_powerup.taken,
        HitPoints::new(1.0 - (26.95f32 + 0.1 - 27.0))
    );
    // a paid class heals at a cost: not modelled
    let ship = Master {
        class: class::WARSHIP,
        ..master
    };
    assert!(
        unit.heal(
            &mut combat,
            ship.class,
            ship.hit_points,
            HitPoints::new(0.1),
            &roles()
        )
        .is_err()
    );
}

/// `FUN_140e6cfc0` resets every listed unit the world holds, a sleeping one too, and passes
/// over an id it does not hold.
#[test]
fn an_order_resets_its_units_sleeping_ones_too() {
    let unit = |id: i32| Unit {
        id: object_id(id),
        ..Unit::default()
    };
    let ai = || UnitAi {
        patrol_path: Some(PatrolPath::default()),
        patrol_waypoint: WaypointIndex::of_data(2),
        adjusted_idle_timeout: Millis::new(100),
        secondary_timer: Millis::new(900),
        ..UnitAi::default()
    };
    let player = Player {
        objects: vec![unit(1)].into(),
        sleeping: vec![unit(2)].into(),
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![Player::default(), player],
    )
    .with_ais([(object_id(1), ai()), (object_id(2), ai())]);
    world.reset_order_units(&object_ids([2, 9, 1])).unwrap();
    for unit in world.players[1]
        .objects
        .iter()
        .chain(&world.players[1].sleeping)
    {
        let ai = world.parts.unit_ai.get(unit.id).unwrap();
        assert_eq!(
            (ai.patrol_path.is_none(), ai.secondary_timer),
            (true, Millis::new(750))
        );
    }
}

#[test]
fn load_goes_to_its_player_alone_unless_the_player_shares() {
    // records/425906646's update 818: player 6, with allies, does not share (+0x1426 0)
    let relations = ByPlayer::new(vec![
        relation_row([0, 3, 3]),
        relation_row([3, 0, 0]),
        relation_row([3, 0, 0]),
    ]);
    let mut adds = Vec::new();
    give_load(
        (&relations, false),
        PlayerId::new(1),
        &mut adds,
        attribute::FOOD,
        Amount::new(10.0),
        &roles(),
        slots(),
    )
    .unwrap();
    assert_eq!(
        adds,
        test_ids_adds([(attribute::FOOD, 10.0), (attribute::GATHERED, 10.0)])
    );
    assert!(
        give_load(
            (&relations, true),
            PlayerId::new(1),
            &mut adds,
            attribute::GOLD,
            Amount::new(10.0),
            &roles(),
            slots()
        )
        .is_err()
    );
}

/// A gate's slot 0x678 (`FUN_1411d4a30`): a standing, unlocked object of class 39 whose
/// master transforms (`+0x9ba` not -1), between allies; any other object answers false.
#[test]
fn only_standing_unlocked_transforming_gate_lets_an_ally_pass() {
    let object = |id: i32, master: MasterId, state: ObjectState, locked: bool| {
        Object::with_building(
            Unit {
                id: object_id(id),
                life: Lifecycle {
                    master_id: master,
                    state,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                gate: Some(Gate {
                    locked,
                    ..Gate::default()
                }),
                ..Default::default()
            },
        )
    };
    let player = |objects: Vec<Unit>| Player {
        objects: objects.into(),
        relations: relation_row([0, 0]),
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let split = World::split_layers(vec![
        object(1, master_id(64), ObjectState::Alive, false),
        object(2, master_id(65), ObjectState::Alive, false),
        object(3, master_id(66), ObjectState::Alive, false),
        object(4, master_id(64), ObjectState::Dying, false),
        object(5, master_id(64), ObjectState::Alive, true),
    ]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 2],
        vec![player(Vec::new()), player(split.0)],
    )
    .with_parts(split.1);
    let master = |id: i16, class: ClassId, transform_unit: i16| Master {
        id: master_id(id),
        kind: MasterKind::BUILDING,
        class,
        building: Some(BuildingMaster {
            transform_unit: opt_master(transform_unit),
            ..BuildingMaster::default()
        }),
        ..Master::default()
    };
    // a gate's class on an object that is no building answers as the base slot 0x678: no
    let not_building = Master {
        kind: MasterKind::COMBAT,
        ..master(67, class::GATE, 63)
    };
    let masters = Masters::new([
        master(64, class::GATE, 63),
        master(65, class::GATE, -1),
        master(66, class::VILLAGER, 63),
        not_building,
    ]);
    world.model.masters = vec![Arc::new(masters.clone()), Arc::new(masters)];
    let mut passes: Vec<bool> = (0..5)
        .map(|index| {
            world
                .gate_passes(PlayerId::new(1), ListIndex::new(index), PlayerId::new(1))
                .unwrap()
        })
        .collect();
    world.players[1].objects[ListIndex::new(0)].life.master_id = master_id(67);
    passes.push(
        world
            .gate_passes(PlayerId::new(1), ListIndex::new(0), PlayerId::new(1))
            .unwrap(),
    );
    assert_eq!(passes, [true, false, false, false, false, false]);
}

#[test]
fn gate_opens_for_its_allies_while_they_update() {
    // records/425906646's update 636: player 2's gate 27687 (master 64, class 39 with a
    // transform unit) has no record, marks or shape while its team updates, so 27274's plan
    // passes it; the next player, an enemy, finds it closed again
    let gate = Object::with_building(
        Unit {
            id: object_id(27687),
            life: Lifecycle {
                master_id: master_id(64),
                state: ObjectState::Alive,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Building {
            gate: Some(Gate::default()),
            ..Default::default()
        },
    );
    let player = |objects: Vec<Unit>, relations: Vec<u8>| Player {
        objects: objects.into(),
        relations: relation_row(relations),
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        vec![
            player(Vec::new(), vec![0, 3, 3, 3]),
            player(Vec::new(), vec![3, 0, 0, 3]),
            player(vec![gate.unit], vec![3, 0, 0, 3]),
            player(Vec::new(), vec![3, 3, 3, 0]),
        ],
    )
    .with_buildings([(object_id(27687), gate.parts.building)]);
    let master = Master {
        id: master_id(64),
        kind: MasterKind::BUILDING,
        class: class::GATE,
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class: ObstructionClass::GATE,
        building: Some(BuildingMaster {
            transform_unit: Some(master_id(63)),
            ..BuildingMaster::default()
        }),
        ..Master::default()
    };
    game.model.masters = (0..4)
        .map(|_| Masters::new([master.clone()]))
        .map(Arc::new)
        .collect();
    game.spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![1; 400]),
    );
    game.spatial.relations = game
        .players
        .iter()
        .map(|player| player.relations.clone())
        .collect();
    let record = Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::GATE,
    };
    game.spatial.insert(
        object_id(27687),
        Placed {
            position: Position::new(10.0, 10.5, 0.0),
            radius: Footprint::new(1.0, 0.5),
            class: class::GATE,
            player: PlayerId::new(2),
            master: master_id(64),
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: Some(record),
        },
    );
    let blocked = |world: &World| {
        world
            .spatial
            .tiles()
            .word(Tile::new(9, 10))
            .is_some_and(|word| word & BLOCKED != 0)
    };
    assert!(blocked(&game));
    let mut open = vec![false; 4];
    // player 1, an ally, opens both its own and player 2's gates
    game.open_gates(PlayerId::new(1), &mut open).unwrap();
    assert_eq!(open, [false, true, true, false]);
    assert!(
        game.spatial
            .get(object_id(27687))
            .unwrap()
            .obstruction
            .is_none()
    );
    assert!(!blocked(&game));
    // player 2 is next and an ally of both: they stay open
    game.close_gates(PlayerId::new(1), &mut open).unwrap();
    assert_eq!(open, [false, true, true, false]);
    game.open_gates(PlayerId::new(2), &mut open).unwrap();
    // player 3, next, is an enemy: they close
    game.close_gates(PlayerId::new(2), &mut open).unwrap();
    assert_eq!(open, [false; 4]);
    assert_eq!(
        game.spatial.get(object_id(27687)).unwrap().obstruction,
        Some(record)
    );
    assert!(blocked(&game));
    game.open_gates(PlayerId::new(3), &mut open).unwrap();
    assert_eq!(open, [false, false, false, true]);
    assert!(blocked(&game));
    // a locked gate stays closed for its allies
    game.close_gates(PlayerId::new(3), &mut open).unwrap();
    game.parts
        .building_mut(game.players[2].objects[ListIndex::new(0)].id)
        .unwrap()
        .gate = Some(Gate {
        locked: true,
        ..Gate::default()
    });
    game.open_gates(PlayerId::new(1), &mut open).unwrap();
    assert!(blocked(&game));
}

/// records/425906646's gate 27745 of player 3 (annexes 27746 and 27747) in update 2865: an
/// ally walking between the posts opens it; a standing Gaia animal does not count, an enemy
/// counts but is no ally, a wall next to it neither.
#[test]
fn gate_counts_the_moving_units_between_its_posts() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(40, 40),
        Tiles::new(GridSize::new(40, 40), vec![1; 1600]),
    );
    spatial.relations = ByPlayer::new(vec![
        relation_row([0, 3, 3]),
        relation_row([3, 0, 3]),
        relation_row([3, 3, 0]),
    ]);
    let placed = |x: f32, class, player, speed, kind| Placed {
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master: ids::ID_0,
        obstruction_kind: kind,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: Speed::ZERO < speed,
        unknown: None,
        obstruction: Some(Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
    };
    spatial.insert(
        object_id(27745),
        placed(
            20.0,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionKind::KIND_2,
        ),
    );
    spatial.insert(
        object_id(27746),
        placed(
            18.5,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionKind::KIND_2,
        ),
    );
    spatial.insert(
        object_id(27747),
        placed(
            21.5,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionKind::KIND_2,
        ),
    );
    spatial.insert(
        object_id(1),
        placed(
            19.6,
            class::VILLAGER,
            PlayerId::new(1),
            Speed::new(0.8),
            ObstructionKind::CIRCLE,
        ),
    );
    let (units, allied) = gate_units(
        &mut spatial,
        object_id(27745),
        &[object_id(27746), object_id(27747)],
        objects_ref::ObjectsRef::empty(),
    )
    .unwrap();
    assert_eq!((units, allied), (vec![object_id(1)], 1));
    spatial.insert(
        object_id(2),
        placed(
            20.4,
            class::PREDATOR,
            PlayerId::new(0),
            Speed::ZERO,
            ObstructionKind::CIRCLE,
        ),
    );
    spatial.insert(
        object_id(3),
        placed(
            20.8,
            class::CLASS_6,
            PlayerId::new(2),
            Speed::new(0.9),
            ObstructionKind::CIRCLE,
        ),
    );
    spatial.insert(
        object_id(4),
        placed(
            21.0,
            class::WALL,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionKind::KIND_2,
        ),
    );
    let (units, allied) = gate_units(
        &mut spatial,
        object_id(27745),
        &[object_id(27746), object_id(27747)],
        objects_ref::ObjectsRef::empty(),
    )
    .unwrap();
    assert_eq!((units, allied), (vec![object_id(1), object_id(3)], 1));
    assert!(
        gate_units(
            &mut spatial,
            object_id(27745),
            &[object_id(27746)],
            objects_ref::ObjectsRef::empty()
        )
        .is_err()
    );
}

/// records/425906646's gate 27745 of player 1 (annexes 27746 and 27747, master 88: open, no
/// height, its `transform_unit` the shut 91 of height 2; 92 transforms into nothing) at (20, 20)
/// with the moving soldier 1 (player 1's object 3) between its posts, at world time 400.
fn gate_world() -> World {
    let gate_master = |id: i16, transform_unit: i16, height| Master {
        id: master_id(id),
        kind: MasterKind::BUILDING,
        class: class::GATE,
        height,
        hit_points: MasterHitPoints::of_data(1650),
        // a gate moves (its master's slot 0xe8): its last move time is the mover's
        building: Some(BuildingMaster {
            transform_unit: opt_master(transform_unit),
            stack_unit: None,
            head_unit: None,
            foundation_terrain: None,
            tech: None,
            annexes: [None; 4],
            ..BuildingMaster::default()
        }),
        ..moving_master(walker().stats)
    };
    let soldier = Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        ..Master::default()
    };
    let unit = |id, master, x: f32, annex_of| {
        Object::with_building(
            Unit {
                id,
                place: Static::at(Position::new(x, 20.0, 0.0)),
                life: Lifecycle {
                    master_id: master,
                    state: ObjectState::Alive,
                    hp: HitPoints::new(10.0),
                    ..Lifecycle::default()
                },
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
            Building {
                annex_of,
                ..Default::default()
            },
        )
    };
    let mut gate = unit(object_id(27745), master_id(88), 20.0, None);
    gate.parts.building.gate = Some(Gate::default());
    gate.parts.building.annexes =
        Annexes::new([Some(object_id(27746)), Some(object_id(27747)), None, None]);
    gate.parts.motion = Some(Motion::Moving(walker()));
    let split = World::split_layers(vec![
        gate,
        unit(
            object_id(27746),
            master_id(88),
            18.5,
            Some(MainBuildingId::new(object_id(27745))),
        ),
        unit(
            object_id(27747),
            master_id(88),
            21.5,
            Some(MainBuildingId::new(object_id(27745))),
        ),
        unit(object_id(1), master_id(4), 19.6, None),
    ]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: split.0.into(),
                ..Player::default()
            },
        ],
    )
    .with_parts(split.1);
    world.world_time = WorldTime::from_millis(400);
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::new([
            gate_master(88, 91, Distance::new(0.0)),
            gate_master(91, 88, Distance::new(2.0)),
            gate_master(92, -1, Distance::new(0.0)),
            soldier,
        ])),
    ];
    world.spatial = Spatial::with_tiles(
        GridSize::new(40, 40),
        Tiles::new(GridSize::new(40, 40), vec![1; 1600]),
    );
    world.spatial.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([3, 0])]);
    let placed = |x: f32, class, speed, kind| Placed {
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player: PlayerId::new(1),
        master: ids::ID_0,
        obstruction_kind: kind,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: Speed::ZERO < speed,
        unknown: None,
        obstruction: Some(Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
    };
    world.spatial.insert(
        object_id(27745),
        placed(20.0, class::GATE, Speed::ZERO, ObstructionKind::KIND_2),
    );
    world.spatial.insert(
        object_id(27746),
        placed(18.5, class::GATE, Speed::ZERO, ObstructionKind::KIND_2),
    );
    world.spatial.insert(
        object_id(27747),
        placed(21.5, class::GATE, Speed::ZERO, ObstructionKind::KIND_2),
    );
    world.spatial.insert(
        object_id(1),
        placed(
            19.6,
            class::CLASS_6,
            Speed::new(0.8),
            ObstructionKind::CIRCLE,
        ),
    );
    world
}

/// [`gate_world`]'s gate as the long pather asks it (`FUN_140f9eb70` on an allied gate's record,
/// [`crate::pathing::long_path::gate_lets_pass`]): a tile of the gate's footprint leads to its
/// anchor, whose first object of class 39 is the gate; it lets its own player pass while it
/// stands below state 3, unlocked, of a master that transforms; an enemy, a tile of no building
/// and a gate that is locked, dying or of a master without a transform stay closed.
#[test]
fn the_long_pather_asks_the_gate_on_its_anchor() {
    use crate::pathing::long_path::gate_lets_pass;
    fn gate(world: &mut World) -> &mut Unit {
        world.players[1]
            .objects
            .by_id_mut(object_id(27745))
            .unwrap()
    }
    let mut gate_world = gate_world();
    let mut visibility = VisionVisibility::new(
        GridSize::new(40, 40),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    );
    let _ = visibility
        .link_building(
            Position::new(20.0, 20.0, 0.0),
            Footprint::new(1.5, 0.5),
            tuning().visibility,
        )
        .unwrap();
    let side = Tile::new(19, 20);
    let anchor = visibility.anchor_of(side).unwrap();
    assert_ne!(anchor, side);
    gate_world.spatial.sight = Some(visibility);
    let ask = |world: &World, player: u8, tile| {
        gate_lets_pass(&world.spatial, world.objects(), PlayerId::new(player), tile).unwrap()
    };
    assert!(
        gate_world
            .spatial
            .tile_list(anchor)
            .contains(&object_id(27745))
    );
    assert!(ask(&gate_world, 1, side) && ask(&gate_world, 1, anchor));
    assert!(!ask(&gate_world, 0, side));
    assert!(!ask(&gate_world, 1, Tile::new(5, 5)));
    gate_world
        .parts
        .building_mut(object_id(27745))
        .unwrap()
        .gate = Some(Gate {
        locked: true,
        ..Gate::default()
    });
    assert!(!ask(&gate_world, 1, side));
    gate_world
        .parts
        .building_mut(object_id(27745))
        .unwrap()
        .gate = Some(Gate::default());
    gate(&mut gate_world).life.state = ObjectState::Dying;
    assert!(!ask(&gate_world, 1, side));
    gate(&mut gate_world).life.state = ObjectState::Alive;
    gate(&mut gate_world).life.master_id = master_id(92);
    assert!(!ask(&gate_world, 1, side));
    gate_world.spatial.sight = None;
    assert!(!ask(&gate_world, 1, side));
}

/// The player's gate toggle (`FUN_14162f8c0`) on records/425906646's gate 27745 (annexes 27746
/// and 27747): the open gate (master 88) becomes its `transform_unit`, the shut 91 (a height),
/// which kills the moving soldier between its posts and takes the world time plus 1 as its last
/// move time; the shut one opens again (no height) without either. A gate without a
/// `transform_unit` stays; one with masters of its own is not modelled.
#[test]
fn gate_toggle_shuts_on_the_units_between_its_posts_or_opens() {
    let mut gate_world = gate_world();
    let state = |world: &World| {
        let unit = &world.players[1].objects[ListIndex::new(0)];
        (
            unit.life.master_id.get(),
            world.parts.mover_of(unit.id).map(|mover| mover.last_move),
            world.players[1].objects[ListIndex::new(3)].life.hp,
        )
    };
    gate_world
        .toggle_gate(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    assert_eq!(
        state(&gate_world),
        (91, Some(WorldTime::from_millis(401)), HitPoints::ZERO)
    );
    gate_world.players[1].objects[ListIndex::new(3)].life.hp = HitPoints::new(10.0);
    gate_world.world_time = WorldTime::from_millis(500);
    gate_world
        .toggle_gate(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    assert_eq!(
        state(&gate_world),
        (88, Some(WorldTime::from_millis(401)), HitPoints::new(10.0))
    );
    gate_world.players[1].objects[ListIndex::new(0)]
        .life
        .master_id = master_id(92);
    gate_world
        .toggle_gate(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    assert_eq!(
        gate_world.players[1].objects[ListIndex::new(0)]
            .life
            .master_id,
        master_id(92)
    );
    gate_world
        .parts
        .building_mut(gate_world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .gate = Some(Gate {
        other_masters: true,
        ..Gate::default()
    });
    assert!(
        gate_world
            .toggle_gate(PlayerId::new(1), ListIndex::new(0))
            .is_err()
    );
}

/// [`Unit::gate_update`] of [`gate_world`]'s gate as master `master` with its gate state `gate`:
/// the gate's master within the update after it (its toggle's, [`World::toggle_gate`]), its
/// state after, or the error.
fn gate_update_of(
    master: MasterId,
    gate: Option<Gate>,
    setup: impl FnOnce(&mut World),
) -> GenieResult<(MasterId, Option<Gate>)> {
    let mut world = gate_world();
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    players.life.master_id = master;
    world.parts.building_mut(object_id(27745)).unwrap().gate = gate;
    setup(&mut world);
    let master = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            unit.gate_update(actions, step, &roles())?;
            Ok(unit.life.master_id)
        },
    )?;
    Ok((
        master,
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .gate,
    ))
}

/// The gate's update (`FUN_1411e7730`): a shut (height 2, master 91), unlocked gate with an ally
/// moving between its posts opens within its update (its player's toggle: master 88), its close
/// timer 0; a locked one stays shut. An open gate (no height) with nothing in it starts its close
/// timer at 0.5 s, runs it down by the world's dt (0.05 s), and at 0 shuts within its update.
/// Its first update clears every time it runs; a gate whose master transforms into nothing is
/// passed over, and one without its gate state is not modelled.
#[test]
fn gate_opens_for_an_ally_inside_and_shuts_after_its_close_timer() {
    let gate = |locked, close_timer| {
        Some(Gate {
            locked,
            close_timer,
            first_update: true,
            ..Gate::default()
        })
    };
    let ran = |locked, close_timer| {
        Some(Gate {
            first_update: false,
            ..gate(locked, close_timer).unwrap()
        })
    };
    assert_eq!(
        gate_update_of(master_id(91), gate(false, Seconds::new(0.3)), |_| ()),
        Ok((master_id(88), ran(false, Seconds::ZERO)))
    );
    assert_eq!(
        gate_update_of(master_id(91), gate(true, Seconds::new(0.3)), |_| ()),
        Ok((master_id(91), ran(true, Seconds::new(0.3))))
    );
    // the soldier stands aside: nothing in the open gate
    let aside = |world: &mut World| {
        let placed = *world.spatial.get(object_id(1)).unwrap();
        world.spatial.insert(
            object_id(1),
            Placed {
                position: Position::new(20.0, 30.0, 0.0),
                ..placed
            },
        );
    };
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::ZERO), aside),
        Ok((master_id(88), ran(false, GATE_CLOSE_DELAY)))
    );
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.5)), aside),
        Ok((master_id(88), ran(false, Seconds::new(0.5 - 0.05))))
    );
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.05)), aside),
        Ok((master_id(91), ran(false, Seconds::ZERO)))
    );
    // the soldier in the open gate holds its timer
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.5)), |_| ()),
        Ok((master_id(88), ran(false, Seconds::new(0.5))))
    );
    assert_eq!(
        gate_update_of(master_id(92), gate(false, Seconds::new(0.5)), |_| ()),
        Ok((master_id(92), gate(false, Seconds::new(0.5))))
    );
    let err = gate_update_of(master_id(91), None, |_| ()).unwrap_err();
    assert_eq!(err.reason(), "a gate without its gate state");
}

#[test]
fn the_forced_replay_takes_the_game_state_for_what_froze() {
    // a frozen unit takes the game's (and runs again), a frozen unit the game no longer
    // holds goes, a unit that ran keeps the slice's, a stopped player takes its resources
    let mut world =
        one_player(vec![live(7), live(8), live(9)], [0.0; 4]).with_ais(live_ais(&[7, 8, 9]));
    world.players[0].objects[ListIndex::new(0)].stopped = true;
    world.players[0].objects[ListIndex::new(2)].stopped = true;
    world.players[0].stopped = false;
    let game_unit = |id, timer| {
        let ai = UnitAi {
            retarget_timer: timer,
            ..building()
        };
        Object::with_ai(
            Unit {
                id,
                ..Unit::default()
            },
            ai,
        )
    };
    let game = GameState {
        frame_seed: FrameSeed::of_data(77),
        players: vec![vec![
            game_unit(object_id(7), Millis::new(900)),
            game_unit(object_id(8), Millis::new(901)),
        ]],
        resources: vec![[1.0, 2.0, 3.0, 4.0]],
        ..GameState::default()
    };
    assert_eq!(world.force(&game), vec![]);
    assert_eq!(world.frame_seed, FrameSeed::of_data(77));
    assert_eq!(ids(&world), vec![7, 8]);
    assert_eq!(retarget(&world), vec![900, 40]);
    assert!(world.players[0].objects.iter().all(|unit| !unit.stopped));
    // a stopped player runs again with the game's resources; what differed is forced too
    world.players[0].stopped = true;
    let game = GameState {
        resync: vec![object_id(8)],
        ..game
    };
    let _ = world.force(&game);
    assert_eq!(
        (
            world.players[0].stopped,
            world.players[0].resources().map(Amount::get)
        ),
        (false, [1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(retarget(&world), vec![900, 901]);
    // a game object the slice never made is named, not added
    let made = GameState {
        players: vec![vec![
            game_unit(object_id(7), Millis::new(900)),
            game_unit(object_id(8), Millis::new(901)),
            game_unit(object_id(10), Millis::new(5)),
        ]],
        ..GameState::default()
    };
    assert_eq!(
        world.force(&made),
        vec![(
            PlayerId::new(0),
            Some(object_id(10)),
            GenieError::unsupported("an object the game made and the slice did not")
        )]
    );
}

/// The forced replay's removal of a frozen object the game no longer holds is the leave of an
/// object whose update returned 1 ([`World::leave_world`]): its owner's watched list lets it go
/// (`FUN_141091310`) and a recyclable master's missile goes to the pool of type 60.
#[test]
fn the_forced_replay_removal_leaves_as_an_update_leave_does() {
    let missile = Unit {
        id: object_id(9),
        stopped: true,
        life: Lifecycle {
            master_id: master_id(54),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = one_player(vec![live(7), missile], [0.0; 4]).with_ais(live_ais(&[7]));
    world
        .set_part(
            object_id(9),
            Part::Missile(Some(Missile {
                max_range: Distance::ZERO,
                fired_from: None,
                charge: false,
                flying: true,
                step: Offset::ZERO,
            })),
        )
        .unwrap();
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: master_id(54),
        kind: MasterKind::MISSILE,
        recyclable: RecycleMode::RECYCLED,
        ..Master::default()
    }]))];
    world.players[0].watched.push(Watched {
        id: object_id(9),
        cell: Cell::Tile(0),
        bits: VisionBits::NONE,
    });
    let game = GameState {
        players: vec![vec![Object::with_ai(live(7), live_ai())]],
        ..GameState::default()
    };
    assert_eq!(world.force(&game), vec![]);
    assert_eq!(ids(&world), vec![7]);
    assert_eq!(world.players[0].watched, Vec::new());
    assert_eq!(world.recycling.missiles, [(object_id(9), true)]);
}

/// A player with the villagers' masters (a population of one each) and room for more.
fn one_player(objects: Vec<Unit>, resources: [f32; 4]) -> World {
    let mut attributes = attributes(250, resources);
    (attributes[4], attributes[11], attributes[32]) =
        (Amount::new(5.0), Amount::new(4.0), Amount::new(10.0));
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: objects.into(),
            attributes,
            ..Player::default()
        }],
    );
    // the town centre (109, work rate 1); the villagers (46 food, a population of one, 25
    // seconds at the centre); master 99 without costs or population, and 97 with a population
    // of one, made in 2 seconds
    let trained = |id, time| Master {
        id,
        kind: MasterKind::COMBAT,
        available: Availability::AVAILABLE,
        creatable: Some(CreatablePart {
            trained_at: vec![ids::TOWN_CENTRE],
            train_locations: vec![TrainLocation {
                location: Some(ids::TOWN_CENTRE),
                time,
                button: LocationButton::of_data(1),
                hotkey: HotkeyId::of_data(0),
            }],
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let villager = |id| Master {
        class: class::VILLAGER,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 2),
        ],
        creatable: Some(CreatablePart {
            cost: costs(&[(attribute::FOOD, 46, true), (attribute::HEADROOM, 1, false)]),
            ..trained(id, WholeSeconds::of_data(25))
                .creatable
                .unwrap_or_default()
        }),
        ..trained(id, WholeSeconds::of_data(25))
    };
    let centre = Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        action: Some(ActionPart {
            work_rate: WorkRate::new(1.0),
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    let quick = Master {
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 2),
        ],
        ..trained(master_id(97), WholeSeconds::of_data(2))
    };
    world.model.masters = vec![Arc::new(Masters::new([
        villager(VILLAGER_MALE),
        villager(VILLAGER_FEMALE),
        trained(master_id(99), WholeSeconds::of_data(25)),
        quick,
        centre,
    ]))];
    stock_masters(&mut world);
    world
}

#[test]
fn queue_pays_its_cost_and_the_building_starts_making() {
    let mut world = one_player(vec![centre(9)], [200.0; 4]);
    world.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let player = &world.players[0];
    assert_eq!(player.resources(), amounts([154.0, 200.0, 200.0, 200.0]));
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    // the make action works in the tick it is created in
    let Some(Making::Unit(make)) = production.action else {
        panic!("a make action");
    };
    assert_eq!(
        (make.progress, make.state, make.master),
        (
            Seconds::new(0.104),
            MakeObjState::Making,
            production.queue.slots[0].master
        )
    );
    let queue = &production.queue;
    assert_eq!(
        (
            queue.count.get(),
            queue.total.get(),
            queue.slots[0].train_time,
            queue.slots[0].costs[0]
        ),
        (
            1,
            1,
            WholeSeconds::of_data(25),
            (Some(attribute::FOOD), Amount::new(46.0))
        )
    );
    assert!(player.unsupported.is_none());
}

#[test]
fn queued_villager_is_male_or_female_by_an_action_stream_draw() {
    let mut world = one_player(vec![centre(9)], [200.0; 4]);
    world.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let draw = streams.get_mut(Stream::Action).raw64();
    let expected = if draw.wrapping_mul(100) % 100 < 50 {
        Some(VILLAGER_MALE)
    } else {
        Some(master_id(293))
    };
    assert_eq!(
        world
            .parts
            .production
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .unwrap()
            .queue
            .slots[0]
            .master,
        expected
    );
}

#[test]
fn queue_the_player_cannot_pay_or_at_missing_building_queues_nothing() {
    // too little food for the villager (46 here): no entry, no cost, no draw (records/
    // 425906646's update 31: player 5's refused QUEUE leaves player 6's draws alone)
    let mut poor = one_player(vec![centre(9)], [40.0, 0.0, 0.0, 0.0]);
    poor.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    assert!(poor.players[0].unsupported.is_none());
    assert_eq!(poor.players[0].attributes[0], Amount::new(40.0));
    let tc = &poor.players[0].objects[ListIndex::new(0)];
    assert!(
        poor.parts
            .production
            .get(tc.id)
            .is_none_or(|production| production.queue.entries().is_empty())
    );
    let mut idle = one_player(vec![centre(9)], [40.0, 0.0, 0.0, 0.0]);
    idle.tick_with(Millis::new(104), &[]);
    assert_eq!(
        poor.streams.get_mut(Stream::Action).draws,
        idle.streams.get_mut(Stream::Action).draws
    );
    // the packet's check drops a building the world lacks and refuses the packet
    let mut missing = one_player(vec![centre(9)], [200.0; 4]);
    missing.tick_with(Millis::new(104), &[queue(9 - 1, 83, 1)]);
    assert!(missing.players[0].unsupported.is_none());
    assert_eq!(missing.players[0].resources(), amounts([200.0; 4]));
}

/// `FUN_141614450` over two town centres: unit by unit the one with room and the least load
/// (`FUN_1411d0e90`) takes it, the last pick kept on a tie: five of master 99 (25 seconds each)
/// go to 9, 10, 10, 9, 9.
#[test]
fn queue_at_several_buildings_shares_its_units_out_by_load() {
    let several = |amount| {
        Command::Queue(QueueCommand {
            player: player_id(0),
            buildings: unit_list(object_ids([9, 10])),
            building_master: master_id(109),
            unit: master_id(99),
            amount,
            button: QueueButton::NONE,
        })
    };
    let total = |world: &World, list_index: ListIndex| {
        world
            .parts
            .production
            .get(world.players[0].objects[list_index].id)
            .map_or(0, |production| production.queue.total.get())
    };
    let mut world = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    world.tick_with(Millis::new(104), &[several(QueuedUnits::of_data(5))]);
    assert_eq!(
        (
            total(&world, ListIndex::new(0)),
            total(&world, ListIndex::new(1))
        ),
        (3, 2)
    );
    // a centre with work queued: 25 seconds less the work done the update gave it
    let mut world = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    world.tick_with(Millis::new(104), &[queue(9, 99, 1)]);
    let Some(Making::Unit(make)) = world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .and_then(|production| production.action)
    else {
        panic!("a make action");
    };
    assert_eq!(
        world
            .production_load(
                PlayerId::new(0),
                &world.players[0].objects[ListIndex::new(0)]
            )
            .unwrap(),
        Seconds::new(25.0) - make.progress
    );
    assert_eq!(
        world
            .production_load(
                PlayerId::new(0),
                &world.players[0].objects[ListIndex::new(1)]
            )
            .unwrap(),
        Seconds::ZERO
    );
    // the empty one takes the first, then the loaded one the second (25 above its load)
    world.tick_with(Millis::new(104), &[several(QueuedUnits::of_data(2))]);
    assert_eq!(
        (
            total(&world, ListIndex::new(0)),
            total(&world, ListIndex::new(1))
        ),
        (2, 1)
    );
}

/// `FUN_141614450` gives a building room and load only where the unit trains (`FUN_141263610`,
/// [`World::trains_at`]): a building of another master keeps room −1 and takes none.
/// 427647190's players queued villagers at their trophy (master 1649, which trains none) and
/// the game queued nothing there.
#[test]
fn queue_at_building_that_does_not_train_the_unit_queues_nothing_there() {
    let trophy = |id| {
        let mut unit = town_centre(id);
        unit.life.master_id = master_id(1649);
        unit
    };
    let with_trophy = |objects| {
        let mut one_player = one_player(objects, [200.0; 4]);
        let master = Master {
            id: master_id(1649),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        one_player.model.masters = vec![Arc::new(Masters::new(
            one_player.model.masters[0].iter().cloned().chain([master]),
        ))];
        one_player
    };
    let total = |world: &World, list_index: ListIndex| {
        world
            .parts
            .production
            .get(world.players[0].objects[list_index].id)
            .map_or(0, |production| production.queue.total.get())
    };
    let both = Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(object_ids([9, 10])),
        building_master: master_id(109),
        unit: master_id(99),
        amount: QueuedUnits::of_data(3),
        button: QueueButton::NONE,
    });
    let mut world = with_trophy(vec![trophy(9), centre(10)]);
    world.tick_with(Millis::new(104), &[both]);
    assert!(world.players[0].unsupported.is_none());
    assert_eq!(
        (
            total(&world, ListIndex::new(0)),
            total(&world, ListIndex::new(1))
        ),
        (0, 3)
    );
    // the trophy alone: no room, nothing queued, nothing paid
    let mut alone = with_trophy(vec![trophy(9)]);
    alone.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    assert!(alone.players[0].unsupported.is_none());
    assert_eq!(alone.players[0].resources(), amounts([200.0; 4]));
    assert!(
        !alone
            .parts
            .production
            .contains(alone.players[0].objects[ListIndex::new(0)].id)
    );
}

/// `FUN_141614450` with players ordering each other's (`+0x1074`): a building of another
/// player takes part as its own, and its part (`FUN_1411cbc40`) queues and pays for its owner
/// (`+0x138`); the cost check before reads the ordering player's stock. Without the setting the
/// packet's check drops it.
#[test]
fn queue_at_another_player_building_queues_and_pays_for_its_owner() {
    let two = |cheats| {
        let mut one_player = one_player(vec![], [200.0; 4]);
        let mut other = one_player.players[0].clone();
        other.objects = vec![centre(10)].into();
        other.attributes[..4].copy_from_slice(&[
            Amount::new(100.0),
            Amount::ZERO,
            Amount::ZERO,
            Amount::ZERO,
        ]);
        one_player.players.push(other);
        one_player.reindex();
        one_player.player_time.push(Millis::ZERO);
        one_player
            .model
            .masters
            .push(one_player.model.masters[0].clone());
        one_player
            .settings
            .flags
            .set(OptionFlags::CHEATS_ENABLED, cheats);
        one_player
    };
    let mut world = two(true);
    world.tick_with(Millis::new(104), &[queue(10, 83, 1)]);
    assert!(
        world
            .players
            .iter()
            .all(|player| player.unsupported.is_none())
    );
    assert_eq!(world.players[0].resources(), amounts([200.0; 4]));
    assert_eq!(world.players[1].attributes[0], Amount::new(54.0));
    let production = world
        .parts
        .production
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(production.queue.total, QueuedUnits::ONE);
    let mut world = two(false);
    world.tick_with(Millis::new(104), &[queue(10, 83, 1)]);
    assert_eq!(world.players[1].attributes[0], Amount::new(100.0));
    assert!(
        !world
            .parts
            .production
            .contains(world.players[1].objects[ListIndex::new(0)].id)
    );
}

/// `FUN_141614450` reads a listed object live or sleeping (`FUN_140e6b750`): a sleeping one the
/// unit does not train at keeps room −1, and the town centre beside it takes the unit.
#[test]
fn queue_passes_over_sleeping_object_the_unit_does_not_train_at() {
    let mut world = one_player(vec![centre(9)], [200.0; 4]);
    let mut asleep = town_centre(11);
    asleep.life.master_id = master_id(99);
    let _ = world.push_asleep(PlayerId::new(0), asleep).unwrap();
    let both = Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(object_ids([11, 9])),
        building_master: master_id(109),
        unit: master_id(99),
        amount: QueuedUnits::of_data(2),
        button: QueueButton::NONE,
    });
    world.tick_with(Millis::new(104), &[both]);
    assert!(world.players[0].unsupported.is_none());
    let production = world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(production.queue.total, QueuedUnits::of_data(2));
    assert!(
        !world
            .parts
            .production
            .contains(world.players[0].sleeping[ListIndex::new(0)].id)
    );
}

/// RESEARCH's building (`0x140e7e270`): with more than one listed, the least loaded of the
/// packet's and the listed ones the world holds; a tie keeps the one taken; one alone is the
/// packet's.
#[test]
fn research_at_several_buildings_takes_the_least_loaded() {
    let mut world = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    world.tick_with(Millis::new(104), &[queue(9, 99, 1)]);
    let at = |player, building, count, buildings: Vec<i32>| ResearchCommand {
        player,
        building,
        count,
        buildings: object_ids(buildings),
        tech: TechId::of_data(0),
        queue_id: QueueId::of_data(-1),
        shift: false,
    };
    let loaded = |research_command: ResearchCommand| {
        world
            .least_loaded(&research_command, &research_command.buildings)
            .unwrap()
    };
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(9)), 3, vec![9, 77, 10])),
        Some(object_id(10))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(10)), 2, vec![10, 9])),
        Some(object_id(10))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(9)), 1, vec![9, 10])),
        Some(object_id(9))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(77)), 1, vec![])),
        None
    );
}

/// `do_command`'s case 0x65 (`FUN_140e6f470`), RESEARCH's check before its handler: listing
/// none is refused; the packet's building, when the world holds it, must be the player's own
/// (any while players may order each other's), and one listed alone must be held; of more
/// listed, another player's or one the world lacks becomes −1, and a human's left without one
/// is refused.
#[test]
fn research_check_refuses_another_player_buildings() {
    let mut world = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    world.players.push(Player::default());
    world.reindex();
    let at = |player, building, count, buildings: Vec<i32>| ResearchCommand {
        player,
        building,
        count,
        buildings: object_ids(buildings),
        tech: TechId::of_data(0),
        queue_id: QueueId::of_data(-1),
        shift: false,
    };
    // its own: listed alone, or with the others checked
    assert_eq!(
        world
            .research_check(&at(player_id(0), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        Some(vec![])
    );
    assert_eq!(
        world
            .research_check(&at(player_id(0), Some(object_id(9)), 3, vec![9, 77, 10]))
            .unwrap(),
        Some(object_ids([9, -1, 10]))
    );
    assert_eq!(
        world
            .research_check(&at(player_id(0), Some(object_id(9)), 0, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        world
            .research_check(&at(player_id(0), Some(object_id(9)), -2, vec![]))
            .unwrap(),
        None
    );
    // player 1's: another's building, one alone the world lacks, a list of none of its own
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(9)), 2, vec![9, 10]))
            .unwrap(),
        None
    );
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(77)), 1, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(77)), 2, vec![9, 10]))
            .unwrap(),
        None
    );
    // a building the world lacks with a list skips the packet's own test
    assert_eq!(
        world
            .research_check(&at(player_id(0), Some(object_id(77)), 2, vec![77, 10]))
            .unwrap(),
        Some(object_ids([-1, 10]))
    );
    // a computer's list left without one still runs
    world.players[1].kind = PlayerKind::Computer;
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(77)), 2, vec![9, 10]))
            .unwrap(),
        Some(object_ids([-1, -1]))
    );
    // while players may order each other's
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    world.players[1].kind = PlayerKind::Human;
    assert_eq!(
        world
            .research_check(&at(player_id(1), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        Some(vec![])
    );
}

/// RESEARCH (`0x140e7e270`) of Castle Age (0x66) at the town centre: queued as any technology;
/// only in game mode 16 does it mark the world's treaty monitor (`+0x820`'s `+0x17`), which the
/// slice does not hold.
#[test]
fn research_of_castle_age_queues_it_but_in_game_mode_16() {
    use crate::technology::{Location, TechDef, TechState};
    let castle = |mode| {
        let mut world = one_player(vec![centre(9)], [1000.0; 4]);
        world.settings.game_mode = mode;
        world.model.techs = Arc::new(vec![TechDef::default(); 0x67]);
        let mut techs = vec![TechState::default(); 0x67];
        techs[0x66] = TechState {
            state: ResearchState::Available,
            research_cap: ResearchCap::of_data(-1),
            building_master: None,
            locations: vec![Location {
                building: Some(ids::TOWN_CENTRE),
                time: WholeSeconds::of_data(160),
                button: LocationButton::of_data(1),
                adjustment: Seconds::ZERO,
            }],
            ..TechState::default()
        };
        world.players[0].techs = techs;
        let at = ResearchCommand {
            player: player_id(0),
            building: Some(object_id(9)),
            count: 1,
            buildings: vec![],
            tech: TechId::of_data(0x66),
            queue_id: QueueId::of_data(-1),
            shift: false,
        };
        let research_listed = research_listed(&mut world, &at);
        let queued = world
            .parts
            .production
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .map_or(0, |production| production.queue.total.get());
        (research_listed, queued)
    };
    let (result, queued) = castle(GameMode::of_data(0));
    assert_eq!((result.is_ok(), queued), (true, 1));
    assert!(castle(GameMode::of_data(16)).0.is_err());
}

/// [`one_player`]'s town centre 9 with a unit AI and technology 5, available (state 1) at the
/// centre's master 109, its first cost 50 food.
fn research_world() -> World {
    use crate::technology::{Cost, Location, TechDef, TechState};
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    world.parts.set_unit_ai(
        world.players[0].objects[ListIndex::new(0)].id,
        Some(UnitAi::default()),
    );
    world.model.techs = Arc::new(vec![TechDef::default(); 6]);
    Arc::make_mut(&mut world.model.techs)[5].costs[0] = Cost {
        attribute: Some(attribute::FOOD),
        amount: CostAmount::of_data(50),
        paid: CostPaid::PLAYER,
    };
    let mut techs = vec![TechState::default(); 6];
    techs[5] = TechState {
        state: ResearchState::Available,
        research_cap: ResearchCap::of_data(-1),
        building_master: None,
        overrides: [
            CostOverride {
                amount: CostAmount::ZERO,
                attribute: None,
                used: OverrideUse::DAT,
            },
            CostOverride::default(),
            CostOverride::default(),
        ],
        locations: vec![Location {
            building: Some(ids::TOWN_CENTRE),
            time: WholeSeconds::of_data(30),
            button: LocationButton::of_data(1),
            adjustment: Seconds::ZERO,
        }],
        ..TechState::default()
    };
    world.players[0].techs = techs;
    world
}

/// RESEARCH of technology `tech` at player 0's building 9 with the target `target`.
fn research_at(tech: i16, target: i32) -> ResearchCommand {
    ResearchCommand {
        player: player_id(0),
        building: Some(object_id(9)),
        count: 1,
        buildings: vec![],
        tech: TechId::of_data(tech),
        queue_id: QueueId::of_data(target),
        shift: false,
    }
}

/// RESEARCH (`0x140e7e270`): its listed costs come off the pending debit, and an alive building
/// at one of the technology's locations queues it, its unit AI on the action value 0x26c; a
/// target, a technology the tree lacks, one neither available nor repeatable, a building
/// nowhere, a dead one or one at no location do nothing past the debit.
#[test]
fn research_queues_at_an_alive_location_after_its_costs_come_off_the_debit() {
    let queued = |world: &World| {
        world
            .parts
            .production
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .map_or(0, |production| production.queue.total.get())
    };
    let mut game = research_world();
    research_listed(&mut game, &research_at(5, -1)).unwrap();
    assert_eq!(queued(&game), 1);
    assert_eq!(
        game.players[0].pending_debit,
        [-50.0, 0.0, 0.0, 0.0].map(Amount::new)
    );
    let ai = game
        .parts
        .unit_ai
        .get(game.players[0].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(ai.current_action, Some(AiAction::Research));
    let untouched = research_world();
    for (tech, target) in [(5, 9), (6, -1), (-1, -1)] {
        let mut world = research_world();
        research_listed(&mut world, &research_at(tech, target)).unwrap();
        assert_eq!(world, untouched);
    }
    let refused = |change: &dyn Fn(&mut World)| {
        let mut world = research_world();
        change(&mut world);
        research_listed(&mut world, &research_at(5, -1)).unwrap();
        (queued(&world), world.players[0].pending_debit[0])
    };
    assert_eq!(
        refused(&|world| world.players[0].techs[5].state = ResearchState::Researching),
        (0, Amount::new(-50.0))
    );
    assert_eq!(
        refused(&|world| world.players[0].objects[ListIndex::new(0)].id = object_id(8)),
        (0, Amount::new(-50.0))
    );
    assert_eq!(
        refused(
            &|world| world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying
        ),
        (0, Amount::new(-50.0))
    );
    assert_eq!(
        refused(&|world| world.players[0].techs[5].locations[0].building = Some(master_id(110))),
        (0, Amount::new(-50.0))
    );
}

/// A queued research starts in the building's production list (`0x140ad1d80`): the technology
/// takes state 2 at the building's master, the action state 6 with no work done.
#[test]
fn queued_research_starts_at_the_buildings_master() {
    let mut world = research_world();
    research_listed(&mut world, &research_at(5, -1)).unwrap();
    world
        .produce(PlayerId::new(0), ListIndex::new(0), Seconds::ZERO)
        .unwrap();
    let master = world.players[0].objects[ListIndex::new(0)].life.master_id;
    let action = world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .and_then(|production| production.action);
    let Some(Making::Research(make_tech)) = action else {
        panic!("a research action: {action:?}");
    };
    assert_eq!(
        (make_tech.state, make_tech.progress),
        (MakeTechState::Researching, Seconds::ZERO)
    );
    assert_eq!(world.players[0].techs[5].state, ResearchState::Researching);
    assert_eq!(world.players[0].techs[5].building_master, Some(master));
}

/// RESEARCH's unmodelled parts: a technology the data file lacks, the shared kind, a linked
/// technology, a building of another player, a computer player's research and a building with
/// an unmodelled action.
#[test]
fn research_unmodelled_parts_fail() {
    let fails = |change: &dyn Fn(&mut World)| {
        let mut research_world = research_world();
        change(&mut research_world);
        research_listed(&mut research_world, &research_at(5, -1))
            .unwrap_err()
            .reason()
    };
    assert_eq!(
        fails(&|world| Arc::make_mut(&mut world.model.techs).truncate(5)),
        "a technology the data file has not got"
    );
    assert_eq!(
        fails(&|world| Arc::make_mut(&mut world.model.techs)[5].kind = TechKind::SHARED),
        "RESEARCH of the shared kind (FUN_1411c3fd0 over the packet's buildings)"
    );
    assert_eq!(
        fails(&|world| {
            let _ = Arc::make_mut(&mut world.model.linked_techs).insert(tech_id(5), Linked::Cycle);
        }),
        "RESEARCH of a linked technology (FUN_140f3a0f0, FUN_140f39e80)"
    );
    assert_eq!(
        fails(&|world| {
            let tc = world
                .remove_live(PlayerId::new(0), ListIndex::new(0))
                .unwrap();
            world.players.push(Player {
                objects: vec![tc.unit].into(),
                ..Player::default()
            });
            world.reindex();
            world.model.masters.push(world.model.masters[0].clone());
        }),
        "RESEARCH at another player's object"
    );
    assert_eq!(
        fails(&|world| world.players[0].kind = PlayerKind::Computer),
        "a computer player's RESEARCH (FUN_1411d5520, player+0x7a8)"
    );
    assert_eq!(
        fails(&|world| {
            world.set_unread(
                world.players[0].objects[ListIndex::new(0)].id,
                GenieError::unsupported("an action"),
            );
        }),
        "RESEARCH at a building with an unmodelled action"
    );
}

/// `FUN_1411cbc40`: a queue adds its units' population to attribute 80 (the queued units),
/// which keeps it until the unit spawns; the make action's start counts the one it makes in
/// 81.
#[test]
fn queue_counts_its_units_population_as_queued() {
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    world.tick_with(Millis::new(104), &[queue(9, 83, 3)]);
    let amounts = &world.players[0].attributes;
    assert_eq!(
        (amounts[80], amounts[81]),
        (Amount::new(3.0), Amount::new(1.0))
    );
    // a unit without population (master 99, no storage of kind 4) adds none
    let mut other = one_player(vec![centre(9)], [1000.0; 4]);
    other.tick_with(Millis::new(104), &[queue(9, 99, 2)]);
    assert_eq!(other.players[0].attributes[80], Amount::ZERO);
}

#[test]
fn queue_takes_as_many_as_the_room_and_the_resources_allow() {
    // food for two of three: two entries, two draws
    let mut two = one_player(vec![centre(9)], [100.0, 0.0, 0.0, 0.0]);
    two.tick_with(Millis::new(104), &[queue(9, 83, 3)]);
    let mut one = one_player(vec![centre(9)], [100.0, 0.0, 0.0, 0.0]);
    one.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let queued = |world: &World| {
        world
            .parts
            .production
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .unwrap()
            .queue
            .total
    };
    assert_eq!(queued(&two), QueuedUnits::of_data(2));
    assert_eq!(
        two.players[0].attributes[0],
        Amount::new(100.0 - 2.0 * 46.0)
    );
    assert_eq!(
        two.streams.get_mut(Stream::Action).draws,
        one.streams.get_mut(Stream::Action).draws + 1
    );
    // 14 queued: room for one of five
    let mut full = one_player(vec![centre(9)], [1000.0; 4]);
    full.tick_with(Millis::new(104), &[queue(9, 99, 14), queue(9, 99, 5)]);
    assert_eq!(queued(&full), QueuedUnits::of_data(15));
    // a computer player fills to 2
    let mut computer = one_player(vec![centre(9)], [1000.0; 4]);
    computer.players[0].kind = PlayerKind::Computer;
    computer.tick_with(Millis::new(104), &[queue(9, 99, 5)]);
    assert_eq!(queued(&computer), QueuedUnits::of_data(2));
}

/// `FUN_1411cbc40` joins units of the last entry's master and button whose costs are the same;
/// another button opens an entry with that button's train time; a full buffer of ten grows by
/// ten, the serials numbering the entries.
#[test]
fn queued_units_join_the_last_entry_and_full_buffer_grows() {
    let with = |amount, button| {
        Command::Queue(QueueCommand {
            player: player_id(0),
            buildings: unit_list(object_ids([9])),
            building_master: master_id(109),
            unit: master_id(99),
            amount,
            button,
        })
    };
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    world.tick_with(
        Millis::new(104),
        &[
            with(QueuedUnits::of_data(2), QueueButton::NONE),
            with(QueuedUnits::of_data(3), QueueButton::NONE),
            with(QueuedUnits::ONE, QueueButton::of_data(1)),
        ],
    );
    let queue = &world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(
        (
            queue.count.get(),
            queue.total.get(),
            queue.changes.get(),
            queue.next_serial.get()
        ),
        (2, 6, 3, 2)
    );
    assert_eq!(
        (
            queue.slots[0].count.get(),
            queue.slots[1].count.get(),
            queue.slots[1].button
        ),
        (5, 1, QueueButton::of_data(1))
    );
    assert_eq!(
        (queue.slots[1].train_time, queue.slots[1].serial.get()),
        (WholeSeconds::of_data(25), 1)
    );
    assert_eq!(world.players[0].tracker.queued, 6);
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    let alternating: Vec<_> = (0..11)
        .map(|index| with(QueuedUnits::ONE, QueueButton::of_data(index % 2)))
        .collect();
    world.tick_with(Millis::new(104), &alternating);
    let queue = &world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(
        (queue.slots.len(), queue.count.get(), queue.total.get()),
        (20, 11, 11)
    );
    let serials: Vec<_> = queue
        .entries()
        .iter()
        .map(|entry| entry.serial.get())
        .collect();
    assert_eq!(serials, (0..11).collect::<Vec<i64>>());
}

/// `FUN_1411e7dc0`: a computer player's housed building cancels its queue
/// (`FUN_1411cff00`): the villager's 46 food come back and attribute 80 empties; a disabled
/// queue (`+0xbf6` clear) makes nothing.
#[test]
fn computer_player_housed_building_cancels_its_queue() {
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    world.players[0].kind = PlayerKind::Computer;
    world.players[0].attributes[4] = Amount::new(0.5);
    world.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let players = &world.players[0];
    assert_eq!(
        (
            players.attributes[0],
            players.attributes[80],
            players.attributes[81]
        ),
        (Amount::new(1000.0), Amount::ZERO, Amount::ZERO)
    );
    let production = world
        .parts
        .production
        .get(players.objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (production.queue.count.get(), production.queue.total.get()),
        (0, 0)
    );
    assert!(production.action.is_none());
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    let tc = world.players[0].objects[ListIndex::new(0)].id;
    world
        .set_part(
            tc,
            Part::Production(Some(Box::new(Production {
                queue: Queue {
                    enabled: false,
                    ..Queue::default()
                },
                ..Production::default()
            }))),
        )
        .unwrap();
    world.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let player = &world.players[0];
    assert_eq!(
        (player.attributes[80], player.attributes[81]),
        (Amount::new(1.0), Amount::ZERO)
    );
    assert!(
        world
            .parts
            .production
            .get(player.objects[ListIndex::new(0)].id)
            .unwrap()
            .action
            .is_none()
    );
}

/// Game mode 16 with attribute 6 at most 1: a military unit (master 99, class 0) is queued
/// one at a time, the player's flag (`+0x1378`) set; a second QUEUE queues nothing.
#[test]
fn game_mode_16_queues_one_military_unit_at_time() {
    let mut world = one_player(vec![centre(9)], [1000.0; 4]);
    world.settings.game_mode = GameMode::of_data(16);
    world.tick_with(Millis::new(104), &[queue(9, 99, 3)]);
    world.tick_with(Millis::new(104), &[queue(9, 99, 2)]);
    let queue = &world
        .parts
        .production
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(queue.total, QueuedUnits::ONE);
    assert!(world.players[0].one_military_queued);
}

#[test]
fn an_unmodelled_command_stops_its_player() {
    let mut world = one_player(vec![town_centre(9)], [200.0; 4]);
    let why = GenieError::unsupported("QUEUE at two buildings");
    world.tick_with(
        Millis::new(104),
        &[Command::Unmodelled(UnmodelledCommand {
            player: player_id(0),
            packet: Vec::new(),
            why: why.clone(),
        })],
    );
    assert_eq!(
        world.players[0].unsupported,
        Some((Tick::new(1), ListIndex::new(0), why))
    );
}

/// The masters of the ownership changes' tests, by player: Gaia's (0), player 1's and
/// player 2's archer 4 (class 0, its dead archer 3 of type 10; line of sight 6, 7, 8; fog
/// visibility 0, 1, 2; hit points 30, 35, 40) and sheep 705 (class 58; line of sight 2, 3,
/// 4; fog visibility 0, 1, 2; food 100); player 2 lacks the dead archer, and the dead
/// archer of player 1 is of type 25 (a doppelganger's, which the copy leaves).
fn hands_masters() -> Vec<Arc<Masters>> {
    let table = |player: u8| {
        let archer = Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            class: class::CLASS_0,
            line_of_sight: Distance::new(6.0 + f32::from(player)),
            fog_visibility: FogVisibility::of_data(player),
            hit_points: MasterHitPoints::of_data(30 + 5 * i16::from(player)),
            death_spawns: [Some(master_id(3)), None],
            storage: vec![
                (Some(attribute::HEADROOM), Amount::new(1.0), 2),
                (Some(attribute::POPULATION), Amount::ZERO, 0),
                (None, Amount::ZERO, 0),
            ],
            ..Master::default()
        };
        let dead = Master {
            id: master_id(3),
            kind: if player == 1 {
                MasterKind::DOPPELGANGER
            } else {
                MasterKind::STATIC
            },
            class: class::CLASS_0,
            hit_points: MasterHitPoints::of_data(9 + i16::from(player)),
            ..Master::default()
        };
        let sheep = Master {
            id: master_id(705),
            kind: MasterKind::COMBAT,
            class: class::LIVESTOCK,
            line_of_sight: Distance::new(2.0 + f32::from(player)),
            fog_visibility: FogVisibility::of_data(player),
            storage: vec![(Some(attribute::FARM_FOOD), Amount::new(100.0), 1)],
            ..Master::default()
        };
        if player == 2 {
            Masters::new([archer, sheep])
        } else {
            Masters::new([archer, dead, sheep])
        }
    };
    vec![Arc::new(table(0)), Arc::new(table(1)), Arc::new(table(2))]
}

/// A 40 × 40 world of three players' sight with the registry, holding object `id` of player
/// `player_id`, class `class` and master `master` at (10.5, 10.5) in player `player_id`'s registry
/// list, its sight on.
pub(crate) fn hands_world(
    id: i32,
    player_id: PlayerId,
    class: ClassId,
    master: MasterId,
    los: Distance,
) -> Spatial {
    let tiles = 40 * 40;
    let mut spatial = Spatial::new(GridSize::new(40, 40));
    let mut visibility = VisionVisibility::new(
        GridSize::new(40, 40),
        (0..3)
            .map(|_| VisibleMap {
                counts: vec![-1; tiles],
                explored: TileCount::ZERO,
            })
            .collect(),
        vec![VisionBits::NONE; tiles],
        Vec::new(),
        0,
        vec![PlayerKind::Human; 3],
    );
    let mut registry = Registry::new(3, 65);
    registry
        .entries
        .get_or_insert_with(object_id(id), Default::default)
        .unwrap()
        .mask = VisionBits::from_bits_retain(1 << player_id.get());
    registry
        .update_unit(UnitUpdate {
            id: object_id(id),
            owner: PlayerId::new(player_id.index() as u8),
            position: Point::new(10.5, 10.5),
            radii: Footprint::new(0.2, 0.2),
            class,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1 << player_id.get()),
        })
        .unwrap();
    visibility.registry = Some(registry);
    let at = Position::new(10.5, 10.5, 0.0);
    visibility.add_seer(
        object_id(id),
        Seer {
            player: player_id,
            master,
            los,
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: capture_kind(player_id, class, master, &roles()),
            kind: MasterKind::COMBAT,
        },
    );
    let _ = visibility.sight_on(
        object_id(id),
        at,
        Footprint::new(0.2, 0.2),
        SightShape::Circle,
        SightCentre::Tile,
        SightLayer {
            roles: &roles(),
            visibility_tuning: tuning().visibility,
        },
    );
    spatial.sight = Some(visibility);
    spatial.insert(
        object_id(id),
        Placed {
            position: at,
            radius: Footprint::new(0.2, 0.2),
            class,
            player: player_id,
            master,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    spatial
}

/// A live unit `id` of master `master` with a unit AI holding a notify.
/// Unit `id` of master `master` at (10.5, 10.5) (its unit AI [`hands_ai`]).
fn hands_unit(id: i32, master: MasterId) -> Unit {
    Unit {
        id: object_id(id),
        place: Static::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// The unit AI of [`hands_unit`] `id`: a notify of its own damage.
fn hands_ai(id: i32) -> UnitAi {
    let mut ai = UnitAi::default();
    ai.notifies
        .push(Notify::own(object_id(id), NotifyKind::DAMAGED, [1, 2, 3]));
    ai
}

/// [`change_hands`] of `unit` (its unit AI `ai`) with the players' tables `masters` and the own
/// masters `own`: the own master it answers goes into `own`; whether it changed hands.
fn hands(
    (unit, ai): (&mut Unit, Option<&mut UnitAi>),
    owners: (usize, PlayerId),
    (masters, own): (&[Arc<Masters>], &mut PartTable<Master>),
    spatial: &mut Spatial,
    now: WorldTime,
) -> GenieResult<bool> {
    // none of these objects holds an action: an empty layer
    let mut actions = Action::default();
    let answer = change_hands(
        (unit, &mut actions, ai),
        owners,
        MasterTables::new(masters, own),
        spatial,
        now,
        &roles(),
        tuning().visibility,
    )?;
    Ok(answer
        .map(|master| {
            let _ = own.insert(unit.id, *master);
        })
        .is_some())
}

/// `FUN_14120f800` on MarvinBlue/426048681's Gaia archer 21712 (master 4) that player 1
/// takes over: its records and place take player 1, its sight moves from Gaia's map to
/// player 1's with Gaia's line of sight, it takes a master of its own (a copy of Gaia's,
/// with player 1's fog visibility) and a copy of Gaia's dead archer, its unit AI's notifies
/// go with the world time and it tells its unit AI it changed hands.
#[test]
fn taken_gaia_archer_keeps_copy_of_gaia_master_with_its_new_owner_fog() {
    let masters = hands_masters();
    let mut own = PartTable::default();
    let mut spatial = hands_world(
        21712,
        PlayerId::new(0),
        class::CLASS_0,
        master_id(4),
        Distance::new(6.0),
    );
    let mut unit = hands_unit(21712, master_id(4));
    let mut unit_ai = hands_ai(21712);
    let dead = dead_master_of(&unit, 0, MasterTables::new(&masters, &own), None);
    assert_eq!(
        hands(
            (&mut unit, Some(&mut unit_ai)),
            (0, PlayerId::new(1)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::from_millis(1250)
        ),
        Ok(true)
    );
    let mine = own.get(unit.id).unwrap();
    assert_eq!(
        (mine.hit_points, mine.line_of_sight, mine.fog_visibility),
        (
            MasterHitPoints::of_data(30),
            Distance::new(6.0),
            FogVisibility::VISIBLE
        )
    );
    assert_eq!(
        dead.as_ref().map(|master| master.hit_points),
        Some(MasterHitPoints::of_data(9))
    );
    assert_eq!(
        spatial.get(object_id(21712)).map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
    let visibility = spatial.sight.as_ref().unwrap();
    let registry = visibility.registry.as_ref().unwrap();
    assert_eq!(registry.lists[0][0][0].owner, PlayerId::new(1));
    let seer = visibility.seer(object_id(21712)).copied().unwrap();
    assert_eq!(
        (seer.player, seer.los),
        (PlayerId::new(1), Distance::new(6.0))
    );
    let tile = |x: usize, y: usize| y * 40 + x;
    assert_eq!(visibility.maps[0].counts[tile(10, 10)], 0);
    assert_eq!(visibility.maps[1].counts[tile(16, 10)], 1);
    let ai = &unit_ai;
    assert_eq!(ai.last_update_time, Millis::new(1250));
    assert_eq!(
        ai.notifies,
        [Notify::own(
            object_id(21712),
            NotifyKind::OWNER_CHANGED,
            [1, 0, 0]
        )]
    );
}

/// `FUN_14120f800` on a sheep: its master of its own takes the new owner's line of sight
/// (class 58), which its sight on the new owner's map reads; its master has no death spawn.
/// A master of its own it had stays (with the new fog visibility), and so does a dead
/// master; a dead master of a type the copy leaves (25) gives none.
#[test]
fn taken_sheep_sees_with_its_new_owner_line_of_sight() {
    let masters = hands_masters();
    let mut own = PartTable::default();
    let mut spatial = hands_world(
        9520,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(705),
        Distance::new(3.0),
    );
    let mut unit = hands_unit(9520, master_id(705));
    let mut unit_ai = hands_ai(9520);
    assert_eq!(
        dead_master_of(&unit, 1, MasterTables::new(&masters, &own), None),
        None
    );
    assert_eq!(
        hands(
            (&mut unit, Some(&mut unit_ai)),
            (1, PlayerId::new(2)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    let mine = own.get(unit.id).unwrap();
    assert_eq!(
        (mine.line_of_sight, mine.fog_visibility),
        (Distance::new(4.0), FogVisibility::VISIBLE_IF_ALIVE)
    );
    let visibility = spatial.sight.as_ref().unwrap();
    assert_eq!(
        visibility.seer(object_id(9520)).map(|seer| seer.los),
        Some(Distance::new(4.0))
    );
    assert_eq!(visibility.maps[2].counts[10 * 40 + 14], 1);
    // back to player 1: the copy stays, with player 1's fog and line of sight
    assert_eq!(
        hands(
            (&mut unit, Some(&mut unit_ai)),
            (2, PlayerId::new(1)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    let mine = own.get(unit.id).unwrap();
    assert_eq!(
        (mine.line_of_sight, mine.fog_visibility),
        (Distance::new(3.0), FogVisibility::VISIBLE)
    );
    // player 1's archer: its dead archer is of type 25, and none is copied
    let mut spatial = hands_world(
        7,
        PlayerId::new(1),
        class::CLASS_0,
        master_id(4),
        Distance::new(7.0),
    );
    let mut unit = hands_unit(7, master_id(4));
    let mut other_ai = hands_ai(7);
    assert_eq!(
        dead_master_of(&unit, 1, MasterTables::new(&masters, &own), None),
        None
    );
    assert_eq!(
        hands(
            (&mut unit, Some(&mut other_ai)),
            (1, PlayerId::new(0)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    // a dead master it holds stays: none is made
    assert_eq!(
        dead_master_of(
            &unit,
            0,
            MasterTables::new(&masters, &own),
            Some(&Master::default())
        ),
        None
    );
    assert_eq!(
        hands(
            (&mut unit, Some(&mut other_ai)),
            (0, PlayerId::new(1)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    assert_eq!(
        own.get(unit.id).map(|master| master.hit_points),
        Some(MasterHitPoints::of_data(35))
    );
}

/// A unit of player `player_id` at (`x`, 10.5) for the owner search of [`hands_world`]'s object: a
/// combat-type seer in state 2 on its tile's list.
fn owner_search_unit(spatial: &mut Spatial, id: i32, player_id: PlayerId, x: f32) {
    spatial.sight.as_mut().unwrap().add_seer(
        object_id(id),
        Seer {
            player: player_id,
            master: master_id(4),
            los: Distance::new(6.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: capture_kind(player_id, class::CLASS_0, master_id(4), &roles()),
            kind: MasterKind::COMBAT,
        },
    );
    spatial.insert(
        object_id(id),
        Placed {
            position: Position::new(x, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::CLASS_0,
            player: player_id,
            master: master_id(4),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
}

/// Players of attribute 97 `attribute_97` and game status `status`, as an Artifact's owner
/// search reads them.
fn artifact_players(attribute_97: &[f32], status: &[GameStatus]) -> Vec<Player> {
    status
        .iter()
        .enumerate()
        .map(|(player, &game_status)| {
            let mut attributes = vec![Amount::ZERO; 98];
            attributes[97] = Amount::new(attribute_97.get(player).copied().unwrap_or(0.0));
            Player {
                attributes,
                game_status,
                ..Player::default()
            }
        })
        .collect()
}

/// The Artifact action's owner search (`FUN_140aabc40`) passes over the units of a player that
/// is out (`+0x89` not 0) and over such a player as a taker: 428265662's Gaia sheep 27530 in
/// update 642 stays Gaia's next to the units of player 7, who resigned in update 428.
#[test]
fn an_owner_search_passes_over_player_out_of_the_game() {
    let unit = owner_search_unit;
    let mut spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut spatial, 1, PlayerId::new(1), 11.5);
    let none = [0.0; 3];
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(1)))
    );
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[GameStatus::Playing, GameStatus::Lost, GameStatus::Playing]
            ),
            &roles()
        ),
        Ok(None)
    );
    // of two players, the nearest takes it: player 2 once player 1 is out
    unit(&mut spatial, 2, PlayerId::new(2), 12.5);
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(1)))
    );
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[GameStatus::Playing, GameStatus::Lost, GameStatus::Playing]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(2)))
    );
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[GameStatus::Playing, GameStatus::Lost, GameStatus::Won]
            ),
            &roles()
        ),
        Ok(None)
    );
}

/// The owner search's own owner and attribute 97 (`FUN_140aabc40`): a unit of the animal's own
/// owner keeps it, and so does that owner's attribute 97 above 0 for livestock; another
/// player's attribute 97 above 0 counts its units 100 more, so it takes the animal from its
/// owner. A unit of a class that never counts (a building's, 3) and a Gaia unit with `+0x924`
/// set do not count; an animal inside another object searches about its record there.
#[test]
fn an_owner_search_keeps_the_owner_animal_unless_attribute_97_takes_it() {
    let unit = owner_search_unit;
    let none = [0.0; 3];
    let mut spatial = hands_world(
        27530,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut spatial, 1, PlayerId::new(1), 11.5);
    unit(&mut spatial, 2, PlayerId::new(2), 11.5);
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(None)
    );
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &[0.0, 0.0, 1.0],
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(2)))
    );
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &[0.0, 1.0, 1.0],
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(None)
    );
    let mut spatial = hands_world(
        27530,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut spatial, 0, PlayerId::new(0), 11.5);
    unit(&mut spatial, 3, PlayerId::new(2), 12.5);
    let building = Placed {
        class: class::BUILDING,
        ..*spatial.get(object_id(3)).unwrap()
    };
    spatial.insert(object_id(3), building);
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(None)
    );
    let mut spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut spatial, 1, PlayerId::new(1), 11.5);
    spatial.go_inside(object_id(27530), Position::new(10.5, 10.5, 0.0));
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(1)))
    );
}

/// The owner search's unmodelled parts: an animal the world does not hold, a unit of a player
/// beyond the lists, and a stopped unit that counts and may have moved onto the tiles.
#[test]
fn an_owner_search_unmodelled_parts_fail() {
    let none = [0.0; 3];
    let reason = |spatial: &Spatial, me, status: &[GameStatus]| {
        spatial
            .artifact_owner(me, &artifact_players(&none, status), &roles())
            .unwrap_err()
            .reason()
    };
    let spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    assert_eq!(
        reason(
            &spatial,
            object_id(99),
            &[
                GameStatus::Playing,
                GameStatus::Playing,
                GameStatus::Playing
            ]
        ),
        "an owner search of an object the world does not hold"
    );
    let mut spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    owner_search_unit(&mut spatial, 1, PlayerId::new(3), 11.5);
    assert_eq!(
        reason(
            &spatial,
            object_id(27530),
            &[
                GameStatus::Playing,
                GameStatus::Playing,
                GameStatus::Playing
            ]
        ),
        "an owner search's unit of a player beyond the list"
    );
    let mut spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    owner_search_unit(&mut spatial, 1, PlayerId::new(1), 20.5);
    let stopped = Placed {
        unknown: Some(10.0),
        ..*spatial.get(object_id(1)).unwrap()
    };
    spatial.insert(object_id(1), stopped);
    assert_eq!(
        reason(
            &spatial,
            object_id(27530),
            &[
                GameStatus::Playing,
                GameStatus::Playing,
                GameStatus::Playing
            ]
        ),
        "an owner search near a stopped unit that counts"
    );
}

/// The owner search walks the tiles' lists (`+0x28`, `FUN_140aabc40`), as the game does: a unit
/// on a full tile (`0xfa` objects: `FUN_140f5f0a0` files no more) is not on its list and does not
/// count, one on the next tile does.
#[test]
fn an_owner_search_counts_only_the_units_on_the_tiles_lists() {
    let mut spatial = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    for id in 1000..1250 {
        spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(11.5, 10.5, 0.0),
                radius: Footprint::new(0.2, 0.2),
                class: class::CLASS_0,
                player: PlayerId::new(0),
                master: master_id(4),
                obstruction_kind: ObstructionKind::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    owner_search_unit(&mut spatial, 1, PlayerId::new(1), 11.5);
    let none = [0.0; 3];
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(None)
    );
    owner_search_unit(&mut spatial, 2, PlayerId::new(1), 12.5);
    assert_eq!(
        spatial.artifact_owner(
            object_id(27530),
            &artifact_players(
                &none,
                &[
                    GameStatus::Playing,
                    GameStatus::Playing,
                    GameStatus::Playing
                ]
            ),
            &roles()
        ),
        Ok(Some(PlayerId::new(1)))
    );
}

/// `FUN_14120f800` for a new owner whose table lacks the master writes the new owner into
/// the records and changes nothing else; one inside another (or not in state 2) changes
/// hands without moving its sight.
#[test]
fn an_ownership_change_without_the_master_writes_the_records_alone() {
    let mut masters = hands_masters();
    let mut own = PartTable::default();
    masters[2] = Arc::new(Masters::new([]));
    let mut spatial = hands_world(
        21712,
        PlayerId::new(0),
        class::CLASS_0,
        master_id(4),
        Distance::new(6.0),
    );
    let before = spatial.sight.as_ref().unwrap().maps.clone();
    let mut unit = hands_unit(21712, master_id(4));
    let mut unit_ai = hands_ai(21712);
    assert_eq!(
        hands(
            (&mut unit, Some(&mut unit_ai)),
            (0, PlayerId::new(2)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::from_millis(9)
        ),
        Ok(false)
    );
    let visibility = spatial.sight.as_ref().unwrap();
    assert_eq!(
        visibility.registry.as_ref().unwrap().lists[0][0][0].owner,
        PlayerId::new(2)
    );
    assert_eq!(visibility.maps, before);
    assert_eq!(
        visibility.seer(object_id(21712)).map(|seer| seer.player),
        Some(PlayerId::new(0))
    );
    assert_eq!(
        spatial.get(object_id(21712)).map(|placed| placed.player),
        Some(PlayerId::new(0))
    );
    assert!(!own.contains(unit.id));
    assert_eq!(unit_ai.notifies.len(), 1);
    unit.life.inside = Some(ContainerId::new(object_id(100)));
    assert_eq!(
        hands(
            (&mut unit, Some(&mut unit_ai)),
            (0, PlayerId::new(1)),
            (&masters, &mut own),
            &mut spatial,
            WorldTime::from_millis(9)
        ),
        Ok(true)
    );
    let visibility = spatial.sight.as_ref().unwrap();
    assert_eq!(visibility.maps, before);
    assert_eq!(
        visibility
            .seer(object_id(21712))
            .map(|seer| (seer.player, seer.los)),
        Some((PlayerId::new(1), Distance::new(6.0)))
    );
    assert_eq!(
        spatial.get(object_id(21712)).map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
}

/// `RGE_Combat_Object::update` on MarvinBlue/426048681's Gaia archer 21712 in update 1: its
/// mask now holds player 1's bit, so player 1 takes it over (slot 0x70) and `+0x924` goes 0;
/// it leaves Gaia's list for the end of player 1's, its class count with it (attribute 40).
/// A player whose table lacks the master ends the loop without taking it; outside the
/// standard modes a computer player that does not allow it is passed over.
#[test]
fn gaia_archer_player_sees_goes_to_that_player() {
    let world = |masters: Vec<Arc<Masters>>, computer: bool, mode: i32| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    objects: vec![hands_unit(21712, master_id(4))].into(),
                    attributes: vec![Amount::ZERO; 300],
                    ..Player::default()
                },
                Player {
                    objects: vec![live(500)].into(),
                    attributes: vec![Amount::ZERO; 300],
                    kind: if computer {
                        PlayerKind::Computer
                    } else {
                        PlayerKind::Human
                    },
                    ..Player::default()
                },
                Player {
                    attributes: vec![Amount::ZERO; 300],
                    ..Player::default()
                },
            ],
        )
        .with_ais([(object_id(21712), hands_ai(21712))])
        .with_ais(live_ais(&[500]));
        world.settings.game_mode = GameMode::of_data(mode);
        world.players[0].attributes[40] = Amount::new(1.0);
        world.model.masters = masters;
        world.spatial = hands_world(
            21712,
            PlayerId::new(0),
            class::CLASS_0,
            master_id(4),
            Distance::new(6.0),
        );
        let visibility = world.spatial.sight.as_mut().unwrap();
        visibility.unified[10 * 40 + 10] = VisionBits::from_bits_retain(0b111);
        world
    };
    let mut game = world(hands_masters(), false, 0);
    game.tick(Millis::new(100));
    assert_eq!(game.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = game.players[1].objects.iter().map(|unit| unit.id).collect();
    assert_eq!(ids, [object_id(500), object_id(21712)]);
    let archer = &game.players[1].objects[ListIndex::new(1)];
    assert_eq!(game.spatial.capture_of(archer.id), CaptureKind::None);
    assert_eq!(
        game.parts
            .own_master
            .get(archer.id)
            .map(|master| master.fog_visibility),
        Some(FogVisibility::VISIBLE)
    );
    let visibility = game.spatial.sight.as_ref().unwrap();
    assert_eq!(
        visibility
            .seer(object_id(21712))
            .map(|seer| (seer.player, seer.capture)),
        Some((PlayerId::new(1), CaptureKind::None))
    );
    assert_eq!(
        [
            game.players[0].attributes[40],
            game.players[1].attributes[40]
        ],
        amounts([0.0, 1.0])
    );
    // player 1 lacks the master: the loop ends with it, and Gaia keeps the archer
    let mut masters = hands_masters();
    masters[1] = Arc::new(Masters::new([]));
    let mut game = world(masters, false, 0);
    game.tick(Millis::new(100));
    assert_eq!(
        game.spatial
            .capture_of(game.players[0].objects[ListIndex::new(0)].id),
        CaptureKind::BySight
    );
    assert_eq!(
        game.spatial
            .get(object_id(21712))
            .map(|placed| placed.player),
        Some(PlayerId::new(0))
    );
    // a computer player 1 in a scenario passes it to player 2
    let mut game = world(hands_masters(), true, 3);
    game.tick(Millis::new(100));
    assert_eq!(game.players[2].objects.len(), 1);
    assert_eq!(
        game.spatial
            .get(object_id(21712))
            .map(|placed| placed.player),
        Some(PlayerId::new(2))
    );
    // unless it allows taking Gaia's units
    let mut world = world(hands_masters(), true, 3);
    world.allow_gaia_capture = vec![false, true, false];
    world.tick(Millis::new(100));
    assert_eq!(
        world
            .spatial
            .get(object_id(21712))
            .map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
}

/// A world for the teardowns: player 1 holds the sheep 20 (master 705), the bird 30 (96),
/// the villager 40 (83) and town centres 9149 and 9150 (109, one worker each); player 2 (an
/// ally) holds the foundation 10028 (70) with one builder.
fn teardown_world() -> World {
    let walker = walker();
    let unit = |id, master| Unit {
        id,
        place: Static::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let tc = |id| Unit {
        id,
        place: Static::at(Position::new(20.0, 20.0, 0.0)),
        workers: WorkerCount::of_data(1),
        life: Lifecycle {
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let foundation = Object::with_building(
        Unit {
            id: object_id(10028),
            place: Static::at(Position::new(30.0, 30.0, 0.0)),
            workers: WorkerCount::of_data(1),
            life: Lifecycle {
                state: ObjectState::Foundation,
                master_id: ids::HOUSE,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::ZERO,
                desolid: false,
                builders: BuilderCount::of_data(1),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    let split = World::split_layers(vec![foundation]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    unit(object_id(20), master_id(705)),
                    unit(object_id(30), master_id(96)),
                    unit(object_id(40), VILLAGER_MALE),
                    tc(object_id(9149)),
                    tc(object_id(9150)),
                ]
                .into(),
                relations: ByPlayer::new(vec![
                    Relation::Ally,
                    Relation::Neutral,
                    Relation::from(2),
                ]),
                ..Player::default()
            },
            Player {
                objects: split.0.into(),
                relations: ByPlayer::new(vec![
                    Relation::Ally,
                    Relation::from(2),
                    Relation::Neutral,
                ]),
                ..Player::default()
            },
        ],
    )
    .with_parts(split.1)
    .with_motions([20, 30, 40].map(|id| (object_id(id), Motion::Moving(walker.clone()))));
    // no sprites: a MoveTo the masters make shows none
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        standing_sprite: None,
        moving: Some(MovingPart {
            move_sprite: None,
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    let table = Masters::new([
        master(master_id(705), MasterKind::COMBAT, class::LIVESTOCK),
        master(master_id(96), MasterKind::COMBAT, class::CLASS_11),
        master(VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER),
        master(ids::TOWN_CENTRE, MasterKind::BUILDING, class::TOWER),
        master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING),
    ]);
    world.model.masters = vec![
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table),
    ];
    world
}

/// The action list's clearing (`FUN_140a98620`): the Artifact's teardown (`FUN_140aab0d0`)
/// lets its town centre go and its `MoveTo`'s stops the sheep; the Fly's (`FUN_140a84750`)
/// gives the bird its top speed as its animation speed, then the base teardown's `MoveTo`
/// stops it (`+0x7b8` 0 at once; without a `MoveTo` the top speed stays); the
/// Enter's lets its building go; a counted builder's Build leaves another player's
/// foundation's builders (`World::leave_builders`) and lets it go; a wait for a foundation
/// goes.
#[test]
fn clearing_the_actions_tears_the_artifact_fly_enter_and_build_down() {
    let mut world = teardown_world();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(Some(Artifact {
            target: Some(object_id(9149)),
            goal: Some(Position::new(20.0, 20.0, 0.0)),
            sub: Some(ArtifactSub::Walk {
                move_to: MoveToAction {
                    point: Position::ORIGIN,
                    target: Some(object_id(9149)),
                    range: Distance::new(1.0),
                    ..MoveToAction::sub_in(MoveTo::Moving)
                },
            }),
            ..Artifact::new(PlayerId::new(1), None)
        }));
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let sheep = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(sheep.id).list.artifact().copied(),
        None
    );
    assert_eq!(
        world.parts.mover_of(sheep.id).unwrap().animated.speed,
        Speed::new(0.0)
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(3)].workers,
        WorkerCount::of_data(0)
    );
    let bird = &mut world.players[1].objects[ListIndex::new(1)];
    world
        .parts
        .actions
        .of_mut(bird.id)
        .unwrap()
        .list
        .set_fly(Some(Fly {
            state: FlyState::Flying {
                move_to: Some(MoveToAction {
                    state: MoveTo::Moving,
                    voice: Voice::Loud,
                    ..MoveToAction::sub_to_point(
                        Position::new(12.0, 12.0, 0.0),
                        Distance::ZERO,
                        None,
                    )
                }),
            },
            timer: Seconds::new(1.0),
            home: Point::new(10.0, 10.0),
            task: Task::default(),
        }));
    world.parts.mover_mut(bird.id).unwrap().stats.speed = Speed::new(1.3);
    let flying = world.parts.actions_of(bird.id).list.fly().copied();
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(1))
        .unwrap();
    let bird = &mut world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (
            world.parts.actions_of(bird.id).list.fly().copied(),
            world.parts.anim_speed_of(bird.id)
        ),
        (None, Some(Speed::ZERO))
    );
    world
        .parts
        .actions
        .of_mut(bird.id)
        .unwrap()
        .list
        .set_fly(flying.map(|fly| Fly {
            state: FlyState::Flying { move_to: None },
            ..fly
        }));
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(1))
        .unwrap();
    let bird = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (
            world.parts.actions_of(bird.id).list.fly().copied(),
            world.parts.anim_speed_of(bird.id)
        ),
        (None, Some(Speed::new(1.3)))
    );
    let villager = &mut world.players[1].objects[ListIndex::new(2)];
    world
        .parts
        .actions
        .of_mut(villager.id)
        .unwrap()
        .list
        .set_enter(Some(EnterAction {
            target: Some(ContainerId::new(object_id(9150))),
            state: EnterState::Wait,
            point: Position::new(20.0, 20.0, 0.0),
            range: Distance::ZERO,
            timer: Seconds::ZERO,
            retry: true,
            task_class: Some(class_id(0)),
            moving: None,
        }));
    // a walking villager: an Enter without a MoveTo does not stop it
    world.parts.set_anim_speed(villager.id, Speed::new(1.0));
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let villager = &mut world.players[1].objects[ListIndex::new(2)];
    assert_eq!(
        world.parts.actions_of(villager.id).list.enter().copied(),
        None
    );
    assert_ne!(
        world.parts.mover_of(villager.id).unwrap().animated.speed,
        Speed::ZERO
    );
    world
        .parts
        .actions
        .of_mut(villager.id)
        .unwrap()
        .list
        .set_wait_solidify(Some(WaitSolidify::new(
            Task::default(),
            FoundationId::new(object_id(10028)),
        )));
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let villager = &world.players[1].objects[ListIndex::new(2)];
    assert_eq!(
        world
            .parts
            .actions_of(villager.id)
            .list
            .wait_solidify()
            .copied(),
        None
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(4)].workers,
        WorkerCount::of_data(0)
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(2)].id)
        .unwrap()
        .list
        .set_build(Some(BuildAction {
            target: Some(FoundationId::new(object_id(10028))),
            state: BuildState::Idle,
            point: Position::new(-1.0, -1.0, -1.0),
            task: BuildTask {
                range: Distance::ZERO,
                work_value: WorkValue::new(0.0),
                work_rate: WorkRate::new(1.0),
                resource: None,
                id: TaskId::of_task(-1),
                moving: None,
            },
            added: true,
            timer: Seconds::ZERO,
            site: None,
        }));
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let foundation = &world.players[2].objects[ListIndex::new(0)];
    assert_eq!(foundation.workers, WorkerCount::of_data(0));
    assert_eq!(
        world
            .parts
            .building_of(foundation.id)
            .construction
            .map(|construction| construction.builders.get()),
        Some(0)
    );
}

/// `FUN_1411e63f0` then `FUN_1411e6580` at a built building's death: each storage entry of mode
/// 4 with an attribute and an amount above 0 leaves its owner (negated), then the population
/// the object holds itself (attribute 4: it holds nothing after); each entry of mode 16 (a flag
/// bit 0x10) leaves its own store when the store holds its attribute.
#[test]
fn built_building_storage_leaves_its_owner_and_its_store() {
    let master = Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(5.0), 4),
            (None, Amount::new(5.0), 4),
            (Some(attribute::RELICS_HELD), Amount::ZERO, 4),
            (Some(attribute::POPULATION), Amount::new(2.0), 16),
            (Some(AttributeId::named(12)), Amount::new(3.0), 0x30),
            (Some(AttributeId::named(13)), Amount::new(1.0), 16),
        ],
        ..Master::default()
    };
    let mut unit = Unit {
        id: object_id(1),
        ..Unit::default()
    };
    let mut combat = Combat {
        store: vec![
            (attribute::POPULATION, Amount::new(6.0)),
            (AttributeId::named(12), Amount::new(4.0)),
        ],
        ..Default::default()
    };
    unit.life.attribute_kind = Some(attribute::HEADROOM);
    unit.life.amount = Amount::new(3.0);
    let adds = storage_leaves((&mut unit, &mut combat), &master, &roles(), slots());
    assert_eq!(
        adds,
        test_ids_adds([(attribute::HEADROOM, -5.0), (attribute::HEADROOM, -3.0)])
    );
    assert_eq!(unit.life.held(), carrying(None, 0.0));
    assert_eq!(
        combat.store,
        test_ids_adds([(attribute::POPULATION, 4.0), (AttributeId::named(12), 1.0)])
    );
    // holding no population: the master's entries alone
    let adds = storage_leaves((&mut unit, &mut combat), &master, &roles(), slots());
    assert_eq!(adds, test_ids_adds([(attribute::HEADROOM, -5.0)]));
    assert_eq!(
        combat.store,
        test_ids_adds([(attribute::POPULATION, 2.0), (AttributeId::named(12), -2.0)])
    );
}

/// A world for the deaths: Gaia holds the tree 50 (master 349, type 15, one worker) and the boar
/// 60 (master 48, type 70, one worker, a unit AI that counts the hunter 41 among its attackers);
/// player 1 holds the villagers 40 (gathering the tree), 41 (hunting the boar) and 42 (a counted
/// builder of its foundation 10028, one builder, one worker), each with hit points 0.
fn deaths_world() -> World {
    let dead = |id| Unit {
        id,
        place: Static::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            hp: HitPoints::ZERO,
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let chop = hunting(50).task;
    let tree = Resource {
        id: object_id(50),
        position: Position::new(12.5, 10.5, 0.0),
        class: class::TREE,
        master: master_id(349),
        player: PlayerId::new(0),
        workers: WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::new(1.0),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let gather = GatherAction::new(chop, &tree, FarmMode::Fixed, rate, &roles()).unwrap();
    let villagers = vec![
        dead(object_id(40)),
        dead(object_id(41)),
        dead(object_id(42)),
    ];
    // their action layers, the world's table's
    let mut layers = [(); 3].map(|()| Action::default());
    layers[0].list.set_gather(Some(GatherAction {
        state: GatherState::Work,
        ..gather
    }));
    layers[1].list.set_hunt(Some(hunting(60)));
    layers[2].list.set_build(Some(BuildAction {
        target: Some(FoundationId::new(object_id(10028))),
        state: BuildState::Place,
        point: Position::new(-1.0, -1.0, -1.0),
        task: BuildTask {
            range: Distance::ZERO,
            work_value: WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            moving: None,
        },
        added: true,
        timer: Seconds::ZERO,
        site: None,
    }));
    let gaia = vec![
        Unit {
            id: object_id(50),
            place: Static::at(Position::new(12.5, 10.5, 0.0)),
            workers: WorkerCount::of_data(1),
            life: Lifecycle {
                master_id: master_id(349),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Unit {
            id: object_id(60),
            place: Static::at(Position::new(14.5, 10.5, 0.0)),
            workers: WorkerCount::of_data(1),
            life: Lifecycle {
                class: Class::Animated,
                master_id: master_id(48),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
    ];
    let foundation = Object::with_building(
        Unit {
            id: object_id(10028),
            place: Static::at(Position::new(30.0, 30.0, 0.0)),
            workers: WorkerCount::of_data(1),
            life: Lifecycle {
                state: ObjectState::Foundation,
                master_id: ids::HOUSE,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::ZERO,
                desolid: false,
                builders: BuilderCount::of_data(1),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    // player 1's turn: the boar's unit AI keeps its attackers
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: gaia.into(),
                ..Player::default()
            },
            Player {
                objects: villagers.into_iter().chain([foundation.unit]).collect(),
                relations: relation_row([0, 1]),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    )
    .with_buildings([(object_id(10028), foundation.parts.building)])
    .with_actions([40, 41, 42].map(object_id).into_iter().zip(layers));
    let boar = UnitAi {
        attackers: Attackers::of_saved(&[41]),
        ..building()
    };
    world.parts.set_unit_ai(object_id(60), Some(boar));
    // no dying graphic (-1): slot 0x7a0 gives the master's, which these masters lack
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        death_sprite: None,
        ..Master::default()
    };
    let table = Masters::new([
        master(master_id(349), MasterKind::TREE, class::TREE),
        master(master_id(48), MasterKind::COMBAT, class::PREY),
        master(VILLAGER_MALE, MasterKind::COMBAT, class::VILLAGER),
        master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING),
    ]);
    world.model.masters = vec![Arc::new(table.clone()), Arc::new(table)];
    world
}

/// `RGE_Action_Object::update` (`0x14106ac90`) deletes a dead unit's action list in the update
/// of its death (`FUN_140a98620`), before any action's update: the Gather lets its tree go (no
/// worker left, it goes to sleep), the Hunt lets its boar go (notify 699 from the hunter, its
/// class 4), the Build takes its builder out of the foundation's builders and lets it go.
#[test]
fn dead_unit_actions_go_and_let_their_targets_go_in_the_update_of_its_death() {
    let mut world = deaths_world();
    world.tick(Millis::new(100));
    assert_eq!(
        (
            world.players[0].unsupported.clone(),
            world.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let villagers = &world.players[1].objects;
    for unit in &villagers.as_slice()[..3] {
        assert!(ObjectState::Dead <= unit.life.state, "{} died", unit.id);
        assert_eq!(
            world.parts.actions_of(unit.id).top_action(),
            None,
            "{} lost its actions",
            unit.id
        );
    }
    let tree = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (tree.workers, tree.life.goto_sleep),
        (WorkerCount::of_data(0), true)
    );
    let boar = &world.players[0].objects[ListIndex::new(1)];
    assert_eq!(boar.workers, WorkerCount::of_data(0));
    let notify = world
        .parts
        .unit_ai
        .get(boar.id)
        .unwrap()
        .notifies
        .last()
        .copied();
    assert_eq!(
        notify.map(|notify| (notify.caller, notify.kind, notify.params[0])),
        Some((Some(object_id(41)), NotifyKind::NO_LONGER_ON_ME, 4))
    );
    let foundation = &villagers[ListIndex::new(3)];
    assert_eq!(foundation.workers, WorkerCount::of_data(0));
    assert_eq!(
        world
            .parts
            .building_of(foundation.id)
            .construction
            .map(|construction| construction.builders.get()),
        Some(0)
    );
}

/// `FUN_140aaddb0`: a Build counts its builder in a building annex's main building (`+0xbb8`),
/// in any other object itself, and in none once the world no longer holds its target; a dead
/// builder of the annex leaves the main building's builders.
#[test]
fn build_on_an_annex_counts_its_builder_in_the_main_building() {
    let mut world = deaths_world();
    let mut main = world.object_of(&world.players[1].objects[ListIndex::new(3)]);
    main.unit.id = object_id(10030);
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(3)].id)
        .unwrap()
        .annex_of = Some(MainBuildingId::new(object_id(10030)));
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(3)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(0);
    let _ = world.push_live(PlayerId::new(1), main).unwrap();
    assert_eq!(
        world.counted_building(object_id(10028)),
        Ok(Some(object_id(10030)))
    );
    assert_eq!(
        world.counted_building(object_id(50)),
        Ok(Some(object_id(50)))
    );
    assert_eq!(world.counted_building(object_id(999)), Ok(None));
    world.tick(Millis::new(100));
    assert_eq!(
        (
            world.players[0].unsupported.clone(),
            world.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let builders = |id| {
        world.players[1]
            .objects
            .iter()
            .find(|unit| unit.id == id)
            .and_then(|unit| world.parts.building_of(unit.id).construction)
            .map(|construction| construction.builders.get())
    };
    assert_eq!(
        (builders(object_id(10028)), builders(object_id(10030))),
        (Some(0), Some(0))
    );
}

/// A `MoveTo` of its own to an object takes the object (`FUN_140aa0df0` with quiet 0, then
/// `FUN_140a7a230`: slot 0x178 wakes the sleeping tree 50) and lets it go at every end: another
/// move to an object takes the new one before the clearing's teardown (`0x140aa1f30`, then the
/// base teardown `FUN_140a73fc0`) lets the old one go (no worker left on the tree: it is flagged
/// to sleep), and a clearing of the list lets the town centre go.
#[test]
fn move_to_an_object_takes_it_and_lets_it_go_when_it_is_replaced_or_cleared() {
    let mut world = teardown_world();
    let tree = Unit {
        id: object_id(50),
        place: Static::at(Position::new(12.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: master_id(349),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let _ = world.push_asleep(PlayerId::new(0), tree).unwrap();
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([Master {
            id: master_id(349),
            kind: MasterKind::TREE,
            class: class::TREE,
            ..Master::default()
        }])));
    }
    let placed = |x: f32, class, master| Placed {
        position: Position::new(x, 10.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player: PlayerId::new(0),
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world
        .spatial
        .insert(object_id(50), placed(12.5, class::TREE, master_id(349)));
    world.spatial.insert(
        object_id(9149),
        placed(20.0, class::TOWER, ids::TOWN_CENTRE),
    );
    let _ = world
        .move_to_object(
            PlayerId::new(1),
            ListIndex::new(2),
            object_id(50),
            Position::new(12.5, 10.5, 0.0),
        )
        .unwrap();
    assert_eq!(world.players[0].sleeping.len(), 0);
    let tree = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!((tree.id.get(), tree.workers), (50, WorkerCount::of_data(1)));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(2)].id)
            .list
            .move_to()
            .map(|move_to_action| ObjectId::field(move_to_action.target)),
        Some(50)
    );
    let _ = world
        .move_to_object(
            PlayerId::new(1),
            ListIndex::new(2),
            object_id(9149),
            Position::new(20.0, 10.5, 0.0),
        )
        .unwrap();
    let tree = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (tree.workers, tree.life.goto_sleep),
        (WorkerCount::of_data(0), true)
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(3)].workers,
        WorkerCount::of_data(2)
    );
    world
        .clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    assert_eq!(
        world.players[1].objects[ListIndex::new(3)].workers,
        WorkerCount::of_data(1)
    );
}

/// A `MoveTo` of its own that ends (its update in state 1 returns 1, the action list removes it,
/// `FUN_140a98e20`) lets its target go (the base teardown, slot 0x180); one that dies with the
/// unit does too (`FUN_140a98620`).
#[test]
fn move_to_an_object_lets_it_go_when_it_ends_or_its_unit_dies() {
    let mut world = deaths_world();
    world.players[0].objects[ListIndex::new(0)].workers = WorkerCount::of_data(2);
    let mover = walker();
    let walker = |id| {
        Object::with_actions(
            Unit {
                id,
                place: Static::at(Position::new(11.5, 10.5, 0.0)),
                pasture: None,
                my_pasture: None,
                life: Lifecycle {
                    class: Class::Animated,
                    master_id: VILLAGER_MALE,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Action {
                list: ActionList::default().with_move_to(Some(MoveToAction {
                    state: MoveTo::Arrived,
                    point: Position::ORIGIN,
                    target: Some(object_id(50)),
                    range: Distance::ZERO,
                    sprite: None,
                    voice: Voice::Loud,
                    on_target_moved: TargetMoved::Follows,
                    wait: MoveWait::default(),
                })),
                ..Default::default()
            },
        )
    };
    let mut dead = walker(object_id(43));
    dead.unit.life.hp = HitPoints::ZERO;
    world.set_live_for_test(
        PlayerId::new(1),
        [walker(object_id(44)), dead].map(|object| Object::moving(object, mover.clone())),
    );
    world.spatial.insert(
        object_id(50),
        Placed {
            position: Position::new(12.5, 10.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::TREE,
            player: PlayerId::new(0),
            master: master_id(349),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world.tick(Millis::new(100));
    assert_eq!(
        (
            world.players[0].unsupported.clone(),
            world.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .move_to()
            .copied(),
        None
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(1)].id)
            .list
            .move_to()
            .copied(),
        None
    );
    let tree = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (tree.workers, tree.life.goto_sleep),
        (WorkerCount::of_data(0), true)
    );
}

/// A `MoveTo` of its own checks its target first (`0x140aa2a70`,
/// [`MoveToAction::check_held`]): a dying one (state 7) is let go and released (slot 0x180: the
/// tree counts a worker less), and the list keeps the `MoveTo` (code 4), now to its point; one
/// the world no longer holds is cleared, with nothing released.
#[test]
fn move_to_of_its_own_lets_dying_target_go_and_forgets_lost_one() {
    let run = |target: i32, tree_state: ObjectState| {
        let mut world = deaths_world();
        world.players[0].objects[ListIndex::new(0)].workers = WorkerCount::of_data(2);
        world.players[0].objects[ListIndex::new(0)].life.state = tree_state;
        let mover = walker();
        let walker = Object::with_actions(
            Unit {
                id: object_id(44),
                place: Static::at(Position::new(11.5, 10.5, 0.0)),
                pasture: None,
                my_pasture: None,
                life: Lifecycle {
                    class: Class::Animated,
                    master_id: VILLAGER_MALE,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Action {
                list: ActionList::default().with_move_to(Some(MoveToAction {
                    state: MoveTo::Waiting,
                    point: Position::new(12.5, 10.5, 0.0),
                    target: ObjectId::of_field(target),
                    range: Distance::ZERO,
                    sprite: None,
                    voice: Voice::Loud,
                    on_target_moved: TargetMoved::Follows,
                    wait: MoveWait {
                        state: MoveWaitState::Waiting,
                        timer: Seconds::ZERO,
                        delay: Seconds::new(60.0),
                    },
                })),
                ..Default::default()
            },
        );
        world.set_live_for_test(PlayerId::new(1), [Object::moving(walker, mover)]);
        world.tick(Millis::new(100));
        assert_eq!(world.players[1].unsupported.clone(), None);
        let move_to = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .move_to()
            .copied();
        let workers = world.players[0].objects[ListIndex::new(0)].workers;
        (
            move_to.map(|move_to_action| {
                (move_to_action.state, ObjectId::field(move_to_action.target))
            }),
            workers,
        )
    };
    assert_eq!(
        run(50, ObjectState::Removed),
        (Some((MoveTo::Waiting, -1)), WorkerCount::of_data(1))
    );
    assert_eq!(
        run(51, ObjectState::Alive),
        (Some((MoveTo::Waiting, -1)), WorkerCount::of_data(2))
    );
    assert_eq!(
        run(50, ObjectState::Alive),
        (Some((MoveTo::Waiting, 50)), WorkerCount::of_data(2))
    );
}

/// The hunt's `set_state(8)` (`0x140ac60e0` case 8): without a second target its search
/// (`FUN_1411671e0`; without planners by distance) finds the town centre, which becomes the
/// second target (`FUN_140a7a190`): the caller takes it (slot 0x178). With the drop site held
/// already it walks there and takes nothing new.
#[test]
fn hunter_drop_off_takes_the_drop_site_it_found() {
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![Player::default(), Player::default()],
    );
    placeable(&mut game);
    let mut walker = walker();
    walker.stats.restriction = RestrictionId::named(0);
    let mut hunter = Unit {
        id: object_id(40),
        place: Static::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            attribute_kind: Some(attribute::FOOD),
            amount: Amount::new(10.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut actions = Action {
        list: ActionList::default().with_hunt(Some(hunting(-1))),
        ..Default::default()
    };
    let tc = Unit {
        id: object_id(9149),
        place: Static::at(Position::new(20.0, 20.0, 0.0)),
        life: Lifecycle {
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let masters = Masters::new([
        Master {
            id: VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            standing_sprite: None,
            action: Some(ActionPart {
                drop_sites: vec![ids::TOWN_CENTRE],
                ..ActionPart::default()
            }),
            moving: Some(MovingPart {
                // no sprites: the sprites' table holds none
                move_sprite: None,
                ..MovingPart::default()
            }),
            ..Master::default()
        },
        Master {
            id: ids::TOWN_CENTRE,
            kind: MasterKind::BUILDING,
            class: class::TOWER,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        },
    ]);
    let placed = |x: f32, radius: f32, class, master| Placed {
        position: Position::new(x, x, 0.0),
        radius: Footprint::new(radius, radius),
        class,
        player: PlayerId::new(1),
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    game.spatial.insert(
        object_id(40),
        placed(10.5, 0.2, class::VILLAGER, VILLAGER_MALE),
    );
    game.spatial.insert(
        object_id(9149),
        placed(20.0, 2.0, class::TOWER, ids::TOWN_CENTRE),
    );
    let others = ObjectList::from(vec![tc]);
    let drop_off = |(hunter, actions, walker): (&mut Unit, &mut Action, &mut Mover),
                    played: &mut World| {
        let world = DropWorld {
            top_speed: TopSpeed::Own,
            masters: OwnerMasters::new(no_own_masters(), Some(&masters)),
            spatial: &mut played.spatial,
            zones: &mut played.zones,
            pathing: &mut played.pathing,
            object: played.streams.get_mut(Stream::Object),
            world_time: WorldTime::ZERO,
            player_kind: PlayerKind::Human,
            groups: &played.groups,
        };
        let mut hunt = *actions.list.hunt().unwrap();
        hunter.drop_off_in(
            (&mut hunt, actions),
            Some(walker),
            world,
            OthersRef::alone(PlayerId::new(1), &others),
            &played.model.graphics,
            objects_ref::ObjectsRef::empty(),
        )
    };
    assert_eq!(
        drop_off((&mut hunter, &mut actions, &mut walker), &mut game),
        Ok(Some(object_id(9149)))
    );
    let hunt = actions.list.hunt().copied().unwrap();
    assert_eq!(
        (hunt.state.bare(), hunt.carry_to),
        (
            HuntState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(9149)))
        )
    );
    assert_eq!(
        drop_off((&mut hunter, &mut actions, &mut walker), &mut game),
        Ok(None)
    );
}

/// The Hunt's update (`0x140ac7a00`) first checks its second target: a drop site past state 6
/// is let go (`FUN_140a7a190` with none: slot 0x180) and dropped, one the world's table no longer
/// holds only dropped; the teardown (`FUN_140a73fc0`) of a dead hunter's Hunt and of a failed
/// one (state 13, removed by `FUN_140a98e20`) lets its drop site go too.
#[test]
fn hunt_lets_its_drop_site_go_when_it_dies_or_the_hunter_does() {
    let mut game = deaths_world();
    let site = |state| Unit {
        id: object_id(9149),
        place: Static::at(Position::new(20.0, 20.0, 0.0)),
        workers: WorkerCount::of_data(1),
        life: Lifecycle {
            state,
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let _ = game
        .push_live(PlayerId::new(1), site(ObjectState::Alive))
        .unwrap();
    // the dead hunter 41's Hunt walks its load to the town centre
    game.parts
        .actions
        .of_mut(game.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .carry_to = Some(DropSiteId::new(object_id(9149)));
    game.tick(Millis::new(100));
    assert_eq!(
        (
            game.players[0].unsupported.clone(),
            game.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let workers = |world: &World| {
        world.players[1]
            .objects
            .iter()
            .find(|unit| unit.id == object_id(9149))
            .unwrap()
            .workers
    };
    assert_eq!(workers(&game), WorkerCount::of_data(0));
    // a live hunter waiting to search (state 3) whose drop site is past state 6 (the world's
    // table holds it still); one whose drop site the table no longer holds only drops it
    let mut world = deaths_world();
    let _ = world
        .push_live(PlayerId::new(1), site(ObjectState::Removed))
        .unwrap();
    world.spatial.insert(
        object_id(9149),
        Placed {
            position: Position::new(20.0, 20.0, 0.0),
            radius: Footprint::new(2.0, 2.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let hunter = &mut world.players[1].objects[ListIndex::new(1)];
    hunter.life.hp = HitPoints::new(1.0);
    world
        .parts
        .actions
        .of_mut(hunter.id)
        .unwrap()
        .list
        .set_hunt(Some(HuntAction {
            state: HuntState::Search,
            target: None,
            carry_to: Some(DropSiteId::new(object_id(9149))),
            ..hunting(-1)
        }));
    let mut gone = world.clone();
    gone.spatial = Spatial::default();
    world.tick(Millis::new(100));
    assert_eq!(
        (
            world.players[0].unsupported.clone(),
            world.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(workers(&world), WorkerCount::of_data(0));
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(1)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.carry_to),
        Some(None)
    );
    gone.tick(Millis::new(100));
    assert_eq!(workers(&gone), WorkerCount::of_data(1));
    assert_eq!(
        gone.parts
            .actions_of(gone.players[1].objects[ListIndex::new(1)].id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.carry_to),
        Some(None)
    );
    // a failed hunt (state 13) is removed: its teardown lets the drop site go too
    let mut failed = deaths_world();
    let _ = failed
        .push_live(PlayerId::new(1), site(ObjectState::Alive))
        .unwrap();
    failed.spatial = world.spatial.clone();
    let hunter = &mut failed.players[1].objects[ListIndex::new(1)];
    hunter.life.hp = HitPoints::new(1.0);
    failed
        .parts
        .actions
        .of_mut(hunter.id)
        .unwrap()
        .list
        .set_hunt(Some(HuntAction {
            state: HuntState::Failed,
            target: None,
            carry_to: Some(DropSiteId::new(object_id(9149))),
            ..hunting(-1)
        }));
    failed.tick(Millis::new(100));
    assert_eq!(failed.players[1].unsupported, None);
    assert_eq!(workers(&failed), WorkerCount::of_data(0));
    assert_eq!(
        failed
            .parts
            .actions_of(failed.players[1].objects[ListIndex::new(1)].id)
            .list
            .hunt()
            .copied(),
        None
    );
}

/// A Gather's and a Hunt's second target at the start of their updates (`0x140a8e1d0`,
/// `0x140ac7a00`): a drop site past state 6 is let go and goes, one the world's table lacks only
/// goes, one in state 2 (or 6) stays.
#[test]
fn second_target_past_state_6_is_let_go_and_one_gone_dropped() {
    use crate::objects::unit::second_target_checked;
    assert_eq!(
        second_target_checked(
            Some(DropSiteId::new(object_id(9149))),
            Presence::Held,
            Some(ObjectState::Removed)
        ),
        (None, Some(DropSiteId::new(object_id(9149))))
    );
    assert_eq!(
        second_target_checked(
            Some(DropSiteId::new(object_id(9149))),
            Presence::Held,
            Some(ObjectState::Undead)
        ),
        (Some(DropSiteId::new(object_id(9149))), None)
    );
    assert_eq!(
        second_target_checked(
            Some(DropSiteId::new(object_id(9149))),
            Presence::Held,
            Some(ObjectState::Alive)
        ),
        (Some(DropSiteId::new(object_id(9149))), None)
    );
    assert_eq!(
        second_target_checked(Some(DropSiteId::new(object_id(9149))), Presence::Gone, None),
        (None, None)
    );
    assert_eq!(
        second_target_checked(None, Presence::Gone, None),
        (None, None)
    );
}

/// A foundation's attacking units (`+0xce0`): slot 0x178 of a building (`FUN_1411dba70`) below
/// state 2 whose master has a stack unit (`+0x986`) lists each taker of a player its owner is
/// not allied with, once per take; slot 0x180 (`FUN_1411d6020`) drops the releaser's first entry,
/// the others keeping their order. An ally's take, a built building's and one of a master
/// without a stack unit list nothing. Both still count the workers.
#[test]
fn foundation_lists_its_unallied_takers_and_drops_them_when_they_let_it_go() {
    let unit = |id, master| Unit {
        id,
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let foundation = |id, master| Unit {
        life: Lifecycle {
            state: ObjectState::Foundation,
            ..unit(id, master).life
        },
        ..unit(id, master)
    };
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    foundation(object_id(10028), ids::TOWN_CENTRE_621),
                    foundation(object_id(10029), ids::HOUSE),
                ]
                .into(),
                relations: ByPlayer::new(vec![
                    Relation::Ally,
                    Relation::Ally,
                    Relation::from(2),
                    Relation::Relation4,
                ]),
                ..Player::default()
            },
            Player {
                objects: vec![
                    unit(object_id(40), VILLAGER_MALE),
                    unit(object_id(41), VILLAGER_MALE),
                ]
                .into(),
                relations: ByPlayer::new(vec![
                    Relation::Ally,
                    Relation::from(2),
                    Relation::Ally,
                    Relation::from(2),
                ]),
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(50), VILLAGER_MALE)].into(),
                relations: ByPlayer::new(vec![
                    Relation::Ally,
                    Relation::Relation4,
                    Relation::from(2),
                    Relation::Ally,
                ]),
                ..Player::default()
            },
        ],
    );
    let building = |id: i16, stack_unit: i16| Master {
        id: master_id(id),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        building: Some(BuildingMaster {
            stack_unit: opt_master(stack_unit),
            ..BuildingMaster::default()
        }),
        ..Master::default()
    };
    let table = Masters::new([
        building(621, 109),
        building(70, -1),
        Master {
            id: VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            ..Master::default()
        },
    ]);
    game.model.masters = vec![
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table),
    ];
    let by = |world: &World, player_id, list_index| world.releaser(player_id, list_index).unwrap();
    let (enemy, other, ally) = (
        by(&game, PlayerId::new(2), ListIndex::new(0)),
        by(&game, PlayerId::new(2), ListIndex::new(1)),
        by(&game, PlayerId::new(3), ListIndex::new(0)),
    );
    for taker in [enemy, other, enemy, ally] {
        game.take(object_id(10028), taker).unwrap();
        game.take(object_id(10029), taker).unwrap();
    }
    let site = |world: &World| world.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        game.parts
            .building_of(object_id(10028))
            .attacking_units
            .clone(),
        [object_id(40), object_id(41), object_id(40)]
    );
    assert_eq!(site(&game).workers, WorkerCount::of_data(4));
    assert_eq!(
        game.parts
            .building_of(game.players[1].objects[ListIndex::new(1)].id)
            .attacking_units,
        Vec::new()
    );
    game.release(object_id(10028), enemy).unwrap();
    assert_eq!(
        (
            game.parts
                .building_of(object_id(10028))
                .attacking_units
                .clone(),
            site(&game).workers
        ),
        (vec![object_id(41), object_id(40)], WorkerCount::of_data(3))
    );
    game.release(object_id(10028), ally).unwrap();
    assert_eq!(
        game.parts
            .building_of(object_id(10028))
            .attacking_units
            .clone(),
        [object_id(41), object_id(40)]
    );
    game.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    game.take(object_id(10028), other).unwrap();
    game.release(object_id(10028), enemy).unwrap();
    assert_eq!(
        game.parts
            .building_of(object_id(10028))
            .attacking_units
            .clone(),
        [object_id(41), object_id(40)]
    );
}

/// Slot 0x180 by class: the static object's (`FUN_141197d00`, types 10 and 15) flags an object
/// alive without workers left to sleep (not one of NaN hit points: `COMISS`, `JNC`); the animated
/// object's (`FUN_141071ea0`, types 20 to 40) only counts the worker less. A sleeping object
/// (`FUN_1415417a0` finds it) counts one less and stays asleep without the flag.
#[test]
fn only_static_object_let_go_by_its_last_worker_goes_to_sleep() {
    let by = Releaser {
        id: object_id(40),
        class: Some(class::VILLAGER),
        player: PlayerId::new(1),
    };
    let let_go = |kind, hp| {
        let mut unit = Unit {
            workers: WorkerCount::of_data(1),
            life: Lifecycle {
                hp,
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        released(&mut unit, (None, None), (kind, false), by).unwrap();
        (unit.workers, unit.life.goto_sleep)
    };
    assert_eq!(
        let_go(MasterKind::STATIC, HitPoints::new(1.0)),
        (WorkerCount::of_data(0), true)
    );
    assert_eq!(
        let_go(MasterKind::TREE, HitPoints::new(1.0)),
        (WorkerCount::of_data(0), true)
    );
    assert_eq!(
        let_go(MasterKind::ANIMATED, HitPoints::new(1.0)),
        (WorkerCount::of_data(0), false)
    );
    assert_eq!(
        let_go(MasterKind::DOPPELGANGER, HitPoints::new(1.0)),
        (WorkerCount::of_data(0), false)
    );
    assert_eq!(
        let_go(MasterKind::STATIC, HitPoints::new(f32::NAN)),
        (WorkerCount::of_data(0), false)
    );
    let mut world = deaths_world();
    let mut tree = world
        .remove_live(PlayerId::new(0), ListIndex::new(0))
        .unwrap();
    tree.unit.workers = WorkerCount::of_data(2);
    let _ = world.push_asleep(PlayerId::new(0), tree).unwrap();
    world.release(object_id(50), by).unwrap();
    let tree = &world.players[0].sleeping[ListIndex::new(0)];
    assert_eq!(
        (tree.workers, tree.life.goto_sleep),
        (WorkerCount::of_data(1), false)
    );
}

/// The Wonder (master 276) keeps its action list up to state 5 (`0x14106ac90`'s threshold 5,
/// any other object's 2).
#[test]
fn only_the_wonder_keeps_its_actions_past_state_2() {
    let life = |master_id, state| Lifecycle {
        state,
        master_id,
        ..Lifecycle::default()
    };
    assert!(!loses_actions(
        &life(VILLAGER_MALE, ObjectState::Alive),
        &roles()
    ));
    assert!(loses_actions(
        &life(VILLAGER_MALE, ObjectState::Dying),
        &roles()
    ));
    assert!(!loses_actions(
        &life(ids::WONDER, ObjectState::Dead),
        &roles()
    ));
    assert!(loses_actions(
        &life(ids::WONDER, ObjectState::Undead),
        &roles()
    ));
}

/// The Artifact's move to an object (slot 8 with an object, `FUN_140aaca30`): the old target
/// is let go, then the new one taken (slot 0x68 twice, `+0x60` 0).
#[test]
fn an_artifact_move_to_an_object_lets_the_old_one_go_and_takes_the_new() {
    let mut world = teardown_world();
    for (id, at) in [(9149, [20.0, 20.0]), (9150, [25.0, 20.0])] {
        world.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(at[0], at[1], 0.0),
                radius: Footprint::new(2.0, 2.0),
                class: class::TOWER,
                player: PlayerId::new(1),
                master: ids::TOWN_CENTRE,
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(Some(Artifact::new(PlayerId::new(1), None)));
    world
        .artifact_take_object(PlayerId::new(1), ListIndex::new(0), object_id(9149))
        .unwrap();
    assert_eq!(
        world.players[1].objects[ListIndex::new(3)].workers,
        WorkerCount::of_data(2)
    );
    world
        .artifact_take_object(PlayerId::new(1), ListIndex::new(0), object_id(9150))
        .unwrap();
    assert_eq!(
        [
            world.players[1].objects[ListIndex::new(3)].workers.get(),
            world.players[1].objects[ListIndex::new(4)].workers.get()
        ],
        [1, 2]
    );
    let artifact = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(
        (artifact.target, artifact.goal),
        (Some(object_id(9150)), Some(Position::new(25.0, 20.0, 0.0)))
    );
    assert_eq!(
        world
            .parts
            .anim_speed_of(world.players[1].objects[ListIndex::new(0)].id),
        Some(Speed::ZERO)
    );
}

/// A building's slot 0x70 (`FUN_1411cd860`), as Aftermath's trade workshops change hands: Gaia's
/// workshop 7 (master 1378, class 1, the artifact's unit AI) finds player 1's villager 500 and
/// asks to change hands; after the update its whole change runs. Built, it gives its storage
/// of flag 4 (5 of attribute 4) from Gaia to player 1, leaves Gaia's watched list for player
/// 1's, completes in player 1's tracker, loses its gather point and its actions, and comes to
/// the end of player 1's list with its record again.
#[test]
fn gaia_building_with_the_artifact_unit_ai_changes_hands_whole() {
    let master = |id, class, los| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        line_of_sight: los,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    let workshop = Master {
        kind: MasterKind::BUILDING,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(5.0), 4),
            (None, Amount::ZERO, 0),
            (None, Amount::ZERO, 0),
        ],
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class: ObstructionClass::UNIT,
        building: Some(BuildingMaster {
            stack_unit: None,
            foundation_terrain: None,
            tech: None,
            head_unit: None,
            transform_unit: None,
            ..BuildingMaster::default()
        }),
        creatable: Some(CreatablePart {
            build_time: WholeSeconds::of_data(10),
            ..master(master_id(1378), class::ARTIFACT, Distance::new(4.0))
                .creatable
                .unwrap_or_default()
        }),
        ..master(master_id(1378), class::ARTIFACT, Distance::new(4.0))
    };
    let table = Masters::new([
        workshop,
        master(VILLAGER_MALE, class::VILLAGER, Distance::new(4.0)),
    ]);
    let torch = Object::with_building(
        Unit {
            life: Lifecycle {
                master_id: master_id(1378),
                state: ObjectState::Alive,
                hp: HitPoints::new(20000.0),
                ..Lifecycle::default()
            },
            ..hands_unit(7, master_id(1378))
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::new(10.0),
                desolid: false,
                builders: BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(20000),
                build_time: WholeSeconds::of_data(10),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    let villager = Unit {
        place: Static::at(Position::new(11.5, 10.5, 0.0)),
        ..hands_unit(500, VILLAGER_MALE)
    };
    let split = World::split_layers(vec![torch]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: split.0.into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    )
    .with_parts(split.1)
    .with_ais([(
        object_id(7),
        UnitAi {
            ai_kind: UnitAiKind::Artifact,
            current_order: None,
            current_action: None,
            current_target: None,
            idle_timeout: Millis::new(5000),
            adjusted_idle_timeout: Millis::new(4000),
            retarget_timer: Millis::new(1000),
            ..UnitAi::default()
        },
    )]);
    world.model.masters = vec![
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table),
    ];
    world
        .set_part(
            object_id(7),
            Part::GatherPoint(Some(GatherPoint {
                state: GatherPointState::Point,
                point: Position::new(12.0, 12.0, 0.0),
                target: None,
                target_master: None,
            })),
        )
        .unwrap();
    world.spatial = hands_world(
        7,
        PlayerId::new(0),
        class::ARTIFACT,
        master_id(1378),
        Distance::new(4.0),
    );
    world.players[0].attributes[4] = Amount::new(5.0);
    world.players[0].watched.push(Watched {
        id: object_id(7),
        cell: Cell::Tile(10 * 40 + 10),
        bits: VisionBits::NONE,
    });
    world.spatial.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([4, 1, 4]),
        relations([4, 4, 1]),
    ]);
    world.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let visibility = world.spatial.sight.as_mut().unwrap();
    visibility.add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::COMBAT,
        },
    );
    let registry = visibility.registry.as_mut().unwrap();
    registry
        .entries
        .get_or_insert_with(object_id(500), Default::default)
        .unwrap()
        .mask = VisionBits::from_bits_retain(0b11);
    registry
        .update_unit(UnitUpdate {
            id: object_id(500),
            owner: PlayerId::new(1),
            position: Point::new(11.5, 10.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(0b11),
        })
        .unwrap();
    world.tick(Millis::new(100));
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(world.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = world.players[1]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(ids, [object_id(500), object_id(7)]);
    assert_eq!(
        world.spatial.get(object_id(7)).map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
    assert_eq!(
        [
            world.players[0].attributes[4],
            world.players[1].attributes[4]
        ],
        amounts([0.0, 5.0])
    );
    let watched = |player_id: PlayerId| {
        world.players[player_id.index()]
            .watched
            .iter()
            .any(|watched| watched.id == object_id(7))
    };
    assert_eq!(
        [watched(PlayerId::new(0)), watched(PlayerId::new(1))],
        [false, true]
    );
    let shop = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(world.parts.gather_point.get(shop.id), None);
    assert_eq!(
        world.parts.own_master.get(shop.id).map(|master| master.id),
        Some(master_id(1378))
    );
    assert!(
        world
            .spatial
            .get(object_id(7))
            .is_some_and(|placed| placed.obstruction.is_some())
    );
}

/// The artifact's unit AI in a tick, with Gaia's torch 7 (master 854, class 1, a combat
/// unit): on Gaia's turn its processMisc (`FUN_140c21d40`) finds player 1's villager 500,
/// the one player's object in its lists, and asks its unit to change hands (slot 0x70); the
/// torch leaves Gaia's list for the end of player 1's.
#[test]
fn gaia_torch_goes_to_the_one_player_near_it() {
    let master = |id, class, los| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        line_of_sight: los,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    let table = Masters::new([
        master(ids::ID_854, class::ARTIFACT, Distance::new(4.0)),
        master(VILLAGER_MALE, class::VILLAGER, Distance::new(4.0)),
    ]);
    let torch = hands_unit(7, ids::ID_854);
    let villager = Unit {
        place: Static::at(Position::new(11.5, 10.5, 0.0)),
        ..hands_unit(500, VILLAGER_MALE)
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![torch].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    )
    .with_ais([(
        object_id(7),
        UnitAi {
            ai_kind: UnitAiKind::Artifact,
            current_order: None,
            current_action: None,
            current_target: None,
            idle_timeout: Millis::new(5000),
            adjusted_idle_timeout: Millis::new(4000),
            retarget_timer: Millis::new(1000),
            ..UnitAi::default()
        },
    )]);
    world.model.masters = vec![
        Arc::new(table.clone()),
        Arc::new(table.clone()),
        Arc::new(table),
    ];
    world.spatial = hands_world(
        7,
        PlayerId::new(0),
        class::ARTIFACT,
        ids::ID_854,
        Distance::new(4.0),
    );
    world.spatial.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([4, 1, 4]),
        relations([4, 4, 1]),
    ]);
    world.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let visibility = world.spatial.sight.as_mut().unwrap();
    visibility.add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::COMBAT,
        },
    );
    let registry = visibility.registry.as_mut().unwrap();
    registry
        .entries
        .get_or_insert_with(object_id(500), Default::default)
        .unwrap()
        .mask = VisionBits::from_bits_retain(0b11);
    registry
        .update_unit(UnitUpdate {
            id: object_id(500),
            owner: PlayerId::new(1),
            position: Point::new(11.5, 10.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(0b11),
        })
        .unwrap();
    world.tick(Millis::new(100));
    assert_eq!(world.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = world.players[1]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(ids, [object_id(500), object_id(7)]);
    assert_eq!(
        world.spatial.get(object_id(7)).map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
    let torch = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        world.parts.own_master.get(torch.id).map(|master| master.id),
        Some(master_id(854))
    );
    let ai = world.parts.unit_ai.get(torch.id).unwrap();
    assert_eq!(
        ai.notifies,
        [Notify::own(
            object_id(7),
            NotifyKind::OWNER_CHANGED,
            [1, 0, 0]
        )]
    );
    assert_eq!(ai.idle_timer, Millis::ZERO);
}

/// The livestock capture in a tick (`FUN_140aab570` from the Artifact's check in the second
/// update): player 1's villager stands beside Gaia's sheep 9520, which stops (its Artifact's
/// slot 0x38: the standing sprite's speed) and goes to player 1 (slot 0x70: a master of its
/// own with player 1's line of sight), the end of player 1's list, with its food (storage
/// flag 1 to the new owner).
#[test]
fn gaia_sheep_player_stands_by_goes_to_it() {
    let mut walker = walker();
    walker.velocity = None;
    walker.stats.speed = Speed::new(0.8);
    let sheep = Unit {
        // the walker's own place, the origin
        place: Static::default(),
        ..hands_unit(9520, master_id(705))
    };
    let villager = Unit {
        place: Static::at(Position::new(11.5, 10.5, 0.0)),
        ..hands_unit(500, VILLAGER_MALE)
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![sheep].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    )
    .with_motions([(object_id(9520), Motion::Moving(walker))])
    .with_actions([(
        object_id(9520),
        Action {
            list: ActionList::default().with_artifact(Some(Artifact::new(PlayerId::GAIA, None))),
            ..Default::default()
        },
    )]);
    let mut masters = hands_masters();
    for masters in &mut masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([Master {
            id: VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            ..Master::default()
        }])));
    }
    world.model.masters = masters;
    world.spatial = hands_world(
        9520,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(705),
        Distance::new(2.0),
    );
    world.spatial.relations = ByPlayer::new(vec![
        relation_row([1, 4, 4]),
        relation_row([4, 1, 4]),
        relation_row([4, 4, 1]),
    ]);
    world.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world.spatial.sight.as_mut().unwrap().add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::COMBAT,
        },
    );
    world.tick(Millis::new(100));
    assert_eq!(world.players[0].objects.len(), 1);
    world.tick(Millis::new(100));
    assert_eq!(world.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = world.players[1]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(ids, [object_id(500), object_id(9520)]);
    let sheep = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        world
            .parts
            .own_master
            .get(sheep.id)
            .map(|master| master.line_of_sight),
        Some(Distance::new(3.0))
    );
    // its standing sprite has no factor: the top speed
    assert_eq!(
        world
            .parts
            .mover_of(sheep.id)
            .map(|mover| mover.stats.speed),
        Some(Speed::new(0.8))
    );
    assert_eq!(
        world
            .parts
            .actions_of(sheep.id)
            .list
            .artifact()
            .map(|artifact| artifact.gaia),
        Some(false)
    );
    assert_eq!(world.players[1].attributes[16], Amount::new(100.0));
    assert_eq!(
        world
            .spatial
            .get(object_id(9520))
            .map(|placed| placed.player),
        Some(PlayerId::new(1))
    );
}

/// `FUN_141302450`: the scenario modes' descriptors have `+0x9c` 0; a mode above 16 reads the
/// first descriptor's.
#[test]
fn the_standard_modes_are_all_but_the_scenario_ones() {
    let standard_mode = |mode| GameMode::of_data(mode).standard();
    let standard: Vec<i32> = (0..=17).filter(|&mode| standard_mode(mode)).collect();
    assert_eq!(standard, [0, 1, 2, 6, 7, 8, 9, 10, 11, 12, 13, 16, 17]);
    assert!(standard_mode(-1));
}

/// The attributes an ownership change moves with its object, by its master of its own: the
/// old owner loses storage flags 2, 16 and 32 and counts one archer less (40), the new one
/// counts one more and gains flags 1, 2 and 32.
#[test]
fn an_object_changing_hands_takes_its_storage_and_class_counts_along() {
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    );
    world.players[0].attributes[4] = Amount::new(5.0);
    world.players[0].attributes[40] = Amount::new(2.0);
    world
        .hand_over_attributes(PlayerId::GAIA, PlayerId::new(1), None)
        .unwrap();
    assert_eq!(world.players[0].attributes[4], Amount::new(5.0));
    let own = hands_masters()[0].get(master_id(4)).cloned();
    world
        .hand_over_attributes(PlayerId::GAIA, PlayerId::new(1), own.as_ref())
        .unwrap();
    assert_eq!(
        [
            world.players[0].attributes[4],
            world.players[0].attributes[40]
        ],
        amounts([4.0, 1.0])
    );
    assert_eq!(
        [
            world.players[1].attributes[4],
            world.players[1].attributes[40]
        ],
        amounts([1.0, 1.0])
    );
    assert_eq!(world.players[1].attributes[11], Amount::ZERO);
}

/// `FUN_141082e80` before the kill's counts: a killed villager (class 4) counts for its owner an
/// animal attacker (class 9 or 10) in attribute 227, and by the attacker's player type a human's
/// in 229, Gaia's in 226, a computer's in 228 (type 0 none), the attacker's class as the hit
/// saw it (an attacker the world does not hold then, out for its update or gone, counts too); a
/// computer player's killed object (its AI is told) is not modelled.
#[test]
fn killed_villager_counts_its_attacker_kind_and_player_kind() {
    let unit = |id, master_id| Unit {
        id,
        life: Lifecycle {
            master_id,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |kind, objects| Player {
        objects,
        kind,
        attributes: vec![Amount::ZERO; 500],
        ..Player::default()
    };
    let world = |attacker_kind| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            vec![
                player(
                    PlayerKind::Gaia,
                    vec![unit(object_id(50), master_id(126))].into(),
                ),
                player(
                    PlayerKind::Human,
                    vec![
                        unit(object_id(40), VILLAGER_MALE),
                        unit(object_id(41), VILLAGER_MALE),
                    ]
                    .into(),
                ),
                player(
                    attacker_kind,
                    vec![unit(object_id(60), master_id(4))].into(),
                ),
            ],
        );
        let masters = Masters::new([
            Master {
                id: VILLAGER_MALE,
                class: class::VILLAGER,
                ..Master::default()
            },
            Master {
                id: master_id(126),
                class: class::PREY,
                ..Master::default()
            },
            Master {
                id: master_id(4),
                class: class::CLASS_0,
                ..Master::default()
            },
        ]);
        world.model.masters = vec![masters; 3].into_iter().map(Arc::new).collect();
        world
    };
    let villager = Master {
        id: VILLAGER_MALE,
        class: class::VILLAGER,
        ..Master::default()
    };
    let counts = |world: &World| {
        [226, 227, 228, 229].map(|attribute| world.players[1].attributes[attribute])
    };
    // Gaia's wolf: an animal, Gaia's type
    let mut game = world(PlayerKind::Human);
    game.count_owner_loss(
        PlayerId::new(1),
        villager.class,
        class::PREY,
        PlayerId::new(0),
    )
    .unwrap();
    assert_eq!(counts(&game), amounts([1.0, 1.0, 0.0, 0.0]));
    // the archers of a human, a computer and the editor's phantom
    for (kind, seen) in [
        (PlayerKind::Human, [0.0, 0.0, 0.0, 1.0]),
        (PlayerKind::Computer, [0.0, 0.0, 1.0, 0.0]),
        (PlayerKind::Phantom, [0.0; 4]),
    ] {
        let mut kind_world = world(kind);
        kind_world
            .count_owner_loss(
                PlayerId::new(1),
                villager.class,
                class::CLASS_0,
                PlayerId::new(2),
            )
            .unwrap();
        assert_eq!(counts(&kind_world), amounts(seen));
    }
    let mut world = world(PlayerKind::Human);
    world
        .count_owner_loss(
            PlayerId::new(1),
            villager.class,
            class::PREDATOR,
            PlayerId::new(2),
        )
        .unwrap();
    assert_eq!(counts(&world), amounts([0.0, 1.0, 0.0, 1.0]));
    world.players[1].kind = PlayerKind::Computer;
    assert!(
        world
            .count_owner_loss(
                PlayerId::new(1),
                villager.class,
                class::CLASS_0,
                PlayerId::new(2)
            )
            .is_err()
    );
}

/// [`killed_sheep_goes_to_gaia`]'s world: player 1's town centre 10029 (`killer_last`: the
/// list's last object) and its sheep 9520, and the hit that kills the sheep for Gaia.
fn killed_sheep(killer_last: bool) -> (World, Victim) {
    let sheep = Unit {
        id: object_id(9520),
        life: Lifecycle {
            master_id: master_id(705),
            hp: HitPoints::new(2.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let objects = if killer_last {
        vec![sheep, town_centre(10029)]
    } else {
        vec![town_centre(10029), sheep]
    }
    .into();
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
            Player {
                objects,
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
        ],
    )
    .with_ais([(object_id(9520), UnitAi::default())]);
    let master = Master {
        id: master_id(705),
        class: class::LIVESTOCK,
        storage: vec![(Some(attribute::FARM_FOOD), Amount::new(100.0), 1)],
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([master.clone()])),
        Arc::new(Masters::new([master])),
    ];
    stock_masters(&mut world);
    world.spatial.insert(
        object_id(9520),
        Placed {
            position: Position::new(107.0, 48.0, 0.0),
            radius: Footprint::new(0.35, 0.35),
            class: class::LIVESTOCK,
            player: PlayerId::new(1),
            master: master_id(705),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let mut victim = world
        .victim(object_id(9520), PlayerId::new(1))
        .unwrap()
        .unwrap();
    victim.hp = HitPoints::ZERO;
    victim.damaged = vec![(Some(object_id(10029)), [10029, -10, 14])];
    victim.killed = Some(Kill {
        attacker: object_id(10029),
        attacker_player: PlayerId::new(1),
        to_gaia: true,
    });
    // the town centre's damage (FUN_141082e80): the owner's counts, then slot 0x8b0's
    victim.kill_counts = vec![
        KillCount::Owner {
            attacker_class: ClassId::of_master(80),
            attacker_player: PlayerId::new(1),
        },
        KillCount::Killer(PlayerId::new(1)),
    ];
    victim.kill_state = KillState::Counted;
    (world, victim)
}

/// MarvinBlue/426048731's update 1250: 10029's second hit kills its sheep 9520 (2 hit points,
/// 12 damage): it has none, Gaia takes it (its list, its storage, its unit AI's notifies),
/// and the players count the kill (300 + 1 and 325 + 1; a sheep costs nothing).
#[test]
fn killed_sheep_goes_to_gaia() {
    let (mut world, victim) = killed_sheep(false);
    write_hit(&mut world, (PlayerId::new(1), ListIndex::new(0)), victim);
    assert_eq!(world.players[1].objects.len(), 1);
    let gone = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!((gone.id.get(), gone.life.hp), (9520, HitPoints::ZERO));
    assert_eq!(
        world
            .spatial
            .get(object_id(9520))
            .map(|placed| placed.player),
        Some(PlayerId::new(0))
    );
    assert_eq!(world.players[1].attributes[301], Amount::new(1.0));
    assert_eq!(world.players[1].attributes[326], Amount::new(1.0));
    assert_eq!(world.players[0].attributes[16], Amount::new(100.0));
}

/// The update lends its object back to its slot for a world call that may reach it
/// ([`step::Step::lend`], the game's world holding the object while its update runs):
/// the world holds it as the update left it, what the call changes on it comes back, and its
/// slot holds the stand-in again after; a removal in the call that moves the object is
/// followed. One the call takes from its list fails the update, the stand-in in nobody's list.
#[test]
fn lent_object_is_the_world_for_the_call() {
    let mut game = house_builder();
    let last = game.players[1].objects.last_place().unwrap();
    let id = game.players[1].objects[last].id;
    let seen = in_update(
        &mut game,
        (PlayerId::new(1), last),
        0,
        |unit, actions, step| {
            unit.life.hp = HitPoints::new(7.0);
            let during = step.lend((&mut *unit, &mut *actions), |world| {
                let hp = world.object(id).map(|(_, unit)| unit.life.hp.get());
                world.object_mut(id).expect("lent").1.life.amount = Amount::new(3.0);
                let _ = world
                    .swap_remove_live(PlayerId::new(1), ListIndex::new(0))
                    .unwrap();
                Ok(hp)
            })?;
            let out = step.world.object(id).is_none();
            Ok((during, unit.life.amount.get(), out, unit.id))
        },
    );
    assert_eq!(
        seen.map_err(|error| error.reason().to_owned()),
        Ok((Some(7.0), 3.0, true, id))
    );
    assert_eq!(
        game.players[1].objects.position_of(id),
        Some(ListIndex::new(0))
    );
    let mut house_builder = house_builder();
    let failed = in_update(
        &mut house_builder,
        (PlayerId::new(1), last),
        0,
        |unit, actions, step| {
            step.lend((&mut *unit, &mut *actions), |world| {
                let _ = world.players[1].objects.pop();
                Ok(())
            })
        },
    );
    assert_eq!(
        failed.map_err(|error| error.kind()),
        Err(crate::ErrorKind::Unreachable)
    );
    let ids: Vec<ObjectId> = house_builder.players[1]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect();
    assert!(!ids.contains(&TAKEN_OUT), "{ids:?}");
}

/// The hit's kill gives Gaia the sheep in slot 0 of its owner's list where the hit runs
/// (`FUN_1411664a0`: the list's last object takes its slot): the town centre that steps, the
/// last, moves into it while it is out, goes back there, and the step reports that slot for the
/// list's loop to file it at.
#[test]
fn kill_in_the_step_reports_the_object_new_slot() {
    let (mut world, mut victim) = killed_sheep(true);
    let stepped = world
        .step_object(
            (PlayerId::new(1), ListIndex::new(1)),
            (Millis::ZERO, None, false),
            |unit, actions, step| {
                if let Some(kill_left) = write_back(step, &mut victim)? {
                    file_kill(step, (unit, actions), kill_left, &roles())?;
                }
                Ok(Outcome::Stay)
            },
        )
        .unwrap();
    assert_eq!(stepped.slot, ListIndex::new(0));
    assert_eq!(
        world.players[1].objects.position_of(object_id(10029)),
        Some(ListIndex::new(0))
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].id,
        object_id(9520)
    );
}

/// A hit's kill reaches the world where the hit runs (the damage slot `FUN_1412108e0` kills and
/// counts at once: `FUN_141082e80`, slot 0x8b0): the update's later parts see the sheep Gaia's
/// and the kill counted. The updating unit's own kill (its target's retaliation) counts there
/// too, on the unit out of its slot: its owner counts the loss to Gaia (attribute 325).
#[test]
fn hit_kill_reaches_the_world_where_the_hit_runs() {
    use crate::combat::victims::{KillLeft, file_kill};
    let (mut world, mut victim) = killed_sheep(false);
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, actions, step| {
            if let Some(kill_left) = write_back(step, &mut victim)? {
                file_kill(step, (unit, actions), kill_left, &roles())?;
            }
            let gaia_s = step.world.players[0]
                .objects
                .position_of(object_id(9520))
                .is_some();
            let counted = step.world.players[1].attributes[301];
            let own = KillLeft {
                id: step.me,
                kill: Some(Kill {
                    attacker: object_id(9520),
                    attacker_player: PlayerId::new(0),
                    to_gaia: false,
                }),
                counts: vec![
                    KillCount::Owner {
                        attacker_class: class::LIVESTOCK,
                        attacker_player: PlayerId::new(0),
                    },
                    KillCount::Killer(PlayerId::new(0)),
                ],
                razed: Vec::new(),
            };
            // the town centre as a master of the table, its own kill counted by it
            unit.life.master_id = master_id(705);
            let before = step.world.players[1].attributes[325];
            file_kill(step, (unit, actions), own, &roles())?;
            Ok((
                gaia_s,
                counted,
                step.world.players[1].attributes[325] - before,
            ))
        },
    );
    assert_eq!(seen.unwrap(), (true, Amount::new(1.0), Amount::new(1.0)));
}

/// The updating unit's own kill that gives it to Gaia (a player's livestock): its slot 0x70 at
/// once on the unit out of its slot (its owner's fields Gaia's, [`Unit::ai_change_owner`]), the
/// move to Gaia's list a takeover's after the update ([`crate::world::step::Step`]'s
/// `taken_over`).
#[test]
fn an_own_kill_gives_the_livestock_to_gaia_at_once() {
    use crate::combat::victims::{KillLeft, file_kill};
    let (mut world, _) = killed_sheep(false);
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(1)),
        0,
        |unit, actions, step| {
            let own = KillLeft {
                id: step.me,
                kill: Some(Kill {
                    attacker: object_id(10029),
                    attacker_player: PlayerId::new(1),
                    to_gaia: true,
                }),
                counts: vec![
                    KillCount::Owner {
                        attacker_class: class::CLASS_0,
                        attacker_player: PlayerId::new(1),
                    },
                    KillCount::Killer(PlayerId::new(1)),
                ],
                razed: Vec::new(),
            };
            file_kill(step, (unit, actions), own, &roles())?;
            Ok((
                step.taken_over,
                step.world
                    .spatial
                    .get(object_id(9520))
                    .map(|placed| placed.player),
            ))
        },
    );
    assert_eq!(
        seen,
        Ok((Some((PlayerId::new(0), true)), Some(PlayerId::new(0))))
    );
}

/// A world call in the step that moves the stepping object's stand-in (a removal from its list,
/// the stand-in its last object) puts the object back where the stand-in went; one whose
/// stand-in left the list goes to its end, and the step fails.
#[test]
fn step_puts_its_object_back_where_its_stand_in_went() {
    use crate::objects::unit::Outcome;
    let ids = |world: &World| {
        world.players[1]
            .objects
            .iter()
            .map(|unit| unit.id)
            .collect::<Vec<_>>()
    };
    let mut world = house_builder();
    let before = ids(&world);
    let last = ListIndex::new(before.len() - 1);
    let stepped = world
        .step_object(
            (PlayerId::new(1), last),
            (Millis::new(50), None, false),
            |_, _, step| {
                let _ = step.world.players[1]
                    .objects
                    .swap_remove(ListIndex::new(0))
                    .unwrap();
                Ok(Outcome::Stay)
            },
        )
        .unwrap();
    assert_eq!(stepped.slot, ListIndex::new(0));
    assert_eq!(ids(&world)[0], before[last.get()]);
    let mut world = house_builder();
    let failed = world
        .step_object(
            (PlayerId::new(1), ListIndex::new(0)),
            (Millis::new(50), None, false),
            |_, _, step| {
                step.world.players[1].objects.truncate(0);
                Ok(Outcome::Stay)
            },
        )
        .err()
        .unwrap();
    assert_eq!(failed.kind(), crate::ErrorKind::Unreachable);
    assert_eq!(ids(&world), [before[0]]);
}

/// The list's loop goes on by its own count (`FUN_141166550`): past an object that stays,
/// sleeps or changes hands, and again at the same count after one that leaves, whose slot the
/// list's last object took.
#[test]
fn the_list_loop_runs_its_count_again_only_after_leave() {
    use crate::objects::unit::Outcome;
    let mut world = house_builder();
    let players = world.players[1].objects.len();
    assert_eq!(
        world
            .file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Stay)
            .unwrap(),
        ListCount::GoesOn
    );
    assert_eq!(
        world
            .file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Sleep)
            .unwrap(),
        ListCount::GoesOn
    );
    assert_eq!(world.players[1].objects.len(), players - 1);
    assert_eq!(
        world
            .file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Leave)
            .unwrap(),
        ListCount::Again
    );
    assert_eq!(world.players[1].objects.len(), players - 2);
}

/// A hit on player 2's castle 30 (master 82, 650 stone paid) with its annexes 31 and 32:
/// `World::victim` reads its annexes from the world's links, its main building for an annex,
/// its paid cost and that it is no trebuchet. The write-back of a razing hit (`FUN_1411cea90`):
/// the castle's `damaged_lately_timer`, its annexes' hit points, its kill (`FUN_1412005b0`: a
/// building's class counts no kill, both players count it against the other and its cost),
/// then the razing's history event (7, `data1` the owner) on the attacker's player and its adds.
#[test]
fn razed_building_write_back_reaches_its_annexes_history_and_counts() {
    let building = |id: i32, master_id: MasterId, annex_of: Option<ObjectId>| {
        Object::with_building(
            Unit {
                id: object_id(id),
                life: Lifecycle {
                    master_id,
                    hp: HitPoints::new(100.0),
                    state: ObjectState::Alive,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                annex_of: annex_of.map(MainBuildingId::new),
                ..Default::default()
            },
        )
    };
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 500],
        ..Player::default()
    };
    let (one, two) = (
        World::split_layers(vec![building(9, ids::CASTLE, None)]),
        World::split_layers(vec![
            building(30, ids::CASTLE, None),
            building(31, master_id(1), Some(object_id(30))),
            building(32, master_id(1), Some(object_id(30))),
        ]),
    );
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into()),
            player(one.0.into()),
            player(two.0.into()),
        ],
    )
    .with_parts(one.1.into_iter().chain(two.1));
    let castle = Master {
        id: ids::CASTLE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        creatable: Some(CreatablePart {
            cost: costs(&[(attribute::STONE, 650, true), (attribute::FOOD, 10, false)]),
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(3, 11)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let annex = Master {
        id: master_id(1),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        ..Master::default()
    };
    let masters = Masters::new([castle, annex]);
    world.model.masters = vec![
        Arc::new(masters.clone()),
        Arc::new(masters.clone()),
        Arc::new(masters),
    ];
    world.link_test_annex(object_id(31), object_id(30));
    world.link_test_annex(object_id(32), object_id(30));
    // a history adds events once it has an entry
    world.players[1].history.entries = vec![HistoryEntry::default()];
    let mut main_victim = world
        .victim(object_id(30), PlayerId::new(1))
        .unwrap()
        .unwrap();
    let razed = main_victim.building.clone().unwrap();
    assert_eq!(
        (razed.main, razed.annexes, razed.cost.get(), razed.trebuchet),
        (None, vec![object_id(31), object_id(32)], 650.0, false)
    );
    let victim = world
        .victim(object_id(31), PlayerId::new(1))
        .unwrap()
        .unwrap();
    assert_eq!(
        victim.building.and_then(|building| building.main),
        Some(MainBuildingId::new(object_id(30)))
    );
    main_victim.hp = HitPoints::ZERO;
    main_victim.killed = Some(Kill {
        attacker: object_id(9),
        attacker_player: PlayerId::new(1),
        to_gaia: false,
    });
    // the castle's damage (FUN_141082e80): the owner's counts, then slot 0x8b0's
    main_victim.kill_counts = vec![
        KillCount::Owner {
            attacker_class: class::CLASS_0,
            attacker_player: PlayerId::new(1),
        },
        KillCount::Killer(PlayerId::new(1)),
    ];
    let building = main_victim.building.as_mut().unwrap();
    (building.struck, building.damaged_lately) = (true, DamagedTimer::of_data(60));
    building.razed.push(Razed {
        attacker_player: PlayerId::new(1),
        owner: PlayerId::new(2),
        event: Some(EventKind::CastleDestroyed),
        adds: vec![
            (PlayerId::new(1), attribute::RAZINGS, Amount::new(1.0)),
            (PlayerId::new(2), attribute::RAZINGS_OTHER, Amount::new(1.0)),
            (PlayerId::new(1), attribute::KILLS_VALUE, Amount::new(650.0)),
        ],
    });
    write_hit(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        main_victim,
    );
    let hp: Vec<f32> = world.players[2]
        .objects
        .iter()
        .map(|unit| unit.life.hp.get())
        .collect();
    assert_eq!(hp, [0.0, 0.0, 0.0]);
    assert_eq!(
        world.players[2].objects[ListIndex::new(0)].damaged_lately,
        DamagedTimer::of_data(60)
    );
    let (attacker, owner) = (&world.players[1].attributes, &world.players[2].attributes);
    assert_eq!([attacker[20], owner[154]], amounts([0.0, 0.0]));
    assert_eq!(
        [attacker[302], owner[326], attacker[402]],
        amounts([1.0, 1.0, 650.0])
    );
    assert_eq!([attacker[43], owner[155]], amounts([1.0, 1.0]));
    assert_eq!([attacker[170], owner[152]], amounts([1300.0, 650.0]));
    let event = world.players[1]
        .history
        .event_of_kind(EventKind::CastleDestroyed)
        .unwrap();
    assert_eq!(event.data1, 2.0);
}

/// The hit `victim` written back where it ran, in the update of object `list_index` of player
/// `player_id` ([`crate::combat::victims::write_back`]), its kill and razings with it
/// ([`crate::combat::victims::file_kill`]).
fn write_hit(
    world: &mut World,
    (player_id, list_index): (PlayerId, ListIndex),
    mut victim: Victim,
) {
    in_update(world, (player_id, list_index), 0, |unit, actions, step| {
        if let Some(kill_left) = write_back(step, &mut victim)? {
            file_kill(step, (unit, actions), kill_left, &roles())?;
        }
        Ok(())
    })
    .unwrap();
}

/// A hunt that stopped a deer's decay (`FUN_140ac44b0`: the victim's decay negated) gives it a
/// master of its own (`FUN_141186d10`) whose decay is the new one; its player's master keeps
/// its decay, and a hit that left the decay as it was gives none.
#[test]
fn stopped_decay_gives_the_victim_its_own_master() {
    let unit = |id| Unit {
        id,
        life: Lifecycle {
            master_id: master_id(333),
            hp: HitPoints::new(5.0),
            amount: Amount::new(100.0),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![unit(object_id(60))].into(),
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(9))].into(),
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
        ],
    );
    let deer = Master {
        id: master_id(333),
        kind: MasterKind::COMBAT,
        life: Some(MasterData {
            decay: AmountRate::new(0.25),
            undead: false,
        }),
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([deer.clone()])),
        Arc::new(Masters::new([deer])),
    ];
    let victim = world
        .victim(object_id(60), PlayerId::new(1))
        .unwrap()
        .unwrap();
    assert_eq!(victim.decay, AmountRate::new(0.25));
    let hunter = (PlayerId::new(1), ListIndex::new(0));
    write_hit(&mut world, hunter, victim.clone());
    assert!(!world.parts.own_master.contains(object_id(60)));
    let stopped = Victim {
        decay: AmountRate::new(-0.25),
        ..victim
    };
    write_hit(&mut world, hunter, stopped);
    let decay = |master: &Master| master.life.map(|master_data| master_data.decay);
    assert_eq!(
        world.parts.own_master.get(object_id(60)).map(decay),
        Some(Some(AmountRate::new(-0.25)))
    );
    assert_eq!(
        world.model.masters[0].get(master_id(333)).map(decay),
        Some(Some(AmountRate::new(0.25)))
    );
    // a deer with a master of its own keeps it (`+0x6c0` set: no new copy), its decay changed
    if let Some(own) = world.parts.own_master.get_mut(object_id(60)) {
        own.hit_points = MasterHitPoints::of_data(7);
    }
    let again = world
        .victim(object_id(60), PlayerId::new(1))
        .unwrap()
        .unwrap();
    write_hit(
        &mut world,
        hunter,
        Victim {
            decay: AmountRate::new(0.5),
            ..again
        },
    );
    let own = world.parts.own_master.get(object_id(60)).unwrap();
    assert_eq!(
        (own.hit_points, decay(own)),
        (MasterHitPoints::of_data(7), Some(AmountRate::new(0.5)))
    );
}

/// A hit on a sleeping object (a tree a blast reached) written back where it ran: it takes
/// its hit points and what it holds and sleeps on, still flagged, at 1 hit point or more;
/// below 1 it wakes to the end of its player's live list and takes the rest, the hit's cleared
/// sleep flag among it.
#[test]
fn hit_wakes_sleeping_object_only_below_1_hit_point() {
    let tree = |id| Unit {
        id,
        life: Lifecycle {
            master_id: master_id(349),
            hp: HitPoints::new(20.0),
            amount: Amount::new(100.0),
            state: ObjectState::Alive,
            goto_sleep: true,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let hit = |hp: HitPoints| {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    objects: vec![tree(object_id(51))].into(),
                    sleeping: vec![tree(object_id(50))].into(),
                    attributes: vec![Amount::ZERO; 500],
                    ..Player::default()
                },
                Player {
                    objects: vec![tree(object_id(9))].into(),
                    attributes: vec![Amount::ZERO; 500],
                    ..Player::default()
                },
            ],
        );
        let master = Master {
            id: master_id(349),
            kind: MasterKind::STATIC,
            class: class::TREE,
            ..Master::default()
        };
        world.model.masters = vec![
            Arc::new(Masters::new([master.clone()])),
            Arc::new(Masters::new([master])),
        ];
        let mut victim = world
            .victim(object_id(50), PlayerId::new(1))
            .unwrap()
            .unwrap();
        (victim.hp, victim.amount, victim.goto_sleep) = (hp, Amount::new(90.0), false);
        write_hit(&mut world, (PlayerId::new(1), ListIndex::new(0)), victim);
        let gaia = &world.players[0];
        let ids = |object_list: &ObjectList| -> Vec<i32> {
            object_list.iter().map(|unit| unit.id.get()).collect()
        };
        let (_, unit) = world.object(object_id(50)).unwrap();
        (
            ids(&gaia.objects),
            ids(&gaia.sleeping),
            (
                unit.life.hp.get(),
                unit.life.amount.get(),
                unit.life.goto_sleep,
            ),
        )
    };
    assert_eq!(
        hit(HitPoints::new(5.0)),
        (vec![51], vec![50], (5.0, 90.0, true))
    );
    assert_eq!(
        hit(HitPoints::new(0.5)),
        (vec![51, 50], vec![], (0.5, 90.0, false))
    );
}

/// `FUN_140fde380`: a master of type 70 and up, of class 51 or 54, with a weapon (`+0x810`
/// above 0) is a trebuchet; the paid cost (`+0x880`) sums the entries with a resource and the
/// paid flag, as floats in their order.
#[test]
fn trebuchet_is_master_of_class_51_or_54_with_weapon() {
    use crate::combat::victims::is_trebuchet;
    use crate::combat::victims::paid_cost;
    let master_of = |kind, class, weapons: Vec<ClassAmount>| Master {
        kind,
        class,
        combat: Some(CombatPart {
            weapons,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    assert!(is_trebuchet(
        &master_of(
            MasterKind::BUILDING,
            class::CLASS_54,
            vec![ClassAmount::new(4, 200)]
        ),
        &roles()
    ));
    assert!(is_trebuchet(
        &master_of(
            MasterKind::BUILDING,
            class::CLASS_51,
            vec![ClassAmount::new(4, 1)]
        ),
        &roles()
    ));
    assert!(is_trebuchet(
        &master_of(
            MasterKind::COMBAT,
            class::CLASS_54,
            vec![ClassAmount::new(4, 1)]
        ),
        &roles()
    ));
    assert!(!is_trebuchet(
        &master_of(MasterKind::BUILDING, class::CLASS_51, vec![]),
        &roles()
    ));
    assert!(!is_trebuchet(
        &master_of(
            MasterKind::MISSILE,
            class::CLASS_54,
            vec![ClassAmount::new(4, 1)]
        ),
        &roles()
    ));
    assert!(!is_trebuchet(
        &master_of(
            MasterKind::BUILDING,
            class::BUILDING,
            vec![ClassAmount::new(3, 11)]
        ),
        &roles()
    ));
    let paid = Master {
        creatable: Some(CreatablePart {
            cost: costs(&[
                (attribute::WOOD, 200, true),
                (attribute::GOLD, 200, true),
                (attribute::STONE, 7, false),
            ]),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    assert_eq!(paid_cost(&paid), Amount::new(400.0));
}

/// MarvinBlue/426048731's 1086: SET GATHER POINT on TC 9149 at (107.2451, 46.075863) on the
/// sheep 9520 (master 705).
#[test]
fn set_gather_point_takes_the_point_and_target_or_clears_it() {
    let mut tc = town_centre(9149);
    tc.life.master_id = ids::TOWN_CENTRE;
    let mut world = one_player(vec![tc], [200.0; 4]);
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: GarrisonCapacity::of_data(15),
        ..Master::default()
    }]))];
    let set = |target, point, own| {
        Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point,
            target,
            target_master: Some(master_id(705)),
            own,
        })
    };
    world.tick_with(
        Millis::new(25),
        &[set(
            Some(object_id(9520)),
            Point::new(107.2451, 46.075_863),
            false,
        )],
    );
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(
        gather_of(&world, 0, 0),
        Some(GatherPoint {
            state: GatherPointState::Point,
            point: Position::new(107.2451, 46.075_863, 0.0),
            target: Some(object_id(9520)),
            target_master: Some(master_id(705)),
        })
    );
    world.tick_with(Millis::new(25), &[set(None, Point::new(-1.0, -1.0), false)]);
    assert_eq!(gather_of(&world, 0, 0), None);
    world.tick_with(Millis::new(25), &[set(None, Point::new(-1.0, -1.0), true)]);
    assert_eq!(
        gather_of(&world, 0, 0).map(|gather_point| gather_point.target),
        Some(Some(object_id(9149)))
    );
}

/// The gather point of player `player`'s live object `index`
/// ([`crate::world::parts::Parts::gather_point`]).
fn gather_of(world: &World, player: usize, index: usize) -> Option<GatherPoint> {
    let id = world.players[player].objects[ListIndex::new(index)].id;
    world.parts.gather_point.get(id).copied()
}

/// A town centre of master 109 with the given position and a gather state, for SET GATHER STATE.
fn gathering_centre(id: i32, at: Position) -> Unit {
    let mut tc = centre(id);
    tc.place = Static::at(at);
    tc
}

/// SET GATHER STATE (`FUN_140e727e0`; 428052362's computer players send 0, 1 and 0xff): a building
/// takes itself for its gather point at its own position with the packet's state, 0 clears it.
#[test]
fn set_gather_state_makes_the_building_its_own_gather_point_or_clears_it() {
    let mut world = one_player(
        vec![gathering_centre(9149, Position::new(104.0, 47.0, 0.5))],
        [200.0; 4],
    );
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: GarrisonCapacity::of_data(15),
        ..Master::default()
    }]))];
    let set = |state| {
        Command::SetGatherState(SetGatherStateCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            state,
        })
    };
    let own = |state| {
        Some(GatherPoint {
            state,
            point: Position::new(104.0, 47.0, 0.5),
            target: Some(object_id(9149)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    };
    world.tick_with(Millis::new(25), &[set(Some(GatherPointState::Point))]);
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(gather_of(&world, 0, 0), own(GatherPointState::Point));
    // the state byte is the state, 0xff included
    world.tick_with(Millis::new(25), &[set(Some(GatherPointState::Kept))]);
    assert_eq!(gather_of(&world, 0, 0), own(GatherPointState::Kept));
    world.tick_with(Millis::new(25), &[set(None)]);
    assert_eq!(gather_of(&world, 0, 0), None);
}

/// SET GATHER STATE's test is SET GATHER POINT's: a building that neither garrisons nor trains
/// keeps what it had (a state of 0 clears whatever the building is), and an object the player
/// does not hold is left out.
#[test]
fn set_gather_state_leaves_building_that_takes_no_gather_point_and_gone_object() {
    let mut house = gathering_centre(10909, Position::new(20.0, 30.0, 0.0));
    house.life.master_id = ids::HOUSE;
    let gather = Some(GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(5.0, 6.0, 0.0),
        target: None,
        target_master: None,
    });
    let at = house.id;
    let mut world = one_player(vec![house], [200.0; 4]);
    world.set_part(at, Part::GatherPoint(gather)).unwrap();
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        ..Master::default()
    }]))];
    let set = |state| {
        Command::SetGatherState(SetGatherStateCommand {
            player: player_id(0),
            units: unit_list(object_ids([4242, 10909])),
            state,
        })
    };
    let held = gather_of(&world, 0, 0);
    world.tick_with(Millis::new(25), &[set(Some(GatherPointState::Kept))]);
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(gather_of(&world, 0, 0), held);
    world.tick_with(Millis::new(25), &[set(None)]);
    assert_eq!(gather_of(&world, 0, 0), None);
}

/// SET GATHER POINT on a gather point of state 0xff (`FUN_140e7e9a0`, `FUN_1411c7ff0`): its x
/// and y move, its target and master stay and its state stays 0xff; on itself the state becomes 1
/// with the target kept.
#[test]
fn set_gather_point_keeps_the_target_of_kept_gather_point() {
    let tc = gathering_centre(9149, Position::new(104.0, 47.0, 0.5));
    let gather = Some(GatherPoint {
        state: GatherPointState::Kept,
        point: Position::new(104.0, 47.0, 0.5),
        target: Some(object_id(9149)),
        target_master: Some(ids::TOWN_CENTRE),
    });
    let at = tc.id;
    let mut world = one_player(vec![tc], [200.0; 4]);
    world.set_part(at, Part::GatherPoint(gather)).unwrap();
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: GarrisonCapacity::of_data(15),
        ..Master::default()
    }]))];
    let set = |target: Option<i32>, own| {
        Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point: Point::new(107.25, 46.5),
            target: target.map(object_id),
            target_master: Some(master_id(705)),
            own,
        })
    };
    world.tick_with(Millis::new(25), &[set(Some(9520), false)]);
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(
        gather_of(&world, 0, 0),
        Some(GatherPoint {
            state: GatherPointState::Kept,
            point: Position::new(107.25, 46.5, 0.5),
            target: Some(object_id(9149)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    );
    world.parts.gather_point.get_mut(at).unwrap().target = Some(object_id(9520));
    world.tick_with(Millis::new(25), &[set(None, true)]);
    assert_eq!(
        gather_of(&world, 0, 0),
        Some(GatherPoint {
            state: GatherPointState::Point,
            point: Position::new(104.0, 47.0, 0.5),
            target: Some(object_id(9520)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    );
}

/// SET GATHER POINT's test (`FUN_140e7e9a0`): a building whose master does not garrison takes
/// the gather point when the player trains a unit there (`FUN_141625a20`'s count above 0); one
/// that neither garrisons nor trains ignores it, and the player goes on.
#[test]
fn set_gather_point_takes_building_that_trains_without_garrisoning() {
    let mut barracks = town_centre(10908);
    barracks.life.master_id = master_id(12);
    let mut house = town_centre(10909);
    house.life.master_id = ids::HOUSE;
    let mut world = one_player(vec![barracks, house], [200.0; 4]);
    world.model.masters = vec![Arc::new(Masters::new([
        Master {
            id: master_id(12),
            kind: MasterKind::BUILDING,
            ..Master::default()
        },
        Master {
            id: ids::HOUSE,
            kind: MasterKind::BUILDING,
            ..Master::default()
        },
        Master {
            id: master_id(74),
            kind: MasterKind::COMBAT,
            available: Availability::AVAILABLE,
            creatable: Some(CreatablePart {
                trained_at: vec![master_id(12)],
                ..CreatablePart::default()
            }),
            ..Master::default()
        },
    ]))];
    let set = Command::GatherPoint(GatherPointCommand {
        player: player_id(0),
        units: unit_list(object_ids([10908, 10909])),
        point: Point::new(20.5, 30.5),
        target: None,
        target_master: None,
        own: false,
    });
    world.tick_with(Millis::new(25), &[set]);
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(
        gather_of(&world, 0, 0),
        Some(GatherPoint {
            state: GatherPointState::Point,
            point: Position::new(20.5, 30.5, 0.0),
            target: None,
            target_master: None,
        })
    );
    assert_eq!(gather_of(&world, 0, 1), None);
}

/// `BUILDING_COMMAND_QUEUE` on a town centre with a gather point: the gather point goes
/// into the queue first and clears, then the packet's step; a second packet adds its step
/// alone; SET GATHER POINT empties the queue.
#[test]
fn command_queue_takes_the_gather_point_first_then_the_steps() {
    let mut tc = town_centre(9149);
    tc.life.master_id = ids::TOWN_CENTRE;
    let ai = UnitAi {
        ai_kind: UnitAiKind::Building,
        current_order: None,
        current_action: None,
        current_target: None,
        last_target: None,
        ..UnitAi::default()
    };
    let gather = Some(GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(30.5, 40.5, 0.0),
        target: None,
        target_master: None,
    });
    let at = tc.id;
    let mut world = one_player(vec![tc], [200.0; 4]).with_ais([(at, ai)]);
    world.set_part(at, Part::GatherPoint(gather)).unwrap();
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        garrison: GarrisonCapacity::of_data(15),
        ..Master::default()
    }]))];
    let step = |point| {
        Command::CommandQueue(BuildingQueueCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            target: None,
            point,
        })
    };
    world.tick_with(Millis::new(25), &[step(Point::new(50.0, 60.0))]);
    world.tick_with(Millis::new(25), &[step(Point::new(70.0, 80.0))]);
    assert_eq!(world.players[0].unsupported, None);
    let tc = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(world.parts.gather_point.get(tc.id), None);
    let steps: Vec<_> = world
        .parts
        .unit_ai
        .get(tc.id)
        .unwrap()
        .building_orders
        .iter()
        .map(|queued_order| {
            (
                queued_order.order,
                queued_order.target,
                queued_order.point,
                queued_order.priority.get(),
                queued_order.ready,
            )
        })
        .collect();
    let at = |x, y| (OrderKind::Move, None, Position::new(x, y, -1.0), -1, false);
    assert_eq!(steps, vec![at(30.5, 40.5), at(50.0, 60.0), at(70.0, 80.0)]);
    world.tick_with(
        Millis::new(25),
        &[Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point: Point::new(-1.0, -1.0),
            target: None,
            target_master: None,
            own: false,
        })],
    );
    let left = &world
        .parts
        .unit_ai
        .get(world.players[0].objects[ListIndex::new(0)].id)
        .unwrap()
        .building_orders;
    assert_eq!(left.len(), 0);
}

#[test]
fn unit_due_to_spawn_stops_the_player() {
    let mut world = one_player(vec![centre(9)], [200.0; 4]);
    let quick = queue(9, 97, 1);
    world.tick_with(Millis::new(1000), &[quick]);
    // the building makes the make action in its update: one unit in production
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(world.players[0].attributes[81], Amount::new(1.0));
    world.tick(Millis::new(1000));
    assert_eq!(
        world.players[0]
            .freezes
            .first()
            .map(|freeze| freeze.2.clone()),
        Some(GenieError::unsupported(
            "a spawn without the players' sight"
        ))
    );
}

/// Slot 0x468 (`FUN_141157c50`): the top level's path, its points, length and flags, and
/// that level's waypoint; nothing without a path at the top or without movement state.
#[test]
fn unit_patrol_information_is_its_top_path_and_waypoint() {
    let mut mover = walker();
    let mut walked = PathData::without_path();
    walked.has_path = true;
    walked.points = vec![BVector::new(1.0, 0.0, 1.0), BVector::new(2.0, 0.0, 1.0)];
    let mut top = PathData::without_path();
    top.has_path = true;
    top.points = vec![BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)];
    top.length = Distance::new(10.0);
    top.path_flags = PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD;
    top.waypoint = WaypointIndex::of_data(1);
    mover.paths = vec![walked, top];
    let (saved, waypoint) = patrol_information(Some(&mover)).unwrap();
    assert_eq!(
        saved.path.points,
        [BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)]
    );
    assert_eq!(
        (saved.path.length, saved.flags, waypoint.get()),
        (
            Distance::new(10.0),
            PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD,
            1
        )
    );
    mover.paths[1].has_path = false;
    assert_eq!(patrol_information(Some(&mover)), None);
    assert_eq!(patrol_information(None), None);
}

/// `restorePatrolInformation`'s unit part: the own waypoints become the saved patrol's
/// points (slot 0x448, a whole coordinate moved to the tile's middle), and a forced, running
/// `MoveTo` goes to the saved waypoint; the old `MoveTo` to an object lets it go (its teardown's
/// slot 0x180, the caller's).
#[test]
fn restored_patrol_takes_its_points_and_runs_to_its_waypoint() {
    let saved = PatrolPath {
        path: Path {
            points: vec![
                BVector::new(20.0, 0.5, 20.0),
                BVector::new(30.0, 0.5, 20.25),
            ],
            length: Distance::new(10.0),
        },
        flags: PathFlags::LOOPS | PathFlags::BACK_AND_FORTH | PathFlags::FORWARD,
    };
    let mut motion = Motion::Moving(walker());
    let mut actions = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            state: MoveTo::Moving,
            point: Position::ORIGIN,
            target: Some(object_id(77)),
            range: Distance::ZERO,
            sprite: None,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        })),
        ..Default::default()
    };
    let mut unit = Unit::default();
    unit.life.master_id = VILLAGER_MALE;
    let ai = UnitAi {
        patrol_path: Some(saved),
        patrol_waypoint: WaypointIndex::of_data(1),
        ..UnitAi::default()
    };
    let run = Unit::restore_patrol(Some(&ai), motion.mover_mut(), &tuning().movement)
        .expect("the saved patrol's move");
    assert_eq!(
        (run.point, run.range, run.force, run.pace),
        (
            Position::new(30.0, 20.25, 0.5),
            Distance::ZERO,
            OrderForce::Forced,
            Pace::Run
        )
    );
    assert_eq!(
        motion.mover().unwrap().user_waypoints,
        [
            BVector::new(20.5, 0.5, 20.5),
            BVector::new(30.5, 0.5, 20.25)
        ]
    );
    assert_eq!(
        ai_move_of((&mut unit, &mut actions, &mut motion), run, class::CLASS_6),
        Ok(vec![object_id(77)])
    );
    let move_to = actions.list.move_to().copied().unwrap();
    assert_eq!(
        (
            move_to.goal(),
            move_to.range,
            move_to.sprite.map(|graphic| graphic.id)
        ),
        (
            Some(Position::new(30.0, 20.25, 0.5)),
            Distance::ZERO,
            Some(SpriteId::named(13154))
        )
    );
}

/// `TRIBE_Combat_Object::update` (`0x141208440`) before `RGE_Combat_Object::update`: a unit
/// of class 35 (the petard; also 64 and the masters 527, 528, 1104, 2133 to 2135) below 1 hit
/// point in state 2 blasts about its place (slot 0x848, `FUN_14107f690`, factor 1, itself left
/// out, no main target), then dies. 426048706 (Aftermath): Gaia's petards 7948.. placed at −1
/// hit points blast the players' walls in update 1.
#[test]
fn petard_below_1_hit_point_blasts_about_itself_then_dies() {
    use crate::map::spatial::Placed;
    let petard = Master {
        id: master_id(706),
        kind: MasterKind::COMBAT,
        class: class::CLASS_35,
        hit_points: MasterHitPoints::of_data(45),
        death_sprite: Some(SpriteId::named(1751)),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 100)],
            area_effect: Distance::new(2.0),
            area_damage: DamageShare::new(1.0),
            missile: MasterField::NONE,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let wall = Master {
        id: master_id(117),
        kind: MasterKind::BUILDING,
        class: class::WALL,
        hit_points: MasterHitPoints::of_data(1800),
        combat: Some(CombatPart {
            armors: vec![ClassAmount::new(4, 0)],
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let unit = |id, master, hp, position| Unit {
        id,
        place: Static::at(position),
        pasture: None,
        my_pasture: None,
        life: Lifecycle {
            master_id: master,
            state: ObjectState::Alive,
            hp,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![unit(
                    object_id(7948),
                    master_id(706),
                    HitPoints::new(-1.0),
                    Position::new(101.5, 158.5, 0.0),
                )]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: vec![unit(
                    object_id(7231),
                    master_id(117),
                    HitPoints::new(1080.0),
                    Position::new(102.5, 158.5, 0.0),
                )]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    let tree = Master {
        id: master_id(1250),
        kind: MasterKind::STATIC,
        class: class::TREE,
        hit_points: MasterHitPoints::of_data(500),
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([petard, tree])),
        Arc::new(Masters::new([wall])),
    ];
    // a sleeping tree beside it (426048706's 8858): the blast walks the sleeping too
    let _ = world
        .push_asleep(
            PlayerId::new(0),
            Unit {
                life: Lifecycle {
                    class: Class::Static,
                    ..unit(
                        object_id(8858),
                        master_id(1250),
                        HitPoints::new(500.0),
                        Position::new(100.5, 158.5, 0.0),
                    )
                    .life
                },
                ..unit(
                    object_id(8858),
                    master_id(1250),
                    HitPoints::new(500.0),
                    Position::new(100.5, 158.5, 0.0),
                )
            },
        )
        .unwrap();
    world.model.graphics = Arc::new(Graphics::from([(
        SpriteId::named(1751),
        Graphic {
            id: SpriteId::named(1751),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    )]));
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 158.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial = Spatial::new(GridSize::new(200, 200));
    world.spatial.insert(
        object_id(7948),
        placed(101.5, class::CLASS_35, PlayerId::new(0), master_id(706)),
    );
    world.spatial.insert(
        object_id(7231),
        placed(102.5, class::WALL, PlayerId::new(1), master_id(117)),
    );
    world.spatial.insert(
        object_id(8858),
        placed(100.5, class::TREE, PlayerId::new(0), master_id(1250)),
    );
    // a sleeping tree of id 0 out of the blast's reach but among the objects about it
    // (428491175's oak 0 at (107.5, 0.5), 9 tiles from the petard 5342): the updating petard's
    // place in its list holds no object of that id, so the tree is read as itself and sleeps on
    let far = unit(
        object_id(0),
        master_id(1250),
        HitPoints::new(500.0),
        Position::new(95.5, 158.5, 0.0),
    );
    let _ = world
        .push_asleep(
            PlayerId::new(0),
            Unit {
                life: Lifecycle {
                    class: Class::Static,
                    ..far.life
                },
                ..far
            },
        )
        .unwrap();
    world.spatial.insert(
        object_id(0),
        placed(95.5, class::TREE, PlayerId::new(0), master_id(1250)),
    );
    world.tick(Millis::new(100));
    assert_eq!(world.players[0].unsupported, None);
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].life.hp,
        HitPoints::new(980.0)
    );
    // the sleeping tree is among the objects the blast walks; a petard's blast (its weapon
    // range at most 1) spares its own player's objects, so Gaia's tree sleeps on untouched
    assert_eq!(
        world.players[0]
            .sleeping
            .iter()
            .map(|unit| (unit.id.get(), unit.life.hp.get()))
            .collect::<Vec<_>>(),
        [(8858, 500.0), (0, 500.0)]
    );
    assert!(ObjectState::Dying <= world.players[0].objects[ListIndex::new(0)].life.state);
}

/// A dying building lets its annexes go with its hit points (the static update's death,
/// `0x14118ad50`): a gate (426048706's 7228, master 88) at no hit points dies (3) and goes on to
/// its dying sprite (the animated update's `set_state(4)`); its annex (7229, master 95, holding
/// nothing), after it in the list, dies in its own update of the same tick and reaches state 4
/// too, no longer an annex.
#[test]
fn an_annex_follows_its_dying_main_building_into_state_4() {
    let graphic = Graphic {
        id: SpriteId::named(1751),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let gate = |id| Master {
        id,
        kind: MasterKind::BUILDING,
        class: class::GATE,
        hit_points: MasterHitPoints::of_data(1650),
        standing_sprite: Some(SpriteId::named(1751)),
        death_sprite: Some(SpriteId::named(1751)),
        building: Some(BuildingMaster {
            stack_unit: None,
            foundation_terrain: None,
            tech: None,
            head_unit: None,
            transform_unit: None,
            ..BuildingMaster::default()
        }),
        ..Master::default()
    };
    let building = |id, master, hp| {
        Object::with_building(
            Unit {
                id,
                place: Static::at(Position::new(124.5, 174.5, 0.0)),
                pasture: None,
                my_pasture: None,
                sprite: Some(ActiveSprite::start(
                    graphic,
                    &mut Well512::default(),
                    tuning().sprite,
                )),
                life: Lifecycle {
                    class: Class::Animated,
                    master_id: master,
                    state: ObjectState::Alive,
                    hp,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                construction: Some(Construction {
                    progress: Seconds::new(100.0),
                    desolid: false,
                    builders: BuilderCount::of_data(0),
                    hit_points: MasterHitPoints::of_data(1650),
                    build_time: WholeSeconds::of_data(100),
                    record: Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::UNIT,
                    },
                    snow: Snow::None,
                    sprite_override: false,
                }),
                ..Default::default()
            },
        )
    };
    let split = World::split_layers(vec![
        building(object_id(7228), master_id(88), HitPoints::new(-58.0)),
        Object::with_building(
            building(object_id(7229), master_id(95), HitPoints::new(1650.0)).unit,
            Building {
                annex_of: Some(MainBuildingId::new(object_id(7228))),
                ..building(object_id(7229), master_id(95), HitPoints::new(1650.0))
                    .parts
                    .building
            },
        ),
    ]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: split.0.into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    )
    .with_parts(split.1);
    world.model.masters = vec![
        Arc::new(Masters::new([])),
        Arc::new(Masters::new([gate(master_id(88)), gate(master_id(95))])),
    ];
    world.model.graphics = Arc::new(Graphics::from([(SpriteId::named(1751), graphic)]));
    // the main building's slot of the annex, as a load fills it
    world.link_test_annex(object_id(7229), object_id(7228));
    world.sync_world();
    world.tick(Millis::new(100));
    assert_eq!(world.players[1].unsupported, None);
    let states: Vec<(i32, ObjectState)> = world.players[1]
        .objects
        .iter()
        .map(|unit| (unit.id.get(), unit.life.state))
        .collect();
    assert_eq!(
        states,
        [
            (7228, ObjectState::DyingSprite),
            (7229, ObjectState::DyingSprite)
        ]
    );
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(1)].id)
            .annex_of,
        None
    );
    assert_eq!(world.objects().annexes(object_id(7228)).ids().count(), 0);
}

/// 428799090's palisade gate foundation (34600, master 800) dies at no hit points in update 2674
/// while its two annexes (34601, 34602, master 1192) come before it in the list: they stay
/// foundations with the gate's hit points (0) and no main building, then die in their own
/// update of the next tick on their own sprites, not the gate's states.
#[test]
fn annexes_before_their_dying_foundation_die_in_the_next_update() {
    let graphic_of = |id, flags| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags,
    };
    let master = |id, death_sprite, destruction_sprite| Master {
        id,
        kind: MasterKind::BUILDING,
        class: class::GATE,
        hit_points: MasterHitPoints::of_data(10),
        standing_sprite: Some(SpriteId::named(1751)),
        death_sprite,
        building: Some(BuildingMaster {
            stack_unit: None,
            foundation_terrain: None,
            tech: None,
            head_unit: None,
            transform_unit: None,
            destruction_sprite,
            construction_sprite: None,
            ..BuildingMaster::default()
        }),
        creatable: Some(CreatablePart {
            build_time: WholeSeconds::of_data(70),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let foundation = |id, master_id, hp, annex_of| {
        let mut object = Object::with_building(
            Unit {
                id,
                pasture: None,
                my_pasture: None,
                sprite: Some(ActiveSprite::at_rest(
                    graphic_of(SpriteId::named(1751), SpriteFlags::NONE),
                    Frame::of_data(0),
                )),
                place: Static::at(Position::new(150.0, 46.0, 0.0)),
                life: Lifecycle {
                    class: Class::Animated,
                    master_id,
                    state: ObjectState::Foundation,
                    hp,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                annex_of,
                construction: Some(Construction {
                    progress: Seconds::new(10.0),
                    desolid: false,
                    builders: BuilderCount::of_data(0),
                    hit_points: MasterHitPoints::of_data(10),
                    build_time: WholeSeconds::of_data(70),
                    record: Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::UNIT,
                    },
                    snow: Snow::None,
                    sprite_override: false,
                }),
                ..Default::default()
            },
        );
        object.parts.motion = Some(Motion::Animated(Animated::default()));
        object
    };
    let split = World::split_layers(vec![
        foundation(
            object_id(34602),
            master_id(1192),
            HitPoints::new(1.0),
            Some(MainBuildingId::new(object_id(34600))),
        ),
        foundation(
            object_id(34601),
            master_id(1192),
            HitPoints::new(1.0),
            Some(MainBuildingId::new(object_id(34600))),
        ),
        foundation(object_id(34600), master_id(800), HitPoints::ZERO, None),
    ]);
    let mut game = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: split.0.into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    )
    .with_parts(split.1);
    game.link_test_annex(object_id(34602), object_id(34600));
    game.link_test_annex(object_id(34601), object_id(34600));
    game.model.masters = vec![
        Arc::new(Masters::new([])),
        Arc::new(Masters::new([
            master(master_id(800), Some(SpriteId::named(38)), None),
            master(
                master_id(1192),
                Some(SpriteId::named(37)),
                Some(SpriteId::named(112)),
            ),
        ])),
    ];
    game.model.graphics = Arc::new(Graphics::from([
        (
            SpriteId::named(1751),
            graphic_of(SpriteId::named(1751), SpriteFlags::NONE),
        ),
        (
            SpriteId::named(38),
            graphic_of(
                SpriteId::named(38),
                SpriteFlags::ANIMATED | SpriteFlags::ONCE,
            ),
        ),
        (
            SpriteId::named(37),
            graphic_of(
                SpriteId::named(37),
                SpriteFlags::ANIMATED | SpriteFlags::ONCE,
            ),
        ),
        (
            SpriteId::named(112),
            graphic_of(
                SpriteId::named(112),
                SpriteFlags::ANIMATED | SpriteFlags::ONCE,
            ),
        ),
    ]));
    let looks = |world: &World| -> Vec<(ObjectId, ObjectState, f32, Option<MainBuildingId>)> {
        world.players[1]
            .objects
            .iter()
            .map(|unit| {
                (
                    unit.id,
                    unit.life.state,
                    unit.life.hp.get(),
                    world.parts.building_of(unit.id).annex_of,
                )
            })
            .collect()
    };
    game.tick(Millis::new(100));
    assert_eq!(game.players[1].unsupported, None);
    assert_eq!(
        looks(&game),
        [
            (object_id(34602), ObjectState::Foundation, 0.0, None),
            (object_id(34601), ObjectState::Foundation, 0.0, None),
            (object_id(34600), ObjectState::DyingSprite, 0.0, None)
        ]
    );
    game.tick(Millis::new(100));
    assert_eq!(game.players[1].unsupported, None);
    let states: Vec<(ObjectId, ObjectState)> =
        looks(&game).iter().map(|look| (look.0, look.1)).collect();
    assert_eq!(
        states,
        [
            (object_id(34602), ObjectState::DyingSprite),
            (object_id(34601), ObjectState::DyingSprite),
            (object_id(34600), ObjectState::DyingSprite)
        ]
    );
    let sprites: Vec<Option<SpriteId>> = game.players[1]
        .objects
        .iter()
        .map(|unit| unit.sprite.map(|active_sprite| active_sprite.graphic.id))
        .collect();
    assert_eq!(
        sprites,
        [
            Some(SpriteId::named(112)),
            Some(SpriteId::named(112)),
            Some(SpriteId::named(38))
        ]
    );
}

/// The regeneration's end: an object at its most hit points after it (slot 0xc0 at least slot
/// 200) has the armor its charge's hits took back (`+0x9e4`/`+0x9e6` zeroed); below its most it
/// keeps the loss.
#[test]
fn unit_regenerated_to_its_most_has_its_armor_back() {
    let berserk = Master {
        id: master_id(692),
        kind: MasterKind::COMBAT,
        hit_points: MasterHitPoints::of_data(54),
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(20.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let unit = |hp| {
        (
            Unit {
                id: object_id(1),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Combat {
                armor_removed: ArmorPair::new(2, 1),
                ..Default::default()
            },
        )
    };
    let (mut healed, mut healed_combat) = unit(HitPoints::new(53.0));
    assert_eq!(
        healed.regenerate(
            &mut healed_combat,
            &berserk,
            Seconds::new(3.0),
            &tuning().turn,
        ),
        None
    );
    assert_eq!(
        (healed.life.hp, healed_combat.armor_removed),
        (HitPoints::new(54.0), ArmorPair::ZERO)
    );
    let (mut hurt, mut hurt_combat) = unit(HitPoints::new(40.0));
    assert_eq!(
        hurt.regenerate(
            &mut hurt_combat,
            &berserk,
            Seconds::new(3.0),
            &tuning().turn,
        ),
        None
    );
    assert_eq!(
        (hurt.life.hp, hurt_combat.armor_removed),
        (HitPoints::new(41.0), ArmorPair::new(2, 1))
    );
}

/// `TRIBE_Combat_Object::update`'s regeneration (`0x141208440`): the rate (slot 0x260,
/// `FUN_1412033a0`) is the stinger plus (the powerup plus (a hero's 30 plus the master's)); a
/// whole hit point comes per 60 / |rate| seconds of the world's dt plus the time held back
/// (`+0xa10`), the fraction kept as time. A positive rate heals up to the most (slot 0xb8); a
/// negative one takes off at most the hit points (slot 0xb0); below 1 hit point, out of state
/// 2 or below 1.2e-7 in size nothing changes.
#[test]
fn unit_regenerates_whole_hit_points_and_keeps_the_rest_as_time() {
    use crate::objects::hit_points::regeneration_rate;
    let berserk = Master {
        id: master_id(692),
        kind: MasterKind::COMBAT,
        hit_points: MasterHitPoints::of_data(54),
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(20.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let unit = |hp| Unit {
        id: object_id(1),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    // 20 a minute: a point per 3 s; 2 s of dt plus 1.5 held is one point, the rest held back
    // (in f32: 3.5 / 3 less its whole point, times 3)
    let mut regenerating = unit(HitPoints::new(30.0));
    let mut unit_combat = Combat {
        regen_timer: Seconds::new(1.5),
        ..Default::default()
    };
    assert_eq!(
        regenerating.regenerate(
            &mut unit_combat,
            &berserk,
            Seconds::new(2.0),
            &tuning().turn,
        ),
        None
    );
    assert_eq!(
        (regenerating.life.hp, unit_combat.regen_timer),
        (
            HitPoints::new(31.0),
            Seconds::new(((3.5f32 / 3.0) % 1.0) * 3.0)
        )
    );
    // at the most nothing is added, and the time still runs
    let mut full = unit(HitPoints::new(54.0));
    let mut full_combat = Combat::default();
    assert_eq!(
        full.regenerate(
            &mut full_combat,
            &berserk,
            Seconds::new(3.0),
            &tuning().turn,
        ),
        None
    );
    assert_eq!(
        (full.life.hp, full_combat.regen_timer),
        (HitPoints::new(54.0), Seconds::ZERO)
    );
    // a hero's 30 and the powerups: stinger + (powerup + (30 + master))
    let hero = Master {
        creatable: Some(CreatablePart {
            hero_flag: HeroFlags::FULL_HERO,
            ..berserk.clone().creatable.unwrap_or_default()
        }),
        ..berserk.clone()
    };
    let powered_combat = Combat {
        regen_powerup: Regeneration::new(5.0),
        regen_stinger: Regeneration::new(5.0),
        ..Default::default()
    };
    assert_eq!(
        regeneration_rate(&hero, &powered_combat, &tuning().turn),
        Regeneration::new(60.0)
    );
    let not_hero = Master {
        creatable: Some(CreatablePart {
            hero_flag: HeroFlags::REGENERATES | HeroFlags::INVERTED,
            ..berserk.clone().creatable.unwrap_or_default()
        }),
        ..berserk.clone()
    };
    assert_eq!(
        regeneration_rate(&not_hero, &powered_combat, &tuning().turn),
        Regeneration::new(30.0)
    );
    // a gate of Aftermath's: -1000 a minute takes 1 hit point per 0.06 s, at most what it has
    let gate = Master {
        hit_points: MasterHitPoints::of_data(1650),
        kind: MasterKind::BUILDING,
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(-1000.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let mut gate_unit = unit(HitPoints::new(1650.0));
    let mut gate_combat = Combat::default();
    assert_eq!(
        gate_unit.regenerate(&mut gate_combat, &gate, Seconds::new(0.104), &tuning().turn),
        None
    );
    assert_eq!(gate_unit.life.hp, HitPoints::new(1649.0));
    let mut low = unit(HitPoints::new(1.0));
    let mut low_combat = Combat::default();
    assert_eq!(
        low.regenerate(&mut low_combat, &gate, Seconds::new(1.0), &tuning().turn),
        None
    );
    assert_eq!(low.life.hp, HitPoints::ZERO);
    // below 1 hit point or out of state 2: nothing
    let mut dead = unit(HitPoints::new(0.5));
    let mut dead_combat = Combat::default();
    assert_eq!(
        dead.regenerate(&mut dead_combat, &gate, Seconds::new(1.0), &tuning().turn),
        None
    );
    assert_eq!(
        (dead.life.hp, dead_combat.regen_timer),
        (HitPoints::new(0.5), Seconds::ZERO)
    );
}

/// `TRIBE_Combat_Object::update`'s hit point powerup check: a powerup above its room comes off
/// the hit points, at least 1 left (`MAXSS`), and takes the room; at most the room, nothing.
#[test]
fn hit_point_powerup_above_its_room_comes_off_the_hit_points() {
    let with = |hp, up, room| {
        (
            Unit {
                life: Lifecycle {
                    hp: HitPoints::new(hp),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Combat {
                hp_powerup: HpPowerup {
                    taken: HitPoints::new(up),
                    max: HitPoints::new(room),
                },
                ..Default::default()
            },
        )
    };
    let (mut unit, mut unit_combat) = with(100.0, 10.0, 4.0);
    unit.clamp_hp_powerup(&mut unit_combat, &tuning().turn);
    let at_room = HpPowerup {
        taken: HitPoints::new(4.0),
        max: HitPoints::new(4.0),
    };
    assert_eq!(
        (unit.life.hp, unit_combat.hp_powerup),
        (HitPoints::new(94.0), at_room)
    );
    let (mut low, mut low_combat) = with(3.0, 10.0, 4.0);
    low.clamp_hp_powerup(&mut low_combat, &tuning().turn);
    assert_eq!(
        (low.life.hp, low_combat.hp_powerup),
        (HitPoints::new(1.0), at_room)
    );
    let (mut nan, mut nan_combat) = with(f32::NAN, 10.0, 4.0);
    nan.clamp_hp_powerup(&mut nan_combat, &tuning().turn);
    assert_eq!(nan.life.hp, HitPoints::new(1.0));
    let (mut kept, mut kept_combat) = with(100.0, 4.0, 4.0);
    kept.clamp_hp_powerup(&mut kept_combat, &tuning().turn);
    assert_eq!(
        (kept.life.hp, kept_combat.hp_powerup),
        (HitPoints::new(100.0), at_room)
    );
}

/// `RGE_Combat_Object::update`: the attack timer runs down by dt times the object's rate
/// (`+0x87c`), which a terrain look sets ([`crate::combat::abilities`]).
#[test]
fn the_attack_timer_runs_down_at_the_object_rate() {
    let unit = Object::with_combat(
        Unit {
            id: object_id(1),
            ..Unit::default()
        },
        Combat {
            attack_timer: Seconds::new(1.0),
            abilities: Abilities {
                attack_rate: 0.5,
                ..Abilities::default()
            },
            ..Default::default()
        },
    );
    let (units, parts) = World::split_layers([unit]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: units.into(),
            ..Player::default()
        }],
    )
    .with_parts(parts);
    stock_masters(&mut world);
    world.tick(Millis::new(100));
    assert_eq!(
        world
            .parts
            .combat_of(world.players[0].objects[ListIndex::new(0)].id)
            .attack_timer,
        Seconds::new(1.0 - 0.1 * 0.5)
    );
}

/// records/425984813: a barracks (12) that dies short of its build time plays its destruction
/// sprite (389), not its death sprite (2573) (`RGE_Static_Object::update`'s sprite after the
/// death), a foundation (state 0) as a building in state 2. The built one's death takes back
/// its completion's snow (`FUN_1411dbb70` to state 3 from 2); a foundation's keeps it.
#[test]
fn building_short_of_its_build_time_that_dies_plays_its_destruction_sprite() {
    for state in [ObjectState::Foundation, ObjectState::Alive] {
        building_short_of_its_build_time_dies(state);
    }
}

fn building_short_of_its_build_time_dies(state: ObjectState) {
    use crate::objects::sprite::{ActiveSprite, Graphic};
    let graphic_of = |id| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let barracks = Master {
        id: master_id(12),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        standing_sprite: Some(SpriteId::named(207)),
        death_sprite: Some(SpriteId::named(2573)),
        hit_points: MasterHitPoints::of_data(1200),
        building: Some(BuildingMaster {
            destruction_sprite: Some(SpriteId::named(389)),
            construction_sprite: None,
            ..BuildingMaster::default()
        }),
        creatable: Some(CreatablePart {
            build_time: WholeSeconds::of_data(50),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let foundation = Object::with_building(
        Unit {
            id: object_id(5),
            life: Lifecycle {
                class: Class::Animated,
                state,
                hp: HitPoints::new(0.5),
                master_id: master_id(12),
                ..Lifecycle::default()
            },
            sprite: Some(ActiveSprite::at_rest(
                graphic_of(SpriteId::named(207)),
                Frame::of_data(0),
            )),
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::new(10.0),
                desolid: false,
                builders: BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(1200),
                build_time: WholeSeconds::of_data(50),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::NONE,
                },
                snow: Snow::Completed,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    let split = World::split_layers(vec![foundation]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: split.0.into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        }],
    )
    .with_parts(split.1);
    world.model.masters = vec![Arc::new(Masters::new([barracks]))];
    world.model.graphics = Arc::new(
        [
            (SpriteId::named(207), graphic_of(SpriteId::named(207))),
            (SpriteId::named(389), graphic_of(SpriteId::named(389))),
            (SpriteId::named(2573), graphic_of(SpriteId::named(2573))),
        ]
        .into_iter()
        .collect(),
    );
    world.tick(Millis::new(10));
    assert_eq!(world.players[0].unsupported, None);
    let unit = &world.players[0].objects[ListIndex::new(0)];
    assert_eq!(unit.life.state, ObjectState::DyingSprite);
    assert_eq!(unit.sprite.unwrap().graphic.id, SpriteId::named(389));
    let snow = world
        .parts
        .building_of(object_id(5))
        .construction
        .as_ref()
        .map(|construction| construction.snow);
    let kept = if state == ObjectState::Alive {
        Snow::None
    } else {
        Snow::Completed
    };
    assert_eq!(snow, Some(kept));
}

/// GAME's cheat code (sub-command 6, `FUN_1415c92c0`): without the game's cheats it does
/// nothing; with them the cheat would run, which the slice does not model: its player stops.
#[test]
fn cheat_code_does_nothing_without_the_game_cheats() {
    let mut world = sent_off_world(vec![]);
    let before = world.players.clone();
    let cheat = Command::Game(GameCommand {
        issuer: player_id(1),
        sub: GameSub::Cheat,
        player: 1,
        argument: 4,
        speed: 0.0,
        value: 0,
    });
    world.run(&cheat);
    assert_eq!(world.players, before);
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    world.run(&cheat);
    assert!(world.players[1].stopped);
}

/// A unit of the AI-move tests: the walker of the hunt tests at (10.5, 10.5), master 83, on no
/// pasture, with its motion part.
fn mover_unit(id: i32) -> (Unit, Action, Motion) {
    (
        Unit {
            id: object_id(id),
            place: Static::at(Position::new(10.5, 10.5, 0.0)),
            my_pasture: None,
            life: Lifecycle {
                master_id: VILLAGER_MALE,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action::default(),
        Motion::Moving(walker()),
    )
}

/// A unit's own Attack on `target` (second target `target2`) in `state`, without a `MoveTo`.
fn own_attack(target: i32, target2: i32, state: AttackState) -> Attack {
    Attack {
        target: AttackTargetId::of_field(target),
        target2: AttackTargetId::of_field(target2),
        state,
        timer: Seconds::ZERO,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        point: Position::new(-1.0, -1.0, -1.0),
        seen: Position::ORIGIN,
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: None,
            idle_attack_sprite: None,
            missile: None,
            fire_frame: Frame::of_data(-1),
            interrupt_frame: Frame::of_data(-1),
        },
        voice: Voice::Loud,
        on_target_moved: TargetMoved::Follows,
        same_owner: false,
    }
}

/// The unit's slot 0x340 ([`Unit::ai_move_with`]) for the unit of class `class` of player 1:
/// what it let go, or why it stopped.
fn ai_move_of(
    (unit, actions, motion): (&mut Unit, &mut Action, &mut Motion),
    ai_move: AiMove,
    class: ClassId,
) -> GenieResult<Vec<ObjectId>> {
    let by = Releaser {
        id: unit.id,
        class: Some(class),
        player: PlayerId::new(1),
    };
    let (mut releases, mut leaves) = (Vec::new(), Vec::new());
    let mut rng = Well512::default();
    let (masters, graphics) = scout_sprites();
    let _ = unit.ai_move_with(
        (actions, None),
        motion.lent(),
        ai_move,
        (class, PlayerId::new(1)),
        (&mut releases, &mut leaves, by),
        (
            MoveSprites {
                masters: OwnerMasters::new(no_own_masters(), Some(&masters)),
                graphics: &graphics,
                look: None,
                roles: static_roles(),
                sprite_tuning: tuning().sprite,
                move_to_tuning: &tuning().move_to,
                top_speed: TopSpeed::Own,
            },
            &mut rng,
        ),
    )?;
    assert!(releases.iter().all(|&(_, releaser)| releaser == by));
    Ok(releases
        .into_iter()
        .map(|(object_id, _)| object_id)
        .collect())
}

/// The master 83 of [`mover_unit`] with its running (13154), walking (13153) and standing
/// (13151) sprites, and their table.
fn scout_sprites() -> (Masters, Graphics) {
    let master = Master {
        id: VILLAGER_MALE,
        standing_sprite: Some(SpriteId::named(13151)),
        moving: Some(MovingPart {
            run_sprite: Some(SpriteId::named(13154)),
            move_sprite: Some(SpriteId::named(13153)),
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    let graphics = [13154, 13153, 13151]
        .into_iter()
        .map(|id| {
            let graphic = Graphic {
                id: SpriteId::named(id),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(16),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            };
            (graphic.id, graphic)
        })
        .collect();
    (Masters::new([master]), graphics)
}

const AI_POINT: Position = Position::new(14.5, 12.0, 0.0);

fn forced(pace: Pace) -> AiMove {
    AiMove {
        point: AI_POINT,
        range: Distance::new(0.5),
        force: OrderForce::Forced,
        pace,
    }
}

/// `FUN_1410866e0` on a unit whose Attack is done (state 1, 426391614's scout 123 in update
/// 1052): the Attack's slot 0x40 is the base one (`FUN_140a78490`, 0), so a `MoveTo` to the point
/// within the move's range, running unless the move walks, replaces the list; the Attack's
/// teardown lets both its targets go. The `MoveTo` starts (state 3: no target, so never within
/// 0.01, and the unit has speed).
#[test]
fn an_ai_move_replaces_done_attack_with_move_to_the_point() {
    let (mut unit, mut actions, mut motion) = mover_unit(123);
    actions
        .list
        .set_attack(Some(own_attack(9149, 9150, AttackState::Done)));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::CLASS_47
        ),
        Ok(vec![object_id(9149), object_id(9150)])
    );
    assert_eq!(actions.list.attack().copied(), None);
    let mt = actions.list.move_to().copied().expect("the AI's MoveTo");
    assert_eq!(
        (
            mt.state,
            mt.goal(),
            ObjectId::field(mt.target),
            mt.range,
            mt.sprite.map(|graphic| graphic.id)
        ),
        (
            MoveTo::Start,
            Some(AI_POINT),
            -1,
            Distance::new(0.5),
            Some(SpriteId::named(13154))
        )
    );
    // walking: the master's walking sprite
    let (mut unit, mut actions, mut motion) = mover_unit(123);
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Walk),
            class::CLASS_47
        ),
        Ok(vec![])
    );
    let walked = actions.list.move_to().copied().unwrap().sprite;
    assert_eq!(
        walked.map(|graphic| graphic.id),
        Some(SpriteId::named(13153))
    );
}

/// The Gather's slot 0x40 (`0x140a8b740`) takes a move to a point: the target goes (slot 0x68
/// with none, the caller's release), the point is the move's and `set_state(15)` walks there; the
/// unit's list keeps the Gather (425780724's villager in update 5526, its Gather idle).
#[test]
fn an_ai_move_hands_the_point_to_the_gather() {
    let task = HuntTask {
        id: task_id(2),
        range: Distance::new(0.25),
        resource: Some(AttributeId::named(17)),
        sprites: TaskSprites::NONE,
        work_value: WorkValue::new(1.0),
        productivity: None,
        kind4: None,
    };
    let bush = Resource {
        id: object_id(28222),
        position: Position::new(23.5, 93.5, 0.0),
        class: class::CLASS_7,
        master: master_id(59),
        player: PlayerId::new(0),
        workers: WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::new(1.2),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let mut gather = GatherAction::new(task, &bush, FarmMode::Fixed, rate, &roles()).unwrap();
    gather.state = GatherState::Idle;
    let (mut unit, mut actions, mut motion) = mover_unit(31139);
    actions.list.set_gather(Some(gather));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::VILLAGER
        ),
        Ok(vec![object_id(28222)])
    );
    let gather_action = actions.list.gather().copied().expect("the Gather stays");
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.point,
            gather_action
                .state
                .move_to()
                .copied()
                .and_then(|move_to_action| move_to_action.goal())
        ),
        (
            GatherState::Moved { move_to: None },
            None,
            AI_POINT,
            Some(AI_POINT)
        )
    );
    assert_eq!(
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Start)
    );
    assert_eq!(actions.list.move_to().copied(), None);
}

/// The Hunt's slot 0x40 (`FUN_140ac52c0`) takes a point unless its task has a work range: its
/// target goes, `+0x68` is −1 and `set_state(15)` walks to the point (431193960's villager in
/// update 1465, its Hunt idle). A task with a range answers 0: the `MoveTo` replaces the list and
/// the Hunt's teardown lets its target and its drop site go.
#[test]
fn an_ai_move_hands_the_point_to_hunt_without_work_range() {
    let (mut unit, mut actions, mut motion) = mover_unit(47);
    let mut hunt = hunting(9149);
    hunt.state = HuntState::Idle;
    hunt.carry_to = Some(DropSiteId::new(object_id(9150)));
    actions.list.set_hunt(Some(hunt));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::VILLAGER
        ),
        Ok(vec![object_id(9149)])
    );
    let hunt_action = actions.list.hunt().copied().expect("the Hunt stays");
    assert_eq!(
        (
            hunt_action.state.bare(),
            hunt_action.target,
            hunt_action.carry_to,
            hunt_action.point,
            hunt_action.target_class
        ),
        (
            HuntState::WalkPoint { move_to: None },
            None,
            Some(DropSiteId::new(object_id(9150))),
            AI_POINT,
            None
        )
    );
    assert_eq!(actions.list.move_to().copied(), None);
    let (mut unit, mut actions, mut motion) = mover_unit(47);
    hunt.task.range = Distance::new(1.0);
    actions.list.set_hunt(Some(hunt));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::VILLAGER
        ),
        Ok(vec![object_id(9149), object_id(9150)])
    );
    assert_eq!(actions.list.hunt().copied(), None);
    assert_eq!(
        actions.list.move_to().map(MoveToAction::goal),
        Some(Some(AI_POINT))
    );
}

/// Without the force a head `MoveTo` whose point (`+0x38`) is the move's stands, a point one or
/// an object one (the target's place when it last took it); forced, or to another point,
/// the new `MoveTo` replaces it (its teardown stops the unit and lets its target go).
#[test]
fn an_unforced_ai_move_leaves_move_to_the_same_point() {
    let unforced = AiMove {
        force: OrderForce::Unforced,
        ..forced(Pace::Run)
    };
    let old = MoveToAction::to_point(AI_POINT, Distance::ZERO, None, Speed::ZERO);
    let (mut unit, mut actions, mut motion) = mover_unit(5);
    actions.list.set_move_to(Some(old));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            unforced,
            class::CLASS_47
        ),
        Ok(vec![])
    );
    assert_eq!(actions.list.move_to().copied(), Some(old));
    let to_object = MoveToAction {
        point: AI_POINT,
        target: Some(object_id(9149)),
        ..old
    };
    actions.list.set_move_to(Some(to_object));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            unforced,
            class::CLASS_47
        ),
        Ok(vec![])
    );
    assert_eq!(actions.list.move_to().copied(), Some(to_object));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::CLASS_47
        ),
        Ok(vec![object_id(9149)])
    );
    assert_eq!(
        actions
            .list
            .move_to()
            .map(|move_to_action| (ObjectId::field(move_to_action.target), move_to_action.range)),
        Some((-1, Distance::new(0.5)))
    );
    actions.list.set_move_to(Some(old));
    let elsewhere = AiMove {
        point: Position::new(1.0, 2.0, 0.0),
        ..unforced
    };
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            elsewhere,
            class::CLASS_47
        ),
        Ok(vec![])
    );
    assert_eq!(
        actions.list.move_to().copied().unwrap().goal(),
        Some(Position::new(1.0, 2.0, 0.0))
    );
}

/// `FUN_141216c50`: an unpacked trebuchet (class 54) packs instead (not modelled); a packed one
/// (class 51) moves as any unit. The base slot 0x40 of an Explore and of a wait for a foundation
/// takes nothing; an action the slice does not hold stops it.
#[test]
fn an_ai_move_of_trebuchet_packs_and_the_base_slot_takes_nothing() {
    let (mut unit, mut actions, mut motion) = mover_unit(7);
    assert!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::CLASS_54
        )
        .is_err()
    );
    assert_eq!(actions.list.move_to().copied(), None);
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::CLASS_51
        ),
        Ok(vec![])
    );
    assert_eq!(
        actions.list.move_to().map(MoveToAction::goal),
        Some(Some(AI_POINT))
    );
    let (unit, mut actions, mut motion) = mover_unit(7);
    actions.list.set_wait_solidify(Some(WaitSolidify::new(
        Task::default(),
        FoundationId::new(object_id(10028)),
    )));
    let none = Graphics::new();
    assert_eq!(
        unit.head_takes_point(
            (&mut actions, None),
            motion.lent(),
            AI_POINT,
            PlayerId::new(1),
            MoveSprites {
                masters: OwnerMasters::new(no_own_masters(), None),
                graphics: &none,
                look: None,
                roles: static_roles(),
                sprite_tuning: tuning().sprite,
                move_to_tuning: &tuning().move_to,
                top_speed: TopSpeed::Own,
            },
        ),
        Ok(PointMove::Declined)
    );
    let (unit, mut actions, mut motion) = mover_unit(7);
    let unread = Unread::new(GenieError::unsupported("a test's action"));
    assert!(
        unit.head_takes_point(
            (&mut actions, Some(&unread)),
            motion.lent(),
            AI_POINT,
            PlayerId::new(1),
            MoveSprites {
                masters: OwnerMasters::new(no_own_masters(), None),
                graphics: &none,
                look: None,
                roles: static_roles(),
                sprite_tuning: tuning().sprite,
                move_to_tuning: &tuning().move_to,
                top_speed: TopSpeed::Own,
            },
        )
        .is_err()
    );
}

/// A unit without speed: the `MoveTo`'s start fails it (state 13), whose `set_state` (not
/// quiet) shows the master's standing sprite: the mover's speed is the standing sprite's and
/// the unit's `+0x7b8` clears.
#[test]
fn an_ai_move_of_unit_without_speed_fails_at_its_start_standing() {
    let (mut unit, mut actions, mut motion) = mover_unit(8);
    let mover = motion.mover_mut().unwrap();
    mover.stats.speed = Speed::ZERO;
    motion.lent().set_anim_speed(Speed::new(1.0));
    assert_eq!(
        ai_move_of(
            (&mut unit, &mut actions, &mut motion),
            forced(Pace::Run),
            class::CLASS_47
        ),
        Ok(vec![])
    );
    assert_eq!(
        actions
            .list
            .move_to()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Failed)
    );
    assert_eq!(motion.lent().anim_speed(), Some(Speed::ZERO));
}

/// The motion step gave up on a unit's own `MoveTo` (`LAB_140aa320c`): any unit but a human's
/// villager fails it (`set_state(13)`), whose `set_state` (`0x140aa29e0`, not quiet) shows the
/// standing sprite (slot 0x68 with slot 0x798's): the mover's speed becomes the standing one
/// (426049848's deer 14064 in update 2077, which then evaded at 0.737, not 1.4) and `+0x7b8`
/// clears. A human's villager waits once (`set_state(2)`), which shows nothing.
#[test]
fn an_own_move_to_that_gives_up_fails_standing_unless_villager_waits() {
    let standing = Graphic {
        id: SpriteId::named(764),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let walking = Graphic {
        id: SpriteId::named(765),
        ..standing
    };
    let graphics = Graphics::from([
        (SpriteId::named(764), standing),
        (SpriteId::named(765), walking),
    ]);
    let masters = Masters::new([Master {
        id: master_id(65),
        standing_sprite: Some(SpriteId::named(764)),
        ..Master::default()
    }]);
    let mut rng = Well512::default();
    let deer = |rng: &mut Well512| {
        let (mut unit, mut actions, mut motion) = mover_unit(14064);
        unit.life.master_id = master_id(65);
        let mover = motion.mover_mut().unwrap();
        // the deer's speed 0.737 with its standing sprite's factor 1, its walking one's 1.9
        mover.stats.base = Speed::new(0.737);
        mover.stats.speed = Speed::new(1.4003);
        unit.sprite = Some(ActiveSprite::start(walking, rng, tuning().sprite));
        motion.lent().set_anim_speed(Speed::new(1.0));
        actions.list.set_move_to(Some(MoveToAction::to_point(
            Position::new(119.5, 172.5, 0.0),
            Distance::ZERO,
            None,
            Speed::ZERO,
        )));
        (unit, actions, motion)
    };
    let (mut rng_unit, mut actions, mut motion) = deer(&mut rng);
    rng_unit
        .own_move_gave_up(
            &mut actions,
            motion.lent(),
            Patience::Fails,
            UnitStance::Aggressive,
            (
                OwnerMasters::new(no_own_masters(), Some(&masters)),
                &graphics,
                &mut rng,
            ),
            tuning().sprite,
        )
        .unwrap();
    assert_eq!(
        actions
            .list
            .move_to()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Failed)
    );
    assert_eq!(
        motion.mover().map(|mover| mover.stats.speed),
        Some(Speed::new(0.737))
    );
    assert_eq!(
        rng_unit
            .sprite
            .map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(764))
    );
    assert_eq!(motion.lent().anim_speed(), Some(Speed::ZERO));
    let (mut unit, mut deer_actions, mut deer_motion) = deer(&mut rng);
    unit.own_move_gave_up(
        &mut deer_actions,
        deer_motion.lent(),
        Patience::WaitsOnce,
        UnitStance::Aggressive,
        (
            OwnerMasters::new(no_own_masters(), Some(&masters)),
            &graphics,
            &mut rng,
        ),
        tuning().sprite,
    )
    .unwrap();
    assert_eq!(
        deer_actions
            .list
            .move_to()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Waiting)
    );
    assert_eq!(
        deer_motion.mover().map(|mover| mover.stats.speed),
        Some(Speed::new(1.4003))
    );
    assert_eq!(
        unit.sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(765))
    );
}

/// The unit AI's attack command ([`crate::unit_ai::world_view::UnitView::attack`]) of the scout 123
/// (master 448, class 47, 426049998's) on Gaia's wolf 50 half a tile east, its attack timer
/// `timer`, inside `inside` if any: the scout (with its parts) and what the command answered and
/// took.
fn scout_attack(timer: f32, inside: Option<ObjectId>) -> (Object, Attacked, Vec<i32>) {
    scout_attack_on(timer, inside, None, OrderForce::Unforced)
}

/// `work` on the unit AI's side ([`crate::unit_ai::world_view::UnitView`]) of the first of player
/// 1's live objects `own` (the scout 123 of master 448 at (10, 10)), player 1 holding `tracker`, in
/// a world of Gaia's player `gaia`, `spatial`, the masters `every`, `graphics` and the unit groups
/// `groups`, as the unit's update runs it: what `work` gives and the world after the update.
fn on_side<R>(
    (gaia, own, tracker): (Player, Vec<Object>, Tracker),
    (spatial, every, graphics): (Spatial, Vec<Masters>, Graphics),
    groups: UnitGroups,
    work: impl FnOnce(&mut UnitView<'_, '_>) -> R,
) -> (R, World) {
    let mut attributes = vec![Amount::ZERO; 8];
    attributes[4] = Amount::new(10.0);
    let (rows, parts): (Vec<Unit>, Vec<_>) = own
        .into_iter()
        .map(|object| (object.unit.clone(), (object.unit.id, object.parts)))
        .unzip();
    let scout = Player {
        objects: rows.into(),
        relations: relation_row([0, 0]),
        attributes,
        tracker,
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![gaia, scout],
    );
    for (id, parts) in parts {
        world.load_parts(id, parts).unwrap();
    }
    (
        world.spatial,
        world.model.masters,
        world.model.graphics,
        world.groups,
    ) = (
        spatial,
        every.into_iter().map(Arc::new).collect(),
        Arc::new(graphics),
        groups,
    );
    let answer = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, actions, step| {
            let mut side = UnitView {
                step,
                unit,
                actions,
                attacker: Attacker {
                    ai_kind: UnitAiKind::Soldier,
                    player: PlayerId::new(1),
                    player_kind: PlayerKind::Human,
                    state: ObjectState::Alive,
                    held: None,
                    top_action: None,
                    master: ids::ID_448,
                    kind: MasterKind::COMBAT,
                    class: Some(class::CLASS_47),
                    attack_priority: AttackPriority::PRIORITY_0,
                    missile: None,
                    range: Distance::new(1.0),
                    min_range: Distance::ZERO,
                    charge_kind: ChargeKind::NONE,
                    area_effect: Distance::ZERO,
                    blast_level: BlastLevel::NONE,
                    los: Distance::new(4.0),
                    position: Position::new(10.0, 10.0, 0.0),
                    radius: Footprint::new(0.2, 0.2),
                    speed: Speed::new(1.2),
                    scenario: false,
                    firepower: Firepower::new(0.0),
                    capture: CaptureKind::None,
                },
                player: PlayerId::new(1),
                player_kind: PlayerKind::Human,
                new_owner: None,
                civ: CivId::new(0),
                ai_settings: AiSettings::default(),
            };
            Ok(work(&mut side))
        },
    );
    (answer.unwrap(), world)
}

/// [`scout_attack`] of a scout whose own Attack is `own`, the command `force`d or not.
fn scout_attack_on(
    timer: f32,
    inside: Option<ObjectId>,
    own: Option<&Attack>,
    force: OrderForce,
) -> (Object, Attacked, Vec<i32>) {
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = Graphics::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
    ]);
    let scout = Master {
        id: ids::ID_448,
        kind: MasterKind::COMBAT,
        class: class::CLASS_47,
        standing_sprite: Some(SpriteId::named(300)),
        combat: Some(CombatPart {
            attack_sprite: Some(SpriteId::named(301)),
            weapon_range: Distance::new(1.0),
            reload: Seconds::new(2.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: MasterKind::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            place: Static::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    let mut rng = Well512::default();
    let mut walker = walker();
    walker.stats.speed = Speed::new(1.2);
    let mut unit = Object::with_combat(
        Object::with_actions(
            Unit {
                id: object_id(123),
                place: Static::at(Position::new(10.0, 10.0, 0.0)),
                sprite: Some(ActiveSprite::start(standing, &mut rng, tuning().sprite)),
                life: Lifecycle {
                    master_id: ids::ID_448,
                    inside: inside.map(ContainerId::new),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Action {
                list: ActionList::default().with_attack(own.copied()),
                ..Default::default()
            },
        ),
        Combat {
            attack_timer: Seconds::new(timer),
            ..Default::default()
        },
    );
    unit.parts.motion = Some(Motion::Moving(walker));
    let ai = UnitAi {
        stance: UnitStance::StandGround,
        current_order: None,
        ai_kind: UnitAiKind::Soldier,
        ..UnitAi::default()
    };
    let every = vec![Masters::new([wolf]), Masters::new([scout])];
    let (attacked, world) = on_side(
        (gaia, vec![unit], Tracker::default()),
        (spatial, every, graphics),
        UnitGroups::default(),
        |side| {
            let mut ai = ai.clone();
            side.attack(object_id(50), force, &mut ai, &mut Well512::default())
                .unwrap()
        },
    );
    // the takes reach Gaia's objects through the view: each counts a worker
    let taken = world.players[0].objects.iter();
    let takes = taken.flat_map(|unit| repeat_n(unit.id.get(), unit.workers.get() as usize));
    (
        world.object_of(&world.players[1].objects[ListIndex::new(0)]),
        attacked,
        takes.collect(),
    )
}

/// The attack command of unit 123 of master `who` (in a table with `also`) on Gaia's wolf 50, as
/// [`scout_attack_on`], the unit in its owner's done tracker list of its master: the unit, the
/// answer, its owner's attribute 4 as the command returns and its owner's tracker after it.
fn attack_by(who: Master, also: Vec<Master>) -> (Object, Attacked, f32, Tracker) {
    let (timer, inside, own, force) = (0.0, None, None, OrderForce::Forced);
    let attacker_master = who.id;
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = Graphics::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
    ]);
    let scout = Master {
        id: ids::ID_448,
        kind: MasterKind::COMBAT,
        class: class::CLASS_47,
        standing_sprite: Some(SpriteId::named(300)),
        combat: Some(CombatPart {
            attack_sprite: Some(SpriteId::named(301)),
            weapon_range: Distance::new(1.0),
            reload: Seconds::new(2.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: MasterKind::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            place: Static::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    let mut rng = Well512::default();
    let mut walker = walker();
    walker.stats.speed = Speed::new(1.2);
    let mut unit = Object::with_combat(
        Object::with_actions(
            Unit {
                id: object_id(123),
                place: Static::at(Position::new(10.0, 10.0, 0.0)),
                sprite: Some(ActiveSprite::start(standing, &mut rng, tuning().sprite)),
                life: Lifecycle {
                    master_id: attacker_master,
                    inside,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Action {
                list: ActionList::default().with_attack(own),
                ..Default::default()
            },
        ),
        Combat {
            attack_timer: Seconds::new(timer),
            ..Default::default()
        },
    );
    unit.parts.motion = Some(Motion::Moving(walker));
    let ai = UnitAi {
        stance: UnitStance::StandGround,
        current_order: None,
        ai_kind: UnitAiKind::Soldier,
        ..UnitAi::default()
    };
    let _ = scout;
    let masters = Masters::new(once(who).chain(also));
    let every = vec![Masters::new([wolf]), masters];
    let lines = UnitLines::default();
    let mut tracker = Tracker::default();
    tracker.add(
        object_id(123),
        key(every.get(1), &lines, attacker_master, &roles()),
    );
    let ((attacked, adds), world) = on_side(
        (gaia, vec![unit], tracker),
        (spatial, every, graphics),
        UnitGroups::default(),
        |side| {
            let mut ai = ai.clone();
            let attacked = side
                .attack(object_id(50), force, &mut ai, &mut Well512::default())
                .unwrap();
            (attacked, side.step.world.players[1].attributes[4].get())
        },
    );
    let unit = world.object_of(&world.players[1].objects[ListIndex::new(0)]);
    (unit, attacked, adds, world.players[1].tracker.clone())
}

/// A villager's attack command (`FUN_141081a80`): a builder (118, task swap group 1) switches to
/// its player's villager 83 first (slot 0x90 with 0: its storage leaves and the new one's comes,
/// the tracker promotes it), then attacks with the new master.
#[test]
fn villager_attack_command_switches_it_to_the_villager_first() {
    let villager = |id, storage| Master {
        id,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(25),
        standing_sprite: Some(SpriteId::named(300)),
        storage,
        combat: Some(CombatPart {
            attack_sprite: Some(SpriteId::named(301)),
            weapon_range: Distance::new(0.5),
            reload: Seconds::new(2.0),
            ..CombatPart::default()
        }),
        action: Some(ActionPart {
            task_by_group: TaskGroup::MALE_VILLAGERS,
            ..ActionPart::default()
        }),
        ..moving_master(walker().stats)
    };
    let (unit, attacked, housed, tracker) = attack_by(
        villager(
            ids::BUILDER,
            vec![(Some(attribute::HEADROOM), Amount::new(1.0), 2)],
        ),
        vec![villager(
            VILLAGER_MALE,
            vec![(Some(attribute::HEADROOM), Amount::new(3.0), 2)],
        )],
    );
    assert_eq!(unit.unit.life.master_id, VILLAGER_MALE);
    assert!(attacked.attacks);
    assert!(
        unit.parts
            .actions
            .list
            .attack()
            .is_some_and(|attack| attack.target == Some(AttackTargetId::new(object_id(50))))
    );
    // the owner's attribute 4 (10 before) as the attack command returns
    assert_eq!(housed, 10.0 - 1.0 + 3.0);
    assert_eq!(
        (
            tracker.list(ListKey::master(master_id(118))),
            tracker.list(ListKey::master(master_id(83)))
        ),
        (&[][..], &[Some(object_id(123))][..])
    );
}

/// The scout 124 (player 1, master 448, at (10.3, 10), its unit AI `helper`, its own Attack
/// `own`), a helper against Gaia's wolf 50 asked by the scout 123.
fn helper_scout(helper: UnitAi, own: Option<&Attack>) -> Object {
    let unit = Object::with_actions(
        Unit {
            id: object_id(124),
            place: Static::at(Position::new(10.3, 10.0, 0.0)),
            life: Lifecycle {
                master_id: ids::ID_448,
                state: ObjectState::Alive,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_attack(own.copied()),
            ..Default::default()
        },
    );
    Object::with_ai(unit, helper)
}

/// `AskForHelp`'s world: the scout 123's view (player 1, master 448, at (10, 10)) with the
/// `helper` among its owner's other objects, Gaia's wolf 50 at (10.5, 10), the lists the last
/// search left (`searched`, none before any) and the world's unit groups `groups`; `work` runs on
/// the view.
fn help_view<R>(
    helper: Object,
    searched: Option<ByRelation<Vec<Record>>>,
    groups: &UnitGroups,
    work: impl FnOnce(&mut UnitView<'_, '_>) -> R,
) -> R {
    let scout = Master {
        id: ids::ID_448,
        kind: MasterKind::COMBAT,
        class: class::CLASS_47,
        combat: Some(CombatPart {
            weapon_range: Distance::new(1.0),
            weapons: vec![ClassAmount::new(4, 3)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: MasterKind::COMBAT,
        class: class::PREDATOR,
        combat_level: CombatLevel::MILITARY,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(124),
        placed(10.3, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            place: Static::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    if let Some(lists) = searched {
        spatial.set_searched(lists);
    }
    let unit = Unit {
        id: object_id(123),
        place: Static::at(Position::new(10.0, 10.0, 0.0)),
        life: Lifecycle {
            master_id: ids::ID_448,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let masters = Masters::new([scout]);
    let every = vec![Masters::new([wolf]), masters];
    let tracker = Tracker::default();
    let graphics = Graphics::new();
    let (answer, _) = on_side(
        (gaia, vec![unit.into(), helper], tracker),
        (spatial, every, graphics),
        groups.clone(),
        work,
    );
    answer
}

/// `AskForHelp`'s helper test ([`UnitView::helper_free`]) of [`helper_scout`] against the wolf.
///
/// [`UnitView::helper_free`]: crate::unit_ai::world_view::UnitView::helper_free
fn helper_free(helper: UnitAi, own: Option<&Attack>) -> bool {
    helper_free_of(
        PlayerId::new(1),
        helper_scout(helper, own),
        &UnitGroups::default(),
    )
}

/// [`helper_free`] of the unit `helper` of player `owner` with the world's unit groups `groups`.
fn helper_free_of(owner: PlayerId, helper: Object, groups: &UnitGroups) -> bool {
    help_view(helper, None, groups, |side| {
        let scout = player_master(&side.step.world.model.masters, side.player, ids::ID_448)
            .unwrap()
            .clone();
        let helper = OthersRef::of(
            &side.step.world.players,
            &side.step.world.index,
            PlayerId::new(1),
        )
        .unwrap()
        .own_iter()
        .next()
        .unwrap();
        let helper = Helper {
            owner,
            unit: helper,
            actions: side.step.world.parts.actions_of(helper.id),
            master: &scout,
        };
        side.helper_free(helper, object_id(50), None, &roles())
            .unwrap()
    })
}

/// `AskForHelp` (`FUN_140c42ce0`) asks only an idle helper in no group (no action value, action,
/// order, queued order or target) whose attack check lets it attack the attacker; a helper whose
/// top action's target lies within its range keeps it.
#[test]
fn ask_for_help_asks_only_an_idle_helper_that_may_attack() {
    let idle = UnitAi {
        ai_kind: UnitAiKind::Soldier,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::Aggressive,
        ..UnitAi::default()
    };
    assert!(helper_free(idle.clone(), None));
    let busy = [
        UnitAi {
            current_action: Some(AiAction::Attack),
            ..idle.clone()
        },
        UnitAi {
            current_order: Some(OrderKind::Attack),
            ..idle.clone()
        },
        UnitAi {
            current_target: Some(object_id(50)),
            ..idle.clone()
        },
    ];
    for ai in busy {
        assert!(!helper_free(ai, None));
    }
    // an action of its own makes it busy
    assert!(!helper_free(
        idle,
        Some(&own_attack(50, -1, AttackState::Move { move_to: None }))
    ));
}

/// `AskForHelp`'s helper in a group answers whatever it does while its group patrols, and not
/// while it does not; a patrolling helper whose own Attack's target lies within its range keeps
/// it. A unit without a unit AI never answers; the helper of another player than the asker's
/// answers as one of its own.
#[test]
fn helper_in_group_answers_only_while_its_group_patrols() {
    let idle = UnitAi {
        ai_kind: UnitAiKind::Soldier,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::Aggressive,
        ..UnitAi::default()
    };
    let busy = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        ..idle.clone()
    };
    let groups = UnitGroups {
        groups: vec![
            UnitGroup {
                flags: GroupFlags::PATROL,
                ..UnitGroup::new(group_id(7))
            },
            UnitGroup::new(group_id(8)),
        ],
        ..UnitGroups::default()
    };
    let in_group = |group, own| {
        let mut object = helper_scout(busy.clone(), own);
        object.unit.group.id = group;
        object
    };
    assert!(helper_free_of(
        PlayerId::new(1),
        in_group(Some(group_id(7)), None),
        &groups
    ));
    assert!(!helper_free_of(
        PlayerId::new(1),
        in_group(Some(group_id(8)), None),
        &groups
    ));
    assert!(!helper_free_of(
        PlayerId::new(1),
        in_group(
            Some(group_id(7)),
            Some(&own_attack(50, -1, AttackState::Move { move_to: None }))
        ),
        &groups
    ));
    let mut thoughtless = helper_scout(idle.clone(), None);
    thoughtless.parts.unit_ai = None;
    assert!(!helper_free_of(PlayerId::new(1), thoughtless, &groups));
    assert!(helper_free_of(
        PlayerId::new(2),
        helper_scout(idle, None),
        &groups
    ));
}

/// `FUN_141081a80` on the target of the unit's own Attack: it answers true; forced, the Attack's
/// slot 0x48 (`FUN_140a841d0`) runs on the target first. In state 1 the scout, which may not move
/// (stance 2, no order), keeps the wolf in its reach: where it saw it becomes its place, the
/// target the same (nothing taken). In state 4 the Attack keeps it as it is; unforced nothing
/// changes either.
#[test]
fn an_attack_command_on_its_own_attack_target_asks_that_attack_again() {
    let own = |state: AttackState| Attack {
        state,
        seen: Position::new(-1.0, -1.0, -1.0),
        ..own_attack(50, -1, state)
    };
    let (object, attacked, takes) =
        scout_attack_on(0.0, None, Some(&own(AttackState::Done)), OrderForce::Forced);
    assert_eq!((attacked.attacks, takes), (true, vec![]));
    let asked = object
        .parts
        .actions
        .list
        .attack()
        .copied()
        .expect("the scout's Attack");
    assert_eq!(
        (asked.state, asked.target, asked.seen),
        (
            AttackState::Done,
            Some(AttackTargetId::new(object_id(50))),
            Position::new(10.5, 10.0, 0.0)
        )
    );
    let (object, attacked, _) = scout_attack_on(
        0.0,
        None,
        Some(&own(AttackState::Move { move_to: None })),
        OrderForce::Forced,
    );
    assert!(attacked.attacks);
    assert_eq!(
        object.parts.actions.list.attack().map(|attack| attack.seen),
        Some(Position::from([-1.0; 3]))
    );
    let (object, _, _) = scout_attack_on(
        0.0,
        None,
        Some(&own(AttackState::Done)),
        OrderForce::Unforced,
    );
    assert_eq!(
        object.parts.actions.list.attack().map(|attack| attack.seen),
        Some(Position::from([-1.0; 3]))
    );
}

/// `FUN_141081a80`'s start (`0x140a7f8c0`) of a unit that cannot move (stance 2, no order) in
/// reach: `set_state(6)` at once (426049998's scout 123 in update 1222). Its readyToAttack finds
/// the target on the point; facing it with its attack timer run out the scout swings
/// (`set_state(7)`: the attack sprite, a hit pending); with time left it waits in state 6 on the
/// standing sprite. The constructor takes the target either way. A unit inside another fails at
/// the start (`set_state(13)`: speed 1, the standing sprite).
#[test]
fn an_attack_command_of_unit_that_cannot_move_goes_ready_and_swings_at_once() {
    let (object, attacked, takes) = scout_attack(0.0, None);
    let unit = &object.unit;
    assert_eq!((attacked.attacks, attacked.posted), (true, vec![]));
    assert_eq!(takes, vec![50]);
    let attack = object
        .parts
        .actions
        .list
        .attack()
        .copied()
        .expect("the scout's Attack");
    assert_eq!(
        (attack.state, attack.pending, attack.point),
        (AttackState::Swing, true, Position::new(10.5, 10.0, 0.0))
    );
    assert_eq!(
        unit.sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(301))
    );
    let (object, _, _) = scout_attack(0.5, None);
    let unit = &object.unit;
    let attack = object
        .parts
        .actions
        .list
        .attack()
        .copied()
        .expect("the scout's Attack");
    assert_eq!((attack.state, attack.pending), (AttackState::Ready, false));
    assert_eq!(
        unit.sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(300))
    );
    let (object, attacked, takes) = scout_attack(0.0, Some(object_id(9149)));
    assert_eq!((attacked.attacks, takes), (true, vec![50]));
    assert_eq!(
        object
            .parts
            .actions
            .list
            .attack()
            .map(|attack| attack.state),
        Some(AttackState::Failed)
    );
    assert_eq!(
        object.parts.motion.map(|motion| motion.animated().speed),
        Some(Speed::new(1.0))
    );
}

/// A house's movement values: it never moves (speed 0).
const fn house_moves() -> MoveStats {
    MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::ZERO,
        base: Speed::ZERO,
        turn_rate: TurnRate::MAX,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(1.0, 1.0, 1.0),
        restriction: RestrictionId::named(7),
        restriction_mask: TerrainMask::of_bits(6),
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 1.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    }
}

/// A world after BUILD of a house (master 70) at (30, 88) by villager 9175 of player 1, the
/// house's master moving.
fn house_build_world() -> World {
    let mut masters = builder_masters();
    let house = masters.get_mut(ids::HOUSE).unwrap();
    house.move_stats = Some(house_moves());
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![Unit {
                    id: object_id(9175),
                    place: Static::at(Position::new(30.0, 89.0, 0.0)),
                    life: Lifecycle {
                        class: Class::Animated,
                        state: ObjectState::Alive,
                        hp: HitPoints::new(25.0),
                        master_id: VILLAGER_MALE,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                }]
                .into(),
                attributes: attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    )
    .with_ais([(
        object_id(9175),
        UnitAi {
            ai_kind: UnitAiKind::Civilian,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        },
    )]);
    world.next_object_id = object_id(10028);
    world.world_time = WorldTime::from_millis(72890);
    world.model.masters = vec![Arc::new(Masters::default()), Arc::new(masters)];
    placeable(&mut world);
    world.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        queue_id: QueueId::of_data(-1),
        sprite_override: SpriteOverride::of_data(-1),
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: false,
        frame: Frame::of_data(0),
    }));
    world
}

/// BUILD's foundation is a moving object (type 80): the moving setup (`FUN_1411580a0`) gives it
/// its movement state, though it never moves (speed 0), and the position setter makes its place
/// a move at the world time (`+0x870`), which the walk reads of a building it meets (slot 0x5e8,
/// `FUN_14106d2d0`): a foundation made after a unit's path was can be kept as a hit.
#[test]
fn build_command_foundation_holds_its_movement_state() {
    let mut world = house_build_world();
    assert_eq!(world.players[1].unsupported, None);
    let foundation = world.players[1].objects.last().unwrap();
    assert_eq!(foundation.id, object_id(10028));
    let mover = world
        .parts
        .mover_of(foundation.id)
        .expect("a building's movement state");
    assert_eq!(
        (
            mover.forward,
            mover.waiting,
            mover.last_move,
            mover.moved_at,
            mover.paths.len()
        ),
        (
            BVector::new(1.0, 0.0, 0.0),
            0,
            WorldTime::from_millis(72890),
            Position::new(30.0, 88.0, 0.0),
            0
        )
    );
    world.sync_world();
    let gait = world
        .objects()
        .gait(object_id(10028))
        .expect("the walk's view of the building");
    assert_eq!(
        (gait.moving, gait.last_move, gait.pause),
        (false, WorldTime::from_millis(72890), None)
    );
}

/// BUILD's foundation is desolid: `FUN_1411cf770` makes a copy of its master its own (slot
/// 0x48, `+0x6c0`), without a height (`+0x5e0`) or a blast defense (`+0x62b`).
#[test]
fn build_command_foundation_owns_master_copy_without_height() {
    let world = house_build_world();
    assert_eq!(world.players[1].unsupported, None);
    let house = world.master(player_id(1), master_id(70)).unwrap().clone();
    assert!(Distance::ZERO < house.height);
    assert_eq!(
        world.parts.own_master.get(object_id(10028)),
        Some(&Master {
            height: Distance::ZERO,
            blast_defense: BlastDefense::new(0),
            ..house
        })
    );
}

/// BUILD's foundation takes the setups' draws a new unit takes (`FUN_1411aca00`'s combat setup
/// `FUN_14121df60` and the setups it runs first), with its master's standing sprite current
/// (slot 0x798, `FUN_1412029f0`): a sprite of several facets with sequence flag 2 draws the
/// facet the building faces (the moving setup `FUN_141161450`, "setup" line 0x2c3), before the
/// powerup offset (line 0x1c1). 429730863's palisade 15472 (standing sprite 588: 5 facets,
/// sequence type 3) faces facet 4, 5.0265484.
#[test]
fn build_command_foundation_draws_its_facing_before_its_powerup_offset() {
    let build = |flags: SpriteFlags| {
        let mut masters = builder_masters();
        let house = masters.get_mut(ids::HOUSE).unwrap();
        house.standing_sprite = Some(SpriteId::named(588));
        house.move_stats = Some(house_moves());
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(29910),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player::default(),
                Player {
                    objects: vec![Unit {
                        id: object_id(9175),
                        place: Static::at(Position::new(30.0, 89.0, 0.0)),
                        life: Lifecycle {
                            class: Class::Animated,
                            state: ObjectState::Alive,
                            hp: HitPoints::new(25.0),
                            master_id: VILLAGER_MALE,
                            ..Lifecycle::default()
                        },
                        ..Unit::default()
                    }]
                    .into(),
                    attributes: attributes(250, [200.0, 100.0, 200.0, 100.0]),
                    ..Player::default()
                },
            ],
        )
        .with_ais([(
            object_id(9175),
            UnitAi {
                ai_kind: UnitAiKind::Civilian,
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            },
        )]);
        world.next_object_id = object_id(10028);
        world.model.masters = vec![Arc::new(Masters::default()), Arc::new(masters)];
        world.streams.seed_all(0x5eed);
        let _ = Arc::make_mut(&mut world.model.graphics).insert(
            SpriteId::named(588),
            Graphic {
                id: SpriteId::named(588),
                frames: Frame::of_data(1),
                facets: FacetCount::of_data(5),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::ZERO,
                replay_delay: Seconds::ZERO,
                flags,
            },
        );
        placeable(&mut world);
        let before = world.streams.get_mut(Stream::Object).clone();
        world.run(&Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            queue_id: QueueId::of_data(-1),
            sprite_override: SpriteOverride::of_data(-1),
            builders: unit_list(object_ids([9175])),
            queued: false,
            commits: false,
            frame: Frame::of_data(0),
        }));
        assert_eq!(world.players[1].unsupported, None);
        (world, before)
    };
    // sequence type 3: the facing's draw, then the powerup offset's
    let (mut world, mut before) = build(SpriteFlags::ANIMATED | SpriteFlags::TURNS);
    let facing = before.rand15();
    let _ = before.rand15();
    assert_eq!(world.streams.get_mut(Stream::Object), &before);
    let facets = SpriteFacets {
        count: FacetCount::of_data(5),
        turns: true,
    };
    let foundation = world.players[1].objects.last().unwrap();
    let angle = drawn_angle(facing, facets);
    assert_ne!(angle, Angle::ZERO);
    assert_eq!(
        world.parts.mover_of(foundation.id).map(|mover| mover.angle),
        Some(angle)
    );
    // without sequence flag 2 only the powerup offset's
    let (mut world, mut before) = build(SpriteFlags::ANIMATED);
    let _ = before.rand15();
    assert_eq!(world.streams.get_mut(Stream::Object), &before);
    let foundation = world.players[1].objects.last().unwrap();
    assert_eq!(
        world.parts.mover_of(foundation.id).map(|mover| mover.angle),
        Some(Angle::ZERO)
    );
}

/// [`builder_masters`] with the farmer (259, of the villagers' group 1, which builds as 118)
/// and the farm (50: class 49, its own base, available, 60 wood, a building part of its own).
fn farm_masters() -> Masters {
    let builders = builder_masters();
    let farmer = Master {
        id: master_id(259),
        ..builders.get(VILLAGER_MALE).unwrap().clone()
    };
    let farm = Master {
        id: ids::FARM,
        terrain_restriction: Some(RestrictionId::named(0)),
        base_id: Some(ids::FARM),
        class: class::FARM,
        available: Availability::AVAILABLE,
        radius: Footprint::new(1.5, 1.5),
        clearance: Footprint::new(1.5, 1.5),
        hit_points: MasterHitPoints::of_data(480),
        creatable: Some(CreatablePart {
            cost: costs(&[(attribute::WOOD, 60, true)]),
            build_time: WholeSeconds::of_data(15),
            ..builders
                .get(ids::HOUSE)
                .unwrap()
                .clone()
                .creatable
                .unwrap_or_default()
        }),
        ..builders.get(ids::HOUSE).unwrap().clone()
    };
    Masters::new(builders.iter().cloned().chain([farmer, farm]))
}

/// Player 1's farmer 9175 back at its farm spot (30.5, 88.5), its walk back (state 0xf2)
/// ended; the player holds 100 wood.
fn reseed_world() -> World {
    let task = HuntTask {
        id: task_id(2),
        range: Distance::new(0.25),
        resource: Some(attribute::MEAT),
        sprites: TaskSprites::NONE,
        work_value: WorkValue::new(1.0),
        productivity: None,
        kind4: None,
    };
    let farm = Resource {
        id: object_id(12557),
        position: Position::new(30.5, 88.5, 0.0),
        class: class::FARM,
        master: ids::FARM,
        player: PlayerId::new(1),
        workers: WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::ZERO,
        state: ObjectState::Alive,
        radius: Footprint::new(1.5, 1.5),
    };
    let rate = StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let gather = GatherAction {
        state: GatherState::ReseedWalk { move_to: None },
        target: None,
        worked: true,
        ..GatherAction::new(task, &farm, FarmMode::Fixed, rate, &roles()).unwrap()
    };
    let farmer = Object::with_actions(
        Unit {
            id: object_id(9175),
            place: Static::at(Position::new(30.5, 88.5, 0.0)),
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::new(25.0),
                master_id: master_id(259),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_gather(Some(gather)),
            ..Default::default()
        },
    );
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![farmer.unit].into(),
                attributes: attributes(250, [0.0, 100.0, 0.0, 0.0]),
                ..Player::default()
            },
        ],
    )
    .with_actions([(object_id(9175), farmer.parts.actions)])
    .with_ais([(
        object_id(9175),
        UnitAi {
            ai_kind: UnitAiKind::Civilian,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        },
    )]);
    world.next_object_id = object_id(10028);
    world.model.masters = vec![Arc::new(Masters::default()), Arc::new(farm_masters())];
    placeable(&mut world);
    world
}

/// `FUN_140a8aa60` with the automatic farm queue: the farmer pays a farm (60 wood), its
/// foundation goes on the farm spot and the farmer builds it (as 118, its Gather gone).
#[test]
fn farmer_whose_farm_ran_out_reseeds_it_with_the_automatic_queue() {
    let mut world = reseed_world();
    world.players[1].auto_queues.set(AutoQueues::FARMS, true);
    world
        .reseed_after_walk(PlayerId::new(1), ListIndex::new(0), WalkEnd::Arrived)
        .unwrap();
    let player = &world.players[1];
    assert_eq!(player.attributes[1], Amount::new(40.0));
    assert_eq!(player.farm_queue, QueueCount::of_data(0));
    let foundation = player.objects.last().unwrap();
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.master_id.get(),
            foundation.position()
        ),
        (10028, 50, Position::new(30.5, 88.5, 0.0))
    );
    let farmer = &player.objects[ListIndex::new(0)];
    assert_eq!(farmer.life.master_id, ids::BUILDER);
    assert_eq!(
        world
            .parts
            .actions_of(farmer.id)
            .list
            .build()
            .map(|build_action| build_action.target.map(FoundationId::id)),
        Some(Some(object_id(10028)))
    );
    assert!(world.parts.actions_of(farmer.id).list.gather().is_none());
}

/// Without the automatic queue the farm comes from the mill's queue (one less) unpaid; with
/// it but too little wood, from the queue too.
#[test]
fn reseed_takes_queued_farm() {
    for (auto, wood) in [(false, 100.0), (true, 59.0)] {
        let mut world = reseed_world();
        world.players[1].auto_queues.set(AutoQueues::FARMS, auto);
        world.players[1].attributes[1] = Amount::new(wood);
        world.players[1].farm_queue = QueueCount::of_data(2);
        world
            .reseed_after_walk(PlayerId::new(1), ListIndex::new(0), WalkEnd::Arrived)
            .unwrap();
        let player = &world.players[1];
        assert_eq!(
            (player.attributes[1], player.farm_queue),
            (Amount::new(wood), QueueCount::of_data(1))
        );
        assert_eq!(player.objects.last().unwrap().life.master_id, ids::FARM);
    }
}

/// Without a farm to make (nothing queued, no automatic queue, or no wood) the reseed makes
/// nothing: a human's farmer idles, a computer's is done.
#[test]
fn without_farm_to_make_the_farmer_idles() {
    let mut world = reseed_world();
    world
        .reseed_after_walk(PlayerId::new(1), ListIndex::new(0), WalkEnd::Arrived)
        .unwrap();
    let players = &world.players[1];
    assert_eq!(players.objects.len(), 1);
    assert_eq!(
        world
            .parts
            .actions_of(players.objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state),
        Some(GatherState::Idle)
    );
    let mut world = reseed_world();
    world.players[1].auto_queues.set(AutoQueues::FARMS, true);
    world.players[1].attributes[1] = Amount::new(59.0);
    world.players[1].kind = PlayerKind::Computer;
    world
        .reseed_after_walk(PlayerId::new(1), ListIndex::new(0), WalkEnd::Arrived)
        .unwrap();
    let player = &world.players[1];
    assert_eq!(
        (player.objects.len(), player.attributes[1]),
        (1, Amount::new(59.0))
    );
    assert_eq!(
        world
            .parts
            .actions_of(player.objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state),
        Some(GatherState::Done)
    );
}

/// [`reseed_world`] with one farm queued, `setup` run on it, then the reseed itself
/// ([`World::reseed`]): its answer and the count of player 1's objects after it.
fn reseeded(setup: impl FnOnce(&mut World)) -> (GenieResult<Reseed>, usize) {
    let mut world = reseed_world();
    world.players[1].farm_queue = QueueCount::of_data(1);
    setup(&mut world);
    let made = world.reseed(PlayerId::new(1), ListIndex::new(0));
    (made, world.players[1].objects.len())
}

/// `FUN_140a8aa60` makes nothing for a gatherer without a Gather or without a farm spot, for a
/// villager with only fish traps queued, for a fishing ship (class 21) with only farms queued and
/// for a farm the player cannot make (its master unavailable).
#[test]
fn reseed_without_farm_to_make_makes_nothing() {
    let none: [fn(&mut World); 5] = [
        |world| {
            world
                .parts
                .actions
                .of_mut(world.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .list
                .set_gather(None);
        },
        |world| {
            let gather_action = world
                .parts
                .actions
                .of_mut(world.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .list
                .gather_mut()
                .unwrap();
            gather_action.farm = Point::new(-1.0, -1.0);
        },
        |world| {
            (
                world.players[1].farm_queue,
                world.players[1].fish_trap_queue,
            ) = (QueueCount::ZERO, QueueCount::ONE);
        },
        |world| {
            Arc::make_mut(&mut world.model.masters[1])
                .get_mut(master_id(259))
                .unwrap()
                .class = class::FISHING_SHIP;
        },
        |world| {
            Arc::make_mut(&mut world.model.masters[1])
                .get_mut(ids::FARM)
                .unwrap()
                .available = Availability::UNAVAILABLE;
        },
    ];
    for setup in none {
        assert_eq!(reseeded(setup), (Ok(Reseed::NotMade), 1));
    }
}

/// A fishing ship (class 21) reseeds a fish trap (199) from the dock's queue (one less), with
/// the automatic fish trap queue paying it (60 wood) instead.
#[test]
fn fishing_ship_reseeds_fish_trap() {
    let ship = |world: &mut World| {
        let trap = Master {
            id: FISH_TRAP,
            base_id: Some(FISH_TRAP),
            ..world.model.masters[1].get(ids::FARM).unwrap().clone()
        };
        let ms = &mut world.model.masters[1];
        // a ship builds as itself: its own Build task, no group
        let tasks = ms.get(ids::BUILDER).unwrap().action_part().tasks.clone();
        let ship = Arc::make_mut(ms).get_mut(master_id(259)).unwrap();
        ship.class = class::FISHING_SHIP;
        let action = ship.action.get_or_insert_with(Default::default);
        (action.task_by_group, action.tasks) = (TaskGroup::NONE, tasks);
        *ms = Arc::new(Masters::new(ms.iter().cloned().chain([trap])));
        world.players[1].farm_queue = QueueCount::of_data(0);
    };
    let mut world = reseed_world();
    ship(&mut world);
    world.players[1].fish_trap_queue = QueueCount::of_data(2);
    assert_eq!(
        world.reseed(PlayerId::new(1), ListIndex::new(0)),
        Ok(Reseed::Made)
    );
    let players = &world.players[1];
    assert_eq!(
        (players.attributes[1], players.fish_trap_queue),
        (Amount::new(100.0), QueueCount::ONE)
    );
    let trap = players.objects.last().unwrap();
    assert_eq!(
        (trap.life.master_id, trap.position()),
        (FISH_TRAP, Position::new(30.5, 88.5, 0.0))
    );
    let mut world = reseed_world();
    ship(&mut world);
    world.players[1]
        .auto_queues
        .set(AutoQueues::FISH_TRAPS, true);
    assert_eq!(
        world.reseed(PlayerId::new(1), ListIndex::new(0)),
        Ok(Reseed::Made)
    );
    let player = &world.players[1];
    assert_eq!(
        (player.attributes[1], player.fish_trap_queue),
        (Amount::new(40.0), QueueCount::of_data(0))
    );
}

/// The farm's placement test passes over the gather's old target (the spent farm 12557 on the
/// farm spot, a building): the new foundation goes over it. The same farm when it is not the
/// target fails the test, and nothing is made.
#[test]
fn reseed_placement_passes_over_the_old_farm() {
    let old_farm = |target: i32| {
        move |world: &mut World| {
            let farm = Unit {
                id: object_id(12557),
                place: Static::at(Position::new(30.5, 88.5, 0.0)),
                life: Lifecycle {
                    class: Class::Animated,
                    state: ObjectState::Alive,
                    hp: HitPoints::new(480.0),
                    master_id: ids::FARM,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            };
            let _ = world.push_live(PlayerId::new(1), farm).unwrap();
            open_map(world);
            let at = Placed {
                position: Position::new(30.5, 88.5, 0.0),
                radius: Footprint::new(1.5, 1.5),
                class: class::FARM,
                player: PlayerId::new(1),
                master: ids::FARM,
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            };
            world.spatial.insert(object_id(12557), at);
            world
                .parts
                .actions
                .of_mut(world.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .list
                .gather_mut()
                .unwrap()
                .target = GatherTargetId::of_field(target);
        }
    };
    assert_eq!(reseeded(old_farm(12557)), (Ok(Reseed::Made), 3));
    assert_eq!(reseeded(old_farm(-1)), (Ok(Reseed::NotMade), 2));
}

/// A committing BUILD (`+0x1f` 0) to a builder whose unit AI holds an order not ready
/// (`FUN_140c4dfa0`): the build order joins its queue, then the readiness pass with 0
/// (`FUN_140c42b60` at 0x140e7b252) makes every queued order ready, the ready ones kept, and
/// the unit is not stopped.
#[test]
fn committing_build_or_wall_to_busy_builder_readies_its_queue() {
    let unready = QueuedOrder {
        order: OrderKind::Move,
        ready: false,
        ..QueuedOrder::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![Unit {
                    id: object_id(9175),
                    place: Static::at(Position::new(30.0, 89.0, 0.0)),
                    life: Lifecycle {
                        class: Class::Animated,
                        state: ObjectState::Alive,
                        hp: HitPoints::new(25.0),
                        master_id: VILLAGER_MALE,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                }]
                .into(),
                attributes: attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    )
    .with_ais([(
        object_id(9175),
        UnitAi {
            ai_kind: UnitAiKind::Civilian,
            current_order: Some(OrderKind::Move),
            current_action: None,
            current_target: None,
            orders: vec![unready],
            ..UnitAi::default()
        },
    )]);
    world.next_object_id = object_id(10028);
    world.model.masters = vec![Arc::new(Masters::default()), Arc::new(builder_masters())];
    placeable(&mut world);
    let mut wall = world.clone();
    world.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        queue_id: QueueId::of_data(-1),
        sprite_override: SpriteOverride::of_data(-1),
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: true,
        frame: Frame::of_data(0),
    }));
    assert_eq!(world.players[1].unsupported, None);
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    let queue: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.ready))
        .collect();
    assert_eq!(queue, [(OrderKind::Move, true), (OrderKind::Build, true)]);
    assert_eq!(ai.current_order, Some(OrderKind::Move));
    // BUILD_WALL's line of one tile the same (0x140e7be4b)
    wall.run(&Command::BuildWall(BuildWallCommand {
        player: player_id(1),
        master: master_id(70),
        from: Tile::new(30, 88),
        to: Tile::new(30, 88),
        queue_id: QueueId::of_data(-1),
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: true,
    }));
    assert_eq!(wall.players[1].unsupported, None);
    let ai = wall
        .parts
        .unit_ai
        .get(wall.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    let queue: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.ready))
        .collect();
    assert_eq!(queue, [(OrderKind::Move, true), (OrderKind::Build, true)]);
}

/// `ClosestResourceObject`'s point (`FUN_140c49760`): without a site the search starts from the
/// point at the unit's height; (−1, −1) exactly, or a site, keeps the unit's place; one part −1
/// or not a number is a point.
#[test]
fn resource_search_starts_from_its_point_unless_it_is_none_or_has_site() {
    let unit = Position::new(5.0, 6.0, 0.5);
    assert_eq!(
        search_origin(Point::new(12.5, 30.5), None::<ObjectId>, unit),
        Position::new(12.5, 30.5, 0.5)
    );
    assert_eq!(search_origin(NO_POINT, None::<ObjectId>, unit), unit);
    assert_eq!(
        search_origin(Point::new(12.5, 30.5), Some(object_id(1)), unit),
        unit
    );
    assert_eq!(
        search_origin(Point::new(-1.0, 3.0), None::<ObjectId>, unit),
        Position::new(-1.0, 3.0, 0.5)
    );
    assert!(
        search_origin(Point::new(f32::NAN, -1.0), None::<ObjectId>, unit)
            .x()
            .is_nan()
    );
}

/// Where a stop lands its error names what was running: a command's stop the command, an
/// object's freeze the object, its master, its top action and its unit AI's order.
#[test]
fn stop_error_names_the_command_or_the_object_it_happened_on() {
    let why = GenieError::unsupported("QUEUE at two buildings");
    let mut one_player = one_player(vec![town_centre(9)], [200.0; 4]);
    stock_masters(&mut one_player);
    one_player.tick_with(
        Millis::new(104),
        &[Command::Unmodelled(UnmodelledCommand {
            player: player_id(0),
            packet: Vec::new(),
            why,
        })],
    );
    let (_, _, stopped) = one_player.players[0].unsupported.clone().unwrap();
    assert_eq!(
        stopped.context().collect::<Vec<_>>(),
        [("command", "Unmodelled")]
    );
    let task = GenieError::unsupported("a default task the slice does not model");
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(&[])],
    );
    let _ = world
        .push_live(
            PlayerId::new(0),
            Object::unread(
                Unit {
                    id: object_id(5),
                    life: Lifecycle {
                        master_id: VILLAGER_MALE,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                },
                task,
            ),
        )
        .unwrap();
    stock_masters(&mut world);
    world.tick(Millis::new(10));
    let (_, _, frozen) = world.players[0].freezes[0].clone();
    assert_eq!(
        frozen.context().collect::<Vec<_>>(),
        [("object", "5"), ("master", "83")]
    );
}

/// `FUN_1411671e0`'s planner results: a path to a drop site or into its range counts (0 and 1),
/// a partial one never (2); one the planner does not reach (3, the game's 3 and up) counts only
/// for a fishing ship seeking by path whose footprint gap to it is within 100 squared
/// (429521544's p3 in update 5601).
#[test]
fn fishing_ship_counts_near_drop_site_its_path_does_not_reach() {
    use crate::gathering::drop_sites::counted;
    use crate::pathing::tile_path::Found;
    assert!(counted(
        Found::OffMap,
        DropMeasure::ByPath,
        ClassRoles::VILLAGER,
        || false
    ));
    assert!(counted(
        Found::Goal,
        DropMeasure::ByDistance,
        ClassRoles::VILLAGER,
        || false
    ));
    assert!(!counted(
        Found::Nearest,
        DropMeasure::ByPath,
        ClassRoles::FISHING_SHIP,
        || true
    ));
    assert!(!counted(
        Found::Nothing,
        DropMeasure::ByPath,
        ClassRoles::VILLAGER,
        || true
    ));
    assert!(counted(
        Found::Nothing,
        DropMeasure::ByPath,
        ClassRoles::FISHING_SHIP,
        || true
    ));
    assert!(!counted(
        Found::Nothing,
        DropMeasure::ByPath,
        ClassRoles::FISHING_SHIP,
        || false
    ));
    assert!(!counted(
        Found::Nothing,
        DropMeasure::ByDistance,
        ClassRoles::FISHING_SHIP,
        || true
    ));
}

/// `TRIBE_Building_Object::update`'s farm stage (`0x1411ca2ef`): a farm lays 29 to 31 by its
/// build points over its build time (times 3, at most 2); a rice farm or a pasture on mangrove
/// or rice 65 to 67, elsewhere its foundation terrain plus 2 (63 and 117: 65.., 119..); another
/// master none (427445417's p4 in update 16981, 427557935's p4 in 22813).
#[test]
fn farm_foundation_lays_its_stage_terrain() {
    use crate::construction::building_update::farm_stage_terrain;
    assert_eq!(
        farm_stage_terrain(
            ids::FARM,
            (false, Some(TerrainId::named(7))),
            Seconds::ZERO,
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(29))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::FARM,
            (false, Some(TerrainId::named(7))),
            Seconds::new(7.5),
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(30))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::FARM,
            (false, Some(TerrainId::named(7))),
            Seconds::new(15.0),
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(31))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::ID_1187,
            (true, Some(TerrainId::named(63))),
            Seconds::new(5.0),
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(66))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::ID_1187,
            (false, Some(TerrainId::named(63))),
            Seconds::ZERO,
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(65))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::PASTURE_1889,
            (false, Some(TerrainId::named(117))),
            Seconds::new(15.0),
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(121))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::PASTURE_1889,
            (false, None),
            Seconds::ZERO,
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        Some(TerrainId::named(1))
    );
    assert_eq!(
        farm_stage_terrain(
            ids::HOUSE,
            (false, None),
            Seconds::ZERO,
            WholeSeconds::of_data(15),
            &roles(),
            &tuning().turn
        ),
        None
    );
}

/// `RGE_Moving_Object::update`'s count-down of the last human order time (`+0x650`, at most
/// `FUN_140fe0d10`'s 5000, by the world's dt `+0x7c`, not below 0) is the world's view at once:
/// the unit's own walk later in the update ranks a met unit by the counted-down time
/// (431096717's goat 12959 in update 78: 2001 against villager 16025's 2034, so it does not
/// pause it).
#[test]
fn unit_human_order_count_down_is_the_walk_view_at_once() {
    let mut world = enter_world(vec![], 5);
    let mut counted = |before: u32| {
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        unit.last_human_order = WorldTime::from_millis(before);
        let id = unit.id;
        unit.count_down_human_order(Millis::new(33));
        let seen = world.objects().gait(id).map(|gait| gait.human_order);
        (
            world.players[1].objects[ListIndex::new(0)].last_human_order,
            seen,
        )
    };
    assert_eq!(
        counted(2034),
        (
            WorldTime::from_millis(2001),
            Some(WorldTime::from_millis(2001))
        )
    );
    assert_eq!(
        counted(5000),
        (
            WorldTime::from_millis(4967),
            Some(WorldTime::from_millis(4967))
        )
    );
    assert_eq!(counted(20), (WorldTime::ZERO, Some(WorldTime::ZERO)));
    assert_eq!(
        counted(5001),
        (
            WorldTime::from_millis(5001),
            Some(WorldTime::from_millis(5001))
        )
    );
}

/// `RGE_Action_Object::update` runs the actions of a unit not inside another, else its inside
/// update (`FUN_140a98790`), never both: livestock whose Artifact's Enter got it in keeps the
/// Artifact to its next update, whose inside update deletes it (429705045's sheep 10555 and
/// 10556 entering the transport 10579 in update 137, the Artifact gone after 138); an animal
/// inside at the update's start runs no Artifact step.
#[test]
fn livestock_got_in_by_its_artifact_enter_keeps_the_artifact_to_its_next_update() {
    let mut livestock_enter_world = livestock_enter_world();
    // the sheep out of reach of the house, standing, so that its Enter walks
    let place = |world: &mut World, at: Position| {
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        unit.place_for_test(at);
        let mover = world.parts.mover_mut(unit.id).unwrap();
        mover.moved_at = at;
        mover.velocity = Some(Velocity {
            velocity: BVector::ZERO,
            acceleration: BVector::ZERO,
        });
        world
            .spatial
            .set_position(object_id(9175), at, &roles(), tuning().visibility)
            .unwrap();
    };
    place(&mut livestock_enter_world, Position::new(30.0, 91.0, 0.0));
    assert_eq!(
        livestock_enter_world.enter_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(10028),
            OrderForce::Unforced
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(
        livestock_enter_world.players[1].objects[ListIndex::new(0)]
            .life
            .inside,
        None
    );
    // the first update runs no Artifact (its first update is in the second)
    livestock_enter_world.tick(Millis::new(50));
    // its walk arrived at the house's edge: the next update of the Enter gets it in
    place(&mut livestock_enter_world, Position::new(30.0, 89.1, 0.0));
    let art = livestock_enter_world
        .parts
        .actions
        .of_mut(livestock_enter_world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .artifact_mut()
        .unwrap();
    if let Some(ArtifactSub::Enter { enter }) = &mut art.sub
        && let Some(move_to_action) = enter.state.walk_mut().and_then(Option::as_mut)
    {
        move_to_action.state = MoveTo::Arrived;
    }
    livestock_enter_world.tick(Millis::new(50));
    let players = &livestock_enter_world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        players.life.inside,
        Some(ContainerId::new(object_id(10028)))
    );
    assert_eq!(
        livestock_enter_world
            .parts
            .actions_of(players.id)
            .top_action_kind(),
        Some(ARTIFACT_KIND)
    );
    // inside at the next update's start: no Artifact step, the inside update deletes it
    let before = livestock_enter_world
        .parts
        .actions_of(players.id)
        .list
        .artifact()
        .map(|artifact| (artifact.state, artifact.timer));
    livestock_enter_world.tick(Millis::new(50));
    let unit = &livestock_enter_world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            livestock_enter_world
                .parts
                .actions_of(unit.id)
                .list
                .artifact()
                .copied(),
            livestock_enter_world
                .parts
                .actions_of(unit.id)
                .top_action_kind()
        ),
        (None, None)
    );
    assert_eq!(livestock_enter_world.players[1].freezes, vec![]);
    assert!(before.is_some());
}

/// `RGE_Action_Object::update` with an empty list makes the default task and does no inside
/// update in the same update: livestock inside another object gets its Artifact in one update
/// and loses it in the next, by turns (431108946's sheep 11628 in the transport 11638 from update
/// 61: 107 in state 6 after 63, 65, …, none after 62, 64, …).
#[test]
fn livestock_inside_gets_its_default_artifact_and_loses_it_by_turns() {
    let mut world = livestock_enter_world();
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    world.parts.actions.of_mut(players.id).unwrap().default_task = DefaultTask::Artifact;
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .set_artifact(None);
    players.life.inside = Some(ContainerId::new(object_id(10028)));
    world
        .spatial
        .go_inside(object_id(9175), Position::new(30.0, 88.0, 0.0));
    let mut kinds = Vec::new();
    for _ in 0..4 {
        world.tick(Millis::new(50));
        let unit = &world.players[1].objects[ListIndex::new(0)];
        kinds.push(
            world
                .parts
                .actions_of(unit.id)
                .list
                .artifact()
                .map(|artifact| artifact.state),
        );
    }
    assert_eq!(
        kinds,
        [
            Some(ArtifactState::Check),
            None,
            Some(ArtifactState::Check),
            None
        ]
    );
    assert_eq!(world.players[1].freezes, vec![]);
}

/// `TRIBE_Command::do_command`'s case 0x66 checks BUILD's builders before its handler
/// ([`World::command_check`]): another player's becomes −1, and a human's BUILD left without one
/// is refused, placing and paying nothing (429712917's player 8 in update 96, its BUILD naming
/// player 5's villagers 9618 and 9620). With one of its own the foundation goes down, and only
/// that one builds.
#[test]
fn build_of_another_player_builders_is_refused_and_theirs_are_left_out() {
    let mut world = build_world();
    world.model.masters[0] = Arc::new(builder_masters());
    let theirs = world
        .remove_live(PlayerId::new(1), ListIndex::new(1))
        .unwrap();
    let _ = world.push_live(PlayerId::new(0), theirs).unwrap();
    let build = |builders: Vec<i32>| {
        Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            queue_id: QueueId::of_data(-1),
            sprite_override: SpriteOverride::of_data(-1),
            builders: unit_list(object_ids(builders)),
            queued: false,
            commits: false,
            frame: Frame::of_data(0),
        })
    };
    world.tick_with(Millis::new(50), &[build(vec![9181])]);
    let players = &world.players[1];
    assert_eq!(
        (players.resources().map(Amount::get), players.objects.len()),
        ([200.0, 100.0, 200.0, 100.0], 1)
    );
    assert_eq!(world.next_object_id, object_id(10028));
    assert_eq!(players.freezes, vec![]);
    world.tick_with(Millis::new(50), &[build(vec![9181, 9175])]);
    let player = &world.players[1];
    assert_eq!(player.resources(), amounts([200.0, 75.0, 200.0, 100.0]));
    assert_eq!(player.objects.last().map(|unit| unit.id.get()), Some(10028));
    assert_eq!(
        world
            .parts
            .actions_of(player.objects[ListIndex::new(0)].id)
            .list
            .build()
            .map(|build_action| build_action.target.map(FoundationId::id)),
        Some(Some(object_id(10028)))
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[0].objects[ListIndex::new(0)].id)
            .list
            .build()
            .copied(),
        None
    );
}

/// The archer's master (4, 2 s reload, range 4, its missile 363 a tile up), its arrow (363,
/// speed 7, straight) and a deer (65, 5 hit points, 1.5 high) for [`archer_world`].
fn archer_masters(hit_chance: i16, max_volley: u8, spread: f32) -> Masters {
    let archer = Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        hit_points: MasterHitPoints::of_data(30),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: Some(SpriteId::named(300)),
        creatable: Some(CreatablePart {
            max_volley: MissileCount::of_data(max_volley),
            volley_spread: VolleySpread {
                x: Distance::new(spread),
                y: Distance::new(spread),
                start: Distance::ZERO,
            },
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            attack_sprite: Some(SpriteId::named(301)),
            missile: MasterField::of(Some(master_id(363))),
            weapon_offset: Offset::new(0.0, 0.0, 1.0),
            weapon_range: Distance::new(4.0),
            reload: Seconds::new(2.0),
            hit_chance: HitChance::of_data(hit_chance),
            ..CombatPart::default()
        }),
        ..moving_master(walker().stats)
    };
    let arrow = Master {
        id: master_id(363),
        kind: MasterKind::MISSILE,
        class: class::WALL,
        hit_points: MasterHitPoints::of_data(1),
        radius: Footprint::new(0.1, 0.1),
        standing_sprite: None,
        death_sprite: None,
        death_spawns: [None, None],
        life: Some(MasterData::default()),
        recyclable: RecycleMode::RECYCLED,
        move_stats: Some(MoveStats {
            speed: Speed::new(7.0),
            base: Speed::new(7.0),
            ..walker().stats
        }),
        // the flight reads the speed and the radii
        animated: Some(AnimatedPart {
            max_speed: Speed::new(7.0),
        }),
        missile: Some(MissilePart::default()),
        ..Master::default()
    };
    let deer = Master {
        id: master_id(65),
        kind: MasterKind::COMBAT,
        class: class::PREDATOR,
        hit_points: MasterHitPoints::of_data(5),
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.5),
        combat: Some(CombatPart {
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    Masters::new([archer, arrow, deer])
}

/// A flat 40-tile map: player 1's archer 123 at (10, 10) in its Attack's READY on Gaia's deer 50
/// at (13, 10); its swing fires at frame 5 of 0.1 s. The masters as [`archer_masters`]; the next
/// reusable id −1.
pub(crate) fn archer_world(hit_chance: i16, max_volley: u8, spread: f32) -> World {
    use crate::garrison::tests::{unit, walker};
    let mut players: Vec<Player> = (0..2).map(|_| Player::default()).collect();
    players[0].relations = relation_row([0, 3]);
    players[1].relations = relation_row([3, 0]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(29910),
        PlayerId::new(1),
        vec![Millis::ZERO; 2],
        players,
    );
    world.spatial = Spatial::new(GridSize::new(40, 40));
    let mut terrain = Terrain::new(
        GridSize::new(40, 40),
        vec![TileTerrain::of_save(0, 0, -1); 40 * 40],
        vec![TerrainKind {
            shown: true,
            flags: TerrainFlags::NONE,
            forest: false,
        }],
        Vec::new(),
    );
    terrain.set_elevations(vec![0; 40 * 40]);
    world.spatial.terrain = Some(terrain);
    let sprite = |id| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    world.model.graphics = Arc::new(Graphics::from([
        (SpriteId::named(300), sprite(SpriteId::named(300))),
        (SpriteId::named(301), sprite(SpriteId::named(301))),
    ]));
    let masters = archer_masters(hit_chance, max_volley, spread);
    world.model.masters = vec![Arc::new(masters.clone()), Arc::new(masters)];
    walker(
        &mut world,
        PlayerId::new(1),
        123,
        master_id(4),
        Point::new(10.0, 10.0),
    );
    let archer_id = world.players[1].objects[ListIndex::new(0)].id;
    world.parts.combat_mut(archer_id).unwrap().firepower = Firepower::new(f32::from(max_volley));
    let archer = &mut world.players[1].objects[ListIndex::new(0)];
    if let Some(mover) = world.parts.mover_mut(archer.id) {
        mover.sprite_facets = Some(SpriteFacets {
            count: FacetCount::of_data(8),
            turns: false,
        });
    }
    let mut attack = own_attack(50, -1, AttackState::Ready);
    attack.range = Distance::new(4.0);
    attack.arms.missile = Some(master_id(363));
    attack.arms.fire_frame = Frame::of_data(5);
    attack.arms.sprite = Some(sprite(SpriteId::named(301)));
    attack.arms.idle_sprite = Some(sprite(SpriteId::named(300)));
    world
        .parts
        .actions
        .of_mut(archer.id)
        .unwrap()
        .list
        .set_attack(Some(attack));
    let mut deer = unit(50, master_id(65), HitPoints::new(5.0));
    deer.place_for_test(Position::new(13.0, 10.0, 0.0));
    let _ = world.push_live(PlayerId::new(0), deer).unwrap();
    world.spatial.insert(
        object_id(50),
        Placed {
            position: Position::new(13.0, 10.0, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::PREDATOR,
            player: PlayerId::new(0),
            master: master_id(65),
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionKind::KIND_2,
                class: ObstructionClass::NONE,
            }),
        },
    );
    world.recycling.next_id = object_id(-2);
    world.next_object_id = object_id(200);
    world
}

/// A hit's flare (master 112, the target's player's slot 0x1c0) is made where the volley runs,
/// within the shooter's update: here for the shooter's own player, whose list holds the
/// shooter's stand-in.
#[test]
fn hit_flare_is_made_where_the_volley_runs() {
    let mut world = archer_world(100, 1, 0.0);
    let flare = Master {
        id: master_id(112),
        kind: MasterKind::ANIMATED,
        standing_sprite: None,
        storage: vec![(Some(AttributeId::named(12)), Amount::new(1.0), 0)],
        ..Master::default()
    };
    world.model.masters[1] = Arc::new(Masters::new(
        world.model.masters[1].iter().cloned().chain([flare]),
    ));
    world.players[1].attributes = vec![Amount::ZERO; 300];
    let aimed = Aimed {
        target: Some(object_id(50)),
        position: Position::new(13.0, 10.0, 0.0),
        radius: Radii::new(0.2, 0.2, 1.0),
        class: class::PREDATOR,
        player: PlayerId::new(1),
    };
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, _, step| {
            unit.volley(step, aimed, &roles())?;
            let own = &step.world.players[1].objects;
            Ok(own
                .iter()
                .filter(|unit| unit.life.master_id == master_id(112))
                .count())
        },
    );
    assert_eq!(seen.map_err(|error| error.reason().to_owned()), Ok(1));
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].id,
        object_id(123),
        "the shooter is back in its slot"
    );
}

/// The missiles in player 1's list: id, master, place.
pub(crate) fn missiles(world: &World) -> Vec<(i32, i16, Position)> {
    world.players[1]
        .objects
        .iter()
        .filter(|unit| world.parts.missile.contains(unit.id))
        .map(|unit| (unit.id.get(), unit.life.master_id.get(), unit.position()))
        .collect()
}

/// The deer's hit points in [`archer_world`].
fn deer_hp(world: &World) -> f32 {
    world.players[0].objects[ListIndex::new(0)].life.hp.get()
}

/// `actual` within 1e-5 of `expected` on each axis.
fn assert_near(actual: Position, expected: Position) {
    assert!(
        [
            actual.x() - expected.x(),
            actual.y() - expected.y(),
            actual.z() - expected.z()
        ]
        .iter()
        .all(|delta| delta.abs() < 1e-5),
        "{actual:?} against {expected:?}"
    );
}

/// Slot 0x858 (`FUN_14120c0a0`) and `FUN_141081020`: the archer's swing fires at its firing
/// frame (frame 5 of 0.1 s: the sixth update) its master's missile (363) with the next reusable
/// id (−1), a tile up (its weapon offset) and aimed at the deer a third of its height up; each
/// update the missile flies its speed's share (0.7 of a tile) along that line. It hits the deer
/// in the update its sweep, grown by its radius, meets the deer's footprint: 1 hit point (the
/// least damage, as neither has attacks) and its action done. The dead missile leaves the list
/// four updates after the hit for the pool of type 60, whose id the next swing takes again 20
/// updates after the first (the 2 s reload).
#[test]
fn ranged_swing_fires_its_missile_which_flies_to_the_target_and_hits_it() {
    let mut world = archer_world(100, 0, 0.0);
    for _ in 0..5 {
        world.tick(Millis::new(100));
    }
    assert_eq!(missiles(&world), vec![]);
    let way = [3.0f32, 0.0, -0.5];
    let length = (way[0] * way[0] + way[2] * way[2]).sqrt();
    let flown = |fraction: f32| {
        Position::new(
            10.0 + fraction * 0.7 * way[0] / length,
            10.0,
            1.0 + fraction * 0.7 * way[2] / length,
        )
    };
    for index in 1..=3 {
        world.tick(Millis::new(100));
        let missiles = missiles(&world);
        assert_eq!((missiles.len(), missiles[0].0, missiles[0].1), (1, -2, 363));
        assert_near(missiles[0].2, flown(index as f32));
        assert_eq!(deer_hp(&world), 5.0);
    }
    // the fourth step ends 0.1 short of the deer's footprint (12.8): its sweep meets it
    world.tick(Millis::new(100));
    assert_near(missiles(&world)[0].2, flown(4.0));
    assert_eq!(deer_hp(&world), 4.0);
    let action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(1)].id)
        .list
        .missile()
        .map(|missile_action| (missile_action.state, missile_action.result));
    assert_eq!(action, Some((MissileState::Done, StrikeResult::Hit)));
    for _ in 0..3 {
        world.tick(Millis::new(100));
    }
    assert_eq!(missiles(&world).len(), 1);
    world.tick(Millis::new(100));
    assert_eq!(missiles(&world), vec![]);
    assert_eq!(world.recycling.missiles, vec![(object_id(-2), true)]);
    for _ in 0..12 {
        world.tick(Millis::new(100));
    }
    assert_eq!(missiles(&world), vec![]);
    world.tick(Millis::new(100));
    assert_eq!(
        missiles(&world)
            .iter()
            .map(|missile| missile.0)
            .collect::<Vec<_>>(),
        [-2]
    );
    assert_eq!(world.recycling.missiles, vec![]);
}

/// The missile's sweep reads each object it meets through that object's master pointer
/// (`+0xa0`, [`crate::objects::masters::MasterTables::of_placed`]): the deer of a master of its
/// own without height (as a desolid foundation's copy is) lets the missile pass over it in the
/// fourth update of its flight, where its shared master's height of 1.5 meets it.
#[test]
fn missile_passes_over_object_whose_own_master_has_no_height() {
    let mut world = archer_world(100, 0, 0.0);
    let own = Master {
        height: Distance::ZERO,
        ..world.model.masters[0].get(master_id(65)).unwrap().clone()
    };
    world
        .set_part(object_id(50), Part::OwnMaster(Some(Box::new(own))))
        .unwrap();
    for _ in 0..9 {
        world.tick(Millis::new(100));
    }
    assert_eq!(deer_hp(&world), 5.0);
    // the fifth step ends on the ground, the deer's height: it meets it there
    world.tick(Millis::new(100));
    assert_eq!(deer_hp(&world), 4.0);
}

/// The missile a swing fires joins the end of its owner's live list at its making (slot
/// 0x2d0, which files it in the tracker), inside the archer's update: what the update does after
/// the fire reaches it by id, as the game's table does.
#[test]
fn fired_missile_joins_its_owner_list_at_its_making() {
    let mut world = archer_world(100, 0, 0.0);
    for _ in 0..5 {
        world.tick(Millis::new(100));
    }
    let made = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        100,
        |unit, actions, step| {
            let _ = unit.update(actions, step, &roles(), slots())?;
            let key = key(
                step.world.model.masters.of(step.player),
                &step.world.model.lines,
                master_id(363),
                &roles(),
            );
            let listed = OthersRef::of(&step.world.players, &step.world.index, step.player)
                .unwrap()
                .own(object_id(-2))
                .map(|unit| unit.life.master_id);
            Ok((
                listed,
                step.world.players[step.player.index()]
                    .tracker
                    .list(ListKey::of_master(key.master))
                    .contains(&Some(object_id(-2))),
            ))
        },
    );
    assert_eq!(made, Ok((Some(master_id(363)), true)));
    let ids: Vec<ObjectId> = world.players[1]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect();
    assert_eq!(ids.last(), Some(&object_id(-2)));
}

/// `FUN_140a9c600`'s miss: a hit chance of 0 strays the missile by up to the ground distance
/// times the master's missed spread; it flies past the deer, lands on the ground (its height 0,
/// `FUN_140a9c890`) and goes to the pool as landed (not flying).
#[test]
fn missed_missile_strays_and_lands_on_the_ground() {
    let mut world = archer_world(0, 0, 0.0);
    for masters in &mut world.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(4))
            .unwrap()
            .combat
            .get_or_insert_with(Default::default)
            .missed_spread = MissSpread::new(2.0);
    }
    for _ in 0..6 {
        world.tick(Millis::new(100));
    }
    let at = missiles(&world)[0].2;
    assert_ne!(at.y(), 10.0);
    for _ in 0..4 {
        world.tick(Millis::new(100));
    }
    assert_eq!(missiles(&world)[0].2.z(), 0.0);
    for _ in 0..2 {
        world.tick(Millis::new(100));
    }
    assert_eq!(missiles(&world), vec![]);
    assert_eq!(world.recycling.missiles, vec![(object_id(-2), false)]);
    assert_eq!(deer_hp(&world), 5.0);
    // the next shot takes the landed missile again: on the ground by its fly mode, a tile up
    // again once it is placed (its new position's height is the weapon offset's)
    for _ in 0..14 {
        world.tick(Millis::new(100));
    }
    assert_eq!(
        missiles(&world)
            .iter()
            .map(|missile| missile.0)
            .collect::<Vec<_>>(),
        [-2]
    );
    assert_eq!(world.recycling.missiles, vec![]);
}

/// A volley (`max_attacks_in_volley` 3, firepower 3): with a spread, a swing fires 1 to 3 of its
/// missiles (an Object-stream draw), the first the master's missile on the line and the others
/// scattered off it, fired before it; `next_volley` counts the fired, the rest follow at the next
/// swing and then it is 0. Each missile that meets the deer takes a hit point. Without a spread
/// one missile a swing.
#[test]
fn volley_fires_its_missiles_over_swings_scattered_by_its_spread() {
    let mut world = archer_world(100, 3, 0.5);
    for _ in 0..6 {
        world.tick(Millis::new(100));
    }
    let first = missiles(&world);
    let fired = world
        .parts
        .combat_of(world.players[1].objects[ListIndex::new(0)].id)
        .next_volley
        .get();
    assert!((1..3).contains(&fired), "{fired}");
    assert_eq!(first.len(), usize::from(fired));
    let (main, scattered) = first.split_last().unwrap();
    assert_eq!(main.0, -i32::from(fired) - 1);
    assert!(scattered.iter().all(|missile| missile.2 != main.2));
    for _ in 0..3 {
        world.tick(Millis::new(100));
    }
    assert_eq!(deer_hp(&world), 5.0 - f32::from(fired));
    // the next swing, six updates after the first, fires the rest
    world.tick(Millis::new(100));
    world.tick(Millis::new(100));
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .next_volley,
        MissileCount::of_data(fired)
    );
    world.tick(Millis::new(100));
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .next_volley,
        MissileCount::of_data(0)
    );
    let mut one = archer_world(100, 3, 0.0);
    for _ in 0..6 {
        one.tick(Millis::new(100));
    }
    assert_eq!(
        (
            missiles(&one).len(),
            one.parts
                .combat_of(one.players[1].objects[ListIndex::new(0)].id)
                .next_volley
        ),
        (1, MissileCount::of_data(1))
    );
    // the missile's own +0x900 is its archer's attack count when fired
    let counts: Vec<AttackCount> = one.players[1].objects.as_slice()[1..]
        .iter()
        .filter(|unit| one.parts.missile.contains(unit.id))
        .map(|unit| one.parts.combat_of(unit.id).attack_count)
        .collect();
    assert_eq!(
        counts,
        [one.parts
            .combat_of(one.players[1].objects[ListIndex::new(0)].id)
            .attack_count]
    );
    assert_ne!(counts, [AttackCount::ZERO]);
}

/// `FUN_141226690` with the master's own volley missile (`volley_missile`, 364): the extra
/// missiles are shooterless, each hits by its own master (`FUN_140a9e6e0`'s slot 0x860, the
/// missile as the attacker: 1 hit point), the first stays the master's missile with the shooter.
#[test]
fn volley_own_missiles_hit_without_the_shooter() {
    let mut world = archer_world(100, 3, 0.5);
    for masters in &mut world.model.masters {
        let mut all: Vec<Master> = masters.iter().cloned().collect();
        let own = Master {
            id: master_id(364),
            combat: Some(CombatPart {
                // a missile master that names a missile of its own always hits (`missile_action.missile` 0 or more)
                missile: MasterField::of(Some(master_id(0))),
                ..masters
                    .get(master_id(363))
                    .unwrap()
                    .clone()
                    .combat
                    .clone()
                    .unwrap_or_default()
            }),
            ..masters.get(master_id(363)).unwrap().clone()
        };
        all.push(own);
        Arc::make_mut(masters)
            .get_mut(master_id(4))
            .unwrap()
            .creatable
            .get_or_insert_with(Default::default)
            .volley_missile = Some(master_id(364));
        all.retain(|x| x.id != master_id(4));
        all.push(masters.get(master_id(4)).unwrap().clone());
        *masters = Arc::new(Masters::new(all));
    }
    for _ in 0..6 {
        world.tick(Millis::new(100));
    }
    let fired = world
        .parts
        .combat_of(world.players[1].objects[ListIndex::new(0)].id)
        .next_volley
        .get();
    let first = missiles(&world);
    let masters: Vec<i16> = first.iter().map(|missile| missile.1).collect();
    let mut expected = vec![364; usize::from(fired) - 1];
    expected.push(363);
    assert_eq!(masters, expected);
    let shooters: Vec<Option<ShooterId>> = world.players[1].objects.as_slice()[1..]
        .iter()
        .filter_map(|unit| {
            world
                .parts
                .actions_of(unit.id)
                .list
                .missile()
                .map(|missile_action| missile_action.fired_from)
        })
        .collect();
    let mut expected = vec![None; usize::from(fired) - 1];
    expected.push(Some(ShooterId::new(object_id(123))));
    assert_eq!(shooters, expected);
    for _ in 0..3 {
        world.tick(Millis::new(100));
    }
    assert_eq!(deer_hp(&world), 5.0 - f32::from(fired));
}

/// `FUN_1410d5920`'s setups for a new missile draw as a new object's: an animated standing sprite
/// (flag 1) with a random first frame (flag 4) starts on the Sprite stream (two draws), then its
/// facet and, as it turns (flag 2) with several facets, its angle on the Object stream; the
/// missile's angle is then its flight's. A missile without a standing sprite draws none of them
/// (the draw counters count each step; the streams' indexes restart at each seeding).
#[test]
fn new_missile_sprite_draws_as_new_object_s() {
    let mut plain = archer_world(100, 0, 0.0);
    let mut drawn = archer_world(100, 0, 0.0);
    let sprite = Graphic {
        id: SpriteId::named(302),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: (SpriteFlags::ANIMATED | SpriteFlags::TURNS | SpriteFlags::RANDOM_FIRST_FRAME),
    };
    let _ = Arc::make_mut(&mut drawn.model.graphics).insert(SpriteId::named(302), sprite);
    for masters in &mut drawn.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(363))
            .unwrap()
            .standing_sprite = Some(SpriteId::named(302));
    }
    for _ in 0..6 {
        plain.tick(Millis::new(100));
        drawn.tick(Millis::new(100));
    }
    assert_eq!(missiles(&plain), missiles(&drawn));
    let mut more =
        |stream| drawn.streams.get_mut(stream).draws - plain.streams.get_mut(stream).draws;
    assert_eq!((more(Stream::Sprite), more(Stream::Object)), (2, 2));
}

/// The missile chain's cases the slice does not model stop the shooter's player with their
/// reason, in the update the swing fires (or the missile flies): a charged hit, a target of a
/// computer player (its notify 0x214), a missile master that cannot be pooled, moved or flown,
/// a smart missile and a projectile of type 1.
#[test]
fn the_missile_chain_unmodelled_cases_stop_the_player_with_their_reason() {
    type Change = fn(&mut World);
    let cases: [(Change, &str); 7] = [
        (
            |world| {
                for masters in &mut world.model.masters {
                    Arc::make_mut(masters)
                        .get_mut(master_id(4))
                        .unwrap()
                        .creatable
                        .get_or_insert_with(Default::default)
                        .charge_kind = ChargeKind::of_data(6);
                }
            },
            "a charged hit",
        ),
        (
            |world| world.players[0].kind = PlayerKind::Computer,
            "a hit on a computer player's object",
        ),
        (
            |world| {
                for masters in &mut world.model.masters {
                    Arc::make_mut(masters)
                        .get_mut(master_id(363))
                        .unwrap()
                        .recyclable = RecycleMode::KEPT;
                }
            },
            "a missile of a master that is not recyclable",
        ),
        (
            |world| {
                for masters in &mut world.model.masters {
                    Arc::make_mut(masters)
                        .get_mut(master_id(363))
                        .unwrap()
                        .move_stats = None;
                }
            },
            "a missile master that does not move",
        ),
        (
            |world| {
                for masters in &mut world.model.masters {
                    Arc::make_mut(masters)
                        .get_mut(master_id(363))
                        .unwrap()
                        .missile = None;
                }
            },
            "a missile master without its flight",
        ),
        (
            |world| {
                for masters in &mut world.model.masters {
                    if let Some(missile_part) = Arc::make_mut(masters)
                        .get_mut(master_id(363))
                        .unwrap()
                        .missile
                        .as_mut()
                    {
                        missile_part.smart_mode = SmartMode::LEADS_TARGET;
                    }
                }
            },
            "a smart missile leading its target",
        ),
        (
            |world| {
                for masters in &mut world.model.masters {
                    if let Some(missile_part) = Arc::make_mut(masters)
                        .get_mut(master_id(363))
                        .unwrap()
                        .missile
                        .as_mut()
                    {
                        missile_part.projectile_kind = ProjectileKind::of_data(1);
                    }
                }
            },
            "a missile of projectile type 1 or 2",
        ),
    ];
    for (change, reason) in cases {
        let mut world = archer_world(100, 0, 0.0);
        change(&mut world);
        for _ in 0..6 {
            world.tick(Millis::new(100));
        }
        let stopped = world.players[1]
            .unsupported
            .as_ref()
            .map(|unmodelled| unmodelled.2.to_string());
        assert!(
            stopped
                .as_deref()
                .is_some_and(|text| text.starts_with(reason)),
            "{reason}: {stopped:?}"
        );
    }
}

/// `FUN_140a9e6e0`'s start: a missile whose target died (state 3) lets it go (its release) and
/// flies on without one: past the place it would have hit the live deer, down to the ground, and
/// into the pool as landed.
#[test]
fn missile_whose_target_died_flies_on_without_it_and_lands() {
    let mut world = archer_world(100, 0, 0.0);
    for _ in 0..7 {
        world.tick(Millis::new(100));
    }
    world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    world.tick(Millis::new(100));
    let target = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(1)].id)
        .list
        .missile()
        .map(|missile_action| missile_action.target);
    assert_eq!(target, Some(None));
    // the update the live deer took its hit in passes it by
    world.tick(Millis::new(100));
    assert_eq!(deer_hp(&world), 5.0);
    world.tick(Millis::new(100));
    assert_eq!(missiles(&world)[0].2.z(), 0.0);
    world.tick(Millis::new(100));
    world.tick(Millis::new(100));
    assert_eq!(missiles(&world), vec![]);
    assert_eq!(world.recycling.missiles, vec![(object_id(-2), false)]);
    assert_eq!(deer_hp(&world), 5.0);
}

/// QUEUE's pick (`FUN_141614450`): the building with room and the least load, the last pick holding
/// against an equal load; a comparison with a NaN load replaces nothing; with none with room, none.
#[test]
fn queued_unit_goes_to_the_least_loaded_building_with_room() {
    use crate::commands::least_loaded;
    assert_eq!(
        least_loaded(
            &[2, 2, 2],
            &[Seconds::new(5.0), Seconds::new(3.0), Seconds::new(4.0)],
            None
        ),
        Some(1)
    );
    assert_eq!(
        least_loaded(
            &[2, 0, 2],
            &[Seconds::new(5.0), Seconds::new(3.0), Seconds::new(4.0)],
            None
        ),
        Some(2)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(3.0), Seconds::new(3.0)], Some(1)),
        Some(1)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(f32::NAN), Seconds::new(3.0)], None),
        Some(0)
    );
    assert_eq!(
        least_loaded(
            &[2, 2, 2],
            &[Seconds::new(4.0), Seconds::new(f32::NAN), Seconds::new(3.0)],
            None
        ),
        Some(2)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(3.0), Seconds::new(f32::NAN)], None),
        Some(0)
    );
    assert_eq!(
        least_loaded(&[0, 2], &[Seconds::new(1.0), Seconds::new(3.0)], Some(0)),
        Some(0)
    );
    assert_eq!(
        least_loaded(&[0, 0], &[Seconds::new(3.0), Seconds::new(1.0)], None),
        None
    );
    assert_eq!(least_loaded(&[], &[], None), None);
}

/// [`forage_world`]'s forager on bush 600 (the unit AI's gather command), its last move at its
/// place, the load's mesh built.
fn foraging() -> World {
    let mut world = forage_world();
    world
        .parts
        .mover_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .moved_at = Position::new(29.5, 89.5, 0.0);
    world
        .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    world.build_mesh().unwrap();
    world
}

/// `updates` of the forager's whole updates (50 ms each), up to the first after which `stop` holds.
fn forage_until(world: &mut World, updates: usize, stop: impl Fn(&World) -> bool) {
    for _ in 0..updates {
        let _ = in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update(unit, action, step, &roles(), slots()),
        )
        .unwrap();
        if stop(world) {
            return;
        }
    }
}

/// The forager's Gather, as its update runs it (`0x140a8e1d0`): it walks to its bush (state 4),
/// arrives (6) and works (7) until it holds its master's capacity of 10 berries; then it carries
/// them to its town centre (8, the centre its second target), whose drop gives the player 10
/// food, and walks back to the bush (4), arrives and works again.
#[test]
fn forager_fills_up_carries_its_berries_home_and_comes_back() {
    let mut world = foraging();
    let mut states = vec![GatherState::Walk { move_to: None }];
    for _ in 0..1200 {
        forage_until(&mut world, 1, |_| false);
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let gather = world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .copied()
            .unwrap();
        if states.last() != Some(&gather.state.bare()) {
            states.push(gather.state.bare());
            if matches!(gather.state, GatherState::Drop { .. }) {
                assert_eq!(
                    unit.life.held(),
                    carrying(Some(AttributeId::named(17)), 10.0)
                );
                assert_eq!(
                    (gather.target, gather.carry_to),
                    (
                        Some(GatherTargetId::new(object_id(600))),
                        Some(DropSiteId::new(object_id(700)))
                    )
                );
            }
        }
    }
    assert_eq!(
        states,
        [
            GatherState::Walk { move_to: None },
            GatherState::Face,
            GatherState::Work,
            GatherState::Drop { move_to: None },
            GatherState::Walk { move_to: None },
            GatherState::Face,
            GatherState::Work
        ]
    );
    assert_eq!(world.players[1].attributes[0], Amount::new(10.0));
    let gather_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (gather_action.target, gather_action.carry_to),
        (Some(GatherTargetId::new(object_id(600))), None)
    );
}

/// A bush that dies under its forager (past state 6) is let go (its worker leaves) and the
/// Gather searches (state 3): the unit AI hears 0x202 with the gather action and the target
/// type 7.
#[test]
fn forager_whose_bush_dies_lets_it_go_and_asks_for_search() {
    let mut foraging = foraging();
    let working = |world: &World| {
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state)
            == Some(GatherState::Work)
    };
    forage_until(&mut foraging, 200, working);
    assert_eq!(
        foraging.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(1)
    );
    foraging.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Removed;
    forage_until(&mut foraging, 1, |_| false);
    let unit = &foraging.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        foraging
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .map(|gather_action| (gather_action.state, gather_action.target)),
        Some((GatherState::Search, None))
    );
    assert_eq!(
        foraging.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
    let heard: Vec<_> = foraging
        .parts
        .unit_ai
        .get(unit.id)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| (notify.kind, notify.params[0], notify.params[1]))
        .collect();
    assert_eq!(
        heard,
        [(NotifyKind::REQUIRES_SEARCH, AiAction::Gather.value(), 7)]
    );
}

/// A forager whose bush went while it carried its load drops the load (the player's food) and,
/// without a target, takes the nearest bush its unit AI's search finds (`ClosestResourceObject`
/// of the Gather's target type's group, 1 for bushes, within its sight plus 5: bush 601 beside
/// the centre, state 4); with none it walks back to where it worked to search again (state 11).
#[test]
fn forager_without_its_bush_after_the_drop_takes_the_next_one() {
    let carrying = |near: bool| {
        let mut foraging = foraging();
        for masters in &mut foraging.model.masters {
            let bush = Arc::make_mut(masters).get_mut(master_id(59)).unwrap();
            (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FORAGE);
            Arc::make_mut(masters)
                .get_mut(master_id(120))
                .unwrap()
                .line_of_sight = Distance::new(4.0);
        }
        foraging
            .parts
            .unit_ai
            .get_mut(foraging.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .target_class = None;
        if near {
            foraging.players[0].objects[ListIndex::new(1)]
                .place_for_test(Position::new(25.5, 89.5, 0.0));
            let placed = *foraging.spatial.get(object_id(601)).unwrap();
            foraging.spatial.insert(
                object_id(601),
                Placed {
                    position: Position::new(25.5, 89.5, 0.0),
                    ..placed
                },
            );
        }
        let carries = |world: &World| {
            world
                .parts
                .actions_of(world.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .map(|gather_action| gather_action.state.bare())
                == Some(GatherState::Drop { move_to: None })
        };
        forage_until(&mut foraging, 800, carries);
        let _ = foraging
            .remove_live(PlayerId::new(0), ListIndex::new(0))
            .unwrap();
        foraging.spatial.remove(object_id(600));
        let dropped = |world: &World| {
            world
                .parts
                .actions_of(world.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .map(|gather_action| gather_action.state.bare())
                != Some(GatherState::Drop { move_to: None })
        };
        forage_until(&mut foraging, 400, dropped);
        assert_eq!(foraging.players[1].attributes[0], Amount::new(10.0));
        foraging
            .parts
            .actions_of(foraging.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| (gather_action.state.bare(), gather_action.target))
    };
    assert_eq!(
        carrying(true),
        Some((
            GatherState::Walk { move_to: None },
            Some(GatherTargetId::new(object_id(601)))
        ))
    );
    assert_eq!(
        carrying(false),
        Some((GatherState::WalkBack { move_to: None }, None))
    );
}

/// [`foraging`] up to the forager's drop at its town centre without a target (bush 600 gone
/// on the way, `setup` run before), after which the Gather's state follows.
fn dropped_without_bush(setup: impl FnOnce(&mut World)) -> World {
    let mut foraging = foraging();
    let carries = |world: &World| {
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state.bare())
            == Some(GatherState::Drop { move_to: None })
    };
    forage_until(&mut foraging, 800, carries);
    let _ = foraging
        .remove_live(PlayerId::new(0), ListIndex::new(0))
        .unwrap();
    foraging.spatial.remove(object_id(600));
    setup(&mut foraging);
    let dropped = |world: &World| {
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state.bare())
            != Some(GatherState::Drop { move_to: None })
    };
    forage_until(&mut foraging, 400, dropped);
    assert_eq!(foraging.players[1].attributes[0], Amount::new(10.0));
    foraging
}

/// After the drop without a target and nothing found: a Gather without its point idles for a
/// human player (state 2, its targets and point cleared, the centre let go) and is done for a
/// computer player; with orders queued (on a target the world holds) the unit AI ends its
/// order and the Gather is done. An order on a target gone leaves the queue first.
#[test]
fn forager_with_nothing_found_after_the_drop_idles_or_is_done() {
    let pointless = |world: &mut World| {
        world
            .parts
            .actions
            .of_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .gather_mut()
            .unwrap()
            .point = Position::new(-1.0, -1.0, -1.0);
    };
    let pointless_world = dropped_without_bush(pointless);
    let gather_action = pointless_world
        .parts
        .actions_of(pointless_world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.carry_to,
            gather_action.point
        ),
        (GatherState::Idle, None, None, Position::from([-1.0; 3]))
    );
    assert_eq!(
        pointless_world.players[1].objects[ListIndex::new(1)].workers,
        WorkerCount::of_data(0)
    );
    let dropped_without_bush = dropped_without_bush(|world| {
        pointless(world);
        world.players[1].kind = PlayerKind::Computer;
    });
    assert!(
        dropped_without_bush
            .parts
            .actions_of(dropped_without_bush.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .is_none()
    );
    let order = |target| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Gather,
        priority: OrderPriority::of_data(100),
        target,
        target_owner: owner(0),
        point: Position::new(36.5, 89.5, 0.0),
        range: Distance::ZERO,
        shown: Point::new(36.5, 89.5),
        ready: true,
        target_kind: order_kind(7),
    };
    // the Gather's state 8 after the deposit, its bush gone, two orders queued
    let mut world = foraging();
    let _ = world
        .remove_live(PlayerId::new(0), ListIndex::new(0))
        .unwrap();
    world.spatial.remove(object_id(600));
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .target = None;
    world.parts.unit_ai.get_mut(players.id).unwrap().orders =
        vec![order(Some(object_id(600))), order(Some(object_id(601)))];
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let gather = *actions.list.gather().unwrap();
            let code = unit.gather_after_drop(gather, actions, step, &roles())?;
            unit.end_head_update(actions, step, code)
        },
    )
    .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert!(world.parts.actions_of(players.id).list.gather().is_none());
    let ai = world.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        ai.orders
            .iter()
            .map(|queued_order| queued_order.target)
            .collect::<Vec<_>>(),
        [Some(object_id(601))]
    );
    assert_eq!((ai.current_order, ai.current_action), (None, None));
}

/// The forager's Gather update alone (`gather_update`, 50 ms).
fn gather_step(world: &mut World) -> GenieResult<()> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let gather = *actions.list.gather().unwrap();
            let code = unit.gather_update(gather, actions, step, me, &roles())?;
            unit.end_head_update(actions, step, code)
        },
    )
}

/// What follows the Gather's update ([`Unit::gather_next`]) for the forager on bush 600 with
/// its town centre 700, each worked by one: the end lets both go and removes the Gather; a
/// target the update changed lets the old one go, where the update does; the reseed after the
/// walk back, without a farm to reseed, idles the human player's gatherer and lets both targets
/// go within the update (the build command:
/// [`the_unit_ai_orders_reach_the_unit_and_the_units_they_name`]).
#[test]
fn what_follows_the_gather_update_lets_go_builds_or_reseeds() {
    use crate::gathering::gather::Next;
    type Followed = (
        GenieResult<()>,
        Vec<i32>,
        Option<(Option<ObjectId>, GatherState)>,
    );
    let follow = |next: Next, old: (Option<ObjectId>, Option<DropSiteId>)| -> (World, Followed) {
        let mut foraging = foraging();
        foraging
            .parts
            .actions
            .of_mut(foraging.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .gather_mut()
            .unwrap()
            .carry_to = Some(DropSiteId::new(object_id(700)));
        for id in [600, 700] {
            foraging.object_mut(object_id(id)).unwrap().1.workers = WorkerCount::of_data(1);
        }
        let seen = in_update(
            &mut foraging,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let by = unit.releaser(step)?;
                let mut gather = *actions.list.gather().unwrap();
                let result = unit
                    .gather_next(
                        (&mut gather, actions),
                        step,
                        next,
                        (old.0.map(GatherTargetId::new), old.1),
                        by,
                        &roles(),
                    )
                    .and_then(|code| unit.end_head_update(actions, step, code));
                // the objects let go, as the update leaves them
                let released = [600, 700]
                    .into_iter()
                    .filter(|&id| {
                        OthersRef::of(&step.world.players, &step.world.index, step.player)
                            .unwrap()
                            .find(object_id(id))
                            .is_some_and(|(_, unit)| unit.workers == WorkerCount::of_data(0))
                    })
                    .collect();
                let gather = actions.list.gather().map(|gather_action| {
                    (
                        gather_action.target.map(GatherTargetId::id),
                        gather_action.state,
                    )
                });
                Ok((result, released, gather))
            },
        )
        .unwrap();
        (foraging, seen)
    };
    let (world, seen) = follow(
        Next::Done,
        (Some(object_id(600)), Some(DropSiteId::new(object_id(700)))),
    );
    assert_eq!(seen, (Ok(()), vec![600, 700], None));
    assert!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .is_none()
    );
    let (world, seen) = follow(Next::Stay, (Some(object_id(700)), None));
    assert_eq!(seen.0, Ok(()));
    assert_eq!(seen.1, vec![700]);
    assert!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .is_some()
    );
    let (_, seen) = follow(
        Next::Stay,
        (Some(object_id(600)), Some(DropSiteId::new(object_id(600)))),
    );
    assert_eq!((seen.0, seen.1), (Ok(()), vec![600]));
    let (_, seen) = follow(
        Next::Reseed {
            end: WalkEnd::Arrived,
        },
        (Some(object_id(600)), Some(DropSiteId::new(object_id(700)))),
    );
    assert_eq!(
        seen,
        (Ok(()), vec![600, 700], Some((None, GatherState::Idle)))
    );
}

/// The Gather's other states (`0x140a8e1d0`): done, it goes and lets its bush go; the search's
/// wait (case 2) at its end asks the unit AI again (0x202) and waits 10 seconds more; the wait
/// after a failed walk (case 3) at its end walks back to the Gather's point to search there
/// (state 11).
#[test]
fn the_gather_update_ends_asks_again_and_walks_back() {
    let set = |state, timer| {
        let mut foraging = foraging();
        let gather_action = foraging
            .parts
            .actions
            .of_mut(foraging.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .list
            .gather_mut()
            .unwrap();
        (gather_action.state, gather_action.timer) = (state, timer);
        foraging
    };
    let mut world = set(GatherState::Done, Seconds::ZERO);
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(1)
    );
    gather_step(&mut world).unwrap();
    assert!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .is_none()
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(0)
    );
    let mut world = set(GatherState::Search, Seconds::new(-0.02));
    gather_step(&mut world).unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .map(|gather_action| gather_action.timer),
        Some(Seconds::new(-10.0))
    );
    let heard: Vec<NotifyKind> = world
        .parts
        .unit_ai
        .get(unit.id)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| notify.kind)
        .collect();
    assert_eq!(heard, [NotifyKind::REQUIRES_SEARCH]);
    let mut world = set(GatherState::Search, Seconds::new(-0.2));
    gather_step(&mut world).unwrap();
    assert!(
        (world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .copied()
            .unwrap()
            .timer
            + Seconds::new(0.15))
        .get()
        .abs()
            < 1e-6
    );
    let mut world = set(GatherState::Walk { move_to: None }, Seconds::new(-0.02));
    gather_step(&mut world).unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state.bare()),
        Some(GatherState::WalkBack { move_to: None })
    );
}

/// [`forage_world`]'s forager (player 1's object 0) building house 10028 (master 70, at (30, 88),
/// player 1's object 2) by the build command; the town centre 700 is complete in its tracker
/// and the drop-site table's town centre takes the four basic resources.
fn house_builder() -> World {
    let mut world = forage_world();
    let placed = *world.spatial.get(object_id(700)).unwrap();
    let site = Placed {
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: ClassId::of_master(80),
        master: ids::HOUSE,
        ..placed
    };
    world.spatial.insert(object_id(10028), site);
    let _ = world
        .build_command(
            PlayerId::new(1),
            ListIndex::new(0),
            FoundationId::new(object_id(10028)),
        )
        .unwrap();
    let key = key(
        world.model.masters.get(1).map(|masters| &**masters),
        &world.model.lines,
        ids::TOWN_CENTRE,
        &roles(),
    );
    world.players[1].tracker.add(object_id(700), key);
    world.players[1].tracker.complete(object_id(700), key);
    Arc::make_mut(&mut world.model.drop_sites)[0].resource_kinds = attribute::RESOURCES.to_vec();
    world
}

/// The Build's drop-off (`set_state(8)` from its search, [`BuildAction::drop_again`]) of the
/// villager building foundation 10028 while it carries `held`, its town centre a done drop site
/// of the table (resources 0 to 3 and berries): Ok with the Build's state, its site and range,
/// the centre's workers and what the unit AI heard.
fn drop_again_with(held: Held) -> (BuildState, Option<ObjectId>, f32, i32, Vec<NotifyKind>) {
    let mut world = house_builder();
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    (players.life.attribute_kind, players.life.amount) = (held.kind, held.amount);
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let mut build = *actions.list.build().unwrap();
            let by = unit.releaser(step)?;
            let dropped = build.drop_again(step, unit, by, slots(), &roles());
            *actions.list.build_mut().unwrap() = build;
            dropped
        },
    )
    .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    let build_action = world
        .parts
        .actions_of(players.id)
        .list
        .build()
        .copied()
        .unwrap();
    let heard = world
        .parts
        .unit_ai
        .get(players.id)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| notify.kind)
        .collect();
    (
        build_action.state.bare(),
        build_action.site.map(DropSiteId::id),
        build_action
            .state
            .move_to()
            .copied()
            .map_or(0.0, |move_to_action| move_to_action.range.get()),
        world.players[1].objects[ListIndex::new(2)].workers.get(),
        heard,
    )
}

/// The Build's drop-off: a load the table's town centre takes makes it the Build's site (taken,
/// one worker) with the range of the task it gives (the forager's Gather on berries, 0.25; none
/// for stone: the Build task's, 0); an empty builder walks back to its foundation (state 4); a
/// load no drop site takes leaves no site and tells the unit AI (0x203).
#[test]
fn loaded_builder_drop_off_finds_its_town_centre() {
    assert_eq!(
        drop_again_with(carrying(Some(AttributeId::named(17)), 6.0)),
        (
            BuildState::Drop { move_to: None },
            Some(object_id(700)),
            0.25,
            1,
            vec![]
        )
    );
    assert_eq!(
        drop_again_with(carrying(Some(attribute::STONE), 6.0)),
        (
            BuildState::Drop { move_to: None },
            Some(object_id(700)),
            0.0,
            1,
            vec![]
        )
    );
    assert_eq!(
        drop_again_with(carrying(None, 0.0)),
        (BuildState::Move { move_to: None }, None, 0.0, 0, vec![])
    );
    assert_eq!(
        drop_again_with(carrying(Some(AttributeId::named(5)), 6.0)),
        (
            BuildState::Move { move_to: None },
            None,
            0.0,
            0,
            vec![NotifyKind::NO_DROP_SITE]
        )
    );
}

/// [`forage_world`]'s forager 9175 (at 29.5) seeking a drop site from itself
/// ([`Unit::drop_site_with`] with the search's arguments 10, 9 and 8 `args`) on a map with a tile
/// planner, a second town centre 701 at 45.0 beside centre 700 at 20.0; `setup` changes the
/// world first.
fn drop_site_sought(
    setup: impl FnOnce(&mut World),
    args: SiteSearch,
) -> GenieResult<Option<DropSiteId>> {
    let mut forage_world = forage_world();
    let mut far = forage_world.object_of(&forage_world.players[1].objects[ListIndex::new(2)]);
    far.unit.id = object_id(701);
    far.unit.place_for_test(far.unit.position().with_x(45.0));
    let _ = forage_world.push_live(PlayerId::new(1), far).unwrap();
    let placed = *forage_world.spatial.get(object_id(700)).unwrap();
    forage_world.spatial.insert(
        object_id(701),
        Placed {
            position: Position::new(45.0, 89.5, 0.0),
            ..placed
        },
    );
    forage_world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    forage_world.build_mesh().unwrap();
    setup(&mut forage_world);
    let mut world = DropWorld {
        top_speed: TopSpeed::Own,
        masters: OwnerMasters::new(
            &forage_world.parts.own_master,
            forage_world.model.masters.get(1).map(|masters| &**masters),
        ),
        spatial: &mut forage_world.spatial,
        zones: &mut forage_world.zones,
        pathing: &mut forage_world.pathing,
        object: forage_world.streams.get_mut(Stream::Object),
        world_time: WorldTime::ZERO,
        player_kind: PlayerKind::Human,
        groups: &forage_world.groups,
    };
    let objects =
        OthersRef::of(&forage_world.players, &forage_world.index, PlayerId::new(1)).unwrap();
    let unit = &forage_world.players[1].objects[ListIndex::new(0)];
    unit.drop_site_with(
        (
            forage_world.parts.actions_of(unit.id),
            forage_world.parts.motion.mover(unit.id),
        ),
        &mut world,
        objects,
        ActionState::NO_ACTION,
        args,
        objects_ref::ObjectsRef::new(
            &forage_world.players,
            &forage_world.index,
            &forage_world.model.masters,
            &forage_world.parts,
            &forage_world.model.roles,
            &forage_world.model.tuning,
        ),
    )
}

/// The drop-site search from a unit (`FUN_1411671e0`): the nearer town centre wins by path (its
/// length within 10 tiles measured by the short path too) and by the squared distance of the
/// centres; a centre inside another object is passed over when the search keeps outside, one at
/// work on the searched action type always (an object of type 40 up), so the far centre wins.
/// A drop site of a master the player does not hold fails.
#[test]
fn drop_site_search_from_unit_takes_the_nearest_it_reaches() {
    use crate::gathering::make::{MAKE, MakeAction};
    let keep = |_: &mut World| ();
    assert_eq!(
        drop_site_sought(
            keep,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByPath,
                at_work: None,
            }
        ),
        Ok(Some(DropSiteId::new(object_id(700))))
    );
    assert_eq!(
        drop_site_sought(
            keep,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByDistance,
                at_work: None,
            }
        ),
        Ok(Some(DropSiteId::new(object_id(700))))
    );
    let inside = |world: &mut World| {
        world.players[1].objects[ListIndex::new(2)].life.inside =
            Some(ContainerId::new(object_id(9175)));
    };
    assert_eq!(
        drop_site_sought(
            inside,
            SiteSearch {
                place: SitePlace::Outside,
                nearest: DropMeasure::ByPath,
                at_work: None,
            }
        ),
        Ok(Some(DropSiteId::new(object_id(701))))
    );
    assert_eq!(
        drop_site_sought(
            inside,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByPath,
                at_work: None,
            }
        ),
        Ok(Some(DropSiteId::new(object_id(700))))
    );
    let making = |world: &mut World| {
        world
            .parts
            .actions
            .of_mut(world.players[1].objects[ListIndex::new(2)].id)
            .unwrap()
            .list
            .set_make(Some(MakeAction {
                state: MakeState::Making,
                remaining: Amount::ZERO,
                total: Amount::ZERO,
                task: Task::default(),
            }));
    };
    assert_eq!(
        drop_site_sought(
            making,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByPath,
                at_work: Some(MAKE),
            }
        ),
        Ok(Some(DropSiteId::new(object_id(701))))
    );
    assert_eq!(
        drop_site_sought(
            making,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByPath,
                at_work: None,
            }
        ),
        Ok(Some(DropSiteId::new(object_id(700))))
    );
    let unknown = |world: &mut World| {
        for masters in &mut world.model.masters {
            Arc::make_mut(masters)
                .get_mut(master_id(120))
                .unwrap()
                .action
                .get_or_insert_with(Default::default)
                .drop_sites = vec![ids::TOWN_CENTRE, master_id(999)];
        }
        world.players[1].objects[ListIndex::new(2)].life.master_id = master_id(999);
    };
    assert_eq!(
        drop_site_sought(
            unknown,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByPath,
                at_work: None,
            }
        )
        .unwrap_err()
        .reason(),
        OBJECT_MASTER.reason()
    );
}

/// [`forage_world`]'s villager 9175 with its own `MoveTo` `move_to_action` on a map with a tile
/// planner.
fn own_move_world(move_to_action: MoveToAction) -> World {
    let mut world = forage_world();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_move_to(Some(move_to_action));
    world
}

/// What [`own_move`] gives: the part's result, the `MoveTo` after it, what the unit AI heard
/// and bush 600's workers after the step's releases.
type OwnMoved = (
    GenieResult<()>,
    Option<MoveToAction>,
    Vec<(NotifyKind, [i32; 3])>,
    i32,
);

/// The villager's update's `MoveTo` part ([`Unit::move_to_part`]) in `world`.
fn own_move(world: &mut World) -> OwnMoved {
    let in_update = in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .list
                    .move_to()
                    .map_or(ActionState::NO_ACTION, |move_to_action| {
                        ActionState::new(move_to_action.state.number())
                    }),
                top_speed: TopSpeed::Own,
            };
            let code = unit.move_to_part(actions, step, me, &roles())?;
            unit.end_head_update(actions, step, code)
        },
    );
    let players = &world.players[1].objects[ListIndex::new(0)];
    let heard = world
        .parts
        .unit_ai
        .get(players.id)
        .map_or_else(Vec::new, |ai| {
            ai.notifies
                .iter()
                .map(|notify| (notify.kind, notify.params))
                .collect()
        });
    (
        in_update,
        world.parts.actions_of(players.id).list.move_to().copied(),
        heard,
        world.players[0].objects[ListIndex::new(0)].workers.get(),
    )
}

/// The unit's own `MoveTo` in its update (`0x140aa2a70`): a start toward bush 600 asks for its
/// path and moves; one that arrived tells the unit AI (0x1fa with the move action value, which
/// an AI on another action ignores), lets its target go (one worker less) and goes; a failed
/// one tells 0x1f9 and goes. A target that
/// left the point the walk goes to (more than half a tile) makes it tell 0x1fc.
#[test]
fn unit_own_move_to_walks_tells_its_end_and_lets_its_target_go() {
    let to_bush = |state| MoveToAction {
        state,
        point: Position::ORIGIN,
        target: Some(object_id(600)),
        range: Distance::new(0.25),
        sprite: None,
        voice: Voice::Loud,
        on_target_moved: TargetMoved::Follows,
        wait: MoveWait::default(),
    };
    let mut game = own_move_world(to_bush(MoveTo::Start));
    let (result, move_to, heard, _) = own_move(&mut game);
    assert_eq!(
        (
            result,
            move_to.map(|move_to_action| move_to_action.state),
            heard
        ),
        (Ok(()), Some(MoveTo::Moving), vec![])
    );
    // the bush moves two tiles off the point the walk goes to
    game.spatial
        .set_position(
            object_id(600),
            Position::new(32.5, 91.5, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    let (result, _, heard, _) = own_move(&mut game);
    assert_eq!(result, Ok(()));
    assert_eq!(heard, [(NotifyKind::TARGET_MOVED, [0, 0, 0])]);
    let arrived = |action| {
        let mut world = own_move_world(to_bush(MoveTo::Arrived));
        world.players[0].objects[ListIndex::new(0)].workers = WorkerCount::of_data(1);
        world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .current_action = action;
        own_move(&mut world)
    };
    let (result, move_to_action, heard, workers) = arrived(Some(AiAction::Move));
    assert_eq!((result, move_to_action, workers), (Ok(()), None, 0));
    assert_eq!(
        heard,
        [(NotifyKind::ACTION_COMPLETED, [AiAction::Move.value(), 0, 0])]
    );
    // the unit AI on another action ignores the move's completion (FUN_140c556d0)
    assert_eq!(arrived(Some(AiAction::Gather)).2, []);
    let to_point = MoveToAction {
        point: Position::new(40.5, 89.5, 0.0),
        target: None,
        ..to_bush(MoveTo::Failed)
    };
    let (result, move_to_action, heard, _) = own_move(&mut own_move_world(to_point));
    assert_eq!((result, move_to_action), (Ok(()), None));
    assert_eq!(
        heard,
        [(NotifyKind::ACTION_FAILED, [AiAction::Move.value(), 0, 0])]
    );
}

/// The result of a bird's Fly part, its Fly after it and what its unit AI heard.
type Flown = (GenieResult<()>, Option<Fly>, Vec<(NotifyKind, [i32; 3])>);

/// [`forage_world`]'s villager 9175 as a bird flying (state 4) toward (40.5, 89.5) with its
/// `MoveTo` in `move_to`, two seconds of flight left and a rest of two seconds; `setup`
/// changes the unit, then its update's Fly part runs ([`Unit::fly_part`]): the result, the Fly
/// after it and what its unit AI heard.
fn flown(move_to: MoveTo, setup: impl FnOnce(&mut World)) -> Flown {
    let mut world = forage_world();
    let task = Task {
        wait_time: Seconds::new(2.0),
        ..Task::default()
    };
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .set_fly(Some(Fly {
            state: FlyState::Flying {
                move_to: Some(MoveToAction {
                    state: move_to,
                    voice: Voice::Loud,
                    ..MoveToAction::sub_to_point(
                        Position::new(40.5, 89.5, 0.0),
                        Distance::ZERO,
                        None,
                    )
                }),
            },
            timer: Seconds::new(2.0),
            home: Point::new(29.5, 89.5),
            task,
        }));
    setup(&mut world);
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: ActionState::new(4),
                top_speed: TopSpeed::Own,
            };
            unit.fly_part(actions, step, me)
        },
    );
    let players = &world.players[1].objects[ListIndex::new(0)];
    let heard = world
        .parts
        .unit_ai
        .get(players.id)
        .map_or_else(Vec::new, |ai| {
            ai.notifies
                .iter()
                .map(|notify| (notify.kind, notify.params))
                .collect()
        });
    (
        in_update,
        world.parts.actions_of(players.id).list.fly().copied(),
        heard,
    )
}

/// A bird's Fly in its update: a failed `MoveTo` ends the flight, its failure told to the unit
/// AI (0x1f9 with the move action value); the bird stops and rests (state 2) for the flight's
/// time left and the rest; a bird without movement state is not modelled.
#[test]
fn bird_failed_flight_tells_its_unit_ai_and_rests() {
    let (result, fly, heard) = flown(MoveTo::Failed, |_| ());
    assert_eq!(result, Ok(()));
    let fly = fly.unwrap();
    assert_eq!(fly.state, FlyState::Resting);
    assert_eq!(
        fly.timer,
        Seconds::new(2.0) - Millis::new(50).seconds() + Seconds::new(2.0)
    );
    assert_eq!(
        heard,
        [(NotifyKind::ACTION_FAILED, [AiAction::Move.value(), 0, 0])]
    );
    let (result, _, _) = flown(MoveTo::Failed, |world| {
        let id = world.players[1].objects[ListIndex::new(0)].id;
        world.parts.put_mover(id, None);
    });
    assert_eq!(
        result.unwrap_err().reason(),
        "a bird without movement state"
    );
}

/// [`forage_world`]'s villager 9175 with an Explore toward (40.5, 89.5) on a map with a tile
/// planner, its Explore changed by `set`, through its update's Explore part
/// ([`Unit::explore_part`]): the result, the Explore after it and what its unit AI heard.
fn explored(set: impl FnOnce(&mut ExploreAction)) -> Explored {
    let mut world = forage_world();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    let (mut explore, _) = ExploreAction::new(
        Position::new(40.5, 89.5, 0.0),
        &bare_walk(),
        &tuning().move_to,
    )
    .unwrap();
    set(&mut explore);
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_explore(Some(explore));
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: ActionState::new(ExploreState::Walk { move_to: None }.get()),
                top_speed: TopSpeed::Own,
            };
            unit.explore_part(actions, step, me, &roles())
        },
    );
    let players = &world.players[1].objects[ListIndex::new(0)];
    let heard = world
        .parts
        .unit_ai
        .get(players.id)
        .map_or_else(Vec::new, |ai| {
            ai.notifies
                .iter()
                .map(|notify| (notify.kind, notify.params))
                .collect()
        });
    (
        in_update,
        world.parts.actions_of(players.id).list.explore().copied(),
        heard,
    )
}

/// The Explore's `MoveTo` shows its sprite as it starts (`0x140a87e30` gives `FUN_140aa0d30`
/// the master's walking sprite, an Explore having no task; `0x140aa29e0`'s `set_state(4)` shows
/// it before the motion step).
#[test]
fn an_explore_move_to_shows_the_master_walking_sprite_as_it_starts() {
    let mut world = forage_world();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    let walking = Graphic {
        id: SpriteId::named(31),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let _ = Arc::make_mut(&mut world.model.graphics).insert(SpriteId::named(31), walking);
    let master = world.players[1].objects[ListIndex::new(0)].life.master_id;
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(master)
        .unwrap()
        .moving
        .get_or_insert_with(Default::default)
        .move_sprite = Some(SpriteId::named(31));
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            unit.explore_to(actions, Point::new(40.5, 89.5), step, &roles())?;
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: ActionState::new(ExploreState::Walk { move_to: None }.get()),
                top_speed: TopSpeed::Own,
            };
            unit.explore_part(actions, step, me, &roles())
        },
    );
    assert_eq!(in_update, Ok(()));
    let players = &world.players[1].objects[ListIndex::new(0)];
    let move_to = world
        .parts
        .actions_of(players.id)
        .list
        .explore()
        .and_then(|explore_action| explore_action.state.move_to().copied());
    assert_eq!(
        move_to.map(|move_to_action| (
            move_to_action.state,
            move_to_action.sprite.map(|graphic| graphic.id)
        )),
        Some((MoveTo::Moving, Some(SpriteId::named(31))))
    );
    assert_eq!(
        players.sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(31))
    );
}

/// What [`explored`] gives: the part's result, the Explore after it and the notifies heard.
type Explored = (
    GenieResult<()>,
    Option<ExploreAction>,
    Vec<(NotifyKind, [i32; 3])>,
);

/// The Explore's part of the update (`0x140a880c0`): in state 11 its quiet `MoveTo` to the
/// point starts (code 0: it walks on). A `MoveTo` that ended, or one that failed (code 1 or 3),
/// makes it wait for a new point (state 3): the unit stops and its unit AI hears 0x202 with the
/// explore action value 605. A waiting Explore does nothing; a done (1) or failed (13) one is
/// not modelled.
#[test]
fn an_explore_walks_to_its_point_then_asks_for_the_next() {
    use crate::movement::explore::ExploreState;
    let (outcome, explore, heard) = explored(|_| ());
    assert_eq!(outcome, Ok(()));
    let explore = explore.unwrap();
    assert_eq!(explore.state.get(), 11);
    assert_eq!(
        explore
            .state
            .move_to()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Moving)
    );
    assert_eq!(heard, []);
    let asked = vec![(
        NotifyKind::REQUIRES_SEARCH,
        [AiAction::Explore.value(), 0, 0],
    )];
    for end in [MoveTo::Arrived, MoveTo::Failed] {
        let (result, explore_action, heard) = explored(|explore_action| {
            if let Some(move_to_action) = explore_action.state.move_to_mut() {
                move_to_action.state = end;
            }
        });
        assert_eq!(result, Ok(()));
        let explore_action = explore_action.unwrap();
        assert_eq!(explore_action.state, ExploreState::Wait);
        assert_eq!(heard, asked);
    }
    let (outcome, explore, heard) = explored(|explore_action| {
        let _ = explore_action.wait_for_point();
    });
    assert_eq!(
        (
            outcome,
            explore.map(|explore_action| explore_action.state),
            heard
        ),
        (Ok(()), Some(ExploreState::Wait), vec![])
    );
    for (state, reason) in [
        (
            ExploreState::Done,
            "a done Explore (the action list after it)",
        ),
        (
            ExploreState::Failed,
            "a failed Explore (notify 0x1f9, set_state 2)",
        ),
    ] {
        let (result, _, _) = explored(|explore_action| explore_action.state = state);
        assert_eq!(result.unwrap_err().reason(), reason);
    }
}

/// A hunter back from its drop-off without a deer (`hunt.target` -1, the deer it hunted let
/// go): both deer seen by player 1, the hunter with a sight of 4, the mesh built. `setup` changes
/// the hunter and its unit AI before [`Unit::hunt_after_drop`] runs.
fn hunted_without_deer(setup: impl FnOnce(&mut Action, &mut UnitAi)) -> World {
    let mut world = hunt_world();
    world.spatial.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([3, 0])]);
    world.spatial.unit_diplomacy = ByPlayer::new(vec![relations([1, 4]), relations([4, 1])]);
    for masters in &mut world.model.masters {
        let master = Arc::make_mut(masters).get_mut(master_id(122)).unwrap();
        (
            master.line_of_sight,
            master
                .action
                .get_or_insert_with(Default::default)
                .search_radius,
        ) = (Distance::new(4.0), Distance::new(4.0));
    }
    world
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    world.build_mesh().unwrap();
    let mut registry = Registry::new(2, 65);
    for (id, x) in [(500, 32.5), (501, 36.5)] {
        registry
            .entries
            .get_or_insert_with(object_id(id), Default::default)
            .unwrap()
            .mask = VisionBits::from_bits_retain(1 << 1);
        registry
            .update_unit(UnitUpdate {
                id: object_id(id),
                owner: PlayerId::new(0),
                position: Point::new(x, 89.5),
                radii: Footprint::new(0.2, 0.2),
                class: class::PREY,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1 << 1),
            })
            .unwrap();
    }
    let sight = world.spatial.sight.as_mut().unwrap();
    sight.registry = Some(registry);
    for id in [500, 501] {
        let deer = Seer {
            player: PlayerId::new(0),
            master: master_id(65),
            los: Distance::new(2.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: false,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::COMBAT,
        };
        sight.add_seer(object_id(id), deer);
    }
    world.players[0].objects[ListIndex::new(0)].workers = WorkerCount::of_data(0);
    let players = &world.players[1].objects[ListIndex::new(0)];
    let at = players.position();
    world.parts.mover_mut(players.id).unwrap().moved_at = at;
    let actions = world.parts.actions.of_mut(players.id).unwrap();
    actions.list.hunt_mut().unwrap().target = None;
    let ai = world.parts.unit_ai.get_mut(players.id).unwrap();
    ai.target_class = None;
    setup(actions, ai);
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let mut hunt = *actions.list.hunt().unwrap();
            let code = unit.hunt_after_drop(&mut hunt, actions, step, &roles())?;
            unit.end_head_update(actions, step, code)
        },
    )
    .unwrap();
    world
}

/// The hunt's state, target and drop site after [`hunted_without_deer`], and the two deer's
/// workers.
fn hunt_after(world: &World) -> (Option<(HuntState, i32, i32)>, [i32; 2]) {
    let found = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied();
    let deer = &world.players[0].objects;
    (
        found.map(|hunt_action| {
            (
                hunt_action.state.bare(),
                ObjectId::field(hunt_action.target.map(HuntTargetId::id)),
                ObjectId::field(hunt_action.carry_to.map(DropSiteId::id)),
            )
        }),
        [
            deer[ListIndex::new(0)].workers.get(),
            deer[ListIndex::new(1)].workers.get(),
        ],
    )
}

/// Without a deer after the drop the hunter's `ClosestObject` over its search (class 9, every
/// relation) finds the nearer deer, which the hunt takes and attacks (`FUN_140ac7a00`,
/// `LAB_140ac9509`'s other way).
#[test]
fn hunter_without_its_deer_after_the_drop_takes_the_closest_one() {
    let world = hunted_without_deer(|_, _| ());
    assert_eq!(
        hunt_after(&world),
        (Some((HuntState::Attack { attack: None }, 500, -1)), [1, 0])
    );
}

/// A queued order on a target the world still holds ends the hunt after the drop, so the unit AI
/// runs it; one on a target gone leaves the queue (`FUN_140c5de60`) and the search runs.
#[test]
fn hunter_with_queued_order_after_the_drop_ends_its_hunt() {
    let order = |target: i32| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Hunt,
        priority: OrderPriority::of_data(100),
        target: ObjectId::of_field(target),
        target_owner: owner(0),
        point: Position::new(36.5, 89.5, 0.0),
        range: Distance::ZERO,
        shown: Point::new(36.5, 89.5),
        ready: true,
        target_kind: order_kind(9),
    };
    let held = hunted_without_deer(|_, ai| ai.orders = vec![order(501)]);
    assert_eq!(hunt_after(&held), (None, [0, 0]));
    let gone = hunted_without_deer(|_, ai| ai.orders = vec![order(999)]);
    assert_eq!(
        gone.parts
            .unit_ai
            .get(gone.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .orders
            .len(),
        0
    );
    assert_eq!(
        hunt_after(&gone),
        (Some((HuntState::Attack { attack: None }, 500, -1)), [1, 0])
    );
}

/// A hunt of class 10 searches nothing after the drop, and a wood search finds no tree: either
/// walks back to its point to search again (`set_state(11)`).
#[test]
fn hunter_that_finds_nothing_after_the_drop_walks_to_its_point() {
    for class in [class::PREDATOR, class::TREE] {
        let world = hunted_without_deer(|actions, _| {
            actions.list.hunt_mut().unwrap().target_class = Some(class);
        });
        assert_eq!(
            hunt_after(&world),
            (
                Some((HuntState::WalkSearch { move_to: None }, -1, -1)),
                [0, 0]
            ),
            "class {class}"
        );
        let hunt_action = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .hunt()
            .copied()
            .unwrap();
        assert_eq!(hunt_action.point, Position::new(32.5, 89.5, 0.0));
    }
}

/// How often player `player` watches object `id`.
fn watches(world: &World, player: usize, id: i32) -> usize {
    world.players[player]
        .watched
        .iter()
        .filter(|watched| watched.id == object_id(id))
        .count()
}

/// A site's state, hit points and build points.
fn site_of(world: &World, unit: &Unit) -> (ObjectState, f32, Option<f32>) {
    (
        unit.life.state,
        unit.life.hp.get(),
        world
            .parts
            .building_of(unit.id)
            .construction
            .map(|construction| construction.progress.get()),
    )
}

/// [`house_builder`]'s house 10028 with an annex: a second house foundation 10030 (player 1's
/// last object) in its owner's pending tracker list, linked to the house as the game links an
/// annex (`+0xbc0`, `+0xbb8`).
fn annexed_house() -> World {
    let mut world = house_builder();
    let mut annex = world.object_of(world.players[1].objects.by_id(object_id(10028)).unwrap());
    annex.unit.id = object_id(10030);
    annex.parts.building.annex_of = Some(MainBuildingId::new(object_id(10028)));
    let _ = world.push_live(PlayerId::new(1), annex).unwrap();
    let placed = *world.spatial.get(object_id(10028)).unwrap();
    world.spatial.insert(object_id(10030), placed);
    world.link_test_annex(object_id(10030), object_id(10028));
    let key = key(
        world.model.masters.get(1).map(|masters| &**masters),
        &world.model.lines,
        ids::HOUSE,
        &roles(),
    );
    world.players[1].tracker.add(object_id(10030), key);
    world
}

/// Where houses `first` and `second` stand in their owner's done tracker list of master 70.
fn done_at(world: &World, first: i32, second: i32) -> (Option<usize>, Option<usize>) {
    let key = key(
        world.model.masters.get(1).map(|masters| &**masters),
        &world.model.lines,
        ids::HOUSE,
        &roles(),
    );
    let done = world.players[1]
        .tracker
        .list(ListKey::of_master(key.master));
    let at = |id| done.iter().position(|&object_id| object_id == id);
    (
        at(ObjectId::of_field(first)),
        at(ObjectId::of_field(second)),
    )
}

/// A building that completes in its own update takes its annexes to state 2 first (its
/// `set_state(2)`, `FUN_1411dbb70`: their slot 0xa0 before its own completion): the annex's
/// completion reaches the world at once, its tracker part before the building's.
#[test]
fn building_own_completion_completes_its_annexes_first() {
    let mut world = annexed_house();
    let construction = world
        .parts
        .building_mut(object_id(10028))
        .unwrap()
        .construction
        .as_mut()
        .unwrap();
    construction.progress = Seconds::of_master(construction.build_time);
    let list_index = world.players[1]
        .objects
        .position_of(object_id(10028))
        .unwrap();
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), list_index),
        50,
        |unit, actions, step| {
            unit.complete_on_update(actions, step, &roles())?;
            Ok(step
                .world
                .object(object_id(10030))
                .map(|(_, unit)| unit.life.state))
        },
    );
    assert_eq!(
        seen.map_err(|error| error.reason().to_owned()),
        Ok(Some(ObjectState::Alive))
    );
    let (annex, house) = done_at(&world, 10030, 10028);
    assert!(annex.is_some() && annex < house, "{annex:?} {house:?}");
}

/// The update of [`house_builder`]'s object `id` (player 1) as far as
/// [`Unit::let_removed_links_go`]: its annexes and its main building after it.
fn links_after_update(
    world: &mut World,
    id: ObjectId,
) -> GenieResult<(Vec<ObjectId>, Option<MainBuildingId>)> {
    let list_index = world.players[1].objects.position_of(id).unwrap();
    in_update(
        world,
        (PlayerId::new(1), list_index),
        50,
        |unit, _, step| {
            unit.let_removed_links_go(step, &roles())?;
            let layer = step.world.parts.building_of(unit.id);
            Ok((layer.annexes.ids().collect(), layer.annex_of))
        },
    )
}

/// A building's update (`0x1411c9000`) lets go its annex once the annex is past state 6
/// (removed or gone) and keeps one in state 3; an annex lets go its main building past state 6;
/// a house (the walls' branch) keeps its slots; a link to an object the world does not file
/// stops the update.
#[test]
fn building_lets_its_links_past_state_6_go() {
    let (centre, annex) = (object_id(700), object_id(10030));
    let linked = |annex_state, centre_state| {
        let mut world = house_builder();
        let mut object = world.object_of(world.players[1].objects.by_id(centre).unwrap());
        object.unit.id = annex;
        object.unit.life.state = annex_state;
        let _ = world.push_live(PlayerId::new(1), object).unwrap();
        world.link_test_annex(annex, centre);
        world.players[1]
            .objects
            .by_id_mut(centre)
            .unwrap()
            .life
            .state = centre_state;
        world
    };
    let main = Some(MainBuildingId::new(centre));
    let (alive, dying) = (ObjectState::Alive, ObjectState::Dying);
    for (state, kept) in [
        (dying, vec![annex]),
        (ObjectState::Removed, vec![]),
        (ObjectState::Gone, vec![]),
    ] {
        let mut world = linked(state, alive);
        assert_eq!(
            links_after_update(&mut world, centre).unwrap(),
            (kept, None)
        );
    }
    for (state, kept) in [(dying, main), (ObjectState::Removed, None)] {
        let mut world = linked(alive, state);
        assert_eq!(
            links_after_update(&mut world, annex).unwrap(),
            (vec![], kept)
        );
    }
    let mut annexed_house = annexed_house();
    annexed_house.players[1]
        .objects
        .by_id_mut(annex)
        .unwrap()
        .life
        .state = ObjectState::Removed;
    assert_eq!(
        links_after_update(&mut annexed_house, object_id(10028)).unwrap(),
        (vec![annex], None)
    );
    let mut world = house_builder();
    world
        .parts
        .building_mut(centre)
        .unwrap()
        .annexes
        .link(0, object_id(55_555))
        .unwrap();
    assert_eq!(
        links_after_update(&mut world, centre).map_err(|error| error.reason().to_owned()),
        Err("a building's link to an object the world does not file".to_owned())
    );
}

/// [`house_builder`]'s forager after it built town centre 700 (its Build's target in state 224,
/// the end of the work, its `MoveTo` gone; the centre built) on a map with a tile planner, its
/// update with `setup` run first: the work it found (slot 0x7e8's target and point, the
/// caller's after the update) and the notifies its unit AI heard.
fn built_the_centre(setup: impl FnOnce(&mut World)) -> (World, BuildSearched) {
    let mut world = house_builder();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    for masters in &mut world.model.masters {
        let bush = Arc::make_mut(masters).get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FORAGE);
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .line_of_sight = Distance::new(4.0);
        Arc::make_mut(masters)
            .get_mut(ids::TOWN_CENTRE)
            .unwrap()
            .line_of_sight = Distance::new(16.0);
    }
    Arc::make_mut(&mut world.model.drop_sites)[0].auto_work = true;
    let build_action = world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .build_mut()
        .unwrap();
    (build_action.target, build_action.state) = (
        Some(FoundationId::new(object_id(700))),
        BuildState::Finished,
    );
    let centre = &mut world.players[1].objects[ListIndex::new(2)];
    centre.workers = WorkerCount::of_data(1);
    let centre = centre.id;
    world.parts.building_mut(centre).unwrap().construction = Some(Construction {
        progress: Seconds::new(25.0),
        desolid: false,
        builders: BuilderCount::of_data(0),
        hit_points: MasterHitPoints::of_data(2400),
        build_time: WholeSeconds::of_data(25),
        record: Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::UNIT,
        },
        snow: Snow::None,
        sprite_override: false,
    });
    setup(&mut world);
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let _ = unit.build_part(actions, step, me)?;
            let heard = step
                .world
                .parts
                .unit_ai
                .get(unit.id)
                .unwrap()
                .notifies
                .iter();
            Ok((
                (actions.top_action_kind(), actions.top_target()),
                heard.map(|notify| notify.kind).collect(),
            ))
        },
    );
    (world, in_update)
}

/// What [`built_the_centre`]'s search left within the update: the builder's top action (its
/// type and target) and the notifies' kinds.
type BuildSearched = GenieResult<((Option<ActionKind>, Option<ObjectId>), Vec<NotifyKind>)>;

/// The search for more work around the town centre a builder finished (`0x140ab0b43`,
/// [`Unit::more_work`], within the Build's update): the table's entry for the centre (auto
/// work) takes berries, within the centre's line of sight (16 tiles) the bush at 32.5 is the
/// nearest, and the unit AI's `ClosestResourceObject` names it: the work command on it at its
/// place, within the update (the Gather replaces the Build). An entry without auto work, or a
/// line of sight below the floor (the reach 4 tiles, short of the bushes), finds nothing: the
/// Build waits without a target, lets the centre go and asks its unit AI for a search.
#[test]
fn builder_finds_more_work_around_the_building_it_finished() {
    use crate::gathering::gather::GATHER;
    let (game, found) = built_the_centre(|_| ());
    assert_eq!(found, Ok(((Some(GATHER), Some(object_id(600))), vec![])));
    assert!(
        game.parts
            .actions_of(game.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .is_none()
    );
    let off: fn(&mut World) =
        |world| Arc::make_mut(&mut world.model.drop_sites)[0].auto_work = false;
    let short: fn(&mut World) = |world| {
        for masters in &mut world.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::TOWN_CENTRE)
                .unwrap()
                .line_of_sight = Distance::ZERO;
        }
    };
    for setup in [off, short] {
        let (world, found) = built_the_centre(setup);
        assert_eq!(
            found,
            Ok((
                (Some(ActionKind::BUILD), None),
                vec![NotifyKind::REQUIRES_SEARCH]
            ))
        );
        let build_action = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .copied()
            .unwrap();
        assert_eq!(
            (build_action.state, build_action.target),
            (BuildState::Idle, None)
        );
        assert_eq!(
            world.players[1].objects[ListIndex::new(2)].workers,
            WorkerCount::of_data(0)
        );
    }
}

/// [`house_builder`]'s forager at work on foundation `site` (the Build's state 7, counted among
/// its builders, with a task of rate and value 1), the house counting one builder, `setup` run
/// first: one update of `dt` milliseconds.
fn worked(world: &mut World, site: i32, dt: u32) -> GenieResult<()> {
    let build_action = world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .build_mut()
        .unwrap();
    (build_action.target, build_action.state, build_action.added) =
        (FoundationId::of_field(site), BuildState::Place, true);
    build_action.task = BuildTask {
        range: Distance::new(0.1),
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        moving: None,
    };
    site_mut(world, object_id(10028)).builders = BuilderCount::of_data(1);
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        dt,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            unit.build_part(actions, step, me).map(drop)
        },
    )
}

/// The first work on a desolid foundation starts its construction (`FUN_1411e5670`): its owner
/// watches it once, within the builder's update, and the start's flag clears.
#[test]
fn builder_first_work_starts_the_site_its_owner_watches() {
    let mut world = house_builder();
    assert_eq!(
        worked(&mut world, 10028, 50).map_err(|error| error.reason().to_owned()),
        Ok(())
    );
    let construction = world
        .parts
        .building_of(world.players[1].objects.by_id(object_id(10028)).unwrap().id)
        .construction
        .unwrap();
    assert!(!construction.desolid);
    assert_eq!(watches(&world, 1, 10028), 1);
}

/// The work on an annex (started) whose main building (in state 0, 1 hit point) takes it
/// (`FUN_1411cc430`'s recursion, [`World::work_building`] with the builder lent): the house gets
/// the 2.5 seconds, its build points and its share of hit points, within the builder's update.
#[test]
fn builder_work_on_an_annex_goes_to_its_main_building() {
    let mut world = annexed_house();
    // the annex started: it clears no foundation under it (FUN_1411cd0d0) on the house's start
    site_mut(&mut world, object_id(10030)).desolid = false;
    assert_eq!(
        worked(&mut world, 10030, 2500).map_err(|error| error.reason().to_owned()),
        Ok(())
    );
    let house = world.players[1].objects.by_id(object_id(10028)).unwrap();
    assert_eq!(
        site_of(&world, house),
        (ObjectState::Foundation, 56.0, Some(2.5))
    );
    let annex = world.players[1].objects.by_id(object_id(10030)).unwrap();
    assert_eq!(site_of(&world, annex).2, Some(0.0));
}

/// The work that completes the house (`Construction::complete`) gives its parts at once within
/// the builder's update: the house stands in state 2, leaves its pending tracker list for its
/// done one, its owner gains its paid cost (attributes 98 and 165, `World::built`); then the
/// builder's work ends (`set_state(224)`), out of the builders.
#[test]
fn builder_completion_reaches_the_world_within_its_update() {
    let mut world = house_builder();
    let unit = world.players[1]
        .objects
        .by_id_mut(object_id(10028))
        .unwrap();
    unit.life.hp = HitPoints::new(549.0);
    let construction = site_mut(&mut world, object_id(10028));
    (construction.progress, construction.desolid) = (Seconds::new(24.99), false);
    assert_eq!(
        worked(&mut world, 10028, 50).map_err(|error| error.reason().to_owned()),
        Ok(())
    );
    let house = world.players[1].objects.by_id(object_id(10028)).unwrap();
    assert_eq!(
        (house.life.state, repairers_of(&world, house)),
        (ObjectState::Alive, 0)
    );
    let key = key(
        world.model.masters.get(1).map(|masters| &**masters),
        &world.model.lines,
        ids::HOUSE,
        &roles(),
    );
    assert!(
        world.players[1]
            .tracker
            .list(ListKey::of_master(key.master))
            .contains(&Some(object_id(10028)))
    );
    let attributes = &world.players[1].attributes;
    assert_eq!(
        (attributes[98], attributes[165]),
        (Amount::new(25.0), Amount::new(25.0))
    );
    let build_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (build_action.state, build_action.added),
        (BuildState::Finished, false)
    );
}

/// A builder's completion of its foundation takes the foundation's annexes to state 2 where it
/// runs, before the foundation's tracker part (`FUN_1411dbb70`).
#[test]
fn builder_completion_completes_the_foundation_annexes_where_it_runs() {
    let mut world = annexed_house();
    let construction = site_mut(&mut world, object_id(10028));
    (construction.progress, construction.desolid) = (Seconds::new(24.99), false);
    assert_eq!(
        worked(&mut world, 10028, 50).map_err(|error| error.reason().to_owned()),
        Ok(())
    );
    let annex = world.players[1].objects.by_id(object_id(10030)).unwrap();
    assert_eq!(annex.life.state, ObjectState::Alive);
    let (annex, house) = done_at(&world, 10030, 10028);
    assert!(annex.is_some() && annex < house, "{annex:?} {house:?}");
}

/// A building's `set_state` past 3 takes its annexes along where it runs (`FUN_1411dbb70`,
/// [`Unit::annexes_follow`]): the annex holds the state within the building's update.
#[test]
fn building_later_state_reaches_its_annexes_where_it_runs() {
    let mut world = annexed_house();
    let list_index = world.players[1]
        .objects
        .position_of(object_id(10028))
        .unwrap();
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), list_index),
        0,
        |unit, actions, step| {
            unit.annexes_follow(actions, step, ObjectState::Dying, ObjectState::Removed)?;
            Ok(step
                .world
                .object(object_id(10030))
                .map(|(_, unit)| unit.life.state))
        },
    );
    assert_eq!(
        seen.map_err(|error| error.reason().to_owned()),
        Ok(Some(ObjectState::Removed))
    );
}

/// What [`completed_house`]'s completion left within the update: the attributes its owner
/// gained and how often it watches the house.
type Completed = (Vec<(i16, f32)>, usize);

/// [`house_builder`]'s house 10028 (player 1's object 1) with its build points at its build
/// time, changed by `setup`, after its own update's completion test
/// ([`Unit::complete_on_update`]): the attributes its owner gains and how often it watches the
/// house within the update.
fn completed_house(setup: fn(&mut World)) -> (World, Completed) {
    let mut world = house_builder();
    let construction = site_mut(&mut world, object_id(10028));
    construction.progress = Seconds::of_master(construction.build_time);
    setup(&mut world);
    let in_update = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(1)),
        50,
        |unit, actions, step| {
            let before = attribute_values(step.world, 1);
            unit.complete_on_update(actions, step, &roles())?;
            Ok((
                gained(step.world, 1, &before),
                watches(step.world, 1, 10028),
            ))
        },
    )
    .unwrap();
    (world, in_update)
}

/// A building in state 0 whose build points reached its build time completes in its own update
/// (`0x1411c9000`, [`Unit::complete_on_update`]): the house goes to state 2, solid, into its
/// owner's done tracker list; its owner gains the attributes its master gives, and its
/// construction starts in the completion (its owner watches it). Short of its build time it
/// stays a foundation, and an annex outside the walls' branch (class 27, master 70) waits for
/// its main building.
#[test]
fn foundation_whose_build_points_reached_its_build_time_completes() {
    let (game, gained) = completed_house(|_| ());
    assert_eq!(gained, (vec![(98, 25.0), (165, 25.0)], 1));
    let unit = &game.players[1].objects[ListIndex::new(1)];
    let construction = game.parts.building_of(unit.id).construction.unwrap();
    assert_eq!(
        (unit.life.state, construction.desolid),
        (ObjectState::Alive, false)
    );
    let key = key(
        game.model.masters.get(1).map(|masters| &**masters),
        &game.model.lines,
        ids::HOUSE,
        &roles(),
    );
    assert_eq!(
        game.players[1].tracker.list(ListKey::of_master(key.master)),
        [Some(object_id(10028))]
    );
    let short: fn(&mut World) =
        |world| site_mut(world, object_id(10028)).progress = Seconds::new(24.0);
    let annex: fn(&mut World) = |world| {
        world.players[1].objects[ListIndex::new(1)].life.master_id = master_id(109);
        world.parts.building_mut(object_id(10028)).unwrap().annex_of =
            Some(MainBuildingId::new(object_id(700)));
    };
    for setup in [short, annex] {
        let (world, gains) = completed_house(setup);
        assert_eq!(gains, (vec![], 0));
        assert_eq!(
            world.players[1].objects[ListIndex::new(1)].life.state,
            ObjectState::Foundation
        );
    }
}

/// A building completing in its own update leaves its owner what the builder's completion
/// leaves (`World::built`, the building lent to the world), within the update: a master that
/// disappears when built is destroyed at once (state 7), and a castle's completion records its
/// history event (castle built, the owner's id as data1).
#[test]
fn building_completing_in_its_own_update_leaves_what_builder_completion_leaves() {
    let finish = |world: &mut World| {
        let construction = site_mut(world, object_id(10028));
        construction.progress = Seconds::of_master(construction.build_time);
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(1)),
            50,
            |unit, actions, step| {
                unit.complete_on_update(actions, step, &roles())?;
                let events: Vec<(EventKind, f32)> = step.world.players[1]
                    .history
                    .events
                    .iter()
                    .map(|event| (event.kind, event.data1))
                    .collect();
                Ok((unit.life.state, events))
            },
        )
        .map_err(|error| error.reason().to_owned())
    };
    let mut world = house_builder();
    for masters in &mut world.model.masters {
        let building_master = Arc::make_mut(masters)
            .get_mut(ids::HOUSE)
            .unwrap()
            .building
            .as_mut()
            .unwrap();
        building_master.disappears_when_built = BuiltMode::DISAPPEARS;
    }
    assert_eq!(finish(&mut world), Ok((ObjectState::Removed, vec![])));
    let mut world = house_builder();
    world.players[1]
        .history
        .entries
        .push(HistoryEntry::default());
    world.players[1].attributes.resize(250, Amount::ZERO);
    for masters in &mut world.model.masters {
        let castle = Master {
            id: ids::CASTLE,
            ..masters.get(ids::HOUSE).unwrap().clone()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([castle])));
    }
    world.players[1].objects[ListIndex::new(1)].life.master_id = ids::CASTLE;
    let castle_built = EventKind::CastleBuilt;
    assert_eq!(
        finish(&mut world),
        Ok((ObjectState::Alive, vec![(castle_built, 1.0)]))
    );
}

/// A desolid foundation that dies (`FUN_1411dbb70` to state 3 from state 0) starts its
/// construction ([`Unit::foundation_died`], [`Construction::begin`]): its owner watches it at
/// once, the foundation lent to the world within its own update, and the start's flag clears.
#[test]
fn dying_desolid_foundation_owner_watches_it_within_its_update() {
    let mut world = house_builder();
    for masters in &mut world.model.masters {
        let house = Arc::make_mut(masters).get_mut(ids::HOUSE).unwrap();
        house.death_sprite = None;
        if let Some(building_master) = house.building.as_mut() {
            building_master.destruction_sprite = None;
        }
    }
    let construction = world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap();
    construction.desolid = true;
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(1)),
        0,
        |unit, actions, step| {
            unit.life.state = ObjectState::Dying;
            unit.died(actions, step, ObjectState::Foundation, &roles())?;
            Ok(watches(step.world, 1, 10028))
        },
    );
    assert_eq!(seen.map_err(|error| error.reason().to_owned()), Ok(1));
}

/// [`house_builder`]'s town centre 700 (master 109: 2400 hit points, a garrison heal rate of 2)
/// at world time 3000 with the forager (master 120: 25 hit points, at 10) inside, changed by
/// `setup`, after the centre's whole seconds ([`Unit::building_seconds`]): the forager's hit
/// points and the centre's garrison time.
fn centre_seconds(setup: impl FnOnce(&mut World)) -> GenieResult<(f32, u32)> {
    use crate::objects::masters::BuildingMaster;
    let mut world = house_builder();
    for masters in &mut world.model.masters {
        let centre = Arc::make_mut(masters).get_mut(ids::TOWN_CENTRE).unwrap();
        centre.hit_points = MasterHitPoints::of_data(2400);
        centre.building = Some(BuildingMaster {
            garrison_heal_rate: HitPoints::new(2.0),
            ..BuildingMaster::default()
        });
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .hit_points = MasterHitPoints::of_data(25);
    }
    world.world_time = WorldTime::from_millis(3000);
    world.players[1].objects[ListIndex::new(2)].garrison = vec![object_id(9175)];
    world.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(10.0);
    setup(&mut world);
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(2)),
        50,
        |unit, _, step| unit.building_seconds(step, &roles()),
    )?;
    let hp = world.players[1].objects[ListIndex::new(0)].life.hp.get();
    Ok((
        hp,
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(2)].id)
            .garrison_time
            .millis(),
    ))
}

/// A built building's whole seconds (`0x1411c9000`): since its garrison time, which moves on by
/// them, the units inside below their master's hit points heal by its garrison heal rate; no
/// whole second, a unit at its hit points, or a building not built heals nothing. Not modelled:
/// a building at most a fifth of its hit points (it lets its garrison out), one attribute 255
/// names, a unit inside whose raider countdown runs, and villagers inside repairing a damaged
/// building.
#[test]
fn building_heals_its_garrison_each_whole_second() {
    let reason = |result: GenieResult<(f32, u32)>| result.unwrap_err().reason();
    assert_eq!(centre_seconds(|_| ()), Ok((12.0, 3000)));
    assert_eq!(
        centre_seconds(|world| world.world_time = WorldTime::from_millis(999)),
        Ok((10.0, 0))
    );
    assert_eq!(
        centre_seconds(
            |world| world.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(25.0)
        ),
        Ok((25.0, 3000))
    );
    assert_eq!(
        centre_seconds(
            |world| world.players[1].objects[ListIndex::new(2)].life.state =
                ObjectState::Foundation
        ),
        Ok((10.0, 0))
    );
    assert_eq!(
        reason(centre_seconds(|world| world.players[1].objects
            [ListIndex::new(2)]
        .life
        .hp = HitPoints::new(480.0))),
        "a damaged building lets its garrison out (a fifth of its hit points)"
    );
    assert_eq!(
        reason(centre_seconds(|world| {
            world.players[1].attributes.resize(256, Amount::ZERO);
            world.players[1].attributes[255] = Amount::new(109.0);
        })),
        "a building whose master attribute 255 names (FUN_1411c39b0)"
    );
    assert_eq!(
        reason(centre_seconds(|world| world
            .parts
            .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .raider_countdown =
            RaiderCountdown::of_data(1))),
        "a unit inside whose raider_build_countdown runs (FUN_1411e7220)"
    );
    let repairs = |world: &mut World| {
        for masters in &mut world.model.masters {
            let building_master = Arc::make_mut(masters)
                .get_mut(ids::TOWN_CENTRE)
                .unwrap()
                .building
                .as_mut()
                .unwrap();
            building_master.garrison_repair_rate = HitPoints::new(1.0);
        }
        world.players[1].objects[ListIndex::new(2)].life.hp = HitPoints::new(2000.0);
    };
    assert_eq!(
        reason(centre_seconds(repairs)),
        "a building its villagers inside repair (FUN_1411d33e0)"
    );
}

/// A wall's or a house's update takes the walls' branch (`0x1411c9000`, [`walls_branch`]): no
/// whole seconds, so its garrison time stays and nothing inside heals.
///
/// [`walls_branch`]: crate::construction::building_update::walls_branch
#[test]
fn wall_or_house_counts_no_whole_seconds() {
    use crate::construction::building_update::walls_branch;
    let wall = |world: &mut World| {
        for masters in &mut world.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::TOWN_CENTRE)
                .unwrap()
                .class = class::WALL;
        }
    };
    assert_eq!(centre_seconds(wall), Ok((10.0, 0)));
    let master = |id, class| Master {
        id,
        class,
        ..Master::default()
    };
    assert!(walls_branch(&master(ids::HOUSE, class::BUILDING), &roles()));
    assert!(walls_branch(&master(master_id(117), class::WALL), &roles()));
    assert!(!walls_branch(
        &master(ids::TOWN_CENTRE, class::BUILDING),
        &roles()
    ));
}

/// [`repair_world`] on [`open_map`] with the repairer 9175 (a mover) at `at` and the house
/// 10028 at (30, 88) placed, the player holding 100 of each resource, the mesh built.
pub(crate) fn repairing(at: Position) -> World {
    let mut world = repair_world();
    open_map(&mut world);
    world.spatial.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([3, 0])]);
    let placed = |position, class, master, radius| Placed {
        position,
        radius,
        class,
        player: PlayerId::new(1),
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(
        object_id(9175),
        placed(at, class::VILLAGER, ids::BUILDER, Footprint::new(0.2, 0.2)),
    );
    world.spatial.insert(
        object_id(9175),
        placed(
            at,
            class::VILLAGER,
            master_id(118),
            Footprint::new(0.2, 0.2),
        ),
    );
    world.spatial.insert(
        object_id(10028),
        placed(
            Position::new(30.0, 88.0, 0.0),
            class::BUILDING,
            ids::HOUSE,
            Footprint::new(1.0, 1.0),
        ),
    );
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.place_for_test(at);
    let mut mover = walker();
    mover.moved_at = at;
    world.parts.put_mover(unit.id, Some(mover));
    world.players[1].attributes[..4].copy_from_slice(&[Amount::new(100.0); 4]);
    world.build_mesh().unwrap();
    world
}

/// The work command on the house, then `updates` of the repairer's repair parts
/// ([`Unit::repair_part`], 50 ms each): the Repair's states in turn (None once it ended).
fn repair_for(world: &mut World, updates: usize) -> Vec<Option<RepairState>> {
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let mut states = vec![
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .repair()
            .map(|repair_action| repair_action.state.bare()),
    ];
    for _ in 0..updates {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let me = Me {
                    human_order: WorldTime::ZERO,
                    id: unit.id,
                    group: unit.group.id,
                    action_state: actions
                        .top_action()
                        .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                    top_speed: TopSpeed::Own,
                };
                let code = unit.repair_part(actions, step, me, &roles())?;
                unit.end_head_update(actions, step, code)
            },
        )
        .unwrap();
        let state = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .repair()
            .map(|repair_action| repair_action.state.bare());
        if states.last() != Some(&state) {
            states.push(state);
        }
    }
    states
}

/// A repairer that arrives out of the Repair's reach of the house (0.3 tiles off it) cannot
/// repair: the reach test fails and the quick path finds no way closer, so the Repair fails
/// (state 13), lets the house go, tells its unit AI (0x1f9 with the repair action) and ends; the
/// house keeps its 300 hit points and nothing is paid. Moved within reach (its footprint touching
/// the house's), the same work command repairs: its `MoveTo` arrived as its list added it
/// (`0x140aa2100`: within 0.01), so it faces the house (6) in its first update, joins its
/// repairers and repairs (7), the house gaining hit points.
#[test]
fn repairer_out_of_reach_fails_and_one_within_reach_repairs() {
    let mut world = repairing(Position::new(29.5, 89.5, 0.0));
    assert_eq!(
        repair_for(&mut world, 10),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Failed),
            None
        ]
    );
    let house = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, repairers_of(&world, house), house.workers),
        (HitPoints::new(300.0), 0, WorkerCount::of_data(0))
    );
    let heard = &world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .notifies;
    assert_eq!(
        heard
            .iter()
            .map(|notify| (notify.kind, notify.params))
            .collect::<Vec<_>>(),
        [(NotifyKind::ACTION_FAILED, [AiAction::Repair.value(), 0, 0])]
    );
    let at = Position::new(30.0, 89.2, 0.0);
    let placed = *world.spatial.get(object_id(9175)).unwrap();
    world.spatial.insert(
        object_id(9175),
        Placed {
            position: at,
            ..placed
        },
    );
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    unit.place_for_test(at);
    world.parts.mover_mut(unit.id).unwrap().moved_at = at;
    assert_eq!(
        repair_for(&mut world, 400),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let house = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, repairers_of(&world, house), house.workers),
        (HitPoints::new(309.947_57), 1, WorkerCount::of_data(1))
    );
    assert_eq!(world.players[1].resources(), amounts([100.0; 4]));
}

/// A repair pays from what its owner holds when it pays, within the update (`FUN_141618db0` on
/// the player's attributes): wood spent earlier in the same update is not there to pay with, so
/// the house gains no hit points and the wood stays at 0.
#[test]
fn repair_pays_from_what_its_owner_holds_at_the_payment() {
    let mut world = repairing(Position::new(30.0, 89.2, 0.0));
    world.players[1].attributes.resize(300, Amount::ZERO);
    world.players[1].attributes[BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut world, 100),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let hp = world.players[1].objects[ListIndex::new(1)].life.hp;
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            step.world.players[1].attributes[1] = Amount::ZERO;
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let code = unit.repair_part(actions, step, me, &roles())?;
            unit.end_head_update(actions, step, code)
        },
    )
    .unwrap();
    let house = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, world.players[1].attributes[1]),
        (hp, Amount::ZERO)
    );
}

/// The Repair's payment, its end and another player's target ([`Unit::repair_part`]): with a
/// building repair cost (attribute 271) the repair pays the share of the building's costs its
/// hit points make (the house's in wood, a castle's in stone); a repair that
/// takes the house to its hit points ends (state 1, then the action goes) with the house's armor
/// back, the house let go and out of its repairers, the unit AI told 0x1fa; an ally's house takes
/// its hit points and its repairer at once, through the update's view ([`Others::get_mut`]).
#[test]
fn repair_pays_ends_at_full_hit_points_and_reaches_an_ally_house() {
    let near = Position::new(30.0, 89.2, 0.0);
    let mut game = repairing(near);
    game.players[1].attributes.resize(300, Amount::ZERO);
    game.players[1].attributes[BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut game, 100),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    assert_eq!(
        game.players[1].objects[ListIndex::new(1)].life.hp,
        HitPoints::new(302.449_4)
    );
    assert_eq!(
        game.players[1].resources(),
        amounts([100.0, 99.888_596, 100.0, 100.0])
    );
    // a castle (650 stone, 4800 hit points) pays its share in stone
    let mut near_world = repairing(near);
    for masters in &mut near_world.model.masters {
        let house = masters.get(ids::HOUSE).unwrap().clone();
        let castle = Master {
            id: ids::CASTLE,
            hit_points: MasterHitPoints::of_data(4800),
            creatable: Some(CreatablePart {
                cost: costs(&[(attribute::STONE, 650, true)]),
                ..house.creatable.clone().unwrap_or_default()
            }),
            ..house
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([castle])));
    }
    let castle = &mut near_world.players[1].objects[ListIndex::new(1)];
    (castle.life.master_id, castle.life.hp) = (master_id(82), HitPoints::new(3000.0));
    let spatial = *near_world.spatial.get(object_id(10028)).unwrap();
    near_world.spatial.insert(
        object_id(10028),
        Placed {
            master: ids::CASTLE,
            ..spatial
        },
    );
    near_world.players[1].attributes.resize(300, Amount::ZERO);
    near_world.players[1].attributes[BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut near_world, 100),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    assert_eq!(
        near_world.players[1].objects[ListIndex::new(1)].life.hp,
        HitPoints::new(3_002.440_4)
    );
    assert_eq!(
        near_world.players[1].resources(),
        amounts([100.0, 100.0, 99.668_03, 100.0])
    );
    let mut near_world = repairing(near);
    let house = &mut near_world.players[1].objects[ListIndex::new(1)];
    house.life.hp = HitPoints::new(549.0);
    let house_id = house.id;
    near_world.parts.combat_mut(house_id).unwrap().armor_removed = ArmorPair::new(3, 4);
    let states = repair_for(&mut near_world, 100);
    assert_eq!(
        states,
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work),
            Some(RepairState::Done),
            None
        ]
    );
    let house = &near_world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (
            house.life.hp,
            near_world.parts.combat_of(house.id).armor_removed,
            repairers_of(&near_world, house),
            house.workers
        ),
        (
            HitPoints::new(550.0),
            ArmorPair::ZERO,
            0,
            WorkerCount::of_data(0)
        )
    );
    let heard = &near_world
        .parts
        .unit_ai
        .get(near_world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .notifies;
    assert_eq!(
        heard
            .iter()
            .map(|notify| (notify.kind, notify.params))
            .collect::<Vec<_>>(),
        [(
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Repair.value(), 0, 0]
        )]
    );
    // player 0's house, the players allied: repaired, then repaired to full
    let allied = |hp: HitPoints| {
        let mut world = repairing(near);
        let mut house = world
            .remove_live(PlayerId::new(1), ListIndex::new(1))
            .unwrap();
        (
            house.unit.life.hp,
            house.parts.combat.armor_removed,
            house.unit.workers,
        ) = (hp, ArmorPair::new(3, 4), WorkerCount::ZERO);
        let _ = world.push_live(PlayerId::new(0), house).unwrap();
        world.players[0].attributes = vec![Amount::ZERO; 300];
        world.spatial.relations = ByPlayer::new(vec![relation_row([0, 0]), relation_row([0, 0])]);
        for player in &mut world.players {
            player.relations = relation_row([0, 0]);
        }
        let placed = *world.spatial.get(object_id(10028)).unwrap();
        world.spatial.insert(
            object_id(10028),
            Placed {
                player: PlayerId::new(0),
                ..placed
            },
        );
        let states = repair_for(&mut world, 100);
        let unit = &world.players[0].objects[ListIndex::new(0)];
        (
            states,
            unit.life.hp.get(),
            world.parts.combat_of(unit.id).armor_removed,
            repairers_of(&world, unit),
            unit.workers.get(),
        )
    };
    assert_eq!(
        allied(HitPoints::new(300.0)),
        (
            vec![
                Some(RepairState::Move { move_to: None }),
                Some(RepairState::Face),
                Some(RepairState::Work)
            ],
            302.449_4,
            ArmorPair::new(3, 4),
            1,
            1
        )
    );
    assert_eq!(
        allied(HitPoints::new(549.0)),
        (
            vec![
                Some(RepairState::Move { move_to: None }),
                Some(RepairState::Face),
                Some(RepairState::Work),
                Some(RepairState::Done),
                None
            ],
            550.0,
            ArmorPair::ZERO,
            0,
            0
        )
    );
}

/// A repair of an annex (house 10028 of the main building 10029, both at 300 hit points) is its
/// main building's (`FUN_1411d37d0`): the main building repairs its annexes by the amount first,
/// then itself, so both gain it; the repairer counts among the main building's repairers
/// (`FUN_140ad9050`).
#[test]
fn repair_of_an_annex_repairs_its_main_building_and_counts_there() {
    let mut world = repairing(Position::new(30.0, 89.2, 0.0));
    let mut main = world.object_of(&world.players[1].objects[ListIndex::new(1)]);
    main.unit.id = object_id(10029);
    main.unit.place_for_test(Position::new(34.0, 88.0, 0.0));
    let _ = world.push_live(PlayerId::new(1), main).unwrap();
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .annex_of = Some(MainBuildingId::new(object_id(10029)));
    world.link_test_annex(object_id(10028), object_id(10029));
    let placed = *world.spatial.get(object_id(10028)).unwrap();
    world.spatial.insert(
        object_id(10029),
        Placed {
            position: Position::new(34.0, 88.0, 0.0),
            ..placed
        },
    );
    assert_eq!(
        repair_for(&mut world, 100),
        [
            Some(RepairState::Move { move_to: None }),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let [annex, main] = [
        &world.players[1].objects[ListIndex::new(1)],
        &world.players[1].objects[ListIndex::new(2)],
    ];
    assert_eq!(
        (
            annex.life.hp,
            main.life.hp,
            repairers_of(&world, annex),
            repairers_of(&world, main)
        ),
        (HitPoints::new(302.449_4), HitPoints::new(302.449_4), 0, 1)
    );
}

/// [`repairing`] with a building repair cost and `count` repairers on the house (9175, then copies
/// 9185 and 9195 at its east and west sides, all within reach), each repairing in turn: the
/// house's hit points and the player's wood after 10 updates (every repairer at work) and after
/// 100 more.
fn repaired_by(count: usize) -> [(f32, f32); 2] {
    let mut repairing = repairing(Position::new(30.0, 89.2, 0.0));
    repairing.players[1].attributes.resize(300, Amount::ZERO);
    repairing.players[1].attributes[BUILDING_REPAIR_COST] = Amount::new(1.0);
    for (index, at) in [
        Position::new(31.2, 88.0, 0.0),
        Position::new(28.8, 88.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    .take(count - 1)
    {
        let mut object = repairing.object_of(&repairing.players[1].objects[ListIndex::new(0)]);
        object.unit.id = object_id(9185 + 10 * index as i32);
        object.unit.place_for_test(at);
        object
            .parts
            .motion
            .as_mut()
            .and_then(Motion::mover_mut)
            .unwrap()
            .moved_at = at;
        let placed = *repairing.spatial.get(object_id(9175)).unwrap();
        repairing.spatial.insert(
            object.unit.id,
            Placed {
                position: at,
                ..placed
            },
        );
        let _ = repairing.push_live(PlayerId::new(1), object).unwrap();
    }
    let repairers: Vec<ListIndex> = once(0).chain(2..=count).map(ListIndex::new).collect();
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    for &list_index in &repairers {
        repairing
            .order_work(PlayerId::new(1), list_index, &work)
            .unwrap();
    }
    let update = |world: &mut World| {
        for &list_index in &repairers {
            in_update(
                world,
                (PlayerId::new(1), list_index),
                50,
                |unit, actions, step| {
                    let me = Me {
                        human_order: WorldTime::ZERO,
                        id: unit.id,
                        group: unit.group.id,
                        action_state: actions
                            .top_action()
                            .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                        top_speed: TopSpeed::Own,
                    };
                    let code = unit.repair_part(actions, step, me, &roles())?;
                    unit.end_head_update(actions, step, code)
                },
            )
            .unwrap();
        }
    };
    let seen = |world: &World| {
        (
            world.players[1].objects[ListIndex::new(1)].life.hp.get(),
            world.players[1].attributes[1].get(),
        )
    };
    (0..10).for_each(|_| update(&mut repairing));
    let before = seen(&repairing);
    assert_eq!(
        usize::from(repairers_of(
            &repairing,
            &repairing.players[1].objects[ListIndex::new(1)]
        )),
        count
    );
    (0..100).for_each(|_| update(&mut repairing));
    [before, seen(&repairing)]
}

/// More repairers repair faster, but not in proportion (`0x140adaa20` case 6): each of `count`
/// repairs `((count − 1) × 0.5 + 1) / count` of one's work, so two repair 1.5 times as fast as one
/// and three twice as fast. The cost (`FUN_1411d37d0`: the building repair cost times the amount
/// over the master's hit points, of the master's costs) follows the hit points repaired: more
/// repairers pay more wood each second, the same for each hit point (the house's 25 wood over its
/// 550 hit points).
#[test]
fn more_repairers_repair_faster_but_not_in_proportion_and_pay_by_the_hit_point() {
    let gained = |[(hp0, wood0), (hp1, wood1)]: [(f32, f32); 2]| (hp1 - hp0, wood0 - wood1);
    let one = gained(repaired_by(1));
    let two = gained(repaired_by(2));
    let three = gained(repaired_by(3));
    assert_eq!(one, (2.499_389_6, 0.113_677_98));
    assert_eq!(two, (3.747_558_6, 0.170_898_44));
    assert_eq!(three, (4.998_779_3, 0.226_593_02));
    let near = |left: f32, right: f32| (left - right).abs() < 1e-3;
    assert!(near(two.0 / one.0, 1.5) && near(three.0 / one.0, 2.0));
    for (hp, wood) in [one, two, three] {
        assert!(
            near(wood / hp, 25.0 / 550.0),
            "{wood} wood for {hp} hit points"
        );
    }
}

/// A builder's whole Build of house 10028 ([`house_builder`] with the Build's work value 1, a
/// tile planner and the builder's sprites of 8 facets) through its updates (`0x140ab0370`): it
/// walks to the house (state 4, its `MoveTo` moving, then arrived), faces it (6) and works (7)
/// a build point a second until the house's 25 complete it (state 2; the Build's 224); then its
/// search for more work finds nothing around a house (no drop-site entry), so the Build waits
/// without a target (3), lets the house go and asks its unit AI for a search (0x202).
#[test]
fn builder_walks_to_its_house_builds_it_and_waits_after() {
    let mut world = house_builder();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    let players = &mut world.players[1].objects[ListIndex::new(0)];
    let here = players.position();
    let mover = world.parts.mover_mut(players.id).unwrap();
    mover.moved_at = here;
    mover.stats.facets = Some(SpriteFacets {
        count: FacetCount::of_data(8),
        turns: false,
    });
    world
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .build_mut()
        .unwrap()
        .task
        .work_value = WorkValue::new(1.0);
    let mut seen = Vec::new();
    for index in 0..520 {
        let _ = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update(unit, action, step, &roles(), slots()),
        )
        .unwrap();
        let build_action = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .copied()
            .unwrap();
        let house = world.players[1].objects[ListIndex::new(1)].life.state;
        let now = (
            build_action.state.bare(),
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            house,
        );
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((index, now));
        }
    }
    assert_eq!(
        seen,
        [
            (
                0,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Moving),
                    ObjectState::Foundation
                )
            ),
            (
                8,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Arrived),
                    ObjectState::Foundation
                )
            ),
            (9, (BuildState::Work, None, ObjectState::Foundation)),
            (10, (BuildState::Place, None, ObjectState::Foundation)),
            (511, (BuildState::Finished, None, ObjectState::Alive)),
            (512, (BuildState::Idle, None, ObjectState::Alive)),
        ]
    );
    let house = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (
            world
                .parts
                .building_of(house.id)
                .construction
                .unwrap()
                .progress,
            house.workers
        ),
        (Seconds::new(25.0), WorkerCount::of_data(0))
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .build()
            .copied()
            .unwrap()
            .target,
        None
    );
    let heard = world
        .parts
        .unit_ai
        .get(unit.id)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| notify.kind);
    assert_eq!(heard.collect::<Vec<_>>(), [NotifyKind::REQUIRES_SEARCH]);
}

/// A loaded builder's drop-off through its updates (`0x140ab0370` case 7, then
/// [`BuildAction::update`]'s state 4): from [`house_builder`]'s house it walks to the town centre
/// its drop-off found (state 8, its quiet `MoveTo` moving, then arrived), gives the player its 6
/// food there and carries nothing, lets the centre go and walks back to the house (4), faces it
/// (6) and builds again (7).
///
/// [`BuildAction::update`]: crate::construction::build::BuildAction::update
#[test]
fn loaded_builder_drops_its_food_at_the_centre_and_builds_on() {
    let mut world = house_builder();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    let builder = &mut world.players[1].objects[ListIndex::new(0)];
    (builder.life.attribute_kind, builder.life.amount) =
        (Some(AttributeId::named(17)), Amount::new(6.0));
    let here = builder.position();
    let mover = world.parts.mover_mut(builder.id).unwrap();
    mover.moved_at = here;
    mover.stats.facets = Some(SpriteFacets {
        count: FacetCount::of_data(8),
        turns: false,
    });
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, step| {
            let mut build = *actions.list.build().unwrap();
            let by = unit.releaser(step)?;
            let dropped = build.drop_again(step, unit, by, slots(), &roles());
            *actions.list.build_mut().unwrap() = build;
            dropped
        },
    )
    .unwrap();
    let mut seen = Vec::new();
    for index in 0..350 {
        let _ = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, action, step| Unit::update(unit, action, step, &roles(), slots()),
        )
        .unwrap();
        let players = &world.players[1].objects[ListIndex::new(0)];
        let build_action = world
            .parts
            .actions_of(players.id)
            .list
            .build()
            .copied()
            .unwrap();
        let now = (
            build_action.state.bare(),
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            build_action.site,
            players.life.amount.get(),
        );
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((index, now));
        }
    }
    let drop = BuildState::Drop { move_to: None };
    assert_eq!(
        seen,
        [
            (
                0,
                (
                    drop,
                    Some(MoveTo::Moving),
                    Some(DropSiteId::new(object_id(700))),
                    6.0
                )
            ),
            (
                177,
                (
                    drop,
                    Some(MoveTo::Arrived),
                    Some(DropSiteId::new(object_id(700))),
                    6.0
                )
            ),
            (
                178,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Start),
                    None,
                    0.0
                )
            ),
            (
                179,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Moving),
                    None,
                    0.0
                )
            ),
            (
                342,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Arrived),
                    None,
                    0.0
                )
            ),
            (343, (BuildState::Work, None, None, 0.0)),
            (344, (BuildState::Place, None, None, 0.0)),
        ]
    );
    assert_eq!(world.players[1].attributes[0], Amount::new(6.0));
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)]
            .life
            .attribute_kind,
        None
    );
}

/// Object `list_index` of player `player_id`'s step (dt 50, outside a turn) whose only part is
/// `step`: what it leaves in the `Step`, and its result, which what follows it sees
/// ([`World::step_object`]).
fn finished_with(
    world: &mut World,
    (player_id, list_index): (PlayerId, ListIndex),
    step: impl FnOnce(&mut step::Step<'_>) -> GenieResult<Outcome>,
) -> GenieResult<Outcome> {
    world
        .step_object(
            (player_id, list_index),
            (Millis::new(50), None, false),
            |_, _, context| step(context),
        )
        .map(|stepped| stepped.outcome)
}

/// A dying building lets its annexes go at once (the static update's death, `0x14118ad50`): in
/// the same update its annex, here the town centre, holds no main building and the building's
/// hit points, and the world's annex slots are clear.
#[test]
fn dying_building_lets_its_annexes_go_in_its_own_update() {
    use crate::objects::sprite::{Graphic, building_death};
    let mut world = house_builder();
    world.link_test_annex(object_id(700), object_id(10028));
    world
        .parts
        .building_mut(world.players[1].objects[ListIndex::new(2)].id)
        .unwrap()
        .annex_of = Some(MainBuildingId::new(object_id(10028)));
    let house = &world.players[1].objects[ListIndex::new(1)];
    let master = world.model.masters[1].get(house.life.master_id).unwrap();
    let progress = world
        .parts
        .building_of(house.id)
        .construction
        .unwrap()
        .progress;
    let id = building_death(master, None, progress).unwrap();
    let shown = Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let _ = Arc::make_mut(&mut world.model.graphics).insert(id, shown);
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(1)),
        0,
        |unit, actions, step| {
            unit.life.hp = HitPoints::new(3.0);
            unit.died(actions, step, ObjectState::Unfinished, &roles())?;
            let (_, annex) = OthersRef::of(&step.world.players, &step.world.index, step.player)
                .unwrap()
                .find(object_id(700))
                .unwrap();
            Ok((
                step.world.parts.building_of(annex.id).annex_of,
                annex.life.hp.get(),
                step.world
                    .parts
                    .building_of(unit.id)
                    .annexes
                    .ids()
                    .collect::<Vec<_>>(),
            ))
        },
    );
    assert_eq!(
        seen.map_err(|error| error.reason().to_owned()),
        Ok((None, 3.0, vec![]))
    );
}

/// The unit AI's orders of a step reach the unit and the units they name: a `GatherObject`'s
/// gather command takes the bush at once (the Gather is the unit's while its update runs); the
/// Gather's reseed without a farm lets it go and its build command gives the builder its Build,
/// both within the update; a drop elsewhere sends the unit to the drop site with the order back
/// to its target queued, and a move off a footprint gives it a `MoveTo` at once.
#[test]
fn the_unit_ai_orders_reach_the_unit_and_the_units_they_name() {
    use crate::gathering::gather::Next;
    use crate::unit_ai::Effect;
    // the calls run in the unit's step, and its top action right after them
    let applied = |world: &mut World, effects: Vec<Effect>| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                for effect in effects {
                    unit.run_effect(actions, effect, None, step, &roles())?;
                }
                Ok(actions.top_action_kind())
            },
        )
        .map_err(|error| error.reason().to_owned())
    };
    // what follows the Gather's update, and the unit's actions right after it
    let followed = |world: &mut World, next: Next| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                let by = unit.releaser(step)?;
                let mut gather = actions.list.gather().copied().unwrap();
                let (target, carry_to) = (gather.target, gather.carry_to);
                let _ = unit.gather_next(
                    (&mut gather, actions),
                    step,
                    next,
                    (target, carry_to),
                    by,
                    &roles(),
                )?;
                let bush = OthersRef::of(&step.world.players, &step.world.index, PlayerId::new(0))
                    .unwrap()
                    .find(object_id(600))
                    .map(|(_, unit)| unit.workers.get());
                let gather = actions
                    .list
                    .gather()
                    .map(|gather_action| (gather_action.target, gather_action.state.bare()));
                let build = actions.list.build().map(|build_action| build_action.target);
                Ok((bush, gather, build))
            },
        )
        .map_err(|error| error.reason().to_owned())
    };
    let mut world = house_builder();
    assert_eq!(
        applied(
            &mut world,
            vec![Effect::Command(AiCommand::GatherObject(object_id(600)))]
        ),
        Ok(Some(GATHER))
    );
    let gatherer = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            world.parts.actions_of(gatherer.id).top_action_kind(),
            world
                .parts
                .actions_of(gatherer.id)
                .list
                .gather()
                .map(|gather_action| (gather_action.target, gather_action.state.bare()))
        ),
        (
            Some(GATHER),
            Some((
                Some(GatherTargetId::new(object_id(600))),
                GatherState::Walk { move_to: None }
            ))
        )
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(0)].workers,
        WorkerCount::of_data(1)
    );
    let reseeded = followed(
        &mut world,
        Next::Reseed {
            end: WalkEnd::Arrived,
        },
    );
    assert_eq!(
        reseeded,
        Ok((Some(0), Some((None, GatherState::Idle)), None))
    );

    let mut world = house_builder();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_build(None);
    assert!(
        applied(
            &mut world,
            vec![Effect::Command(AiCommand::GatherObject(object_id(600)))]
        )
        .is_ok()
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .target = Some(GatherTargetId::new(object_id(10028)));
    assert_eq!(
        followed(&mut world, Next::Build),
        Ok((
            Some(1),
            None,
            Some(Some(FoundationId::new(object_id(10028))))
        ))
    );
    let builder = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(builder.id).top_action_kind(),
        Some(ActionKind::BUILD)
    );

    let mut world = house_builder();
    let at = world
        .object(object_id(700))
        .map(|(_, unit)| unit.position())
        .unwrap();
    let drop = Effect::Command(AiCommand::DropElsewhere {
        site: DropSiteId::new(object_id(700)),
        at,
        back: Some(object_id(600)),
    });
    assert!(applied(&mut world, vec![drop]).is_ok());
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    let queued: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.target))
        .collect();
    assert_eq!(
        (ai.current_target, queued),
        (
            Some(object_id(700)),
            vec![(OrderKind::OnTarget, Some(object_id(600)))]
        )
    );

    let mut world = house_builder();
    let aside = AiMove {
        point: Position::new(25.0, 89.5, 0.0),
        range: Distance::ZERO,
        force: OrderForce::Unforced,
        pace: Pace::Run,
    };
    assert_eq!(
        applied(
            &mut world,
            vec![Effect::Command(AiCommand::MoveAside(aside))]
        ),
        Ok(Some(MOVE_TO))
    );
}

/// An Enter's transport moves to the meeting point within the entering unit's update (slot
/// 0x340, [`World::ship_move`], the unit lent to the world): the transport's `MoveTo` is there
/// before the update ends; one the world does not hold cannot move. Its sprite is
/// [`an_ai_move_replaces_done_attack_with_move_to_the_point`]'s.
#[test]
fn an_enter_transport_moves_to_the_meeting_point_where_the_unit_runs() {
    let mut house_builder = house_builder();
    let mut ship = house_builder.object_of(&house_builder.players[1].objects[ListIndex::new(0)]);
    ship.unit.id = object_id(9176);
    ship.parts.actions.list = ActionList::default();
    let _ = house_builder.push_live(PlayerId::new(1), ship).unwrap();
    let placed = *house_builder.spatial.get(object_id(9175)).unwrap();
    house_builder.spatial.insert(object_id(9176), placed);
    let meet = Position::new(25.0, 89.5, 0.0);
    let moved = |game: &mut World, ship: i32| {
        in_update(
            game,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                step.lend((&mut *unit, &mut *actions), |world| {
                    world.ship_move(ObjectId::of_field(ship).map(ContainerId::new), meet)
                })?;
                let (_, unit) = step.world.object(object_id(9176)).expect("held");
                let actions = step.world.parts.actions_of(unit.id);
                Ok(actions
                    .list
                    .move_to()
                    .map(|move_to_action| (move_to_action.goal(), move_to_action.range)))
            },
        )
        .map_err(|error| error.reason().to_owned())
    };
    assert_eq!(
        moved(&mut house_builder, 9176),
        Ok(Some((Some(meet), Distance::ZERO)))
    );
    let unreached = UNREACHED.reason().to_owned();
    assert_eq!(moved(&mut house_builder, 4242), Err(unreached));
}

/// The updating unit's own kill with what a razing hit counted: player 1's attribute 98 by
/// each of `adds` (attribute, amount).
fn own_razing(adds: &[(AttributeId, f32)]) -> KillLeft {
    KillLeft {
        id: object_id(9175),
        kill: None,
        counts: Vec::new(),
        razed: vec![Razed {
            attacker_player: PlayerId::new(1),
            owner: PlayerId::new(1),
            event: None,
            adds: adds
                .iter()
                .map(|&(attribute_id, value)| (PlayerId::new(1), attribute_id, Amount::new(value)))
                .collect(),
        }],
    }
}

/// The updating unit's own razings count where the hit runs, within its update
/// ([`crate::combat::victims::file_kill`] on the unit out of its slot): the attribute adds in
/// their order, and the first that fails stops the ones after it (a count of an attribute the
/// player does not have stops the next one).
#[test]
fn the_own_razings_count_where_the_hit_runs() {
    use crate::combat::victims::file_kill;
    let razed = |adds: &[(AttributeId, f32)]| {
        let mut world = house_builder();
        world.players[1].attributes[98] = Amount::new(10.0);
        let in_update = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            0,
            |unit, actions, step| {
                file_kill(step, (unit, actions), own_razing(adds), &roles())?;
                Ok(step.world.players[1].attributes[98].get())
            },
        );
        (
            in_update.map_err(|error| error.reason().to_owned()),
            world.players[1].attributes[98].get(),
        )
    };
    assert_eq!(razed(&[(attribute::TOTAL_VALUE, -5.0)]), (Ok(5.0), 5.0));
    let missing = Err("an attribute the player does not have".to_owned());
    assert_eq!(
        razed(&[
            (AttributeId::named(9999), 5.0),
            (attribute::TOTAL_VALUE, -5.0)
        ]),
        (missing, 10.0)
    );
}

/// A step that fails stops what follows it: the tiles whose marks it changed are not handed to
/// the planners after it ([`World::step_object`]'s `hand_changed_tiles`); a step that holds
/// hands them on.
#[test]
fn step_that_fails_stops_what_follows_it() {
    use crate::map::spatial::Obstruction;
    use crate::objects::unit::Outcome;
    let record = Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::UNIT,
    };
    let mut world = house_builder();
    let outcome = finished_with(&mut world, (PlayerId::new(1), ListIndex::new(0)), |step| {
        step.world.spatial.add_record(object_id(10028), record);
        Err(GenieError::unsupported("the step's own gap"))
    });
    assert_eq!(
        outcome.map_err(|error| error.reason().to_owned()),
        Err("the step's own gap".to_owned())
    );
    assert_ne!(world.spatial.take_changes(), Vec::<PlanRect>::new());
    let mut world = house_builder();
    let outcome = finished_with(&mut world, (PlayerId::new(1), ListIndex::new(0)), |step| {
        step.world.spatial.add_record(object_id(10028), record);
        Ok(Outcome::Stay)
    });
    assert_eq!(
        outcome.map_err(|error| error.reason().to_owned()),
        Ok(Outcome::Stay)
    );
    assert_eq!(world.spatial.take_changes(), Vec::<PlanRect>::new());
}

/// What a completion leaves its owner, each at once ([`World::complete_building`] and the
/// routines it calls): a pasture puts its livestock on its edges
/// ([`World::setup_pasture`], from its first part), a building's stack unit is made at its
/// place ([`World::stack_unit`]), its master's technology is researched, and a master that
/// disappears when built is destroyed. A completed mule cart with nothing to find keeps its
/// state ([`World::cart_search`], from its first part).
#[test]
fn completion_parts_reach_their_owner() {
    use crate::construction::build::StackUnit;
    use crate::gathering::pasture::livestock;
    use crate::technology::{TechDef, TechState};
    let made = |world: &World| -> Vec<(i32, i16, Position)> {
        world.players[1].objects.as_slice()[1..]
            .iter()
            .map(|unit| (unit.id.get(), unit.life.master_id.get(), unit.position()))
            .collect()
    };
    let mut game = pasture_game(
        Position::new(60.0, 60.0, 0.0),
        2.0,
        &livestock(slots().master),
    );
    assert_eq!(
        game.setup_pasture(PlayerId::new(1), PastureId::new(object_id(50))),
        Ok(())
    );
    assert_eq!(
        made(&game),
        [
            (100, 1899, Position::new(58.5, 58.5, 0.0)),
            (101, 1899, Position::new(59.4, 58.5, 0.0))
        ]
    );
    let mut pasture_game = pasture_game(
        Position::new(60.0, 60.0, 0.0),
        2.0,
        &livestock(slots().master),
    );
    let stack = StackUnit {
        master: master_id(1899),
        at: Position::new(60.0, 60.0, 0.0),
        hp_lost: HitPoints::ZERO,
    };
    assert_eq!(
        pasture_game.stack_unit(PlayerId::new(1), object_id(50), stack),
        Ok(())
    );
    assert_eq!(
        made(&pasture_game),
        [(100, 1899, Position::new(60.0, 60.0, 0.0))]
    );

    // the house's completion with its master's technology, or disappearing when built
    let completed = |setup: fn(&mut BuildingMaster)| {
        let mut world = house_builder();
        for masters in &mut world.model.masters {
            if let Some(house_master) = Arc::make_mut(masters)
                .get_mut(ids::HOUSE)
                .and_then(|master| master.building.as_mut())
            {
                setup(house_master);
            }
        }
        world.model.techs = Arc::new(vec![TechDef::default()]);
        world.players[1].techs = vec![TechState {
            state: ResearchState::Available,
            research_cap: ResearchCap::of_data(-1),
            building_master: None,
            ..TechState::default()
        }];
        let result = world
            .complete_building(object_id(10028))
            .map_err(|error| error.reason().to_owned());
        (world, result)
    };
    let (world, result) = completed(|building_master| building_master.tech = Some(tech_id(0)));
    assert_eq!(result, Ok(()));
    assert_eq!(world.players[1].techs[0].state, ResearchState::Researched);
    let (world, result) =
        completed(|building_master| building_master.disappears_when_built = BuiltMode::DISAPPEARS);
    assert_eq!(result, Ok(()));
    let house = &world.players[1].objects[ListIndex::new(1)].life;
    assert_eq!(
        (house.state, house.hp),
        (ObjectState::Removed, HitPoints::ZERO)
    );
    let mut world = house_builder();
    assert_eq!(
        world.cart_search(PlayerId::new(1), object_id(9175), ObjectState::Alive),
        Ok(())
    );
    let cart = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            cart.life.state,
            world
                .parts
                .unit_ai
                .get(cart.id)
                .map(|unit_ai| unit_ai.current_target)
        ),
        (ObjectState::Alive, Some(None))
    );
}

/// A built building's death in its own update (`FUN_1411dbb70`, state 3 from 2), each part
/// within the update in the game's order: its owner's watched list lets it go, a castle's count
/// (attribute 134, held at 0) and its paid cost (98 alone for a castle, the combat object's
/// `FUN_14121cc40`) leave the owner, and as the last building of its master it withdraws the
/// master's technology from state 3 (`FUN_141166ab0`'s scan over the owner's objects, the
/// castle lent to the world). Another castle standing keeps the technology.
#[test]
fn built_building_death_reaches_its_owner_within_its_update() {
    use crate::technology::{TechDef, TechState};
    let died = |another: bool| {
        let mut world = enter_world(vec![], 5);
        for masters in &mut world.model.masters {
            let mut castle = Master {
                id: ids::CASTLE,
                death_sprite: None,
                creatable: Some(CreatablePart {
                    cost: costs(&[(attribute::STONE, 650, true)]),
                    ..masters
                        .get(ids::HOUSE)
                        .unwrap()
                        .clone()
                        .creatable
                        .clone()
                        .unwrap_or_default()
                }),
                ..masters.get(ids::HOUSE).unwrap().clone()
            };
            let building_master = castle.building.as_mut().unwrap();
            (building_master.tech, building_master.destruction_sprite) = (Some(tech_id(0)), None);
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([castle])));
        }
        world.players[1].objects[ListIndex::new(1)].life.master_id = ids::CASTLE;
        if another {
            let mut other = world.object_of(&world.players[1].objects[ListIndex::new(1)]);
            other.unit.id = object_id(10031);
            let _ = world.push_live(PlayerId::new(1), other).unwrap();
        }
        world.model.techs = Arc::new(vec![TechDef::default()]);
        world.players[1].techs = vec![TechState {
            state: ResearchState::Researched,
            ..TechState::default()
        }];
        let owner = &mut world.players[1];
        owner.attributes.resize(250, Amount::ZERO);
        (
            owner.attributes[134],
            owner.attributes[98],
            owner.attributes[165],
        ) = (Amount::new(0.5), Amount::new(1000.0), Amount::new(1000.0));
        world.players[1].watched.push(Watched {
            id: object_id(10028),
            cell: Cell::Tile(0),
            bits: VisionBits::NONE,
        });
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(1)),
            0,
            |unit, actions, step| {
                unit.life.state = ObjectState::Dying;
                unit.died(actions, step, ObjectState::Alive, &roles())?;
                let amounts = &step.world.players[1].attributes;
                let withdrawn = step.world.players[1].techs[0].state != ResearchState::Researched;
                Ok((
                    watches(step.world, 1, 10028),
                    [amounts[134], amounts[98], amounts[165]].map(Amount::get),
                    withdrawn,
                ))
            },
        )
        .map_err(|error| error.reason().to_owned())
    };
    assert_eq!(died(false), Ok((0, [0.0, 350.0, 1000.0], true)));
    assert_eq!(died(true), Ok((0, [0.0, 350.0, 1000.0], false)));
}

/// The world's update runs the builder of the walk-and-build test the same way the object's
/// own update does (4 walking, 6 and 7 at the house, 224 at its completion, then idle), and
/// the completion's stop before its unit AI resumes it in the same update: nothing freezes.
#[test]
fn builder_house_completes_in_the_world_update_and_its_unit_ai_resumes() {
    let mut world = house_builder();
    world.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![TerrainId::named(0); 120 * 120],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    world.build_mesh().unwrap();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    let here = unit.position();
    let mover = world.parts.mover_mut(unit.id).unwrap();
    mover.moved_at = here;
    mover.stats.facets = Some(SpriteFacets {
        count: FacetCount::of_data(8),
        turns: false,
    });
    world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .build_mut()
        .unwrap()
        .task
        .work_value = WorkValue::new(1.0);
    let mut seen = Vec::new();
    for index in 0..520 {
        world.tick(Millis::new(50));
        let build_action = world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .copied()
            .unwrap();
        let house = world.players[1].objects[ListIndex::new(1)].life.state;
        let now = (
            build_action.state.bare(),
            build_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            house,
        );
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((index, now));
        }
    }
    assert_eq!(
        seen,
        [
            (
                0,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Moving),
                    ObjectState::Foundation
                )
            ),
            (
                8,
                (
                    BuildState::Move { move_to: None },
                    Some(MoveTo::Arrived),
                    ObjectState::Foundation
                )
            ),
            (9, (BuildState::Work, None, ObjectState::Foundation)),
            (10, (BuildState::Place, None, ObjectState::Foundation)),
            (511, (BuildState::Finished, None, ObjectState::Alive)),
            (512, (BuildState::Idle, None, ObjectState::Alive)),
        ]
    );
    assert_eq!(
        world
            .players
            .iter()
            .map(|player| player.freezes.len())
            .sum::<usize>(),
        0
    );
}

/// [`World::run`] on `command` against its handler called by hand on a copy (`by_hand`, whose
/// error stops `player`'s lane): the same world after. Whether the command changed the world.
fn routes(
    world: &World,
    command: &Command,
    player: PlayerId,
    by_hand: impl FnOnce(&mut World) -> GenieResult<()>,
) -> bool {
    let mut ran = world.clone();
    ran.run(command);
    let mut hand = world.clone();
    if let Err(error) = by_hand(&mut hand) {
        hand.stop_player(player, error);
    }
    assert!(ran == hand, "{command:?}");
    ran != *world
}

/// The command phase runs each unit command by its handler ([`World::run`]), on
/// [`forage_world`]'s villager 9175 and town centre 700; an error stops the issuer's lane. A
/// human player's `UNIT_ORDER` takes the world time as its units' last human order time first.
#[test]
fn the_command_phase_runs_each_unit_command_by_its_handler() {
    let forage_world = forage_world();
    let ids = object_ids([9175]);
    let way = vec![Point::new(40.5, 89.5)];
    let sweep = Sweep {
        patrol: false,
        patrols: SavedPatrols::Dropped,
        queued: false,
        ignore_pending: false,
    };
    let unit_order = |action, human| {
        Command::UnitOrder(UnitOrderCommand {
            player: player_id(1),
            units: unit_list(ids.clone()),
            target: Some(object_id(700)),
            point: Point::new(-1.0, -1.0),
            queue_id: QueueId::of_data(-1),
            shift: Shift::Held,
            scope: GarrisonScope::Target,
            action: UnitOrderAction::of(action),
            param: UnitIndex::FIRST,
            human,
        })
    };
    let changed = [
        routes(
            &forage_world,
            &Command::Stop(StopCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
            }),
            PlayerId::new(1),
            |world| world.stop_command(player_id(1), &ids),
        ),
        routes(
            &forage_world,
            &Command::Patrol(PatrolCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
                waypoints: Waypoints::of(&way).unwrap(),
                patrols: SavedPatrols::Dropped,
            }),
            PlayerId::new(1),
            |world| world.patrol(player_id(1), &ids, &way, SavedPatrols::Dropped),
        ),
        routes(
            &forage_world,
            &Command::AttackMove(AttackMoveCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
                waypoints: Waypoints::of(&way).unwrap(),
                patrols: SavedPatrols::Dropped,
                queued: false,
                ignore_pending: false,
            }),
            PlayerId::new(1),
            |world| world.attack_move(player_id(1), &ids, &way, sweep),
        ),
        routes(
            &forage_world,
            &Command::SeekShelter(SeekShelterCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
            }),
            PlayerId::new(1),
            |world| world.seek_shelter(player_id(1), &ids),
        ),
        routes(
            &forage_world,
            &Command::UnitAiState(UnitAiStateCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
                state: UnitStance::Defensive,
            }),
            PlayerId::new(1),
            |world| world.unit_ai_state(player_id(1), &ids, UnitStance::Defensive),
        ),
        routes(
            &forage_world,
            &Command::AutoScout(AutoScoutCommand {
                player: player_id(1),
                units: unit_list(ids.clone()),
            }),
            PlayerId::new(1),
            |world| world.auto_scout(player_id(1), &ids),
        ),
        routes(
            &forage_world,
            &Command::Unload(UnloadCommand {
                player: player_id(1),
                units: unit_list(object_ids([700])),
                point: Point::new(24.5, 89.5),
                pick: UnloadPick::of_packet(0, -1),
                sound: false,
            }),
            PlayerId::new(1),
            |world| {
                UnloadCommand {
                    player: player_id(1),
                    units: unit_list(object_ids([700])),
                    point: Point::new(24.5, 89.5),
                    pick: UnloadPick::of_packet(0, -1),
                    sound: false,
                }
                .run(world)
            },
        ),
        routes(
            &forage_world,
            &Command::GoBackToWork2(GoBackToWork2Command {
                player: player_id(1),
                units: unit_list(ids.clone()),
            }),
            PlayerId::new(1),
            |world| world.go_back_to_work(player_id(1), &ids),
        ),
        routes(
            &forage_world,
            &Command::Delete(DeleteCommand {
                player: player_id(1),
                id: object_id(9175),
            }),
            PlayerId::new(1),
            |world| world.delete(player_id(1), object_id(9175)),
        ),
        routes(
            &forage_world,
            &Command::TransformObject(TransformObjectCommand {
                player: player_id(1),
                id: object_id(9175),
                master: master_id(120),
            }),
            PlayerId::new(1),
            |world| world.transform_object(player_id(1), object_id(9175), master_id(120)),
        ),
        routes(
            &forage_world,
            &unit_order(5, false),
            PlayerId::new(1),
            |world| {
                world.garrison_order(
                    player_id(1),
                    &ids,
                    Some(object_id(700)),
                    Dispatch::Queued,
                    GarrisonScope::Target,
                )
            },
        ),
        routes(
            &forage_world,
            &unit_order(4, false),
            PlayerId::new(1),
            |world| world.cancel_order(&ids, UnitIndex::of_data(0), Batch::Five),
        ),
        routes(
            &forage_world,
            &unit_order(9, true),
            PlayerId::new(1),
            |world| {
                world.human_order(&ids);
                world.unit_order(player_id(1), &ids, UnitOrderAction::Other(9))
            },
        ),
    ];
    assert_eq!(
        changed,
        [
            true, true, true, false, true, false, false, false, true, false, false, false, true
        ]
    );
}

/// The command phase runs each player's command by its handler ([`World::run`]): a
/// tribute, a resignation, a flare, a market trade (which cannot fail), a GAME sub-command
/// (what the slice cannot model of it stops the player it is for) and an unmodelled command
/// (its player stops).
#[test]
fn the_command_phase_runs_each_player_command_by_its_handler() {
    use crate::players::tribute::Gift;
    let forage_world = forage_world();
    let gifts = [Gift {
        attribute: ResourceWord::of_data(0),
        amount: Amount::new(10.0),
        fee: TributeFee::of_data(0.0),
    }; 4];
    let game = |sub| GameCommand {
        issuer: player_id(1),
        sub,
        player: 0,
        argument: 0,
        speed: 0.0,
        value: 0,
    };
    let why = GenieError::unsupported("a command the slice does not model");
    let changed = [
        routes(
            &forage_world,
            &Command::Tribute(TributeCommand {
                player: player_id(1),
                to: player_id(0),
                gifts,
            }),
            PlayerId::new(1),
            |world| world.tribute(player_id(1), player_id(0), &gifts),
        ),
        routes(
            &forage_world,
            &Command::Resign(ResignCommand {
                player: player_id(1),
                departure: Departure::Resigned,
            }),
            PlayerId::new(1),
            |world| world.resign(player_id(1), Departure::Resigned),
        ),
        routes(
            &forage_world,
            &Command::Flare(FlareCommand {
                player: player_id(1),
                target: None,
                point: Point::new(40.5, 89.5),
                players: 2,
                shown: Vec::new(),
            }),
            PlayerId::new(1),
            |world| world.flare(None, Point::new(40.5, 89.5), 2),
        ),
        routes(
            &forage_world,
            &Command::Trade(TradeCommand {
                player: player_id(1),
                attribute: ResourceWord::of_data(1),
                times: TradeLots::of_data(1),
                market: None,
                buy: true,
            }),
            PlayerId::new(1),
            |world| {
                world
                    .buy(player_id(1), attribute::WOOD, TradeLots::of_data(1))
                    .unwrap();
                Ok(())
            },
        ),
        routes(
            &forage_world,
            &Command::Trade(TradeCommand {
                player: player_id(1),
                attribute: ResourceWord::of_data(1),
                times: TradeLots::of_data(1),
                market: None,
                buy: false,
            }),
            PlayerId::new(1),
            |world| {
                world
                    .sell(player_id(1), attribute::WOOD, TradeLots::of_data(1))
                    .unwrap();
                Ok(())
            },
        ),
        routes(
            &forage_world,
            &Command::Game(game(GameSub::Diplomacy)),
            PlayerId::new(0),
            |world| world.game(&game(GameSub::Diplomacy)),
        ),
        routes(
            &forage_world,
            &Command::Unmodelled(UnmodelledCommand {
                player: player_id(1),
                packet: Vec::new(),
                why: why.clone(),
            }),
            PlayerId::new(1),
            |_| Err(why.clone()),
        ),
    ];
    assert_eq!(changed, [false, true, true, false, false, false, true]);
    // with the game's cheats a cheat code is not modelled: it stops the player it is for, or
    // its issuer for a player outside the game
    let mut cheats = forage_world;
    cheats.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    let cheat = |player| GameCommand {
        issuer: player_id(1),
        sub: GameSub::Cheat,
        player,
        ..game(GameSub::Diplomacy)
    };
    assert!(routes(
        &cheats,
        &Command::Game(cheat(0)),
        PlayerId::new(0),
        |world| world.game(&cheat(0))
    ));
    assert!(routes(
        &cheats,
        &Command::Game(cheat(9)),
        PlayerId::new(1),
        |world| world.game(&cheat(9))
    ));
}

/// An order of units runs after its command's check ([`World::run`]): a MOVE of the villager
/// leaves it as the last commanded units, resets it for the order, takes the world time as its
/// last human order time and moves it; a human player's MOVE left without a unit (Gaia's bush)
/// does nothing, as does a FOLLOW of a target the world lacks; a GUARD takes the builders'
/// check; a RETREAT is not modelled (after the reset).
#[test]
fn the_command_phase_runs_an_order_after_its_check() {
    use crate::commands::Order;
    let forage_world = forage_world();
    let ids = object_ids([9175]);
    let walk = MoveOrder {
        target: None,
        point: Point::new(40.5, 89.5),
        flags: OrderFlags::HUMAN,
        control: ControlKey::Up,
    };
    let order = |units: Vec<Option<ObjectId>>, order| {
        Command::Order(OrderCommand {
            player: player_id(1),
            units: unit_list(units),
            order,
        })
    };
    assert!(routes(
        &forage_world,
        &order(ids.clone(), Order::Move(walk)),
        PlayerId::new(1),
        |world| {
            world.set_last_commanded(PlayerId::new(1), ids.clone());
            world.reset_order_units(&ids).unwrap();
            world.human_order(&ids);
            world.move_order(PlayerId::new(1), &ids, &walk)
        }
    ));
    let none_left = order(object_ids([600]), Order::Move(walk));
    assert!(!routes(
        &forage_world,
        &none_left,
        PlayerId::new(1),
        |_| Ok(())
    ));
    let lost = order(ids.clone(), Order::Follow(object_id(4444)));
    assert!(!routes(&forage_world, &lost, PlayerId::new(1), |_| Ok(())));
    assert!(routes(
        &forage_world,
        &order(ids.clone(), Order::Guard(object_id(700))),
        PlayerId::new(1),
        |world| {
            world.reset_order_units(&ids).unwrap();
            world.human_order(&ids);
            world.guard(PlayerId::new(1), &ids, object_id(700))
        }
    ));
    assert!(routes(
        &forage_world,
        &order(ids.clone(), Order::Retreat(Interact::default())),
        PlayerId::new(1),
        |world| {
            world.set_last_commanded(PlayerId::new(1), ids.clone());
            world.reset_order_units(&ids).unwrap();
            Err(GenieError::unsupported("an order moves its units"))
        }
    ));
}

/// The last commanded units (`FUN_140e6b290`): a MOVE of its own list that its check lets
/// run leaves that list as its player's last commanded units, −1s and all; a refused MOVE
/// leaves them. A count of `0xffff` orders them, and its check writes its −1s back, refused
/// or not.
#[test]
fn an_order_of_the_last_commanded_units_reads_what_the_last_run_order_left() {
    use crate::commands::{Order, UnitList};
    let walk = MoveOrder {
        target: None,
        point: Point::new(40.5, 89.5),
        flags: OrderFlags::HUMAN,
        control: ControlKey::Up,
    };
    let order = |units| {
        Command::Order(OrderCommand {
            player: player_id(1),
            units,
            order: Order::Move(walk),
        })
    };
    let mut game = forage_world();
    game.run(&order(unit_list(object_ids([9175, 600]))));
    assert_eq!(game.last_commanded.get(1), Some(&object_ids([9175, -1])));
    game.run(&order(unit_list(object_ids([600]))));
    assert_eq!(game.last_commanded.get(1), Some(&object_ids([9175, -1])));

    let mut cached = forage_world();
    cached.set_last_commanded(PlayerId::new(1), object_ids([600, 9175]));
    let checked = object_ids([-1, 9175]);
    assert!(routes(
        &cached,
        &order(UnitList::last_commanded()),
        PlayerId::new(1),
        |world| {
            world.set_last_commanded(PlayerId::new(1), checked.clone());
            world.reset_order_units(&checked).unwrap();
            world.human_order(&checked);
            world.move_order(PlayerId::new(1), &checked, &walk)
        }
    ));
    cached.set_last_commanded(PlayerId::new(1), object_ids([600]));
    let mut refused = cached.clone();
    refused.run(&order(UnitList::last_commanded()));
    cached.set_last_commanded(PlayerId::new(1), vec![None]);
    assert_eq!(refused, cached);
}

/// An order's units (`FUN_140e6b290`): comm player 0's and one past the world's are the
/// packet's as they are, with no last commanded units; any other's are the first 60, 500 for
/// a computer, or its last commanded units, none before its first.
#[test]
fn an_order_units_are_capped_unless_its_comm_player_is_outside_the_world() {
    use UnitList;
    let many = object_ids(0..600);
    let mut world = forage_world();
    let listed = unit_list(many.clone());
    assert_eq!(world.command_units(PlayerId::new(0), &listed), many);
    let outside = PlayerId::try_from(world.players.len()).unwrap();
    assert_eq!(world.command_units(outside, &listed), many);
    assert_eq!(world.command_units(PlayerId::new(1), &listed), many[..60]);
    assert_eq!(
        world.command_units(PlayerId::new(1), &UnitList::last_commanded()),
        Vec::<Option<ObjectId>>::new()
    );
    world.players[1].kind = PlayerKind::Computer;
    assert_eq!(world.command_units(PlayerId::new(1), &listed), many[..500]);
    assert_eq!(world.most_units(outside), 0);
    world.set_last_commanded(PlayerId::new(0), object_ids([5]));
    world.set_last_commanded(outside, object_ids([6]));
    world.set_last_commanded(PlayerId::new(1), object_ids([7]));
    assert_eq!(world.last_commanded, [vec![], object_ids([7])]);
    assert_eq!(
        world.command_units(PlayerId::new(0), &UnitList::last_commanded()),
        Vec::<Option<ObjectId>>::new()
    );
    assert_eq!(
        world.command_units(PlayerId::new(1), &UnitList::last_commanded()),
        object_ids([7])
    );
}

/// The command phase runs each building command by its handler ([`World::run`]) on
/// [`forage_world`]'s town centre 700: a queue, a research after its buildings' check, a
/// gather point, the command queue, a gate, the town bell and its all-back-to-work.
#[test]
fn the_command_phase_runs_each_building_command_by_its_handler() {
    let forage_world = forage_world();
    let tc = object_ids([700]);
    let research = || ResearchCommand {
        player: player_id(1),
        building: Some(object_id(700)),
        count: 1,
        buildings: tc.clone(),
        tech: TechId::of_data(22),
        queue_id: QueueId::of_data(-1),
        shift: false,
    };
    let queue = QueueCommand {
        player: player_id(1),
        buildings: unit_list(tc.clone()),
        building_master: master_id(109),
        unit: master_id(83),
        amount: QueuedUnits::ONE,
        button: QueueButton::of_data(0),
    };
    let changed = [
        routes(
            &forage_world,
            &Command::Queue(QueueCommand {
                player: player_id(1),
                buildings: unit_list(tc.clone()),
                building_master: master_id(109),
                unit: master_id(83),
                amount: QueuedUnits::ONE,
                button: QueueButton::of_data(0),
            }),
            PlayerId::new(1),
            |world| queue.run(world),
        ),
        routes(
            &forage_world,
            &Command::Research(ResearchCommand {
                player: player_id(1),
                building: Some(object_id(700)),
                count: 1,
                buildings: tc.clone(),
                tech: TechId::of_data(22),
                queue_id: QueueId::of_data(-1),
                shift: false,
            }),
            PlayerId::new(1),
            |world| {
                let Some(buildings) = world.research_check(&research())? else {
                    return Ok(());
                };
                world.research(&research(), &buildings)
            },
        ),
        routes(
            &forage_world,
            &Command::GatherPoint(GatherPointCommand {
                player: player_id(1),
                units: unit_list(tc.clone()),
                point: Point::new(30.5, 85.5),
                target: None,
                target_master: None,
                own: false,
            }),
            PlayerId::new(1),
            |world| {
                GatherPointCommand {
                    player: player_id(1),
                    units: unit_list(tc.clone()),
                    point: Point::new(30.5, 85.5),
                    target: None,
                    target_master: None,
                    own: false,
                }
                .run(world)
            },
        ),
        routes(
            &forage_world,
            &Command::CommandQueue(BuildingQueueCommand {
                player: player_id(1),
                units: unit_list(tc.clone()),
                target: None,
                point: Point::new(30.5, 85.5),
            }),
            PlayerId::new(1),
            |world| world.command_queue(player_id(1), &tc, None, Point::new(30.5, 85.5)),
        ),
        routes(
            &forage_world,
            &Command::Gate(GateCommand {
                player: player_id(1),
                id: object_id(700),
            }),
            PlayerId::new(1),
            |world| world.gate(player_id(1), object_id(700)),
        ),
        routes(
            &forage_world,
            &Command::TownBell(TownBellCommand {
                player: player_id(1),
                id: object_id(700),
                bell: Bell::Ring,
            }),
            PlayerId::new(1),
            |world| world.town_bell(player_id(1), object_id(700), Bell::Ring),
        ),
        routes(
            &forage_world,
            &Command::AllBackToWork(AllBackToWorkCommand {
                player: player_id(1),
                id: object_id(700),
            }),
            PlayerId::new(1),
            |world| world.all_back_to_work(player_id(1), object_id(700)),
        ),
    ];
    // the town centre's master is bare: only the gate changes it (any building of type 80
    // toggles its lock)
    assert_eq!(changed, [false, false, false, false, true, false, false]);
}

/// `AskForHelp`'s walk of the last search's own and friends' lists
/// ([`UnitView::call_for_help`] of the world's view): the asking unit passes itself; a live
/// helper with a unit AI within 3 tiles that is free ([`helper_free`]) answers, which is not
/// modelled; a busy, dead, unthinking or distant one does not. Without a search before it
/// the registry's last answer is not held.
///
/// [`UnitView::call_for_help`]: crate::unit_ai::world_view::UnitView::call_for_help
#[test]
fn call_for_help_walks_the_lists_for_free_helper_in_reach() {
    use crate::vision::registry::Record;
    let idle = UnitAi {
        ai_kind: UnitAiKind::Soldier,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::Aggressive,
        ..UnitAi::default()
    };
    let record = |id, x| Record {
        id,
        distance: Distance::MAX,
        owner: PlayerId::new(1),
        at: Point::new(x, 10.0),
        radius: Footprint::new(0.2, 0.2),
    };
    let lists = || {
        ByRelation::new([
            Vec::new(),
            vec![record(object_id(123), 10.0), record(object_id(124), 10.3)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ])
    };
    let groups = UnitGroups::default();
    let call = |helper: Object, searched| {
        help_view(helper, searched, &groups, |side| {
            side.call_for_help(object_id(50), None)
        })
    };
    assert_eq!(
        call(helper_scout(idle.clone(), None), Some(lists())),
        helper_answers()
    );
    let busy = UnitAi {
        current_order: Some(OrderKind::Attack),
        ..idle.clone()
    };
    assert_eq!(call(helper_scout(busy, None), Some(lists())), Ok(()));
    let mut dead = helper_scout(idle.clone(), None);
    dead.unit.life.state = ObjectState::Dying;
    assert_eq!(call(dead, Some(lists())), Ok(()));
    let mut unthinking = helper_scout(idle.clone(), None);
    unthinking.parts.unit_ai = None;
    assert_eq!(call(unthinking, Some(lists())), Ok(()));
    let mut far = helper_scout(idle.clone(), None);
    far.unit.place_for_test(Position::new(14.0, 10.0, 0.0));
    assert_eq!(call(far, Some(lists())), Ok(()));
    assert_eq!(
        call(helper_scout(idle, None), None),
        Err(GenieError::unsupported(
            "a search that reads the registry's last answer (before any query)"
        ))
    );
}

/// RESEARCH's handler on its listed buildings, every one kept by its check.
fn research_listed(world: &mut World, research_command: &ResearchCommand) -> GenieResult<()> {
    world.research(research_command, &research_command.buildings)
}

/// [`forage_world`] with player 1 a computer player whose AI module (`player+0x7a8`) holds a
/// path map of the map's size, explored (0) but for the tiles `open` (-1), and the tactical
/// module's slots `slots`.
fn explorer_world(open: &[[i32; 2]], slots: Vec<i32>) -> World {
    use crate::players::ai_module::{AiModule, InfoModule, TacticalModule};
    let mut world = forage_world();
    world.players[1].kind = PlayerKind::Computer;
    let mut path_map = InfluenceMap::new(GridSize::new(120, 120), 0);
    for &[x, y] in open {
        path_map.set(Tile::new(x, y), 0xff);
    }
    world.ai.set(
        PlayerId::new(1),
        AiModule {
            info: InfoModule {
                size: GridSize::new(120, 120),
                path_map,
                quadrants: [[0; 3]; 16],
                explorer: None,
                search: Vec::new(),
            },
            tactical: TacticalModule { slot_times: slots },
        },
    );
    world
}

/// Player 1's villager 9175 asks its owner to hear that its Explore (605) waits for a point
/// (the base handler's case 0x202 hands the notify on): what the AI decided, as the stop that
/// its tasking or detasking ends in names it.
fn explorer_hears(world: &mut World) -> String {
    let heard = world.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
        side.owner_hears(
            Notify::own(object_id(9175), NotifyKind::REQUIRES_SEARCH, [605, 0, 0]),
            None,
        )
    });
    format!("{:?}", heard.unwrap_err())
}

/// A computer player's AI hears its explorer wanting a point: with the wait slot -1 early in
/// the game it pushes the rings about its town centre (700 at (20, 89.5), line of sight 5; the
/// explorer's 4, so the rings lie 12, 19, 26, 33 and 40 tiles out), takes the top point, (28, 80),
/// which the path map holds unexplored, and searches from it: the explorer reaches it, so the AI
/// takes the explorer's place and tasks the unit to it, and the tile becomes explored (0).
#[test]
fn computer_players_ai_tasks_its_explorer_to_the_unexplored_tile() {
    let mut world = explorer_world(&[[28, 80]], vec![]);
    for masters in &mut world.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .line_of_sight = Distance::new(4.0);
        Arc::make_mut(masters)
            .get_mut(ids::TOWN_CENTRE)
            .unwrap()
            .line_of_sight = Distance::new(5.0);
    }
    // no sight: the player sees no tile now
    world.spatial.sight = None;
    let said = explorer_hears(&mut world);
    assert!(said.contains("tasks explorer"), "{said}");
    assert!(said.contains("(28, 80)"), "{said}");
    let info = &world.ai.get(PlayerId::new(1)).unwrap().info;
    assert_eq!(info.explorer, ObjectId::of_field(9175));
    assert_eq!(info.path_map.get(Tile::new(28, 80)), Some(0));
    // the rings' other points wait for the next notify
    assert_eq!(info.search.len(), 30);
}

/// With every tile explored the search finds none and the AI detasks the explorer.
#[test]
fn computer_players_ai_detasks_an_explorer_with_nothing_left_to_explore() {
    let mut world = explorer_world(&[], vec![]);
    let said = explorer_hears(&mut world);
    assert!(said.contains("detasks explorer"), "{said}");
}

/// A human player's owner hears nothing the simulation reads, and a notify of another
/// kind is not the explore case.
#[test]
fn only_computer_players_explorer_notify_reaches_the_ai() {
    let mut human = explorer_world(&[[31, 89]], vec![]);
    human.players[1].kind = PlayerKind::Human;
    let before = human.ai.clone();
    human
        .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            side.owner_hears(
                Notify::own(object_id(9175), NotifyKind::REQUIRES_SEARCH, [605, 0, 0]),
                None,
            )
        })
        .unwrap();
    assert_eq!(human.ai, before);
    let mut world = explorer_world(&[[31, 89]], vec![]);
    world
        .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            side.owner_hears(
                Notify::own(object_id(9175), NotifyKind::from(0x201), [605, 0, 0]),
                None,
            )?;
            side.owner_hears(
                Notify::own(object_id(9175), NotifyKind::REQUIRES_SEARCH, [999, 0, 0]),
                None,
            )
        })
        .unwrap();
}

/// Object `value` as a container.
fn container_id(value: i32) -> ContainerId {
    ContainerId::new(object_id(value))
}

/// A frozen unit's place grows by its master's bound of speed as the master stands at each tick
/// ([`World::grow_unknown`], [`MotionBound::speed`]): its speed (`+0x740`) 0.8 times the factor
/// 1.5 of its standing sprite for 2 s, 2.4 tiles; one filed under a master its player's table
/// does not hold may be anywhere.
#[test]
fn frozen_unit_may_have_moved_its_master_speed() {
    let mut world = forage_world();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let mut placed = *world.spatial.get(id).unwrap();
    // the spatial index's master of the object, which its growth reads
    let forager = Arc::make_mut(&mut world.model.masters[1])
        .get_mut(placed.master)
        .unwrap();
    move_by(forager, walking(Speed::new(0.8)));
    forager.standing_sprite = Some(SpriteId::named(1));
    let fast = Graphic {
        id: SpriteId::named(1),
        frames: Frame::of_data(1),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::of_data(1.5),
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    world.model.graphics = Arc::new(Graphics::from([(fast.id, fast)]));
    placed.moves = true;
    world.spatial.insert(id, placed);
    world.spatial.freeze(id);
    world.grow_unknown(Seconds::new(2.0)).unwrap();
    assert_eq!(
        world.spatial.get(id).unwrap().unknown,
        Some(0.8 * 1.5 * 2.0)
    );
    // the spatial index files it under a master its player's table lacks (an own master's id)
    let mut placed = *world.spatial.get(id).unwrap();
    placed.master = master_id(999);
    world.spatial.insert(id, placed);
    world.spatial.freeze(id);
    world.grow_unknown(Seconds::new(0.05)).unwrap();
    assert_eq!(world.spatial.get(id).unwrap().unknown, Some(f32::INFINITY));
}

/// A world made from its parts (a load, a test's world) gets the spatial index's views once
/// ([`World::sync_world`]): how the players stand to each other; after it the routines that
/// change them write them.
#[test]
fn world_sync_fills_the_relations() {
    let mut world = forage_world();
    world.spatial.relations = ByPlayer::default();
    world.sync_world();
    assert_eq!(world.spatial.relations.len(), world.players.len());
    for (player_id, row) in world.spatial.relations.iter() {
        assert_eq!(row, &world.players[player_id.index()].relations);
    }
}

/// A death reads the master the object has now (`RGE_Animated_Object::update`, slot 0x7a0):
/// a unit whose master gives it a dying sprite plays it (state 4); one whose master gives none
/// is dead at once (state 5), whatever master it was loaded with.
#[test]
fn death_reads_the_dying_sprite_of_the_master_the_object_has_now() {
    let dying = SpriteId::named(1203);
    let state_after_death = |death_sprite| {
        let unit = Unit {
            id: object_id(5),
            life: Lifecycle {
                class: Class::Animated,
                hp: HitPoints::ZERO,
                master_id: master_id(212),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![unit].into(),
                ..Player::default()
            }],
        );
        world.model.masters = vec![Arc::new(Masters::new([Master {
            id: master_id(212),
            death_sprite,
            life: Some(MasterData::default()),
            ..Master::default()
        }]))];
        world.model.graphics = Arc::new(Graphics::from([(
            dying,
            Graphic {
                id: dying,
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(1),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        )]));
        world.tick(Millis::new(10));
        world.players[0].objects[ListIndex::new(0)].life.state
    };
    assert_eq!(state_after_death(None), ObjectState::Dead);
    assert_eq!(state_after_death(Some(dying)), ObjectState::DyingSprite);
}
