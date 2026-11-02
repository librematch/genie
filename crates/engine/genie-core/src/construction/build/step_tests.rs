//! The Build's update on the step ([`BuildAction::update`] through [`Unit::build_part`]), on
//! World fixtures: the state of the builder, its foundation and the world after each update.

use crate::construction::MainBuildingId;
use crate::gathering::DropSiteId;
use crate::map::GridSize;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::movement::Speed;
use crate::movement::move_to::MoveWaitState;
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionType;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::players::Amount;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::test_ids::carrying;
use crate::unit_ai::NotifyKind;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::WorldTime;

use super::tests::{MOVING_TASK, TASK, started};
use super::*;
use crate::map::Footprint;
use crate::map::spatial::{Placed, Tiles};
use crate::math::rng::Well512;
use crate::movement::Me;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait, TargetMoved};
use crate::objects::ObjectId;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ClassId;
use crate::objects::masters::Masters;
use crate::objects::masters::class;
use crate::objects::masters::ids::PASTURE_1889;
use crate::objects::masters::master_type;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::unit_ai::{ActionKind, OrderKind, QueuedOrder, UnitAi};
use crate::world::parts::Object;
use crate::world::step::Step;
use crate::world::tests::in_update;
use crate::world::{Player, World};

/// The builder of [`site_world`].
const BUILDER: ObjectId = object_id(5);
/// Its foundation, a house.
const HOUSE: ObjectId = object_id(10028);
/// The house as the Build's foundation.
const HOUSE_SITE: crate::construction::FoundationId = crate::construction::FoundationId::new(HOUSE);
/// The house's place.
const AT_HOUSE: Position = Position::new(10.5, 10.5, 0.0);
/// The builder's walking sprite, which its `MoveTo`s show by default, and its standing sprite,
/// which `set_state` 1, 2, 3 and 13 show (slot 0x68).
const WALKING: SpriteId = SpriteId::named(10);
const STANDING: SpriteId = SpriteId::named(11);
/// The moving sprite of [`MOVING_TASK`].
const TASK_SPRITE: SpriteId = SpriteId::named(3);

/// A looped sprite of `id`: 10 frames of 16 facets.
const fn graphic(id: SpriteId) -> Graphic {
    Graphic {
        id,
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    }
}

/// The house master 70 of [`crate::world::tests::builder_masters`] as master `id` of `class`.
fn house_as(masters: &Masters, id: MasterId, class: ClassId) -> Master {
    Master {
        id,
        class,
        ..masters.get(ids::HOUSE).cloned().unwrap()
    }
}

/// A player's object `id` on the map: radius `r`, `class`, master `master`, tall.
const fn placed(at: Position, r: f32, class: ClassId, master: MasterId) -> Placed {
    Placed {
        position: at,
        radius: Footprint::new(r, r),
        class,
        player: PlayerId::new(1),
        master,
        obstruction_type: ObstructionType::TYPE_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    }
}

/// A foundation's construction: `progress` seconds of 25, `builders`, `desolid`.
const fn construction(progress: f32, builders: u8, desolid: bool) -> Construction {
    Construction {
        progress: Seconds::new(progress),
        desolid,
        builders: crate::construction::BuilderCount::of_data(builders),
        hit_points: MasterHitPoints::of_data(550),
        build_time: WholeSeconds::of_data(25),
        record: Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::UNIT,
        },
        snow: false,
        sprite_override: false,
    }
}

/// Player 1's builder 5 (the villager master 83 of [`crate::world::tests::builder_masters`] of
/// `class`, walking sprite [`WALKING`], standing sprite [`STANDING`]; a walker of the hunt's
/// tests, last moved at `at`, showing [`WALKING`]; a civilian unit AI without orders) at `at`,
/// building `build`, and its desolid house foundation 10028 (master 70, state 0, 1 hit point,
/// taken by the builder) at (10.5, 10.5), on 20 × 20 tiles of terrain 0 without planners. The
/// masters hold the house as 562 (a lumber camp, a drop site), 1889 (a pasture), 1584 (a gate,
/// class 39), 68 (a mill) and 50 (a farm, class 49).
fn site_world(class: ClassId, at: Position, build: BuildAction) -> World {
    let mut masters = crate::world::tests::builder_masters();
    let others = [
        house_as(&masters, master_id(562), class::BUILDING),
        house_as(&masters, PASTURE_1889, class::BUILDING),
        house_as(&masters, master_id(1584), class::GATE),
        house_as(&masters, master_id(68), class::BUILDING),
        house_as(&masters, ids::FARM, class::FARM),
    ];
    masters = Masters::new(masters.iter().cloned().chain(others));
    let v = masters.get_mut(ids::VILLAGER_MALE).unwrap();
    (v.class, v.move_sprite, v.standing_sprite) = (class, Some(WALKING), Some(STANDING));
    let mut mover = crate::gathering::hunt::tests::walker();
    mover.moved_at = at;
    let builder = Object::with_actions(
        Unit {
            id: BUILDER,
            place: crate::objects::layers::Static::at(at),
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::new(25.0),
                master_id: ids::VILLAGER_MALE,
                attribute_type: None,
                ..Lifecycle::default()
            },
            sprite: Some(ActiveSprite::start(
                graphic(WALKING),
                &mut Well512::default(),
            )),
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        },
        crate::objects::layers::Action {
            list: ActionList::default().with_build(Some(build)),
            ..Default::default()
        },
    );
    let house = Unit {
        id: HOUSE,
        place: crate::objects::layers::Static::at(AT_HOUSE),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Foundation,
            hp: HitPoints::new(1.0),
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        workers: crate::objects::WorkerCount::of_data(1),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![builder.unit, house].into(),
                attributes: vec![Amount::ZERO; 250],
                relations: relation_row([1, 0]),
                ..Player::default()
            },
        ],
    )
    .with_buildings([(
        HOUSE,
        crate::objects::layers::Building {
            construction: Some(construction(0.0, 0, true)),
            ..Default::default()
        },
    )])
    .with_motions([(BUILDER, crate::objects::layers::Motion::Moving(mover))])
    .with_actions([(BUILDER, builder.parts.actions)])
    .with_ais([(
        BUILDER,
        UnitAi {
            ai_type: crate::unit_ai::CIVILIAN,
            current_target: None,
            last_target: None,
            ..UnitAi::default()
        },
    )]);
    t.masters = vec![masters.clone(), masters];
    t.graphics = Graphics::from([
        (WALKING, graphic(WALKING)),
        (STANDING, graphic(STANDING)),
        (TASK_SPRITE, graphic(TASK_SPRITE)),
        (SpriteId::named(7), graphic(SpriteId::named(7))),
    ]);
    t.spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![0; 400]),
    );
    t.spatial
        .insert(HOUSE, placed(AT_HOUSE, 1.0, class::BUILDING, ids::HOUSE));
    let me = Placed {
        obstruction_type: ObstructionType::NONE,
        ..placed(at, 0.2, class, ids::VILLAGER_MALE)
    };
    t.spatial.insert(BUILDER, me);
    t
}

/// The world's planners for its 20 × 20 tiles and their shape mesh.
fn plan(t: &mut World) {
    t.pathing = Pathing::new(
        GridSize::new(20, 20),
        vec![TerrainId::named(0); 400],
        vec![Restriction::new(vec![1.0]); 8],
        0,
    );
    t.pathing.start_mesh(20);
}

/// The builder's top action right after its update: its type and target.
type Top = (Option<ActionType>, Option<ObjectId>);

/// One update of the builder's Build of `dt` ms (its part of the unit's update,
/// [`Unit::build_part`]), its code and the builder's top action right after it (the work it
/// gives within the update, slot 0x7e8, replaces the Build).
fn update(t: &mut World, dt: u32) -> Result<(HeadCode, Top), GenieError> {
    in_update(
        t,
        (PlayerId::new(1), ListIndex::new(0)),
        dt,
        |u, actions, c| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: u.id,
                group: u.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |a| a.state),
            };
            let code = u.build_part(actions, c, me)?;
            Ok((code, (actions.top_action_kind(), actions.top_target())))
        },
    )
}

/// [`update`]'s code alone.
fn code(t: &mut World, dt: u32) -> HeadCode {
    update(t, dt).unwrap().0
}

/// Runs `f` on the builder's Build with its step and what the update reads as it starts
/// ([`Unit::build_reads`]), then puts the Build back.
fn on_build<R>(
    t: &mut World,
    f: impl FnOnce(&mut BuildAction, &mut Step<'_>, &mut Unit, &BuildReads) -> Result<R, GenieError>,
) -> Result<R, GenieError> {
    in_update(
        t,
        (PlayerId::new(1), ListIndex::new(0)),
        20,
        |u, actions, c| {
            let mut b = *actions.list.build().unwrap();
            let reads = u.build_reads(c, b)?;
            let answer = f(&mut b, c, u, &reads);
            *actions.list.build_mut().unwrap() = b;
            answer
        },
    )
}

/// The builder.
fn builder(t: &World) -> &Unit {
    t.players[1].objects.by_id(BUILDER).unwrap()
}

/// The builder's Build.
fn build(t: &World) -> BuildAction {
    *t.parts.actions_of(BUILDER).list.build().unwrap()
}

/// Player 1's object `id`.
fn own(t: &mut World, id: ObjectId) -> &mut Unit {
    t.players[1].objects.by_id_mut(id).unwrap()
}

/// The builder took its standing sprite (slot 0x68).
fn stands(t: &World) -> bool {
    builder(t).sprite.map(|s| s.graphic.id) == Some(STANDING)
}

/// The notifies the builder's unit AI heard.
fn heard(t: &World) -> Vec<(NotifyKind, [i32; 3])> {
    let ai = t.parts.unit_ai.get(builder(t).id).unwrap();
    ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
}

/// The notify 0x202 a Build without a target posts to its unit AI.
const SEARCH: (NotifyKind, [i32; 3]) = (
    NotifyKind::REQUIRES_SEARCH,
    [ActionKind::Build.value(), 0, 0],
);

/// The house's builders, workers and state.
fn house(t: &World) -> (u8, i32, ObjectState) {
    let u = t.players[1].objects.by_id(HOUSE).unwrap();
    (
        t.parts.repairers_of(HOUSE).get(),
        u.workers.get(),
        u.life.state,
    )
}

/// A copy of player 1's object `id` with its parts, to put in under another id.
fn copy_of(t: &World, id: ObjectId) -> Object {
    t.object_of(t.players[1].objects.by_id(id).unwrap())
}

/// Moves object `id` of player 1 to `at`, its body and its place on the map.
fn put(t: &mut World, id: ObjectId, at: Position) {
    own(t, id).place_for_test(at);
    let p = *t.spatial.get(id).unwrap();
    t.spatial.insert(id, Placed { position: at, ..p });
}

/// The foundation leaves the world: its list and the map.
fn remove_house(t: &mut World) {
    let i = t.players[1].objects.position_of(HOUSE).unwrap();
    let _ = t.players[1].objects.take_out(i).unwrap();
    t.spatial.remove(HOUSE);
}

/// `FUN_140aaddb0`: a Build on a building annex (`+0xbb8`) counts its builder in the main
/// building. The join (`0x140aae200` case 7) raises the main building's count and leaves the
/// annex's; the work's extra-builder factor (`0x140ab0370` case 6) reads the main building's
/// two builders; the prelude of the `set_state(4)` of slot 0x48 on the annex again
/// (`FUN_140ab1b30`, [`BuildAction::retake`]) lowers the main building's count.
#[test]
fn builder_on_an_annex_counts_among_its_main_building_builders() {
    const ANNEX: ObjectId = object_id(10029);
    let b = BuildAction {
        state: BuildState::Wait,
        timer: Seconds::new(0.01),
        ..started(ANNEX, Position::ORIGIN, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), b);
    // another builder works on the main building, below 1 hit point: the work stays the annex's
    t.parts.building_mut(HOUSE).unwrap().construction = Some(construction(1.0, 1, false));
    own(&mut t, HOUSE).life.hp = HitPoints::new(0.5);
    let mut annex = copy_of(&t, HOUSE);
    annex.unit.id = ANNEX;
    annex.parts.building.annex_of = Some(MainBuildingId::new(HOUSE));
    annex.unit.life.hp = HitPoints::new(1.0);
    annex.parts.building.construction = Some(construction(1.0, 0, false));
    let _ = t.push_live(PlayerId::new(1), annex).unwrap();
    t.spatial.insert(
        ANNEX,
        placed(
            Position::new(12.5, 10.5, 0.0),
            1.0,
            class::BUILDING,
            ids::HOUSE,
        ),
    );
    let _ = code(&mut t, 30);
    let annex_of = |t: &World| {
        t.parts
            .building_of(t.players[1].objects.by_id(ANNEX).unwrap().id)
            .construction
    };
    let b = build(&t);
    assert_eq!(
        (b.state, b.added, house(&t).0),
        (BuildState::Place, true, 2)
    );
    assert_eq!(annex_of(&t).map(|k| k.builders.get()), Some(0));
    let _ = code(&mut t, 30);
    let rate = 1.0f32 * 1.0 * Millis::new(30).seconds();
    let n = 2.0f32;
    assert_eq!(
        annex_of(&t).map(|k| k.progress),
        Some(Seconds::new(
            1.0 + rate * (((n - 1.0) * EXTRA_BUILDER + 1.0) / n)
        ))
    );
    let mut b = build(&t);
    let main = t
        .parts
        .building_mut(HOUSE)
        .unwrap()
        .construction
        .as_mut()
        .unwrap();
    b.retake(
        Position::ORIGIN,
        None,
        None,
        Some(&mut main.builders),
        false,
    )
    .unwrap();
    assert_eq!(
        (b.state, b.added, house(&t).0),
        (BuildState::Move { move_to: None }, false, 1)
    );
}

/// records/425906646's update 3145: a builder carrying wood when its lumber camp's work ends
/// (`set_state(224)`) drops it there (`FUN_140aadc50`: the player gains it, the builder carries
/// nothing); food kinds become food; at a house it keeps its load, as one carrying nothing.
#[test]
fn builder_drops_its_load_at_drop_site_it_completes() {
    let finish = |held: Held, site: MasterId| {
        let b = BuildAction {
            state: BuildState::Place,
            ..started(HOUSE, AT_HOUSE, Distance::ZERO, TASK)
        };
        let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), b);
        let v = own(&mut t, BUILDER);
        (v.life.attribute_type, v.life.amount) = (held.kind, held.amount);
        own(&mut t, HOUSE).life.master_id = site;
        let state = on_build(&mut t, |b, c, u, reads| {
            b.finish(c, u, reads)?;
            Ok(b.state)
        });
        assert_eq!(state, Ok(BuildState::Finished));
        let v = builder(&t);
        // the four resources live apart from the other attributes
        let gained: Vec<(usize, f32)> = (0..4)
            .map(|k| (k, t.players[1].attributes[k].get()))
            .filter(|&(_, a)| a != 0.0)
            .collect();
        (gained, v.life.held())
    };
    let wood = 6.881_128;
    assert_eq!(
        finish(carrying(Some(attribute::WOOD), wood), master_id(562)),
        (vec![(1, wood)], Held::default())
    );
    assert_eq!(
        finish(carrying(Some(attribute::FARM_FOOD), wood), master_id(562)),
        (vec![(0, wood)], Held::default())
    );
    assert_eq!(
        finish(carrying(Some(attribute::WOOD), wood), ids::HOUSE),
        (vec![], carrying(Some(attribute::WOOD), wood))
    );
    assert_eq!(
        finish(carrying(None, 0.0), master_id(562)),
        (vec![], Held::default())
    );
}

/// 428326873's builders 8774 and 8775 (player 6) walking to the foundation 10913 that its
/// owner deletes in update 4626 (state 7, still in the world): the `MoveTo`'s update finds
/// its target above state 6 and returns 5 (quiet); the Build lets the foundation go (slot
/// 0x68 with none: out of its builders, the release) and walks to its point (`set_state(11)`:
/// the old `MoveTo` stops the unit, a quiet `MoveTo` to the point starts in state 3, with the
/// task's moving sprite: none).
#[test]
fn builder_whose_foundation_dies_walks_to_its_point() {
    // a drop's sprite, which the walk to the point drops for the task's moving one (none)
    let b = BuildAction {
        state: BuildState::Move {
            move_to: Some(MoveToAction {
                sprite: Some(graphic(SpriteId::named(7))),
                ..walk_in(MoveTo::Moving)
            }),
        },
        added: true,
        ..started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(8.0, 9.0, 0.0), b);
    own(&mut t, HOUSE).life.state = ObjectState::Removed;
    t.parts.building_mut(HOUSE).unwrap().construction = Some(construction(3.0, 1, false));
    assert_eq!(code(&mut t, 20), HeadCode::Keeps);
    let b = build(&t);
    assert_eq!(
        (b.state.bare(), b.target, b.added),
        (BuildState::WalkPoint { move_to: None }, None, false)
    );
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (
            walk.state,
            ObjectId::field(walk.target),
            walk.point,
            b.point
        ),
        (MoveTo::Start, -1, AT_HOUSE, AT_HOUSE)
    );
    // the task gives no sprite: FUN_140aa0d30's default, the master's walking one
    assert_eq!(walk.sprite.map(|g| g.id), Some(WALKING));
    let mover = t.parts.mover_of(BUILDER).unwrap();
    assert_eq!(mover.velocity.unwrap().velocity.y(), 0.0);
    assert_eq!((house(&t).0, house(&t).1), (0, 0));
    assert!(heard(&t).is_empty() && !stands(&t));
}

/// A foundation the world removed (425984813's 4045, built and replaced by its stack unit) is
/// no longer the `MoveTo`'s target, which walks on: no walk to the point. A unit without speed
/// whose foundation dies fails its walk to the point at once (`MoveTo` state 13). At the point,
/// failed, or without a `MoveTo` the Build waits (`set_state(3)`: notify 0x202 and the
/// standing sprite without queued orders).
#[test]
fn builder_without_its_foundation_walks_on_then_waits() {
    let gone = BuildAction {
        target: None,
        state: BuildState::Move {
            move_to: Some(walk_in(MoveTo::Arrived)),
        },
        ..started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(8.0, 9.0, 0.0), gone);
    remove_house(&mut t);
    let _ = code(&mut t, 20);
    assert_eq!(build(&t).state, BuildState::Idle);
    let held = BuildAction {
        target: None,
        state: BuildState::Move {
            move_to: Some(walk_in(MoveTo::Moving)),
        },
        ..started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(8.0, 9.0, 0.0), held);
    own(&mut t, HOUSE).life.state = ObjectState::Removed;
    t.parts.mover_mut(BUILDER).unwrap().stats.speed = Speed::ZERO;
    let _ = code(&mut t, 20);
    assert_eq!(walk_state(&t), Some(MoveTo::Failed));
    for end in [Some(MoveTo::Arrived), Some(MoveTo::Failed), None] {
        let w = BuildAction {
            state: BuildState::WalkPoint {
                move_to: end.map(|state| MoveToAction {
                    target: None,
                    ..walk_in(state)
                }),
            },
            target: None,
            ..started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK)
        };
        let mut t = site_world(class::VILLAGER, Position::new(8.0, 9.0, 0.0), w);
        assert_eq!(code(&mut t, 20), HeadCode::Keeps);
        assert_eq!((build(&t).state, walk_state(&t)), (BuildState::Idle, None));
        assert_eq!(heard(&t), [SEARCH]);
        assert!(stands(&t), "{end:?}");
    }
}

/// 428857364's builder 8676 (player 4) waiting for foundation 10645's footprint (state 222)
/// when its owner deletes the foundation in update 367 (state 8, still in the world): the
/// Build walks back to it (`set_state(4)`, then slot 0x68 with none lets it go), but the new
/// `MoveTo` refuses a target above state 6 (`FUN_140a7a230`) and walks to the foundation's
/// place in the same update, with the task's range and no target (the game's builder, 2.3
/// tiles off, plans its path there). A foundation the `MoveTo` takes (state 0) stays its
/// target.
#[test]
fn waiting_builder_whose_foundation_dies_walks_to_its_place() {
    let b = BuildAction {
        state: BuildState::Wait,
        timer: Seconds::new(0.916),
        ..started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK)
    };
    // within the task's range of the place, so the walk ends without a path
    let mut t = site_world(class::VILLAGER, Position::new(10.55, 10.5, 0.0), b);
    own(&mut t, HOUSE).life.state = ObjectState::Gone;
    t.parts.building_mut(HOUSE).unwrap().construction = Some(construction(0.0, 0, false));
    let _ = code(&mut t, 20);
    let b = build(&t);
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (
            b.state.bare(),
            b.target,
            ObjectId::field(walk.target),
            walk.point
        ),
        (BuildState::Move { move_to: None }, None, -1, AT_HOUSE)
    );
    // the MoveTo ran in the same update
    assert_eq!(walk.state, MoveTo::Arrived);
    assert_eq!(house(&t).1, 0);
    let taken = started(HOUSE, AT_HOUSE, Distance::new(0.8), TASK);
    let mut t = site_world(class::VILLAGER, Position::new(10.55, 10.5, 0.0), taken);
    let restarted = |t: &mut World, state| {
        own(t, HOUSE).life.state = state;
        on_build(t, |b, c, u, _| {
            b.restart(c, u)?;
            let walk = b.state.move_to().copied().unwrap();
            Ok((walk.target, walk.point, walk.state))
        })
        .unwrap()
    };
    assert_eq!(
        restarted(&mut t, ObjectState::Foundation),
        (Some(HOUSE), AT_HOUSE, MoveTo::Arrived)
    );
    assert_eq!(
        restarted(&mut t, ObjectState::Removed),
        (None, AT_HOUSE, MoveTo::Start)
    );
}

/// MarvinBlue/426048731 after tick 679: builder 9175 stands on the house foundation 10028's
/// footprint, works on it (state 6) and walks off it in update 681 (state 223) to the free tile
/// the game picks, (29.5, 89.5), with the task's moving sprite (`FUN_140aa0d30`).
#[test]
fn builder_on_its_footprint_faces_it_then_walks_to_free_tile_beside_it() {
    let b = BuildAction {
        target: Some(HOUSE_SITE),
        state: BuildState::Work,
        point: Position::new(-1.0, -1.0, -1.0),
        task: MOVING_TASK,
        added: false,
        timer: Seconds::ZERO,
        site: None,
    };
    let mut t = site_world(class::VILLAGER, Position::new(30.0, 89.0, 0.0), b);
    t.spatial = Spatial::with_tiles(
        GridSize::new(40, 120),
        Tiles::new(GridSize::new(40, 120), vec![4; 40 * 120]),
    );
    t.spatial.insert(
        HOUSE,
        placed(
            Position::new(30.0, 88.0, 0.0),
            1.0,
            class::VILLAGER,
            ids::HOUSE,
        ),
    );
    let me = Placed {
        obstruction_type: ObstructionType::CIRCLE,
        ..placed(
            Position::new(30.0, 89.0, 0.0),
            0.2,
            class::VILLAGER,
            ids::BUILDER,
        )
    };
    t.spatial.insert(BUILDER, me);
    own(&mut t, HOUSE).place_for_test(Position::new(30.0, 88.0, 0.0));
    sighted(&mut t);
    let _ = code(&mut t, 20);
    let b = build(&t);
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (
            b.state.bare(),
            walk.state,
            ObjectId::field(walk.target),
            walk.point
        ),
        (
            BuildState::Clear { move_to: None },
            MoveTo::Start,
            -1,
            Position::new(29.5, 89.5, 0.0)
        )
    );
    assert_eq!(walk.sprite.map(|g| g.id), Some(TASK_SPRITE));
}

/// The quiet `MoveTo` to the house of a Build [`started`] 5 off it, in `state`.
fn walk_in(state: MoveTo) -> MoveToAction {
    MoveToAction {
        state,
        ..started(HOUSE, AT_HOUSE, Distance::new(5.0), TASK)
            .state
            .move_to()
            .copied()
            .unwrap()
    }
}

/// The state of the builder's Build's `MoveTo`, None without one.
fn walk_state(t: &World) -> Option<MoveTo> {
    build(t).state.move_to().copied().map(|m| m.state)
}

/// A Build of the house from (10.5, 10.5)'s point, its `MoveTo` in `state` ([`walk_in`]).
fn walking(state: Option<MoveTo>) -> BuildAction {
    BuildAction {
        state: BuildState::Move {
            move_to: state.map(walk_in),
        },
        ..started(HOUSE, AT_HOUSE, Distance::new(5.0), TASK)
    }
}

/// The waiting `MoveTo` (state 2): the world's dt goes onto its timer; at its delay it asks
/// again (`set_state(3)`), and that start marks the wait asked (`+0x6c` 2).
#[test]
fn build_waiting_move_to_asks_again_at_its_delay() {
    let b = BuildAction {
        state: BuildState::Move {
            move_to: Some(MoveToAction {
                wait: MoveWait {
                    state: MoveWaitState::Waiting,
                    timer: Seconds::new(0.5),
                    delay: Seconds::new(1.0),
                },
                ..walk_in(MoveTo::Waiting)
            }),
        },
        ..walking(None)
    };
    let mut t = site_world(class::VILLAGER, Position::new(2.5, 2.5, 0.0), b);
    let _ = code(&mut t, 250);
    let walk = build(&t).state.move_to().copied().unwrap();
    assert_eq!(
        (walk.state, walk.wait.timer),
        (MoveTo::Waiting, Seconds::new(0.75))
    );
    let _ = code(&mut t, 250);
    let walk = build(&t).state.move_to().copied().unwrap();
    assert_eq!(
        (walk.state, walk.wait.timer),
        (MoveTo::Start, Seconds::ZERO)
    );
    // in reach now: its start arrives at once, no path asked
    put(&mut t, BUILDER, Position::new(11.6, 10.5, 0.0));
    let _ = code(&mut t, 250);
    let walk = build(&t).state.move_to().copied().unwrap();
    assert_eq!(
        (walk.wait.state, walk.state),
        (MoveWaitState::AskedAgain, MoveTo::Arrived)
    );
}

/// The update's case 3 at the `MoveTo`'s end: a villager works wherever it stands
/// (`set_state(6)`); another builder out of the task's range and [`REACH`] of the footprint
/// walks to it again (`set_state(4)`), one within it works; without a sub-action the list's
/// update returns 1 as well.
#[test]
fn builder_at_its_move_to_end_works_or_walks_again() {
    let after = |class, at, move_to| {
        let mut t = site_world(class, at, walking(move_to));
        let _ = code(&mut t, 250);
        (build(&t).state.bare(), walk_state(&t))
    };
    let far = Position::new(2.5, 2.5, 0.0);
    let arrived = Some(MoveTo::Arrived);
    assert_eq!(
        after(class::VILLAGER, far, arrived),
        (BuildState::Work, None)
    );
    assert_eq!(
        after(class::FISHING_SHIP, far, arrived),
        (BuildState::Move { move_to: None }, Some(MoveTo::Start))
    );
    assert_eq!(
        after(class::FISHING_SHIP, Position::new(11.6, 10.5, 0.0), arrived),
        (BuildState::Work, None)
    );
    assert_eq!(after(class::VILLAGER, far, None), (BuildState::Work, None));
}

/// The update's case 3 on a failed `MoveTo` (3): a desolid foundation with another player's
/// marks its owner sees goes at once (a human owner's, [`World::remove_building`]: state 7) and
/// the Build halts; without the marks and without a planner it waits (`set_state(3)`); with a
/// planner that finds the way from where the builder last moved to the foundation
/// (`FUN_140f7c530` below 2) it walks again (`set_state(4)`).
#[test]
fn builder_whose_move_to_failed_waits_or_halts_on_marks() {
    let failed = |prepare: fn(&mut World)| {
        let mut t = site_world(
            class::VILLAGER,
            Position::new(2.5, 2.5, 0.0),
            walking(Some(MoveTo::Failed)),
        );
        prepare(&mut t);
        let _ = code(&mut t, 250);
        (build(&t).state.bare(), walk_state(&t), house(&t).2)
    };
    assert_eq!(
        failed(|_| ()),
        (BuildState::Idle, None, ObjectState::Foundation)
    );
    let marked: fn(&mut World) = |t| {
        t.spatial
            .tiles_mut_for_test()
            .mark_for_test(Tile::new(10, 10), 0x80_0000 << 3);
    };
    // no players' sight in this world: its owner sees nothing, so it waits
    assert_eq!(
        failed(marked),
        (BuildState::Idle, None, ObjectState::Foundation)
    );
    // its owner, player 1, sees every tile (its own mask, bit 1)
    let seen: fn(&mut World) = |t| {
        t.spatial
            .tiles_mut_for_test()
            .mark_for_test(Tile::new(10, 10), 0x80_0000 << 3);
        t.spatial.sight = Some(crate::vision::visibility::Visibility::new(
            GridSize::new(20, 20),
            Vec::new(),
            vec![VisionBits::from_bits_retain(1 << 1); 400],
            Vec::new(),
            0,
            Vec::new(),
        ));
    };
    assert_eq!(
        failed(seen),
        (BuildState::Halted, None, ObjectState::Removed)
    );
    assert_eq!(
        failed(plan),
        (
            BuildState::Move { move_to: None },
            Some(MoveTo::Start),
            ObjectState::Foundation
        )
    );
}

/// The quiet `MoveTo` to the foundation shows its sprite as it starts (its `set_state(4)`,
/// `0x140aa29e0`, slot 0x68), before the motion step: the task's moving sprite, else
/// `FUN_140aa0df0`'s default, the master's walking sprite.
#[test]
fn build_move_to_shows_its_sprite_as_it_starts() {
    for (task, shown) in [(TASK, WALKING), (MOVING_TASK, TASK_SPRITE)] {
        let mut t = site_world(
            class::VILLAGER,
            Position::new(2.5, 2.5, 0.0),
            started(HOUSE, AT_HOUSE, Distance::new(5.0), task),
        );
        plan(&mut t);
        // set_state(4)'s MoveTo, its sprite as its constructor resolves it for the builder
        on_build(&mut t, |b, c, u, _| b.restart(c, u)).unwrap();
        own(&mut t, BUILDER).sprite = None;
        let _ = code(&mut t, 250);
        assert_eq!(walk_state(&t), Some(MoveTo::Moving));
        assert_eq!(builder(&t).sprite.map(|s| s.graphic.id), Some(shown));
    }
}

/// The update's other cases (`0x140ab0370`): a Build done (case 0) returns 1; the walk to its
/// point (states 223 and 11) starts its quiet `MoveTo` there; a villager's walk off the footprint
/// that ended goes back to the foundation (`set_state(4)`); the walk to the action's point that
/// ended or failed waits (`set_state(3)`); a state without a case does nothing.
#[test]
fn build_other_cases_end_walk_to_point_go_back_or_wait() {
    let at = Position::new(2.5, 2.5, 0.0);
    let start = || started(HOUSE, AT_HOUSE, Distance::new(5.0), TASK);
    let done = BuildAction {
        state: BuildState::Done,
        ..start()
    };
    assert_eq!(
        code(&mut site_world(class::VILLAGER, at, done), 250),
        HeadCode::Deletes
    );
    let to_free = Some(MoveToAction {
        target: None,
        point: Position::new(6.5, 2.5, 0.0),
        ..walk_in(MoveTo::Start)
    });
    for state in [
        BuildState::Clear { move_to: to_free },
        BuildState::WalkPoint { move_to: to_free },
    ] {
        let b = BuildAction { state, ..start() };
        let mut t = site_world(class::VILLAGER, at, b);
        plan(&mut t);
        let _ = code(&mut t, 250);
        assert_eq!(walk_state(&t), Some(MoveTo::Moving), "state {state}");
    }
    // a builder of any class off the footprint goes back to the foundation
    for class in [class::VILLAGER, class::FISHING_SHIP] {
        let back = BuildAction {
            state: BuildState::Clear {
                move_to: Some(walk_in(MoveTo::Arrived)),
            },
            ..start()
        };
        let mut t = site_world(class, at, back);
        let _ = code(&mut t, 250);
        assert_eq!(
            (build(&t).state.bare(), walk_state(&t)),
            (BuildState::Move { move_to: None }, Some(MoveTo::Start))
        );
    }
    for move_to in [Some(MoveTo::Arrived), Some(MoveTo::Failed), None] {
        let b = BuildAction {
            state: BuildState::WalkPoint {
                move_to: move_to.map(walk_in),
            },
            ..start()
        };
        let mut t = site_world(class::VILLAGER, at, b);
        let _ = code(&mut t, 250);
        assert_eq!(build(&t).state.bare(), BuildState::Idle, "{move_to:?}");
    }
    let none = BuildAction {
        state: BuildState::Idle,
        ..start()
    };
    let mut t = site_world(class::VILLAGER, at, none);
    assert_eq!(code(&mut t, 250), HeadCode::Keeps);
    assert_eq!(build(&t), none);
}

/// A tree stump (Gaia's 77, master 349: a tree of at most 1 hit point) on the house's
/// footprint, as the footprint clearing reads it.
fn stump(t: &mut World) {
    let m = Master {
        id: master_id(349),
        kind: master_type::STATIC,
        class: class::TREE,
        radius: Footprint::new(0.5, 0.5),
        height: Distance::new(1.0),
        ..Master::default()
    };
    for masters in &mut t.masters {
        *masters = Masters::new(masters.iter().cloned().chain([m.clone()]));
    }
    let _ = t
        .push_live(
            PlayerId::new(0),
            Unit {
                id: object_id(77),
                place: crate::objects::layers::Static::at(Position::new(10.2, 10.2, 0.0)),
                life: Lifecycle {
                    state: ObjectState::Alive,
                    hp: HitPoints::new(0.5),
                    master_id: master_id(349),
                    ..Lifecycle::default()
                },
                pasture: None,
                my_pasture: None,
                ..Unit::default()
            },
        )
        .unwrap();
    t.spatial.insert(
        object_id(77),
        Placed {
            player: PlayerId::new(0),
            obstruction: Some(Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::NONE,
            }),
            ..placed(
                Position::new(10.2, 10.2, 0.0),
                0.5,
                class::TREE,
                master_id(349),
            )
        },
    );
    assert_ne!(t.spatial.tiles().word(Tile::new(10, 10)).unwrap() >> 23, 0);
}

/// The players' sight and the terrain the Build's placement test reads
/// ([`crate::world::tests::placeable_on`]) over the world's map.
fn sighted(t: &mut World) {
    let map = t.spatial.map;
    crate::world::tests::placeable_on(t, map);
}

/// A Build waiting for the house's footprint (state 222), its wait run out in this update.
fn placing() -> BuildAction {
    BuildAction {
        state: BuildState::Wait,
        timer: Seconds::new(0.01),
        ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
    }
}

/// census v5's 240 stops "a foundation clears objects off its footprint": the Build's
/// `set_state(7)` on a desolid foundation clears its footprint with `param_6`
/// (`FUN_1415c3620`): a stump on it loses its record (`FUN_141189670`) and is deleted at once
/// (`+0x686` set); the occupants' test passes over it (state 7) and the tiles hold no other
/// player's mark: the builder joins the work.
#[test]
fn builder_set_state_7_clears_stump_and_joins() {
    let mut t = site_world(class::VILLAGER, Position::new(12.5, 10.5, 0.0), placing());
    stump(&mut t);
    sighted(&mut t);
    let _ = code(&mut t, 20);
    assert_eq!(
        t.spatial.get(object_id(77)).map(|p| p.obstruction),
        Some(None)
    );
    assert_eq!(t.spatial.tiles().word(Tile::new(10, 10)).unwrap() >> 23, 0);
    let tree = t.players[0].objects.by_id(object_id(77)).unwrap();
    assert_eq!(
        (tree.life.state, tree.life.hp, tree.roo_called),
        (ObjectState::Removed, HitPoints::ZERO, true)
    );
    let b = build(&t);
    assert_eq!(
        (b.state, b.added, house(&t).0),
        (BuildState::Place, true, 1)
    );
}

/// `set_state(7)` with another player's marks on the footprint (`FUN_1411d4b80`): the
/// foundation goes at once when its owner is human (`FUN_1411ccbb0`: state 7), a computer's
/// stays, and `set_state(2)` halts the Build: the standing sprite, the foundation let go, no
/// point.
#[test]
fn another_player_marks_on_the_footprint_halt_the_build() {
    for (computer, kept) in [
        (false, ObjectState::Removed),
        (true, ObjectState::Foundation),
    ] {
        let mut t = site_world(class::VILLAGER, Position::new(12.5, 10.5, 0.0), placing());
        t.players[1].computer = computer;
        stump(&mut t);
        sighted(&mut t);
        // player 3's mark on a tile of the footprint (10, 10)
        t.spatial
            .tiles_mut_for_test()
            .mark_for_test(Tile::new(10, 10), 0x80_0000 << 3);
        let _ = code(&mut t, 20);
        let b = build(&t);
        assert_eq!(
            (b.state, b.target, b.point),
            (BuildState::Halted, None, Position::from([-1.0; 3]))
        );
        assert_eq!((house(&t).1, house(&t).2), (0, kept), "{computer}");
        assert!(stands(&t));
    }
}

/// `set_state(7)`'s `LAB_140aaf5d4` on a footprint someone stands on, the builder outside it:
/// the unit there is sent off at once (`FUN_14123bb20`, [`World::evacuate`]: notify 0x216);
/// after 21 waits the builder gives up (the interface's event 0x88, not held): `set_state(2)`
/// halts the Build and its waits (`+0x889`) start again; before, it waits a second more.
#[test]
fn builder_waiting_21_times_on_an_occupied_footprint_gives_up() {
    for (waits, state, after) in [
        (
            crate::construction::FootprintWaits::MOST.get(),
            BuildState::Halted,
            0,
        ),
        (3, BuildState::Wait, 4),
    ] {
        let mut t = site_world(class::VILLAGER, Position::new(12.5, 10.5, 0.0), placing());
        stump(&mut t);
        sighted(&mut t);
        own(&mut t, BUILDER).footprint_waits = crate::construction::FootprintWaits::of_data(waits);
        let mut other = copy_of(&t, BUILDER);
        other.unit.id = object_id(6);
        other.parts.actions.list = ActionList::default();
        other.unit.place_for_test(Position::new(10.6, 10.6, 0.0));
        let _ = t.push_live(PlayerId::new(1), other).unwrap();
        let me = *t.spatial.get(BUILDER).unwrap();
        t.spatial.insert(
            object_id(6),
            Placed {
                position: Position::new(10.6, 10.6, 0.0),
                moves: true,
                ..me
            },
        );
        let _ = code(&mut t, 20);
        let b = build(&t);
        assert_eq!((b.state, builder(&t).footprint_waits.get()), (state, after));
        let sent = t
            .parts
            .unit_ai
            .get(t.players[1].objects.by_id(object_id(6)).unwrap().id)
            .unwrap();
        let sent = sent
            .notifies
            .iter()
            .filter(|n| n.kind == NotifyKind::SENT_OFF);
        assert_eq!(sent.count(), 1);
        if state == BuildState::Halted {
            assert_eq!(
                (b.target, b.point, house(&t).1),
                (None, Position::from([-1.0; 3]), 0)
            );
            assert!(stands(&t));
        } else {
            assert_eq!(b.timer, Seconds::new(1.0));
        }
    }
}

/// `0x140ab0370`'s pasture branch: a human player's builder whose foundation of master 1889
/// is built (its state above 0) ends its unit AI's order and action, tells it of the pasture
/// the stack unit made (notify 0x21d), leaves the builders, lets the foundation go, gives its
/// load of berries to its player as food and ends (`set_state(1)`, returns 1).
#[test]
fn human_builder_of_built_pasture_hands_it_over() {
    let b = BuildAction {
        state: BuildState::Work,
        added: true,
        ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), b);
    let h = own(&mut t, HOUSE);
    h.life.master_id = PASTURE_1889;
    h.life.state = ObjectState::Alive;
    let layer = t.parts.building_mut(HOUSE).unwrap();
    layer.construction = Some(Construction {
        build_time: WholeSeconds::of_data(20),
        ..construction(20.0, 1, false)
    });
    layer.stack_made = Some(object_id(10031));
    let v = own(&mut t, BUILDER);
    (v.life.attribute_type, v.life.amount) = (Some(AttributeId::numbered(17)), Amount::new(7.0));
    let ai = t.parts.unit_ai.get_mut(BUILDER).unwrap();
    (ai.current_order, ai.current_action) = (Some(OrderKind::Build), Some(ActionKind::Build));
    assert_eq!(code(&mut t, 20), HeadCode::Deletes);
    let b = build(&t);
    assert_eq!(
        (b.state, b.target, b.added),
        (BuildState::Done, None, false)
    );
    let built = (
        NotifyKind::PASTURE_BUILT,
        [ActionKind::Build.value(), 10031, 0],
    );
    assert_eq!(heard(&t), [built]);
    let ai = t.parts.unit_ai.get(builder(&t).id).unwrap();
    assert_eq!((ai.current_order, ai.current_action), (None, None));
    assert_eq!((house(&t).0, house(&t).1), (0, 0));
    assert_eq!(t.players[1].attributes[0], Amount::new(7.0));
    assert_eq!(builder(&t).life.attribute_type, None);
    assert!(stands(&t));
}

/// The update's case 1 and case 0xc (`0x140ab0370`): a Build its unit's stop halted (state
/// 2) fails (`set_state(13)`, as 1: the unit takes its standing sprite) and the update returns
/// 0; failed, it posts notify 0x1f9 with its action value to its unit AI, halts (`set_state(2)`:
/// the unit stands, a target goes) and returns 3, which removes it. In state 3 with a target
/// the update does nothing (the default case).
#[test]
fn halted_build_fails_and_its_failure_removes_it() {
    let failed = (NotifyKind::ACTION_FAILED, [ActionKind::Build.value(), 0, 0]);
    let halted = BuildAction {
        state: BuildState::Halted,
        target: None,
        ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), halted);
    assert_eq!(code(&mut t, 20), HeadCode::Keeps);
    assert_eq!(build(&t).state, BuildState::Failed);
    assert!(stands(&t) && heard(&t).is_empty());
    assert_eq!(code(&mut t, 20), HeadCode::Deletes);
    assert_eq!(
        (build(&t).state, build(&t).target),
        (BuildState::Halted, None)
    );
    assert_eq!(heard(&t), [failed]);
    // failed with its foundation: the halt lets it go
    let holding = BuildAction {
        state: BuildState::Failed,
        ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), holding);
    assert_eq!(code(&mut t, 20), HeadCode::Deletes);
    assert_eq!(
        (build(&t).state, build(&t).target),
        (BuildState::Halted, None)
    );
    assert_eq!((heard(&t), house(&t).1), (vec![failed], 0));
    // waiting with a target: nothing
    let waiting = BuildAction {
        state: BuildState::Idle,
        ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), waiting);
    assert_eq!(code(&mut t, 20), HeadCode::Keeps);
    assert_eq!(build(&t), waiting);
    assert!(!stands(&t) && heard(&t).is_empty());
}

/// census v5's 217 stops "a dying foundation" went on to "a Build whose foundation died":
/// a builder at work (state 7, among the builders) on a foundation in state 3
/// (`0x140ab0370`'s prelude): `set_state(4)` toward it (out of its builders, a `MoveTo`
/// arrived beside it), slot 0x68 with none lets it go; the `MoveTo` is there, so
/// `set_state(6)` without a target turns to `set_state(3)`: without queued orders it tells
/// its unit AI 0x202. Master 621 and the gates (class 39) end their work instead once built
/// (`set_state(224)`), a foundation in state 1 or 2 too; the builder without queued orders
/// then looks for more work around it in the same update (`FUN_1415cc4e0`), finds none here,
/// lets it go and waits.
#[test]
fn builder_lets_dead_foundation_go() {
    let run = |master: MasterId, state: ObjectState, progress: f32| {
        let b = BuildAction {
            state: BuildState::Place,
            added: true,
            ..started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK)
        };
        let mut t = site_world(class::VILLAGER, Position::new(10.5, 11.6, 0.0), b);
        plan(&mut t);
        let h = own(&mut t, HOUSE);
        h.life.master_id = master;
        h.life.state = state;
        t.parts.building_mut(HOUSE).unwrap().construction = Some(construction(progress, 2, false));
        let _ = code(&mut t, 20);
        let b = build(&t);
        (
            b.state,
            b.target,
            house(&t).0,
            house(&t).1,
            heard(&t) == [SEARCH],
        )
    };
    assert_eq!(
        run(ids::HOUSE, ObjectState::Dying, 10.0),
        (BuildState::Idle, None, 1, 0, true)
    );
    // a built one in state 2 ends the work, then finds no more
    assert_eq!(
        run(ids::HOUSE, ObjectState::Alive, 25.0),
        (BuildState::Idle, None, 1, 0, true)
    );
    // a dead gate (class 39) not built yet lets go, one built ends the work
    assert_eq!(
        run(master_id(1584), ObjectState::Dying, 10.0),
        (BuildState::Idle, None, 1, 0, true)
    );
    assert_eq!(
        run(master_id(1584), ObjectState::Dying, 25.0),
        (BuildState::Idle, None, 1, 0, true)
    );
}

/// The end of the work (case 0xdf): a villager works the farm it built (class 49), its unit AI
/// dropping its build orders: slot 0x7e8 on the farm at its place, within the update.
#[test]
fn villager_works_the_farm_it_built() {
    let b = BuildAction {
        state: BuildState::Finished,
        ..started(HOUSE, AT_HOUSE, Distance::ZERO, TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), b);
    let h = own(&mut t, HOUSE);
    (h.life.master_id, h.life.state) = (master_id(50), ObjectState::Alive);
    let order = |order| QueuedOrder {
        order,
        target: Some(object_id(99)),
        ..QueuedOrder::default()
    };
    let ai = t.parts.unit_ai.get_mut(BUILDER).unwrap();
    ai.orders = vec![order(OrderKind::Build), order(OrderKind::Hunt)];
    let worked = update(&mut t, 20).unwrap();
    // the work command within the update: [`site_world`]'s villager has no farm task, so it
    // walks to the farm, and the move (slot 0x138, `+0xa14` 1) empties the queue the Build
    // left its Hunt order in
    let walks = Some(crate::movement::move_to::MOVE_TO);
    assert_eq!(worked, (HeadCode::Keeps, (walks, Some(HOUSE))));
    assert_eq!(t.parts.unit_ai.get(builder(&t).id).unwrap().orders, vec![]);
}

/// A builder beside its foundation works in its first update (state 6); census v5's 208 stops
/// "a Build whose foundation is gone": one whose foundation's id the world no longer holds
/// (`0x140ab0370`'s prelude) lets it go (slot 0x68 with none), then `set_state(3)`: its
/// `MoveTo` goes, and without orders it tells its unit AI (0x202).
#[test]
fn builder_at_the_foundation_works_in_its_first_update_the_others_walk_to_it() {
    let near = started(HOUSE, Position::ORIGIN, Distance::ZERO, TASK);
    let far = started(HOUSE, Position::ORIGIN, Distance::new(0.8), TASK);
    let state = |b: BuildAction| (b.state.bare(), b.state.move_to().copied().map(|m| m.state));
    assert_eq!(
        state(near),
        (BuildState::Move { move_to: None }, Some(MoveTo::Arrived))
    );
    assert_eq!(
        state(far),
        (BuildState::Move { move_to: None }, Some(MoveTo::Start))
    );
    let mut t = site_world(class::VILLAGER, Position::new(12.0, 10.5, 0.0), near);
    let _ = code(&mut t, 20);
    assert_eq!(state(build(&t)), (BuildState::Work, None));
    let mut t = site_world(class::VILLAGER, Position::new(2.5, 2.5, 0.0), far);
    remove_house(&mut t);
    assert_eq!(code(&mut t, 20), HeadCode::Keeps);
    let b = build(&t);
    assert_eq!(
        (b.state.bare(), b.target, b.state.move_to().copied()),
        (BuildState::Idle, None, None)
    );
    assert_eq!(heard(&t), [SEARCH]);
}

/// The mill 300 at (15.5, 10.5) beside [`site_world`]'s house, done in its owner's tracker, a
/// drop site of the table for the villagers' wood (resource 1).
fn with_mill(t: &mut World) {
    let mut mill = copy_of(t, HOUSE);
    mill.unit.id = object_id(300);
    mill.unit.life.master_id = master_id(68);
    mill.unit.life.state = ObjectState::Alive;
    mill.unit.workers = crate::objects::WorkerCount::of_data(0);
    mill.parts.building.construction = Some(construction(25.0, 0, false));
    mill.unit.place_for_test(Position::new(15.5, 10.5, 0.0));
    let _ = t.push_live(PlayerId::new(1), mill).unwrap();
    t.spatial.insert(
        object_id(300),
        placed(
            Position::new(15.5, 10.5, 0.0),
            1.0,
            class::BUILDING,
            master_id(68),
        ),
    );
    // a done building of its owner's tracker, which the table's search reads
    let k = crate::players::tracker::key(t.masters.get(1), &t.lines, master_id(68));
    t.players[1].tracker.add(object_id(300), k);
    t.players[1].tracker.complete(object_id(300), k);
    t.drop_sites = vec![crate::unit_ai::task_finder::DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        targets: vec![crate::unit_ai::task_finder::DropTarget {
            class: class::TREE,
            workers: crate::objects::WorkerCount::of_data(-1),
            state: None,
            attribute: Some(attribute::WOOD),
        }],
        resource_types: vec![attribute::WOOD],
        ..crate::unit_ai::task_finder::DropSite::default()
    }];
}

/// `set_state(8)` (`0x140aae200` case 8): the prelude takes a counted builder out of the
/// builders; the drop site found becomes the second target (`FUN_140a7a190`: taken at once) and
/// a quiet `MoveTo` to it within the search's range starts, with the drop task's carrying
/// sprite, else the Build task's moving one. None found lets the old one go, tells the unit AI
/// (notify 0x203) and walks back to the foundation (`set_state(4)`), as a builder carrying
/// nothing does without a search. Without a foundation that `MoveTo` has no target and walks to
/// its zeroed point.
#[test]
fn builder_drop_off_walks_to_the_drop_site_its_search_found() {
    let b = BuildAction {
        state: BuildState::Work,
        added: true,
        ..started(HOUSE, AT_HOUSE, Distance::new(5.0), MOVING_TASK)
    };
    let mut t = site_world(class::VILLAGER, Position::new(2.5, 2.5, 0.0), b);
    with_mill(&mut t);
    t.parts.building_mut(HOUSE).unwrap().construction = Some(construction(0.0, 3, true));
    let mill = |t: &World| {
        t.players[1]
            .objects
            .by_id(object_id(300))
            .unwrap()
            .workers
            .get()
    };
    let drop = |t: &mut World, search| {
        on_build(t, |b, c, u, reads| {
            b.set_drop(c, u, search, reads.by)?;
            Ok(*b)
        })
        .unwrap()
    };
    let site = |sprite| DropSearch::Site {
        id: DropSiteId::new(object_id(300)),
        range: Distance::new(0.5),
        sprite,
    };
    let b = drop(&mut t, site(Some(SpriteId::named(7))));
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (
            b.state.bare(),
            b.site,
            ObjectId::field(walk.target),
            walk.range,
            walk.state
        ),
        (
            BuildState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(300))),
            300,
            Distance::new(0.5),
            MoveTo::Start
        )
    );
    // the drop task's carrying sprite; +0x61 1: it asks again when its target moves
    assert_eq!(
        (walk.sprite.map(|g| g.id), walk.on_target_moved),
        (Some(SpriteId::named(7)), TargetMoved::AsksAgain)
    );
    assert_eq!(
        (b.added, house(&t).0, b.target, mill(&t)),
        (false, 2, Some(HOUSE_SITE), 1)
    );
    let b = drop(&mut t, DropSearch::Nothing);
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (b.state.bare(), b.site, walk.target, walk.state, walk.point),
        (
            BuildState::Move { move_to: None },
            None,
            Some(HOUSE),
            MoveTo::Start,
            AT_HOUSE
        )
    );
    let no_site = (NotifyKind::NO_DROP_SITE, [ActionKind::Build.value(), 0, 0]);
    assert_eq!((mill(&t), heard(&t)), (0, vec![no_site]));
    let b = drop(&mut t, DropSearch::Empty);
    assert_eq!(
        (b.state.bare(), b.state.move_to().copied().map(|m| m.state)),
        (BuildState::Move { move_to: None }, Some(MoveTo::Start))
    );
    assert_eq!(heard(&t).len(), 1);
    // beside the site already: arrived at once
    put(&mut t, BUILDER, Position::new(14.5, 10.5, 0.0));
    let b = drop(&mut t, site(None));
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (b.state.bare(), walk.state),
        (BuildState::Drop { move_to: None }, MoveTo::Arrived)
    );
    // without one, the Build task's moving sprite
    assert_eq!(walk.sprite.map(|g| g.id), Some(TASK_SPRITE));
    t.parts
        .actions_mut(BUILDER)
        .unwrap()
        .list
        .build_mut()
        .unwrap()
        .target = None;
    let lost = drop(&mut t, DropSearch::Empty);
    let walk = lost.state.move_to().copied().unwrap();
    assert_eq!(
        (
            lost.state.bare(),
            ObjectId::field(walk.target),
            walk.point,
            walk.state,
            lost.site
        ),
        (
            BuildState::Move { move_to: None },
            -1,
            Position::ORIGIN,
            MoveTo::Start,
            Some(DropSiteId::new(object_id(300)))
        )
    );
}

/// A Build of the house in state 8 at its drop site 300, its `MoveTo` to the mill (within 0.5,
/// asking again when it moves) in `state`.
fn at_the_mill(state: MoveTo) -> BuildAction {
    BuildAction {
        state: BuildState::Drop {
            move_to: Some(MoveToAction {
                target: Some(object_id(300)),
                point: Position::new(15.5, 10.5, 0.0),
                range: Distance::new(0.5),
                on_target_moved: TargetMoved::AsksAgain,
                ..walk_in(state)
            }),
        },
        site: Some(DropSiteId::new(object_id(300))),
        ..started(HOUSE, AT_HOUSE, Distance::new(5.0), TASK)
    }
}

/// [`site_world`] with [`with_mill`] and the builder at `at` carrying 10 wood, building `b`.
fn drop_world(at: Position, b: BuildAction) -> World {
    let mut t = site_world(class::VILLAGER, at, b);
    with_mill(&mut t);
    let v = own(&mut t, BUILDER);
    (v.life.attribute_type, v.life.amount) = (Some(attribute::WOOD), Amount::new(10.0));
    t
}

/// The builder's wood, its player's and the mill's workers.
fn wood(t: &World) -> (f32, f32, i32) {
    let mill = t.players[1]
        .objects
        .by_id(object_id(300))
        .map_or(-1, |m| m.workers.get());
    (
        builder(t).life.amount.get(),
        t.players[1].attributes[1].get(),
        mill,
    )
}

/// State 8's update (`0x140ab0370` case 7) at the `MoveTo`'s end: at a drop site in state 2
/// that takes it within [`DROP_REACH`] the load goes to the player (wood), the drop site goes
/// (its slot 0x180) and the builder walks back to its foundation (`set_state(4)`). Farther it
/// waits 3 seconds with its animation stopped, then searches again; a drop site that does not
/// take the load halts the Build.
#[test]
fn builder_at_its_drop_site_gives_its_load_and_walks_back() {
    let mut t = drop_world(Position::new(14.0, 10.5, 0.0), at_the_mill(MoveTo::Arrived));
    own(&mut t, object_id(300)).workers = crate::objects::WorkerCount::of_data(1);
    let _ = code(&mut t, 250);
    let b = build(&t);
    assert_eq!(wood(&t), (0.0, 10.0, 0));
    assert_eq!(
        (b.state.bare(), b.site, b.target),
        (BuildState::Move { move_to: None }, None, Some(HOUSE_SITE))
    );
    assert_eq!(walk_state(&t), Some(MoveTo::Start));
    // the MoveTo at its end stays in the list until the next set_state
    let mut t = drop_world(Position::new(13.0, 10.5, 0.0), at_the_mill(MoveTo::Arrived));
    let _ = code(&mut t, 250);
    let b = build(&t);
    assert_eq!(
        (b.state.bare(), b.timer, walk_state(&t)),
        (
            BuildState::Drop { move_to: None },
            Seconds::new(-3.0),
            Some(MoveTo::Arrived)
        )
    );
    assert_eq!(t.parts.anim_speed_of(BUILDER), Some(Speed::ZERO));
    assert_eq!(wood(&t).0, 10.0);
    let _ = code(&mut t, 250);
    assert_eq!(build(&t).timer, Seconds::new(-2.75));
    t.parts
        .actions_mut(BUILDER)
        .unwrap()
        .list
        .build_mut()
        .unwrap()
        .timer = Seconds::new(-0.1);
    let _ = code(&mut t, 250);
    // its search again: the mill out of reach is still the nearest, a MoveTo to it again
    let b = build(&t);
    assert_eq!(
        (b.state.bare(), b.timer, b.site, walk_state(&t)),
        (
            BuildState::Drop { move_to: None },
            Seconds::ZERO,
            Some(DropSiteId::new(object_id(300))),
            Some(MoveTo::Start)
        )
    );
    let mut refused = drop_world(Position::new(14.0, 10.5, 0.0), at_the_mill(MoveTo::Arrived));
    own(&mut refused, object_id(300)).workers = crate::objects::WorkerCount::of_data(1);
    refused.drop_sites[0].targets[0].attribute = Some(attribute::STONE);
    let _ = code(&mut refused, 250);
    let b = build(&refused);
    assert_eq!(
        (b.state.bare(), b.target, b.site),
        (BuildState::Halted, None, None)
    );
    assert_eq!((wood(&refused), house(&refused).1), ((10.0, 0.0, 0), 0));
    assert!(stands(&refused));
}

/// At the `MoveTo`'s end a drop site not in state 2, one gone, or a failed walk search again
/// (`set_state(8)`); without a drop site of the table for the load the search tells the unit
/// AI (notify 0x203) and walks back to the foundation.
#[test]
fn builder_drop_off_without_its_drop_site_searches_again() {
    let searched = |prepare: fn(&mut World), move_to| {
        let mut t = drop_world(Position::new(14.0, 10.5, 0.0), at_the_mill(move_to));
        prepare(&mut t);
        let _ = code(&mut t, 250);
        let b = build(&t);
        (b.state.bare(), wood(&t).0, heard(&t).len())
    };
    // the drop site in state 3, which the table's search passes over as well
    let left: fn(&mut World) = |t| own(t, object_id(300)).life.state = ObjectState::Dying;
    assert_eq!(
        searched(left, MoveTo::Arrived),
        (BuildState::Move { move_to: None }, 10.0, 1)
    );
    let gone: fn(&mut World) = |t| {
        let i = t.players[1].objects.position_of(object_id(300)).unwrap();
        let _ = t.players[1].objects.take_out(i).unwrap();
        t.spatial.remove(object_id(300));
    };
    assert_eq!(
        searched(gone, MoveTo::Arrived),
        (BuildState::Move { move_to: None }, 10.0, 1)
    );
    assert_eq!(searched(|_| (), MoveTo::Failed).1, 10.0);
}

/// State 8 without a foundation: after the load its unit AI drops its orders 0x2c5, 0x2c9
/// and 0x2db on targets gone (`FUN_140c5de60`); with orders left it has no order and the
/// Build is done (`set_state(1)`, the update returns 1), with none it waits (`set_state(3)`:
/// notify 0x202, the standing sprite). A builder without a unit AI and a point of −1 halts
/// (a human's, `set_state(2)`: the drop site goes); with a point it walks there
/// (`set_state(11)`).
#[test]
fn builder_without_foundation_after_its_drop_off() {
    let lone = |point| BuildAction {
        target: None,
        point,
        ..at_the_mill(MoveTo::Arrived)
    };
    let order = |order, target| QueuedOrder {
        order,
        target,
        ..QueuedOrder::default()
    };
    let mut t = drop_world(Position::new(14.0, 10.5, 0.0), lone(AT_HOUSE));
    let ai = t.parts.unit_ai.get_mut(BUILDER).unwrap();
    ai.current_order = Some(OrderKind::Hunt);
    ai.orders = vec![
        order(OrderKind::Hunt, Some(object_id(77))),
        order(OrderKind::Build, Some(object_id(78))),
    ];
    assert_eq!(code(&mut t, 250), HeadCode::Deletes);
    let ai = t.parts.unit_ai.get(builder(&t).id).unwrap();
    assert_eq!(
        ai.orders,
        vec![order(OrderKind::Build, Some(object_id(78)))]
    );
    assert_eq!(ai.current_order, None);
    assert_eq!(
        (build(&t).state.bare(), wood(&t).1),
        (BuildState::Done, 10.0)
    );
    assert!(stands(&t));
    let mut t = drop_world(Position::new(14.0, 10.5, 0.0), lone(AT_HOUSE));
    let ai = t.parts.unit_ai.get_mut(BUILDER).unwrap();
    ai.orders = vec![order(OrderKind::OnTarget, Some(object_id(77)))];
    assert_eq!(code(&mut t, 250), HeadCode::Keeps);
    assert_eq!(t.parts.unit_ai.get(builder(&t).id).unwrap().orders.len(), 0);
    assert_eq!(heard(&t), [SEARCH]);
    assert_eq!(
        (build(&t).state.bare(), build(&t).site),
        (BuildState::Idle, Some(DropSiteId::new(object_id(300))))
    );
    assert!(stands(&t));
    let mut t = drop_world(
        Position::new(14.0, 10.5, 0.0),
        lone(Position::new(-1.0, -1.0, -1.0)),
    );
    t.parts.set_unit_ai(BUILDER, None);
    own(&mut t, object_id(300)).workers = crate::objects::WorkerCount::of_data(1);
    let _ = code(&mut t, 250);
    assert_eq!(
        (build(&t).state.bare(), wood(&t).2),
        (BuildState::Halted, 0)
    );
    assert!(stands(&t));
    let mut t = drop_world(
        Position::new(14.0, 10.5, 0.0),
        lone(Position::new(5.0, 6.0, 0.0)),
    );
    t.parts.set_unit_ai(BUILDER, None);
    let _ = code(&mut t, 250);
    let walk = build(&t).state.move_to().copied().unwrap();
    assert_eq!(
        (build(&t).state.bare(), walk.point, walk.state, wood(&t).1),
        (
            BuildState::WalkPoint { move_to: None },
            Position::new(5.0, 6.0, 0.0),
            MoveTo::Start,
            10.0
        )
    );
}

/// State 8's quiet `MoveTo` lets a target past state 6 go and returns 5: the drop-off
/// searches again, and the table's search passes over the mill past state 6: notify 0x203 and
/// the walk back to the foundation.
#[test]
fn drop_off_whose_site_went_past_state_6_searches_again() {
    let mut t = drop_world(Position::new(2.5, 2.5, 0.0), at_the_mill(MoveTo::Moving));
    own(&mut t, object_id(300)).life.state = ObjectState::Removed;
    let _ = code(&mut t, 250);
    let b = build(&t);
    assert_eq!(
        (
            b.state.move_to().copied().and_then(|m| m.target),
            b.state.bare(),
            b.site
        ),
        (Some(HOUSE), BuildState::Move { move_to: None }, None)
    );
    let no_site = (NotifyKind::NO_DROP_SITE, [ActionKind::Build.value(), 0, 0]);
    assert_eq!(heard(&t), [no_site]);
}

/// Player 1's standing lumber camp 30 (master 562, built) at `at`.
fn camp(t: &mut World, at: Position) {
    let mut camp = copy_of(t, HOUSE);
    camp.unit.id = object_id(30);
    camp.unit.life.master_id = master_id(562);
    camp.unit.life.state = ObjectState::Alive;
    camp.parts.building.construction = Some(construction(25.0, 0, false));
    camp.unit.place_for_test(at);
    let _ = t.push_live(PlayerId::new(1), camp).unwrap();
    t.spatial.insert(
        object_id(30),
        placed(at, 1.0, class::BUILDING, master_id(562)),
    );
}

/// Player 1's villager 6, a copy of the builder without actions, standing at `at`.
fn villager(t: &mut World, at: Position) {
    let mut other = copy_of(t, BUILDER);
    other.unit.id = object_id(6);
    other.parts.actions.list = ActionList::default();
    other.unit.place_for_test(at);
    let _ = t.push_live(PlayerId::new(1), other).unwrap();
    let me = *t.spatial.get(BUILDER).unwrap();
    t.spatial.insert(
        object_id(6),
        Placed {
            position: at,
            moves: true,
            ..me
        },
    );
}

/// The house's desolid annex 10029 (a copy of the house, `+0xbb8` the house) at `at`.
fn annex(t: &mut World, at: Position) -> i32 {
    const ANNEX: i32 = 10029;
    let mut annex = copy_of(t, HOUSE);
    annex.unit.id = object_id(ANNEX);
    annex.parts.building.annex_of = Some(MainBuildingId::new(HOUSE));
    annex.unit.place_for_test(at);
    let _ = t.push_live(PlayerId::new(1), annex).unwrap();
    t.spatial.insert(
        object_id(ANNEX),
        placed(at, 1.0, class::BUILDING, ids::HOUSE),
    );
    t.link_test_annex(object_id(ANNEX), HOUSE);
    ANNEX
}

/// How often villager 6 was sent off (notify 0x216).
fn sent_off(t: &World) -> usize {
    let ai = t
        .parts
        .unit_ai
        .get(t.players[1].objects.by_id(object_id(6)).unwrap().id)
        .unwrap();
    ai.notifies
        .iter()
        .filter(|n| n.kind == NotifyKind::SENT_OFF)
        .count()
}

/// `set_state(7)`'s placement test of the foundation (its master's slot 0x50, `FUN_14123aee0`):
/// a standing building of the player whose box overlaps the house's gives 6, and `set_state(2)`
/// halts the Build (the interface's event 0x88, not held): the standing sprite, the foundation
/// let go, no point.
#[test]
fn foundation_building_overlaps_halts_the_build() {
    let mut t = site_world(class::VILLAGER, Position::new(12.8, 10.5, 0.0), placing());
    sighted(&mut t);
    camp(&mut t, Position::new(11.6, 10.5, 0.0));
    let _ = code(&mut t, 20);
    let b = build(&t);
    assert_eq!(
        (b.state, b.target, b.point),
        (BuildState::Halted, None, Position::from([-1.0; 3]))
    );
    assert!(stands(&t));
}

/// The annexes' tests come first (`0x140aae200` case 7, the foundation's `+0xbc0`): a unit on
/// an annex that may stay is sent off (`FUN_14123bb20`) and the builder waits for it (`bVar21`,
/// `LAB_140aaf5d4`) though no unit stands on the foundation; an annex a building overlaps
/// halts the Build and sends no unit off.
#[test]
fn an_annex_test_sends_its_units_off_or_halts_the_build() {
    for blocked in [false, true] {
        let mut t = site_world(class::VILLAGER, Position::new(15.2, 10.5, 0.0), placing());
        sighted(&mut t);
        let _ = annex(&mut t, Position::new(12.5, 10.5, 0.0));
        villager(&mut t, Position::new(12.6, 10.6, 0.0));
        if blocked {
            camp(&mut t, Position::new(14.0, 10.5, 0.0));
        }
        let _ = code(&mut t, 20);
        let b = build(&t);
        if blocked {
            assert_eq!(
                (b.state, b.target, sent_off(&t)),
                (BuildState::Halted, None, 0)
            );
        } else {
            assert_eq!(
                (b.state, b.target, sent_off(&t)),
                (BuildState::Wait, Some(HOUSE_SITE), 1)
            );
            assert_eq!(
                builder(&t).footprint_waits,
                crate::construction::FootprintWaits::of_data(1)
            );
        }
    }
}

/// A builder outside the foundation's footprint but on an annex's, nearer to its centre than the
/// foundation's radii (`0x140aae200` case 7's loop over `+0xbc0`), walks off the annex
/// (`set_state(223)` with the annex as the target) when units stand on the foundation; the
/// foundation is its target again.
#[test]
fn builder_on_an_annex_walks_off_it() {
    let mut t = site_world(class::VILLAGER, Position::new(12.4, 10.5, 0.0), placing());
    t.spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    );
    t.spatial
        .insert(HOUSE, placed(AT_HOUSE, 1.0, class::BUILDING, ids::HOUSE));
    let me = Placed {
        obstruction_type: ObstructionType::CIRCLE,
        ..placed(
            Position::new(12.4, 10.5, 0.0),
            0.2,
            class::VILLAGER,
            ids::VILLAGER_MALE,
        )
    };
    t.spatial.insert(BUILDER, me);
    sighted(&mut t);
    let a = annex(&mut t, Position::new(12.5, 10.5, 0.0));
    villager(&mut t, Position::new(10.6, 10.6, 0.0));
    let _ = code(&mut t, 20);
    let b = build(&t);
    let walk = b.state.move_to().copied().unwrap();
    assert_eq!(
        (b.state.bare(), b.target, sent_off(&t)),
        (BuildState::Clear { move_to: None }, Some(HOUSE_SITE), 1)
    );
    let (x, y) = (walk.point.x(), walk.point.y());
    let off = |c: f32, p: f32| 1.0 < (p - c).abs();
    assert!(
        off(12.5, x) || off(10.5, y),
        "{:?} off annex {a}",
        walk.point
    );
}
