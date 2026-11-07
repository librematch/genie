//! A building's relics and livestock (`FUN_1411cb850`, `FUN_1411d6ad0`): their counts as they go
//! in and out, the relics its player holds, the technologies updated as a relic comes in and
//! those paid in relics that go back as one leaves.

use crate::garrison::ContainerId;
use crate::garrison::Kept;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::MasterHitPoints;
use crate::objects::masters::ClassId;
use crate::objects::masters::CostAmount;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::ObjectFlags;
use crate::players::Amount;
use crate::players::Culture;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::effect::Change;
use crate::technology::{
    Cost, CostPaid, Effect, EffectCommand, EffectId, Effects, Party, ResearchState, TechDef,
    TechState,
};
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::world::PlayState;
use crate::world::World;
use crate::world::WorldTime;
use std::sync::Arc;

/// The building (type 80) of player 1 the units enter.
const BUILDING: i32 = 7;
/// Player 1's transport (type 70, object flag 0).
const TRANSPORT: i32 = 8;
/// Gaia's relic (class 42).
const RELIC: i32 = 10;
/// Player 1's sheep (class 58), each of an amount held of 100.
const SHEEP: i32 = 11;
/// A second sheep.
const EWE: i32 = 12;
/// A unit of another class (the infantry's, 6).
const SOLDIER: i32 = 13;
/// The master no object is of, whose hit points the relic technology's effect adds to.
const BLESSED: MasterId = MasterId::named(50);
/// The class of no meaning to the counts.
const INFANTRY: ClassId = ClassId::named(6);

/// A master `id` of type `kind` and class `class`, of radius 0.2 (a building's 1).
fn master(id: i16, kind: MasterKind, class: ClassId) -> Master {
    let radius = if kind == MasterKind::BUILDING {
        1.0
    } else {
        0.2
    };
    Master {
        id: MasterId::named(id),
        kind,
        class,
        radius: Footprint::new(radius, radius),
        garrison: GarrisonCapacity::of_data(10),
        ..Master::default()
    }
}

/// The sheep's master: class 58, its first storage 100 of food.
fn sheep_master() -> Master {
    Master {
        storage: vec![(Some(attribute::FOOD), Amount::new(100.0), 1)],
        ..master(594, MasterKind::COMBAT, class::LIVESTOCK)
    }
}

/// Player 1's building and transport, Gaia's relic, player 1's sheep and soldier, outside, on a
/// world whose players hold 40 attributes (none set) and player 1 the culture `culture`; the
/// world's time is 7 s.
fn relics_world(culture: Culture) -> World {
    let unit =
        |id, player, master, x| SceneObject::new(id, player, master, Position::new(x, 1.5, 0.0));
    let mut world = Scene::open(GridSize::new(12, 12))
        .object(SceneObject::new(
            BUILDING,
            PLAYER,
            master(104, MasterKind::BUILDING, class::TOWER),
            Position::new(5.0, 5.0, 0.0),
        ))
        .object(SceneObject::new(
            TRANSPORT,
            PLAYER,
            Master {
                object_flags: ObjectFlags::GARRISON,
                ..master(545, MasterKind::COMBAT, class::TRANSPORT)
            },
            Position::new(9.5, 9.5, 0.0),
        ))
        .object(unit(
            RELIC,
            PlayerId::GAIA,
            master(285, MasterKind::COMBAT, class::RELIC),
            1.5,
        ))
        .object(unit(SHEEP, PLAYER, sheep_master(), 2.5))
        .object(unit(EWE, PLAYER, sheep_master(), 3.5))
        .object(unit(
            SOLDIER,
            PLAYER,
            master(4, MasterKind::COMBAT, INFANTRY),
            4.5,
        ))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 40];
    }
    world.players[1].culture = culture;
    world.world_time = WorldTime::from_millis(7000);
    world
}

/// Unit `id` of player `owner` goes into `container` at once (slot 0x1a0 with 1).
fn enter(world: &mut World, owner: PlayerId, id: i32, container: i32) {
    let index = world.live_index(owner, object_id(id)).unwrap();
    world
        .enter_forced(owner, index, ContainerId::new(object_id(container)))
        .unwrap();
}

/// Unit `id` of player `owner` leaves `container` for a spot beside it (slot 0x1a8, then slot
/// 0x58).
fn leave(world: &mut World, owner: PlayerId, id: i32, container: i32) {
    let held = world.live_index(PLAYER, object_id(container)).unwrap();
    let index = world.live_index(owner, object_id(id)).unwrap();
    let made = world.players[owner.index()].objects[index]
        .master(world.master_tables(), owner)
        .unwrap()
        .clone();
    world
        .leave_container(
            (PLAYER, held),
            (owner, index),
            &made,
            Position::new(2.5, 3.5, 0.0),
        )
        .unwrap();
}

/// `container`'s relic count, its livestock summation and its last garrison time.
fn kept(world: &World, container: i32) -> (i32, f32, WorldTime) {
    let layer = world.parts.building_of(object_id(container));
    (
        layer.relics.count.get(),
        layer.livestock_summation.get(),
        layer.garrison_time,
    )
}

/// Player 1's relics held (attribute 7).
fn relics_held(world: &World) -> f32 {
    world.players[1].attributes[7].get()
}

/// What a building keeps of a unit: a relic by class 42, livestock by class 58 with its master's
/// amount held, nothing of another class or for a player without a culture.
#[test]
fn building_keeps_relics_and_livestock_of_player_of_culture() {
    let roles = relics_world(Culture::NONE).model.roles;
    let relic = master(285, MasterKind::COMBAT, class::RELIC);
    let soldier = master(4, MasterKind::COMBAT, INFANTRY);
    let culture = Culture::of_data(2);
    assert_eq!(Kept::of_unit(culture, &relic, &roles), Some(Kept::Relic));
    assert_eq!(
        Kept::of_unit(culture, &sheep_master(), &roles),
        Some(Kept::Livestock(Amount::new(100.0)))
    );
    assert_eq!(Kept::of_unit(culture, &soldier, &roles), None);
    assert_eq!(Kept::of_unit(Culture::NONE, &relic, &roles), None);
    assert_eq!(Kept::of_unit(Culture::NONE, &sheep_master(), &roles), None);
}

/// A relic of Gaia's in a building of a player of a culture counts in the building's
/// `relic_count` (`+0xc20`) and the player's relics held (attribute 7) one more, and the
/// building's `last_garrison_time` (`+0xbe0`) takes the world's time; its leave counts both one
/// less and leaves the time.
#[test]
fn relic_counts_in_and_out_of_building() {
    let mut world = relics_world(Culture::of_data(2));
    enter(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(
        kept(&world, BUILDING),
        (1, 0.0, WorldTime::from_millis(7000))
    );
    assert_eq!(relics_held(&world), 1.0);
    world.world_time = WorldTime::from_millis(9000);
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(
        kept(&world, BUILDING),
        (0, 0.0, WorldTime::from_millis(7000))
    );
    assert_eq!(relics_held(&world), 0.0);
}

/// Each sheep adds its master's amount held (`+0x610`) to the building's `mLivestockSummation`
/// (`+0xc28`) and takes it off as it leaves; the relic count and the time stay.
#[test]
fn livestock_sums_its_amount_held_in_and_out() {
    let mut world = relics_world(Culture::of_data(2));
    enter(&mut world, PLAYER, SHEEP, BUILDING);
    enter(&mut world, PLAYER, EWE, BUILDING);
    assert_eq!(kept(&world, BUILDING), (0, 200.0, WorldTime::ZERO));
    leave(&mut world, PLAYER, SHEEP, BUILDING);
    assert_eq!(kept(&world, BUILDING), (0, 100.0, WorldTime::ZERO));
}

/// A building of a player without a culture (`+0x88` 0), a transport and a unit of another
/// class keep no count as the units go in and out: the relic count, the summation, the time and
/// the relics held stay.
#[test]
fn no_culture_transport_or_other_class_keeps_no_count() {
    let mut world = relics_world(Culture::NONE);
    enter(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    enter(&mut world, PLAYER, SHEEP, BUILDING);
    assert_eq!(kept(&world, BUILDING), (0, 0.0, WorldTime::ZERO));
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    leave(&mut world, PLAYER, SHEEP, BUILDING);
    assert_eq!(kept(&world, BUILDING), (0, 0.0, WorldTime::ZERO));
    assert_eq!(relics_held(&world), 0.0);
    let mut world = relics_world(Culture::of_data(2));
    enter(&mut world, PlayerId::GAIA, RELIC, TRANSPORT);
    enter(&mut world, PLAYER, SOLDIER, BUILDING);
    assert_eq!(kept(&world, TRANSPORT), (0, 0.0, WorldTime::ZERO));
    assert_eq!(kept(&world, BUILDING), (0, 0.0, WorldTime::ZERO));
    assert_eq!(relics_held(&world), 0.0);
    leave(&mut world, PlayerId::GAIA, RELIC, TRANSPORT);
    leave(&mut world, PLAYER, SOLDIER, BUILDING);
    assert_eq!(kept(&world, TRANSPORT), (0, 0.0, WorldTime::ZERO));
    assert_eq!(kept(&world, BUILDING), (0, 0.0, WorldTime::ZERO));
    assert_eq!(relics_held(&world), 0.0);
}

/// A relic's enter updates every technology of the building's player (`FUN_1413cce70` with
/// −1): one whose requirements are met turns available; a sheep's updates none.
#[test]
fn relic_enter_updates_every_technology() {
    let state = |world: &World| world.players[1].techs[0].state;
    let mut world = relics_world(Culture::of_data(2));
    world.model.techs = Arc::new(vec![TechDef::default()]);
    world.players[1].techs = vec![TechState::default()];
    enter(&mut world, PLAYER, SHEEP, BUILDING);
    assert_eq!(state(&world), ResearchState::Unavailable);
    enter(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(state(&world), ResearchState::Available);
}

/// A technology paid in `relics` relics (its first cost) of effect 0.
fn relic_tech(attribute: AttributeId, relics: i16) -> TechDef {
    TechDef {
        costs: [
            Cost {
                attribute: Some(attribute),
                amount: CostAmount::of_data(relics),
                paid: CostPaid::PLAYER,
            },
            Cost::default(),
            Cost::default(),
        ],
        effect_id: EffectId::of_data(0),
        ..TechDef::default()
    }
}

/// [`relics_world`] with a master of 50 hit points no object is of ([`BLESSED`]), effect 0 that
/// adds 10 to its hit points (and an ally's add and an own multiply, which a withdrawal leaves),
/// and player 1's technologies: researched paid in 2 relics, researched paid in 1, available paid
/// in 5, researched paid in 9 wood. Player 1 holds one relic, and a second goes into its
/// building.
fn withdrawal_world() -> World {
    let mut world = relics_world(Culture::of_data(2));
    for masters in &mut world.model.masters {
        let blessed = Master {
            hit_points: MasterHitPoints::of_data(50),
            ..master(BLESSED.get(), MasterKind::COMBAT, INFANTRY)
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([blessed])));
    }
    let add = |party, change, amount| EffectCommand::Attribute {
        party,
        change,
        master: Some(BLESSED),
        class: None,
        attribute: MasterAttribute::HIT_POINTS,
        amount,
    };
    world.model.effects = Arc::new(Effects::new(vec![Effect {
        commands: vec![
            add(Party::Own, Change::Add, 10.0),
            add(Party::Allies, Change::Add, 5.0),
            add(Party::Own, Change::Multiply, 2.0),
        ],
    }]));
    world.model.techs = Arc::new(vec![
        relic_tech(attribute::RELICS_HELD, 2),
        relic_tech(attribute::RELICS_HELD, 1),
        relic_tech(attribute::RELICS_HELD, 5),
        relic_tech(attribute::WOOD, 9),
    ]);
    let state = |research_state| TechState {
        state: research_state,
        ..TechState::default()
    };
    world.players[1].techs = vec![
        state(ResearchState::Researched),
        state(ResearchState::Researched),
        state(ResearchState::Available),
        state(ResearchState::Researched),
    ];
    world.players[1].attributes[7] = Amount::new(1.0);
    enter(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    world
}

/// Player 1's technologies' states and its master [`BLESSED`]'s hit points.
fn techs_and_blessing(world: &World) -> (Vec<ResearchState>, i16) {
    let states = world.players[1]
        .techs
        .iter()
        .map(|tech_state| tech_state.state)
        .collect();
    let hit_points = world.model.masters[1]
        .get(BLESSED)
        .unwrap()
        .hit_points
        .get();
    (states, hit_points)
}

/// As a relic leaves, each researched technology paid in more relics than the player now holds
/// goes back: its effect's own adds are undone by the player's slot 0x158 (the amount negated:
/// the hit points 50 less 10), and it turns available; one paid in as many relics as are held,
/// one not researched and one paid in wood stay.
#[test]
fn relic_leave_withdraws_technologies_paid_in_more_relics() {
    use ResearchState::{Available, Researched};
    let mut world = withdrawal_world();
    assert_eq!(relics_held(&world), 2.0);
    assert_eq!(
        techs_and_blessing(&world),
        (vec![Researched, Researched, Available, Researched], 50)
    );
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(relics_held(&world), 1.0);
    assert_eq!(
        techs_and_blessing(&world),
        (vec![Available, Researched, Available, Researched], 40)
    );
}

/// In a world being cleared (state 2) a relic's leave takes back no technology, nor while the
/// player's relics held are a NaN (`COMISS`, `JBE`); the counts go down all the same.
#[test]
fn relic_leave_withdraws_nothing_while_clearing_or_on_nan() {
    use ResearchState::{Available, Researched};
    let kept_states = (vec![Researched, Researched, Available, Researched], 50);
    let mut world = withdrawal_world();
    world.game_state = PlayState::Clearing;
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(relics_held(&world), 1.0);
    assert_eq!(kept(&world, BUILDING).0, 0);
    assert_eq!(techs_and_blessing(&world), kept_states);
    let mut world = withdrawal_world();
    world.players[1].attributes[7] = Amount::new(f32::NAN);
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert!(relics_held(&world).is_nan());
    assert_eq!(techs_and_blessing(&world), kept_states);
}

/// A researched technology paid in more relics than are held whose effect the table lacks
/// (`FUN_140ec4510` returns none, `0x1411d6f1c`) still turns available as a relic leaves, and
/// no master changes.
#[test]
fn relic_leave_withdraws_technology_without_its_effect() {
    use ResearchState::{Available, Researched};
    let mut world = withdrawal_world();
    Arc::make_mut(&mut world.model.techs)[0].effect_id = EffectId::of_data(5);
    leave(&mut world, PlayerId::GAIA, RELIC, BUILDING);
    assert_eq!(relics_held(&world), 1.0);
    assert_eq!(
        techs_and_blessing(&world),
        (vec![Available, Researched, Available, Researched], 50)
    );
}
