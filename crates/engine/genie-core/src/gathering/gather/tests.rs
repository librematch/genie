use super::*;
use crate::GenieResult;
use crate::gathering::DropSiteId;
use crate::gathering::FarmStops;
use crate::gathering::gather::WalkEnd;
use crate::gathering::hunt::tests::walker;
use crate::gathering::hunt::{DropOff, resource_kind};
use crate::map::DistanceSquared;
use crate::math::Angle;
use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::move_to::{
    MoveDefaults, MoveTo, MoveToAction, MoveToReads, MoveWait, SpriteReads, TargetMoved, WalkTo,
};
use crate::movement::tests::path_world;
use crate::movement::{Me, Mover};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::{Action, Motion, Static};
use crate::objects::lifecycle::Held;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::WorkValue;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::players::attribute::AttributeId;
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::task_id;
use crate::test_ids::task_key;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::slots;
use crate::testing::tuning;
use crate::unit_ai::UnitStance;
use crate::world::WorldTime;
use crate::world::layer::{ClassRoles, MasterRoles, Roles};
use crate::world::settings::FarmMode;
use crate::world::step::Step;
use crate::world::tests::in_update;
use crate::world::{Player, World};

/// What a human player's villager's Gather reads (master 83, class 4): work rate 0.31, capacity
/// 10, no productivity attribute, no sprites, outside the game's farm setting.
fn reads() -> GatherReads {
    GatherReads {
        work_rate: WorkRate::new(0.31),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
        sprites: HuntSprites::default(),
        class: class::VILLAGER,
        defaults: MoveDefaults::default(),
        master: ids::VILLAGER_MALE,
        stance: UnitStance::Aggressive,
        farm_mode: FarmMode::Fixed,
        player_kind: PlayerKind::Human,
        stop_productivity: Amount::new(1.0),
        food_sold: Amount::ZERO,
        top_speed: TopSpeed::Own,
    }
}

/// The sprites' table of a world without sprites.
static NO_GRAPHICS: Graphics = Graphics::new();

/// What the `MoveTo` constructors read of a villager that carries `held`: no sprites.
fn walks(held: Held) -> Walks<'static> {
    Walks {
        reads: MoveToReads {
            sprites: SpriteReads {
                defaults: MoveDefaults::default(),
                graphics: &NO_GRAPHICS,
            },
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::ORIGIN,
            ground: Footprint::ZERO,
        },
        held,
        top_speed: TopSpeed::Own,
    }
}

/// The quiet `MoveTo` in `state` to the house 10028 at (30, 88) within the task's range, which
/// asks again when its target moves (`FUN_140aa0df0`, `+0x61` 1).
fn to_house(state: MoveTo) -> MoveToAction {
    MoveToAction {
        point: Position::new(30.0, 88.0, 0.0),
        target: Some(object_id(10028)),
        range: task().range,
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_in(state)
    }
}

/// A world of player 1's villager 9177 ([`crate::movement::tests::builder_9177`]) at (28, 89),
/// walking to (30, 88), on the open tiled map of [`crate::movement::tests::tiled_world`] with
/// its mesh; it carries 3 of resource 17 and its animation runs.
struct Rig {
    world: World,
}

impl Rig {
    /// The rig with 4 attributes for the player.
    fn on_the_path() -> Self {
        Self::with_attributes(4)
    }

    /// The house 10028 (a unit of player 0 in `state`), which a `MoveTo` to it reads the state
    /// of.
    fn with_house(mut self, state: ObjectState) -> Self {
        let mut house = Unit {
            id: object_id(10028),
            ..Unit::default()
        };
        house.life.state = state;
        let _ = self.world.push_live(PlayerId::new(0), house).unwrap();
        self
    }

    /// The rig with `count` attributes for the player.
    fn with_attributes(count: usize) -> Self {
        let owner = Player {
            attributes: vec![Amount::ZERO; count],
            ..Player::default()
        };
        let mut rig = Self {
            world: path_world(owner),
        };
        rig.mover_mut().animated.speed = Speed::new(1.0);
        let unit = rig.unit_mut();
        unit.life.master_id = ids::VILLAGER_MALE;
        (unit.life.attribute_kind, unit.life.amount) =
            (Some(AttributeId::named(17)), Amount::new(3.0));
        rig
    }

    /// `work` on the villager's step of `dt` ms with its action layer, in action state `state`.
    fn run<R>(
        &mut self,
        state: GatherState,
        dt: u32,
        work: impl FnOnce(&mut Step<'_>, &mut Unit, &mut Action, Me) -> GenieResult<R>,
    ) -> GenieResult<R> {
        in_update(
            &mut self.world,
            (PlayerId::new(1), ListIndex::new(0)),
            dt,
            |unit, actions, step| {
                let me = Me {
                    human_order: WorldTime::ZERO,
                    id: unit.id,
                    group: None,
                    action_state: ActionState::new(state.get()),
                    top_speed: TopSpeed::Own,
                };
                work(step, unit, actions, me)
            },
        )
    }

    /// The villager.
    fn unit(&self) -> &Unit {
        &self.world.players[1].objects[ListIndex::new(0)]
    }

    /// The villager to change.
    fn unit_mut(&mut self) -> &mut Unit {
        &mut self.world.players[1].objects[ListIndex::new(0)]
    }

    /// Its movement state.
    fn mover_mut(&mut self) -> &mut Mover {
        let id = self.unit().id;
        self.world
            .parts
            .mover_mut(id)
            .expect("the rig's unit moves")
    }

    /// What it carries: the resource and how much.
    fn load(&self) -> Held {
        self.unit().life.held()
    }

    /// It carries `held` of resource `kind`.
    fn set_load(&mut self, kind: Option<AttributeId>, held: f32) {
        let life = &mut self.unit_mut().life;
        (life.attribute_kind, life.amount) = (kind, Amount::new(held));
    }

    /// Its animation stopped (`+0x7b8` 0).
    fn anim_stopped(&self) -> bool {
        self.world.parts.anim_speed_of(self.unit().id) == Some(Speed::new(0.0))
    }

    /// The next draw of the world's Action stream, less `GatherTuning::wait_shift`: a random wait's timer.
    fn next_wait(&self) -> f32 {
        let mut action = self.world.streams.clone();
        action.get_mut(Stream::Action).float01() - tuning().gather.wait_shift
    }
}

/// A forager's task: food (kind 17, berries), range 0.25, work value 1 (the 4v4's 31139).
fn task() -> HuntTask {
    HuntTask {
        id: task_id(2),
        key: task_key(GATHER),
        range: Distance::new(0.25),
        resource: Some(AttributeId::named(17)),
        sprites: TaskSprites::NONE,
        work_value: WorkValue::new(1.0),
        productivity: None,
        kind4: None,
    }
}

/// The 4v4's berry bush 28222 (master 59, class 7) as the builder 31139 takes it in update
/// 4863.
fn bush() -> Resource {
    Resource {
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
    }
}

/// A villager's rate (work rate 1, capacity 10, no productivity attribute).
const fn rate() -> StopRate {
    StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    }
}

fn target(amount: f32) -> Victim {
    Victim {
        id: object_id(28222),
        amount: Amount::new(amount),
        state: ObjectState::Alive,
        class: class::CLASS_7,
        master_id: master_id(59),
        storage: Amount::new(125.0),
        ..Victim::default()
    }
}

/// A farm of master 50 (class 49, radii 1.5) at MarvinBlue/426049433's 12557.
fn farm() -> Resource {
    Resource {
        id: object_id(12557),
        position: Position::new(112.5, 173.5, 3.0),
        class: class::FARM,
        master: ids::FARM,
        player: PlayerId::new(1),
        workers: WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::ZERO,
        state: ObjectState::Alive,
        radius: Footprint::new(1.5, 1.5),
    }
}

#[test]
fn farm_stop_is_tile_centre_of_its_footprint_other_than_the_last() {
    let mut rng = Well512::default();
    rng.set_seed(29910);
    let mut last = Point::ORIGIN;
    for _ in 0..50 {
        let [x, y] = stop_point(
            Position::new(112.5, 173.5, 3.0),
            Footprint::new(1.5, 1.5),
            last,
            &mut rng,
            &tuning().gather,
        )
        .get();
        assert!([111.5, 112.5, 113.5].contains(&x) && [172.5, 173.5, 174.5].contains(&y));
        assert_ne!(Point::new(x, y), last);
        last = Point::new(x, y);
    }
    // no footprint: the farm's own tile, drawn ten times when it is the last stop's
    let mut rng = Well512::default();
    let at = stop_point(
        Position::new(10.0, 20.0, 0.0),
        Footprint::new(0.0, 0.0),
        Point::new(10.5, 20.5),
        &mut rng,
        &tuning().gather,
    );
    assert_eq!((at, rng.draws), (Point::new(10.5, 20.5), 20));
}

#[test]
fn under_the_farm_setting_farm_gather_goes_in_four_timed_stops() {
    let rate = StopRate {
        work_rate: WorkRate::new(0.33),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let gather_action =
        GatherAction::new(task(), &farm(), FarmMode::Walkable, rate, &roles()).unwrap();
    assert_eq!(
        (
            gather_action.stops,
            gather_action.time_per_stop,
            gather_action.farm
        ),
        (
            FarmStops::of_data(4),
            Seconds::new(((10.0f32 * 1.0) / (0.33 * 1.0)) / 4.0),
            Point::new(112.5, 173.5)
        )
    );
    // without the setting, or on a fish trap, one stop
    let one = GatherAction::new(task(), &farm(), FarmMode::Fixed, rate, &roles()).unwrap();
    let trap = Resource {
        master: ids::FISH_TRAP,
        ..farm()
    };
    let trap = GatherAction::new(task(), &trap, FarmMode::Walkable, rate, &roles()).unwrap();
    assert_eq!(
        (one.stops, one.time_per_stop, trap.stops),
        (FarmStops::ONE, Seconds::ZERO, FarmStops::ONE)
    );
}

/// The farm's roles move with the layer (FD-046-D23): under a layer that names the stand-in
/// class 99 a farm and master 99 a farm of stops, a gather on master 99 goes in the four timed
/// stops of master 50's; under build 158041's layer the same target is one stop.
#[test]
fn second_farm_masters_farm() {
    let rate = StopRate {
        work_rate: WorkRate::new(0.33),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let stand_in = Resource {
        class: ClassId::named(99),
        master: master_id(99),
        ..farm()
    };
    let moved = Roles::from_ids(
        &[(99, ClassRoles::FARM)],
        &[(99, MasterRoles::STOP_FARM)],
        &[],
        &[],
    );
    let second = GatherAction::new(task(), &stand_in, FarmMode::Walkable, rate, &moved).unwrap();
    let first = GatherAction::new(task(), &farm(), FarmMode::Walkable, rate, &roles()).unwrap();
    assert_eq!(
        (second.stops, second.time_per_stop, second.farm),
        (first.stops, first.time_per_stop, first.farm)
    );
    assert_eq!(first.stops, FarmStops::of_data(4));
    let stock = GatherAction::new(task(), &stand_in, FarmMode::Walkable, rate, &roles()).unwrap();
    assert_eq!((stock.stops, stock.farm), (FarmStops::ONE, Point::ORIGIN));
}

/// Slot 0x38 (`FUN_140a8e1b0`, `set_state(2)`): the `MoveTo` goes and stops the unit, the
/// timer clears, the unit stands, the target and the drop site go (returned for the
/// caller's release) and the point is −1.
#[test]
fn stop_idles_the_gather_and_lets_its_targets_go() {
    let mut mover = walker();
    let mut gather_action = GatherAction {
        state: GatherState::Walk {
            move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        },
        target: Some(GatherTargetId::new(object_id(31))),
        carry_to: Some(DropSiteId::new(object_id(9))),
        point: Position::new(5.0, 6.0, 0.0),
        timer: Seconds::new(2.0),
        ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    let stand = Graphic {
        id: SpriteId::named(4),
        frames: Frame::of_data(1),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let (mut sprite, mut rng) = (None, Well512::default());
    let released = gather_action
        .halt(
            Some(&mut mover),
            Stand {
                graphic: Some(stand),
                sprite: &mut sprite,
                rng: &mut rng,
                sprite_tuning: &tuning().sprite,
            },
        )
        .unwrap();
    assert_eq!(released, [Some(object_id(31)), Some(object_id(9))]);
    assert_eq!(
        (
            gather_action.state,
            gather_action.timer,
            gather_action.target,
            gather_action.carry_to,
            gather_action.point,
            gather_action.state.move_to().copied()
        ),
        (
            GatherState::Idle,
            Seconds::ZERO,
            None,
            None,
            Position::from([-1.0; 3]),
            None
        )
    );
    assert_eq!(
        sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(4))
    );
    assert_eq!(mover.velocity.unwrap().velocity.y(), 0.0);
}

/// `FUN_140aa0df0` takes a target in state 6 or below and holds a later one as none
/// (`FUN_140a7a230`): the `MoveTo` has no target and walks to its place as to a point, far.
#[test]
fn gather_move_to_refuses_target_past_state_6() {
    let mover = walker();
    let gather_action =
        GatherAction::new(task(), &farm(), FarmMode::Walkable, rate(), &roles()).unwrap();
    let to = |state| WalkTo {
        id: Some(object_id(31)),
        at: Position::new(5.0, 6.0, 0.0),
        gap: Distance::ZERO,
        state: Some(state),
    };
    let walk = |state| {
        gather_action
            .move_to_object(
                to(state),
                &mover,
                &walks(Held::default()),
                &tuning().move_to,
            )
            .unwrap()
            .map(|move_to_action| {
                (
                    ObjectId::field(move_to_action.target),
                    move_to_action.point,
                    move_to_action.state,
                )
            })
    };
    assert_eq!(
        walk(ObjectState::Undead),
        Some((31, Position::new(5.0, 6.0, 0.0), MoveTo::Arrived))
    );
    assert_eq!(
        walk(ObjectState::Removed),
        Some((-1, Position::new(5.0, 6.0, 0.0), MoveTo::Start))
    );
}

#[test]
fn past_its_time_per_stop_farmer_walks_to_the_next_stop() {
    let mut mover = walker();
    let mut gather_action = GatherAction {
        state: GatherState::Work,
        time_per_stop: Seconds::new(1.0),
        time_at_stop: Seconds::new(0.95),
        ..GatherAction::new(task(), &farm(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let mut rng = Well512::default();
    let at = stop_point(
        Position::new(112.5, 173.5, 3.0),
        Footprint::new(1.5, 1.5),
        Point::ORIGIN,
        &mut rng.clone(),
        &tuning().gather,
    );
    let walks = walks(Held::default());
    let to = WalkTo {
        id: gather_action.target.map(GatherTargetId::id),
        at: Position::new(112.5, 173.5, 3.0),
        gap: Distance::ZERO,
        state: Some(ObjectState::Alive),
    };
    gather_action
        .set_walk(
            to,
            Footprint::new(1.5, 1.5),
            &mut mover,
            &mut rng,
            &walks,
            tuning(),
        )
        .unwrap();
    // a farm's stop: a quiet MoveTo to a point of the farm within the task's range, which asks
    // again when it moves (`+0x61` 1)
    let to_stop = MoveToAction {
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_to_point(at.with_height(3.0), task().range, None)
    };
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.last_stop,
            gather_action.state.move_to().copied()
        ),
        (GatherState::Walk { move_to: None }, at, Some(to_stop))
    );
}

#[test]
fn farm_food_sold_at_once_goes_to_the_player_and_the_gathering_goes_on() {
    let gather_action = GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    let mut rig = Rig::with_attributes(250);
    rig.set_load(Some(attribute::FARM_FOOD), 0.0);
    let mut field = Victim {
        class: class::FARM,
        master_id: ids::FARM,
        ..target(15.0)
    };
    let gather_reads = GatherReads {
        work_rate: WorkRate::new(0.5),
        food_sold: Amount::new(1.0),
        ..reads()
    };
    let gathered = rig.run(GatherState::Work, 500, |step, unit, _, _| {
        gather_action.gather(step, unit, &mut field, &gather_reads, &roles())
    });
    assert_eq!(gathered, Ok(Next::Stay));
    // the player gains it as food at once (resource 0 and its gathered total, attribute 166)
    let owner = &rig.world.players[1];
    assert_eq!(
        (
            owner.attributes[0],
            owner.attributes[166],
            rig.load().amount,
            field.amount
        ),
        (
            Amount::new(0.25),
            Amount::new(0.25),
            Amount::ZERO,
            Amount::new(14.75)
        )
    );
}

#[test]
fn gatherer_moved_walks_to_the_point_or_carries_its_load_to_the_drop_site() {
    let mut mover = walker();
    let gatherer = || GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    // a move to a point (or an object that is no drop site): state 15 to the point
    let mut gather_action = gatherer();
    let loaded = walks(carrying(Some(AttributeId::named(17)), 5.0));
    gather_action
        .take_move(
            None,
            Position::new(30.0, 40.0, 0.0),
            PlayerId::new(7),
            &mut mover,
            &loaded,
            &tuning().move_to,
        )
        .unwrap();
    // a quiet MoveTo to the point within the task's range; its `+0x61` stays 0
    let to_point = MoveToAction::sub_to_point(Position::new(30.0, 40.0, 0.0), task().range, None);
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.point,
            gather_action.state.move_to().copied()
        ),
        (
            GatherState::Moved { move_to: None },
            None,
            Position::new(30.0, 40.0, 0.0),
            Some(to_point)
        )
    );
    // a loaded gatherer moved to its own mill carries the load there
    let mill = MoveTarget {
        id: object_id(500),
        player: PlayerId::new(7),
        drop_site: true,
        position: Position::new(12.5, 5.5, 0.0),
        gap: Distance::new(3.0),
    };
    let mut gather_action = gatherer();
    gather_action
        .take_move(
            Some(mill),
            Position::from([1.0; 3]),
            PlayerId::new(7),
            &mut mover,
            &loaded,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.carry_to,
            gather_action.point
        ),
        (
            GatherState::Drop { move_to: None },
            None,
            Some(DropSiteId::new(object_id(500))),
            Position::from([-1.0; 3])
        )
    );
    // a quiet MoveTo to the mill that asks again when it moves, from where the mill stands
    let to_mill = MoveToAction {
        state: MoveTo::Start,
        point: Position::new(12.5, 5.5, 0.0),
        target: Some(object_id(500)),
        range: task().range,
        sprite: None,
        voice: Voice::Quiet,
        on_target_moved: TargetMoved::AsksAgain,
        wait: MoveWait::NEW,
    };
    assert_eq!(gather_action.state.move_to().copied(), Some(to_mill));
    // already on its way there, nothing changes; empty-handed it walks to the point
    let walking = gather_action.state.move_to().copied();
    gather_action
        .take_move(
            Some(mill),
            Position::from([1.0; 3]),
            PlayerId::new(7),
            &mut mover,
            &loaded,
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.state.move_to().copied()
        ),
        (GatherState::Drop { move_to: None }, walking)
    );
    let mut gather_action = gatherer();
    gather_action
        .take_move(
            Some(mill),
            Position::from([1.0; 3]),
            PlayerId::new(7),
            &mut mover,
            &walks(carrying(Some(AttributeId::named(17)), 0.0)),
            &tuning().move_to,
        )
        .unwrap();
    assert_eq!(
        (gather_action.state.bare(), gather_action.carry_to),
        (GatherState::Moved { move_to: None }, None)
    );
}

#[test]
fn at_its_point_human_moved_gatherer_idles_and_computer_is_done() {
    let run = |player_kind: PlayerKind| {
        let mut gather_action = GatherAction {
            state: GatherState::Moved {
                move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
            },
            target: None,
            ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
        };
        let mut rig = Rig::on_the_path();
        let gather_reads = GatherReads {
            player_kind,
            farm_mode: FarmMode::Walkable,
            ..reads()
        };
        let moved = rig.run(
            GatherState::Moved { move_to: None },
            100,
            |step, unit, _, me| gather_action.update_moved(step, unit, me, &gather_reads, &roles()),
        );
        assert_eq!(moved, Ok(Next::Stay));
        (
            gather_action.state,
            gather_action.point,
            gather_action.state.move_to().copied(),
        )
    };
    assert_eq!(
        run(PlayerKind::Human),
        (GatherState::Idle, Position::from([-1.0; 3]), None)
    );
    assert_eq!(run(PlayerKind::Computer).0, GatherState::Done);
}

#[test]
fn gatherer_without_target_or_load_asks_for_another_every_ten_seconds() {
    // the 4v4's forager 31237 at update 10820: its bush went while it walked there
    let run = |master: MasterId, gather_action: &mut GatherAction, dt: u32| {
        let mut rig = Rig::on_the_path();
        rig.set_load(None, 0.0);
        let gather_reads = GatherReads { master, ..reads() };
        rig.run(gather_action.state, dt, |step, unit, _, me| {
            gather_action.update(step, unit, me, None, &gather_reads, &roles())
        })
    };
    let mut gather_action = GatherAction {
        state: GatherState::Walk {
            move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        },
        target: None,
        ..GatherAction::new(task(), &bush(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    assert_eq!(
        run(master_id(120), &mut gather_action, 20),
        Ok(Next::Search)
    );
    assert_eq!(
        (
            gather_action.state,
            gather_action.timer,
            gather_action.state.move_to().copied()
        ),
        (GatherState::Search, Seconds::new(-10.0), None)
    );
    // state 3 counts the world's dt up to 0, then waits 10 seconds again and asks again
    gather_action.timer = Seconds::new(-0.03);
    assert_eq!(run(master_id(120), &mut gather_action, 20), Ok(Next::Stay));
    assert_eq!(
        run(master_id(120), &mut gather_action, 20),
        Ok(Next::Search)
    );
    assert_eq!(gather_action.timer, Seconds::new(-10.0));
    // a farmer waits a second for another farm instead (set_state 0xf3), its unit AI asked
    let mut gather_action = GatherAction {
        state: GatherState::Work,
        target: None,
        ..GatherAction::new(task(), &bush(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    assert_eq!(
        run(master_id(259), &mut gather_action, 20),
        Ok(Next::Search)
    );
    assert_eq!(
        (gather_action.state, gather_action.timer),
        (GatherState::FarmWait, Seconds::new(-1.0))
    );
}

/// A farmer's (master 259) update in its state with `target`, the world's dt `dt` ms, a
/// human's unless `computer`, empty-handed: what it answers, with the gather after it.
fn farm_update(
    gather_action: &mut GatherAction,
    target: Option<&mut Victim>,
    dt: u32,
    player_kind: PlayerKind,
) -> GenieResult<Next> {
    let mut rig = Rig::on_the_path();
    rig.set_load(None, 0.0);
    let gather_reads = GatherReads {
        master: master_id(259),
        player_kind,
        ..reads()
    };
    rig.run(gather_action.state, dt, |step, unit, _, me| {
        gather_action.update(step, unit, me, target, &gather_reads, &roles())
    })
}

/// Case 5 (`FUN_141162db0`): a gatherer with a turn time turns a facet a turn time; not yet
/// facing its target, the switch breaks and it stays in state 6 for the next update.
#[test]
fn gatherer_turning_over_time_faces_its_target_later() {
    let mut gather_action = GatherAction {
        state: GatherState::Face,
        target: Some(GatherTargetId::new(object_id(28222))),
        ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    let mut mover = walker();
    mover.stats.turn_time = Seconds::new(1.0);
    mover.stats.facets = Some(SpriteFacets {
        count: FacetCount::of_data(16),
        turns: true,
    });
    mover.angle = Angle::ZERO;
    let before = mover.angle;
    let mut rig = Rig::on_the_path();
    rig.world.world_time = WorldTime::from_millis(5000);
    rig.set_load(None, 0.0);
    rig.unit_mut().place = Static::at(Position::new(30.0, 40.0, 0.0));
    let id = rig.unit().id;
    rig.world.set_motion(id, Some(Motion::Moving(mover)));
    let gather_reads = GatherReads {
        master: master_id(259),
        ..reads()
    };
    // the bush straight behind the unit (the angle 0 faces +x)
    let mut bush = Victim {
        position: Position::new(20.0, 40.0, 0.0),
        ..target(10.0)
    };
    let faced = rig.run(GatherState::Face, 100, |step, unit, _, me| {
        gather_action.update(step, unit, me, Some(&mut bush), &gather_reads, &roles())
    });
    assert_eq!(faced, Ok(Next::Stay));
    assert_eq!(gather_action.state, GatherState::Face);
    assert_ne!(
        rig.world.parts.mover_of(rig.unit().id).unwrap().angle,
        before
    );
}

/// State 8's drop-off (`0x140a8e1d0` case 7) by its `MoveTo`: the timer's wait (below 0 it
/// counts up, at 0 the drop-off starts again); a waiting `MoveTo` starts again past its
/// delay; a failed one waits a random while, the animation stopped; an arrived one stops the
/// unit, then a site gone starts again, a site that does not take the load idles, one out of
/// reach waits, and one in reach takes the load, as its player's resource kind.
#[test]
fn gatherer_drop_off_waits_walks_and_drops_its_load_at_the_site() {
    let dropping = |move_to: Option<MoveTo>, timer| GatherAction {
        state: GatherState::Drop {
            move_to: move_to.map(MoveToAction::sub_in),
        },
        carry_to: Some(DropSiteId::new(object_id(10028))),
        timer,
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let site = |takes, gap_squared| Some(DropOff { takes, gap_squared });
    let drop = |gather_action: &mut GatherAction, rig: &mut Rig, site| {
        rig.run(
            GatherState::Drop { move_to: None },
            100,
            |step, unit, _, me| gather_action.update_drop(step, unit, me, &reads(), site, &roles()),
        )
    };
    let mut rig = Rig::on_the_path();
    let mut gather_action = dropping(Some(MoveTo::Arrived), Seconds::new(-0.15));
    assert_eq!(drop(&mut gather_action, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(drop(&mut gather_action, &mut rig, None), Ok(Dropped::Again));
    assert_eq!(gather_action.timer, Seconds::ZERO);
    let mut gather = dropping(Some(MoveTo::Waiting), Seconds::ZERO);
    *gather.state.walk_mut().unwrap() =
        gather
            .state
            .move_to()
            .copied()
            .map(|move_to_action| MoveToAction {
                wait: MoveWait {
                    delay: Seconds::new(0.15),
                    ..move_to_action.wait
                },
                ..move_to_action
            });
    let waiting = |gather_action: &GatherAction| {
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| (move_to_action.state, move_to_action.wait.timer))
    };
    assert_eq!(drop(&mut gather, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(waiting(&gather), Some((MoveTo::Waiting, Seconds::new(0.1))));
    assert_eq!(drop(&mut gather, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(waiting(&gather), Some((MoveTo::Start, Seconds::ZERO)));
    let mut gather_action = dropping(Some(MoveTo::Failed), Seconds::ZERO);
    let draw = rig.next_wait();
    assert_eq!(drop(&mut gather_action, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(gather_action.timer, Seconds::new(draw));
    assert!(rig.anim_stopped());
    let mut gather_action = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(drop(&mut gather_action, &mut rig, None), Ok(Dropped::Again));
    assert_eq!(gather_action.state.move_to().copied(), None);
    let mut gather_action = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(
        drop(
            &mut gather_action,
            &mut rig,
            site(false, DistanceSquared::ZERO)
        ),
        Ok(Dropped::Idle)
    );
    assert_eq!(gather_action.state, GatherState::Idle);
    let mut gather_action = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    let far = tuning().gather.drop_reach + 1.0;
    assert_eq!(
        drop(
            &mut gather_action,
            &mut rig,
            site(true, DistanceSquared::new(far))
        ),
        Ok(Dropped::Going)
    );
    assert!(gather_action.timer < Seconds::ZERO);
    let mut gather_action = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(
        drop(
            &mut gather_action,
            &mut rig,
            site(true, DistanceSquared::ZERO)
        ),
        Ok(Dropped::Load(
            resource_kind(Some(AttributeId::named(17)), 4, slots()),
            Amount::new(3.0)
        ))
    );
    assert_eq!(rig.load(), Held::default());
    // an empty list answers 1 ("There is no list."): the drop site is looked for again
    let mut gather_action = dropping(None, Seconds::ZERO);
    assert_eq!(drop(&mut gather_action, &mut rig, None), Ok(Dropped::Again));
}

/// The sub-action list's codes in state 8 (`0x140a8e1d0` case 7): the `MoveTo` to the drop site
/// let its target go (code 5: its check of the target found it past state 6) waits a random
/// while as a failed one does; the `MoveTo` then walks on to the point it held.
#[test]
fn drop_off_whose_site_was_let_go_waits_random_while() {
    let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
    let mut gather_action = GatherAction {
        state: GatherState::Drop {
            move_to: Some(to_house(MoveTo::Moving)),
        },
        carry_to: Some(DropSiteId::new(object_id(10028))),
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let draw = rig.next_wait();
    let dropped = rig.run(
        GatherState::Drop { move_to: None },
        100,
        |step, unit, _, me| gather_action.update_drop(step, unit, me, &reads(), None, &roles()),
    );
    assert_eq!(dropped, Ok(Dropped::Going));
    assert_eq!(gather_action.timer, Seconds::new(draw));
    assert_eq!(
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| ObjectId::field(move_to_action.target)),
        Some(-1)
    );
    assert!(rig.anim_stopped());
}

/// State 8's `MoveTo` to the drop site (`0x140a8e1d0` case 7, `FUN_140a992b0`): it starts
/// toward the site, asking for a path; moving, it walks on while the site stays on its point
/// and starts again once the site left it.
#[test]
fn gatherer_walks_to_its_drop_site_and_starts_again_when_it_moved() {
    let dropping = |move_to| GatherAction {
        state: GatherState::Drop {
            move_to: Some(to_house(move_to)),
        },
        carry_to: Some(DropSiteId::new(object_id(10028))),
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let drop = |gather_action: &mut GatherAction, rig: &mut Rig| {
        rig.run(
            GatherState::Drop { move_to: None },
            100,
            |step, unit, _, me| gather_action.update_drop(step, unit, me, &reads(), None, &roles()),
        )
    };
    let state = |gather_action: &GatherAction| {
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.state)
    };
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    rig.mover_mut().paths.clear();
    let mut gather_action = dropping(MoveTo::Start);
    assert_eq!(drop(&mut gather_action, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&gather_action), Some(MoveTo::Moving));
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    let mut gather_action = dropping(MoveTo::Moving);
    assert_eq!(drop(&mut gather_action, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&gather_action), Some(MoveTo::Moving));
    // the site left the point the MoveTo holds: it starts again
    *gather_action.state.walk_mut().unwrap() =
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| MoveToAction {
                point: Position::new(35.0, 88.0, 0.0),
                ..move_to_action
            });
    assert_eq!(drop(&mut gather_action, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&gather_action), Some(MoveTo::Start));
}

/// State 4 (case 3) by its `MoveTo` ([`GatherAction::update_walk`]): it starts toward the
/// target, asking for a path; moving, it walks on while the target stays on its point and
/// starts again once it left; a waiting one starts again past its delay; an arrived one stops
/// the unit and faces the target (`set_state(6)`); a failed one near a target of more than 3
/// workers searches (`set_state(3)`), else waits a random while; none is not modelled.
#[test]
fn gatherer_walks_to_its_target_and_faces_it_on_arrival() {
    let walking = |move_to: Option<MoveTo>| GatherAction {
        state: GatherState::Walk {
            move_to: move_to.map(to_house),
        },
        target: Some(GatherTargetId::new(object_id(10028))),
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let state = |gather_action: &GatherAction| {
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.state)
    };
    let house = |workers| Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        workers,
        ..target(10.0)
    };
    let walk = |gather_action: &mut GatherAction, rig: &mut Rig, workers| {
        let mut house = house(workers);
        rig.run(
            GatherState::Walk { move_to: None },
            100,
            |step, unit, _, me| {
                gather_action.update(step, unit, me, Some(&mut house), &reads(), &roles())
            },
        )
    };
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    rig.mover_mut().paths.clear();
    let mut gather_action = walking(Some(MoveTo::Start));
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(
        (gather_action.state.bare(), state(&gather_action)),
        (GatherState::Walk { move_to: None }, Some(MoveTo::Moving))
    );
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    let mut gather_action = walking(Some(MoveTo::Moving));
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(state(&gather_action), Some(MoveTo::Moving));
    // the target left the point the MoveTo holds: it starts again
    *gather_action.state.walk_mut().unwrap() =
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| MoveToAction {
                point: Position::new(35.0, 88.0, 0.0),
                ..move_to_action
            });
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(state(&gather_action), Some(MoveTo::Start));
    let mut gather_action = walking(Some(MoveTo::Waiting));
    *gather_action.state.walk_mut().unwrap() =
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| MoveToAction {
                wait: MoveWait {
                    delay: Seconds::new(0.05),
                    ..move_to_action.wait
                },
                ..move_to_action
            });
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(state(&gather_action), Some(MoveTo::Start));
    let mut gather_action = walking(Some(MoveTo::Arrived));
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.state.move_to().copied(),
            gather_action.timer
        ),
        (GatherState::Face, None, Seconds::ZERO)
    );
    let mut gather_action = walking(Some(MoveTo::Failed));
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(4)),
        Ok(Next::Search)
    );
    assert_eq!(
        (gather_action.state.bare(), gather_action.timer),
        (GatherState::Search, SEARCH_WAIT)
    );
    let mut rig = Rig::on_the_path();
    let mut gather_action = walking(Some(MoveTo::Failed));
    let draw = rig.next_wait();
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(3)),
        Ok(Next::Stay)
    );
    assert_eq!(gather_action.timer, Seconds::new(draw));
    assert!(rig.anim_stopped());
    // an empty list answers 1 ("There is no list."): the unit faces its target
    let mut gather_action = walking(None);
    assert_eq!(
        walk(&mut gather_action, &mut rig, WorkerCount::of_data(0)),
        Ok(Next::Stay)
    );
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.state.move_to().copied()
        ),
        (GatherState::Face, None)
    );
}

/// State 4's code 5 (`0x140a8e1d0` case 3: the `MoveTo`'s check found its target past state 6):
/// the Gather's target goes (slot 0x68 with none) and the gatherer walks back to its point
/// within the task's range (`set_state(11)`, no target's radius), a `MoveTo` to the point whose
/// `+0x61` stays 0.
#[test]
fn gatherer_whose_move_to_lost_its_target_walks_back_to_its_point() {
    let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
    let mut gather_action = GatherAction {
        state: GatherState::Walk {
            move_to: Some(to_house(MoveTo::Moving)),
        },
        target: Some(GatherTargetId::new(object_id(10028))),
        point: Position::new(30.0, 88.0, 0.0),
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let mut house = Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        ..target(10.0)
    };
    let next = rig.run(
        GatherState::Walk { move_to: None },
        100,
        |step, unit, _, me| {
            gather_action.update(step, unit, me, Some(&mut house), &reads(), &roles())
        },
    );
    assert_eq!(next, Ok(Next::Stay));
    assert_eq!(
        (gather_action.state.bare(), gather_action.target),
        (GatherState::WalkBack { move_to: None }, None)
    );
    let to_point = MoveToAction::sub_to_point(Position::new(30.0, 88.0, 0.0), task().range, None);
    assert_eq!(gather_action.state.move_to().copied(), Some(to_point));
}

/// The code 5 of the walks to a point: state 11 searches (`set_state(3)`), state 15 idles a
/// human's gatherer and ends a computer's, state 0xf2 ends as an arrived walk does; code 3 is
/// failed in states 11 and 15, and a quiet `MoveTo` never gives the codes above 5.
#[test]
fn walk_to_point_ends_on_its_target_let_go_as_on_its_arrival() {
    let at = |state, target| {
        let mut gather_action = GatherAction {
            state,
            target: None,
            ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
        };
        *gather_action.state.walk_mut().unwrap() = Some(MoveToAction {
            target: ObjectId::of_field(target),
            ..MoveToAction::sub_in(MoveTo::Moving)
        });
        gather_action
    };
    let run = |gather_action: &mut GatherAction, player_kind: PlayerKind| {
        let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
        rig.set_load(None, 0.0);
        let gather_reads = GatherReads {
            player_kind,
            ..reads()
        };
        rig.run(
            gather_action.state,
            100,
            |step, unit, _, me| match gather_action.state {
                GatherState::WalkBack { .. } => {
                    gather_action.update_walk_back(step, unit, me, &gather_reads, &roles())
                }
                GatherState::Moved { .. } => {
                    gather_action.update_moved(step, unit, me, &gather_reads, &roles())
                }
                _ => {
                    gather_action.update_reseed_walk(step, unit, me, None, &gather_reads, &roles())
                }
            },
        )
    };
    let mut gather_action = at(GatherState::WalkBack { move_to: None }, 10028);
    assert_eq!(run(&mut gather_action, PlayerKind::Human), Ok(Next::Search));
    assert_eq!(
        (gather_action.state, gather_action.state.move_to().copied()),
        (GatherState::Search, None)
    );
    let mut gather_action = at(GatherState::Moved { move_to: None }, 10028);
    assert_eq!(run(&mut gather_action, PlayerKind::Human), Ok(Next::Stay));
    assert_eq!(
        (gather_action.state, gather_action.state.move_to().copied()),
        (GatherState::Idle, None)
    );
    let mut gather = at(GatherState::Moved { move_to: None }, 10028);
    assert_eq!(run(&mut gather, PlayerKind::Computer), Ok(Next::Stay));
    assert_eq!(gather.state, GatherState::Done);
    let mut gather = at(GatherState::ReseedWalk { move_to: None }, 10028);
    assert_eq!(
        run(&mut gather, PlayerKind::Human),
        Ok(Next::Reseed {
            end: WalkEnd::Arrived
        })
    );
    assert_eq!(gather.state.move_to().copied(), None);
    // a failed walk: state 15 idles a computer's too
    for (state, player_kind, then) in [
        (
            GatherState::WalkBack { move_to: None },
            PlayerKind::Human,
            GatherState::Search,
        ),
        (
            GatherState::Moved { move_to: None },
            PlayerKind::Computer,
            GatherState::Idle,
        ),
    ] {
        let mut gather_action = at(state, -1);
        *gather_action.state.walk_mut().unwrap() = Some(MoveToAction::sub_in(MoveTo::Failed));
        let _ = run(&mut gather_action, player_kind);
        assert_eq!(gather_action.state, then);
    }
}

/// Case 5 facing its target at once (no turn time): with something left in the target it
/// works (`set_state(7)`): the gathering starts at the unit's place, a load of another kind
/// gives way to the task's, and the unit shows its working sprite.
#[test]
fn gatherer_facing_its_target_works_it() {
    let working = Graphic {
        id: SpriteId::named(7),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let mut rig = Rig::on_the_path();
    rig.mover_mut().stats.turn_time = Seconds::ZERO;
    rig.set_load(Some(attribute::STONE), 4.0);
    let mut gather_action = GatherAction {
        state: GatherState::Face,
        target: Some(GatherTargetId::new(object_id(10028))),
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let mut house = Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        ..target(10.0)
    };
    let mut gather_reads = reads();
    gather_reads.sprites.working = Some(working);
    let next = rig.run(GatherState::Face, 100, |step, unit, _, me| {
        gather_action.update(step, unit, me, Some(&mut house), &gather_reads, &roles())
    });
    assert_eq!(next, Ok(Next::Stay));
    assert_eq!(
        (
            gather_action.state,
            gather_action.worked,
            gather_action.gather_at,
            gather_action.time_at_stop
        ),
        (
            GatherState::Work,
            true,
            Point::new(28.0, 89.0),
            Seconds::ZERO
        )
    );
    assert_eq!(rig.load(), carrying(Some(AttributeId::named(17)), 0.0));
    assert_eq!(
        rig.unit()
            .sprite
            .map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(7))
    );
}

/// A farmer waiting for another farm (state 0xf3), its farm gone.
fn waiting(worked: bool) -> GatherAction {
    GatherAction {
        state: GatherState::FarmWait,
        timer: Seconds::new(-1.0),
        target: None,
        target_class: Some(class::FARM),
        worked,
        gather_at: Point::new(112.0, 173.0),
        ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
    }
}

/// State 0xf3 counts the world's dt up to 0; then a farmer that never gathered idles, one
/// that gathered walks back to where it gathered (state 0xf2).
#[test]
fn farmer_without_its_farm_waits_second_then_walks_back_to_reseed() {
    let mut gather_action = waiting(true);
    assert_eq!(
        farm_update(&mut gather_action, None, 600, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(
        (gather_action.state.bare(), gather_action.timer),
        (GatherState::FarmWait, Seconds::new(-1.0 + 0.6))
    );
    assert_eq!(
        farm_update(&mut gather_action, None, 600, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(
        gather_action.state.bare(),
        GatherState::ReseedWalk { move_to: None }
    );
    assert_eq!(gather_action.point.ground(), Point::new(112.0, 173.0));
    // a quiet MoveTo to the point within the task's range (`+0x61` 0), its start `0x140aa2100`
    let to_point = MoveToAction::sub_to_point(Position::new(112.0, 173.0, 3.0), task().range, None);
    assert_eq!(gather_action.state.move_to().copied(), Some(to_point));
    // without a gather point the walk goes to the point it holds
    let mut gather_action = waiting(true);
    gather_action.gather_at = Point::new(-1.0, -1.0);
    gather_action.point = Position::new(5.0, 6.0, 0.0);
    assert_eq!(
        farm_update(&mut gather_action, None, 1000, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(
        (gather_action.state.bare(), gather_action.point),
        (
            GatherState::ReseedWalk { move_to: None },
            Position::new(5.0, 6.0, 0.0)
        )
    );
    // a farmer that never gathered idles
    let mut gather_action = waiting(false);
    assert_eq!(
        farm_update(&mut gather_action, None, 1000, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.point
        ),
        (GatherState::Idle, None, Position::from([-1.0; 3]))
    );
    // a timer at 0 or more waits on
    let mut gather_action = waiting(true);
    gather_action.timer = Seconds::ZERO;
    assert_eq!(
        farm_update(&mut gather_action, None, 1000, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(gather_action.state.bare(), GatherState::FarmWait);
}

/// A farmer that never gathered, at a farm another farmer works too, walks back to its
/// point to search (state 11, the farm's radius none) and lets the farm go.
#[test]
fn farmer_at_farm_another_works_walks_back_to_search() {
    let mut gather_action = waiting(false);
    gather_action.target = Some(GatherTargetId::new(object_id(12557)));
    let mut farm = Victim {
        id: object_id(12557),
        amount: Amount::new(175.0),
        state: ObjectState::Alive,
        class: class::FARM,
        master_id: ids::FARM,
        radius: Footprint::new(1.5, 1.5),
        workers: WorkerCount::of_data(2),
        ..Victim::default()
    };
    assert_eq!(
        farm_update(&mut gather_action, Some(&mut farm), 1000, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(
        (gather_action.state.bare(), gather_action.target),
        (GatherState::WalkBack { move_to: None }, None)
    );
    assert_eq!(
        gather_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.range),
        Some(task().range)
    );
}

/// State 0xf2's walk: once it arrived without a target, or failed, the caller reseeds; a
/// target standing (state 2) is faced again.
#[test]
fn at_the_end_of_its_walk_back_farmer_reseeds() {
    let walking = |move_to| GatherAction {
        state: GatherState::ReseedWalk {
            move_to: Some(MoveToAction::sub_in(move_to)),
        },
        target: None,
        ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
    };
    let mut gather_action = walking(MoveTo::Arrived);
    assert_eq!(
        farm_update(&mut gather_action, None, 100, PlayerKind::Human),
        Ok(Next::Reseed {
            end: WalkEnd::Arrived
        })
    );
    assert_eq!(gather_action.state.move_to().copied(), None);
    let mut gather_action = walking(MoveTo::Failed);
    assert_eq!(
        farm_update(&mut gather_action, None, 100, PlayerKind::Human),
        Ok(Next::Reseed {
            end: WalkEnd::Failed
        })
    );
    // the farm still standing: the farmer faces it (set_state 6)
    let mut gather_action = walking(MoveTo::Arrived);
    gather_action.target = Some(GatherTargetId::new(object_id(28222)));
    let mut bush = target(10.0);
    assert_eq!(
        farm_update(&mut gather_action, Some(&mut bush), 100, PlayerKind::Human),
        Ok(Next::Stay)
    );
    assert_eq!(gather_action.state, GatherState::Face);
    // a dying one (state 3) does not hold it: the reseed
    let mut gather_action = walking(MoveTo::Arrived);
    gather_action.target = Some(GatherTargetId::new(object_id(28222)));
    let mut dying = Victim {
        state: ObjectState::Dying,
        ..target(10.0)
    };
    assert_eq!(
        farm_update(&mut gather_action, Some(&mut dying), 100, PlayerKind::Human),
        Ok(Next::Reseed {
            end: WalkEnd::Arrived
        })
    );
}

/// Without a reseed a computer's farmer whose walk arrived is done; any other idles and
/// lets its targets go.
#[test]
fn without_reseed_the_farmer_idles_or_computer_is_done() {
    let after = |end: WalkEnd, player_kind: PlayerKind| {
        let mut gather_action = GatherAction {
            state: GatherState::ReseedWalk {
                move_to: Some(MoveToAction::sub_in(MoveTo::Failed)),
            },
            target: Some(GatherTargetId::new(object_id(5))),
            carry_to: Some(DropSiteId::new(object_id(6))),
            ..GatherAction::new(task(), &farm(), FarmMode::Fixed, rate(), &roles()).unwrap()
        };
        let mut mover = walker();
        let (mut sprite, mut rng) = (None, Well512::default());
        let released = gather_action
            .after_reseed(
                end,
                player_kind,
                Some(&mut mover),
                Stand {
                    graphic: None,
                    sprite: &mut sprite,
                    rng: &mut rng,
                    sprite_tuning: &tuning().sprite,
                },
            )
            .unwrap();
        (
            gather_action.state,
            released,
            gather_action.target,
            gather_action.state.move_to().copied(),
        )
    };
    assert_eq!(
        after(WalkEnd::Arrived, PlayerKind::Computer),
        (
            GatherState::Done,
            [None, None],
            Some(GatherTargetId::new(object_id(5))),
            None
        )
    );
    assert_eq!(
        after(WalkEnd::Arrived, PlayerKind::Human),
        (
            GatherState::Idle,
            [Some(object_id(5)), Some(object_id(6))],
            None,
            None
        )
    );
    assert_eq!(
        after(WalkEnd::Failed, PlayerKind::Computer),
        (
            GatherState::Idle,
            [Some(object_id(5)), Some(object_id(6))],
            None,
            None
        )
    );
}

#[test]
fn forager_walks_to_its_bush() {
    let mut mover = walker();
    let mut gather_action =
        GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap();
    assert_eq!(
        (
            gather_action.stops,
            gather_action.gather_at,
            gather_action.target_class
        ),
        (FarmStops::ONE, Point::from([-1.0; 2]), class_of(7))
    );
    let mut held = carrying(Some(attribute::STONE), 0.0);
    let carried = walks(held);
    gather_action
        .start(
            GatherStart {
                target: &bush(),
                player: PlayerId::new(7),
                player_kind: PlayerKind::Human,
                carry_rule: false,
                held: &mut held,
                walks: carried,
            },
            &mut mover,
            &mut Well512::default(),
            &roles(),
            tuning(),
        )
        .unwrap();
    // a quiet MoveTo to the bush that asks again when it moves
    let to_bush = MoveToAction {
        point: Position::new(23.5, 93.5, 0.0),
        target: Some(object_id(28222)),
        range: task().range,
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_in(MoveTo::Start)
    };
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.state.move_to().copied(),
            held
        ),
        (
            GatherState::Walk { move_to: None },
            Some(to_bush),
            carrying(Some(attribute::STONE), 0.0)
        )
    );
    // a load of another food becomes the task's kind
    let mut gather_action =
        GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap();
    let mut held = carrying(Some(attribute::MEAT), 3.0);
    let carried = walks(held);
    gather_action
        .start(
            GatherStart {
                target: &bush(),
                player: PlayerId::new(7),
                player_kind: PlayerKind::Human,
                carry_rule: false,
                held: &mut held,
                walks: carried,
            },
            &mut mover,
            &mut Well512::default(),
            &roles(),
            tuning(),
        )
        .unwrap();
    assert_eq!(held, carrying(Some(AttributeId::named(17)), 3.0));
}

/// Slot 0x48 on the work command's target: the 4v4's forager 31156 walking to bush 28222 (state
/// 4) is sent to it again in update 5026 and walks to it anew (`set_state(4)`); facing or
/// gathering it, it keeps on; a computer player's walking one too. A loaded one sent to its
/// player's drop site takes its load there (`set_state(8)`, the site its second target).
#[test]
fn gatherer_sent_again_to_its_bush_walks_to_it_anew() {
    let mut mover = walker();
    // a walk holds its MoveTo; facing or working, the gatherer holds none
    let walking = |state| {
        let mut gather_action = GatherAction {
            state,
            worked: true,
            ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
        };
        if let Some(move_to_action) = gather_action.state.walk_mut() {
            *move_to_action = Some(MoveToAction::sub_in(MoveTo::Moving));
        }
        gather_action
    };
    let mut gather_action = walking(GatherState::Walk { move_to: None });
    assert_eq!(
        gather_action.retarget(
            GatherRetarget {
                target: &bush(),
                player: PlayerId::new(7),
                player_kind: PlayerKind::Human,
                walks: walks(carrying(Some(AttributeId::named(17)), 0.0)),
                farm_mode: FarmMode::Walkable,
                rate: rate()
            },
            &mut mover,
            &mut Well512::default(),
            &roles(),
            tuning()
        ),
        Ok(())
    );
    let to_bush = MoveToAction {
        point: Position::new(23.5, 93.5, 0.0),
        target: Some(object_id(28222)),
        range: task().range,
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_in(MoveTo::Start)
    };
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.state.move_to().copied(),
            gather_action.target,
            gather_action.carry_to,
            gather_action.point,
            gather_action.target_class
        ),
        (
            GatherState::Walk { move_to: None },
            Some(to_bush),
            Some(GatherTargetId::new(object_id(28222))),
            None,
            Position::new(23.5, 93.5, 0.0),
            class_of(7)
        )
    );
    // the gathering is not started any more (+0x80 cleared first)
    assert!(!gather_action.worked);
    for (state, player_kind) in [
        (GatherState::Face, PlayerKind::Human),
        (GatherState::Work, PlayerKind::Human),
        (GatherState::Walk { move_to: None }, PlayerKind::Computer),
    ] {
        let mut state_gather_action = walking(state);
        assert_eq!(
            state_gather_action.retarget(
                GatherRetarget {
                    target: &bush(),
                    player: PlayerId::new(7),
                    player_kind,
                    walks: walks(carrying(Some(AttributeId::named(17)), 0.0)),
                    farm_mode: FarmMode::Walkable,
                    rate: rate()
                },
                &mut mover,
                &mut Well512::default(),
                &roles(),
                tuning()
            ),
            Ok(())
        );
        // it keeps on: the walk keeps its MoveTo (facing or working, it holds none)
        let kept = matches!(state, GatherState::Walk { .. });
        assert_eq!(
            (
                state_gather_action.state.bare(),
                state_gather_action.state.move_to().copied()
            ),
            (state, kept.then_some(MoveToAction::sub_in(MoveTo::Moving)))
        );
    }
    let mill = Resource {
        id: object_id(500),
        player: PlayerId::new(7),
        drop_site: true,
        ..bush()
    };
    let mut gather_action = walking(GatherState::Walk { move_to: None });
    assert_eq!(
        gather_action.retarget(
            GatherRetarget {
                target: &mill,
                player: PlayerId::new(7),
                player_kind: PlayerKind::Human,
                walks: walks(carrying(Some(AttributeId::named(17)), 3.0)),
                farm_mode: FarmMode::Walkable,
                rate: rate()
            },
            &mut mover,
            &mut Well512::default(),
            &roles(),
            tuning()
        ),
        Ok(())
    );
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.carry_to,
            gather_action.point
        ),
        (
            GatherState::Drop { move_to: None },
            None,
            Some(DropSiteId::new(object_id(500))),
            Position::from([-1.0; 3])
        )
    );
}

/// The forager's gathering: `(work rate) × work_val1 × dt` a tick into its load, the bush
/// losing as much; the last bit below 0.1 goes with a thousandth of the master's storage; a
/// load that would pass the capacity asks for the drop-off.
#[test]
fn forager_gathers_until_its_load_is_full() {
    let gather_action = GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &bush(), FarmMode::Walkable, rate(), &roles()).unwrap()
    };
    let mut rig = Rig::on_the_path();
    rig.set_load(Some(AttributeId::named(17)), 0.0);
    let gather_reads = GatherReads {
        farm_mode: FarmMode::Walkable,
        ..reads()
    };
    let gather = |rig: &mut Rig, bush: &mut Victim| {
        rig.run(GatherState::Work, 500, |step, unit, _, _| {
            gather_action.gather(step, unit, bush, &gather_reads, &roles())
        })
    };
    let mut bush = target(100.0);
    assert_eq!(gather(&mut rig, &mut bush), Ok(Next::Stay));
    assert_eq!(
        (rig.load().amount, bush.amount),
        (Amount::new(0.155), Amount::new(100.0 - 0.155))
    );
    // 0.2 less the work leaves a last bit below 0.1: it goes with the storage's thousandth
    let mut bush = target(0.2);
    let last = 0.2f32 - 0.155;
    assert_eq!(gather(&mut rig, &mut bush), Ok(Next::Drop));
    assert_eq!(bush.amount, Amount::ZERO);
    assert_eq!(
        rig.load().amount,
        Amount::new(0.155 + (0.155 + (125.0 * LAST_BIT + last)))
    );
    rig.set_load(Some(AttributeId::named(17)), 9.9);
    assert_eq!(gather(&mut rig, &mut target(100.0)), Ok(Next::Drop));
    assert_eq!(rig.load().amount, Amount::new(9.9));
}

/// The graphic of sprite `id`: one frame of one facet.
pub(crate) const fn one_frame(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(1),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// The sprites of a forager that carries with sprite 4 and stands with sprite 1.
fn carrying_reads() -> GatherReads {
    GatherReads {
        sprites: HuntSprites {
            carrying: Some(one_frame(4)),
            standing: Some(one_frame(1)),
            ..HuntSprites::default()
        },
        ..reads()
    }
}

/// `0x140a8e1d0`'s cases 3, 5 and 7: the farmers (masters 259, 214, 1891 and 1892) wait for
/// another farm (`set_state(0xf3)`) whatever their target type; the fishing ships (masters 13,
/// 2356 and 2171) only on target type 49 (a fish trap's); any other gatherer searches.
#[test]
fn farmers_wait_for_farm_and_fishing_ships_only_at_farm_class() {
    let roles = roles();
    for farmer in [259, 214, 1891, 1892] {
        assert!(waits_for_farm(master_id(farmer), None, &roles));
        assert!(waits_for_farm(
            master_id(farmer),
            Some(class::CLASS_7),
            &roles
        ));
    }
    for fishing_ship in [13, 2356, 2171] {
        assert!(waits_for_farm(
            master_id(fishing_ship),
            Some(class::FARM),
            &roles
        ));
        assert!(!waits_for_farm(
            master_id(fishing_ship),
            Some(class::CLASS_7),
            &roles
        ));
        assert!(!waits_for_farm(master_id(fishing_ship), None, &roles));
    }
    assert!(!waits_for_farm(
        ids::VILLAGER_MALE,
        Some(class::FARM),
        &roles
    ));
}

/// `FUN_140a8b6b0`: the task's carrying sprite while the unit holds at least 1 (truncated) of
/// the task's resource; else the master's standing sprite: below 1, another resource, a task
/// without a resource, a task without a carrying sprite, or an amount that is not a number.
#[test]
fn gatherer_stands_with_carrying_sprite_while_it_holds_one_of_task_resource() {
    let gather_action =
        GatherAction::new(task(), &bush(), FarmMode::Fixed, rate(), &roles()).unwrap();
    let stand = |gather_action: &GatherAction,
                 gather_reads: &GatherReads,
                 kind: Option<AttributeId>,
                 amount: f32| {
        let mut unit = Unit::default();
        (unit.life.attribute_kind, unit.life.amount) = (kind, Amount::new(amount));
        gather_action
            .stand_sprite(&unit, gather_reads)
            .map(|graphic| graphic.id)
    };
    let berries = Some(AttributeId::named(17));
    let (carrying, standing) = (Some(SpriteId::named(4)), Some(SpriteId::named(1)));
    let gather_reads = carrying_reads();
    assert_eq!(stand(&gather_action, &gather_reads, berries, 1.0), carrying);
    assert_eq!(
        stand(&gather_action, &gather_reads, berries, 0.99),
        standing
    );
    assert_eq!(
        stand(&gather_action, &gather_reads, berries, f32::NAN),
        standing
    );
    assert_eq!(
        stand(&gather_action, &gather_reads, Some(attribute::STONE), 5.0),
        standing
    );
    let without_resource = GatherAction {
        task: HuntTask {
            resource: None,
            ..task()
        },
        ..gather_action
    };
    assert_eq!(stand(&without_resource, &gather_reads, None, 5.0), standing);
    let without_carrying = GatherReads {
        sprites: HuntSprites {
            carrying: None,
            ..gather_reads.sprites
        },
        ..reads()
    };
    assert_eq!(
        stand(&gather_action, &without_carrying, berries, 5.0),
        standing
    );
}

/// A gatherer walking to its target with its timer at 2 and its point at (5, 6).
fn walking_with_point() -> GatherAction {
    GatherAction {
        state: GatherState::Walk {
            move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        },
        timer: Seconds::new(2.0),
        point: Position::new(5.0, 6.0, 0.0),
        ..GatherAction::new(task(), &bush(), FarmMode::Fixed, rate(), &roles()).unwrap()
    }
}

/// `set_state(11)` (`0x140a8cd40` case 0xb): the sub-actions go (the `MoveTo`'s teardown stops
/// the unit), the timer clears and a quiet `MoveTo` (`+0x61` 0) walks to the point within the
/// target's radius plus the task's range; state 8's end without a target passes no radius.
#[test]
fn walk_back_walks_to_its_point_within_radius_and_task_range() {
    let walk = |radius: Distance, set: bool| {
        let mut gather_action = walking_with_point();
        let mut mover = walker();
        let walks = walks(Held::default());
        let walked = if set {
            gather_action.set_walk_back(radius, &mut mover, &walks, &tuning().move_to)
        } else {
            gather_action.walk_back(radius, &mut mover, &walks, &tuning().move_to)
        };
        assert_eq!(walked, Ok(()));
        assert_eq!(mover.animated.speed, Speed::ZERO);
        assert_eq!(
            (gather_action.state.bare(), gather_action.timer),
            (GatherState::WalkBack { move_to: None }, Seconds::ZERO)
        );
        gather_action.state.move_to().copied()
    };
    let to_point = |range| {
        Some(MoveToAction::sub_to_point(
            Position::new(5.0, 6.0, 0.0),
            range,
            None,
        ))
    };
    assert_eq!(
        walk(Distance::new(1.5), true),
        to_point(Distance::new(1.5) + task().range)
    );
    assert_eq!(walk(Distance::ZERO, false), to_point(task().range));
}

/// The walk of states 11 and 0xf2 (`0x140a8cd40` case 0xb) takes the state its caller names,
/// with the walk's `MoveTo` in it.
#[test]
fn walk_to_point_goes_into_state_its_caller_names() {
    let mut gather_action = walking_with_point();
    let mut mover = walker();
    assert_eq!(
        gather_action.walk_to_point(
            |move_to| GatherState::ReseedWalk { move_to },
            Distance::new(0.5),
            &mut mover,
            &walks(Held::default()),
            &tuning().move_to,
        ),
        Ok(())
    );
    assert_eq!(
        gather_action.state,
        GatherState::ReseedWalk {
            move_to: Some(MoveToAction::sub_to_point(
                Position::new(5.0, 6.0, 0.0),
                Distance::new(0.5) + task().range,
                None,
            ))
        }
    );
}

/// `set_state(0xf2)` (`0x140a8cd40` case 0xf2): the point takes `mGatherX`, `mGatherY` unless
/// `mGatherX` is −1 (a `UCOMISS`: a part that is not a number takes them), its height kept;
/// then case 0xb's walk there within the radius plus the task's range.
#[test]
fn reseed_walk_goes_to_where_it_gathered_unless_none() {
    let reseed = |gather_at: Point| {
        let mut gather_action = GatherAction {
            gather_at,
            point: Position::new(5.0, 6.0, 2.0),
            ..walking_with_point()
        };
        let mut mover = walker();
        assert_eq!(
            gather_action.begin_reseed_walk(
                Distance::new(1.0),
                &mut mover,
                &walks(Held::default()),
                &tuning().move_to,
            ),
            Ok(())
        );
        assert_eq!(
            (gather_action.state.bare(), gather_action.timer),
            (GatherState::ReseedWalk { move_to: None }, Seconds::ZERO)
        );
        gather_action
    };
    let gathered = reseed(Point::new(112.0, 173.0));
    assert_eq!(gathered.point, Position::new(112.0, 173.0, 2.0));
    assert_eq!(
        gathered.state.move_to().copied(),
        Some(MoveToAction::sub_to_point(
            Position::new(112.0, 173.0, 2.0),
            Distance::new(1.0) + task().range,
            None,
        ))
    );
    assert_eq!(
        reseed(Point::new(-1.0, 173.0)).point,
        Position::new(5.0, 6.0, 2.0)
    );
    let unordered = reseed(Point::new(f32::NAN, 173.0)).point;
    assert!(unordered.x().is_nan());
    assert_eq!((unordered.y(), unordered.z()), (173.0, 2.0));
}

/// `set_state(0xf3)` and `set_state(3)` (`0x140a8cd40` cases 0xf3 and 3): the sub-actions go,
/// the timer waits a second or ten, the unit stands with its carrying sprite (it holds 3 of
/// the task's resource) and the caller posts the unit AI's notify; `set_state(1)` clears the
/// timer and shows the sprite too. Its sub-actions' teardown is its caller's
/// ([`GatherAction::update_moved`] tears the walk down first), so its case leaves the stop out.
#[test]
fn farm_wait_search_and_done_set_timer_and_stand_unit() {
    type Begin = fn(&mut GatherAction, &mut Step<'_>, &mut Unit, &GatherReads) -> Next;
    let begin = |begin: Begin| {
        let mut rig = Rig::on_the_path();
        let mut gather_action = walking_with_point();
        let gather_reads = carrying_reads();
        let next = rig.run(gather_action.state, 100, |step, unit, _, _| {
            Ok(begin(&mut gather_action, step, unit, &gather_reads))
        });
        let shown = rig
            .unit()
            .sprite
            .map(|active_sprite| active_sprite.graphic.id);
        (
            next,
            gather_action.state,
            gather_action.timer,
            shown,
            rig.anim_stopped(),
        )
    };
    let carrying = Some(SpriteId::named(4));
    assert_eq!(
        begin(|gather_action, step, unit, gather_reads| gather_action
            .begin_farm_wait(step, unit, gather_reads)
            .unwrap()),
        (
            Ok(Next::Search),
            GatherState::FarmWait,
            Seconds::new(-1.0),
            carrying,
            true
        )
    );
    assert_eq!(
        begin(|gather_action, step, unit, gather_reads| gather_action
            .begin_search(step, unit, gather_reads)
            .unwrap()),
        (
            Ok(Next::Search),
            GatherState::Search,
            Seconds::new(-10.0),
            carrying,
            true
        )
    );
    let (next, state, timer, shown, _) = begin(|gather_action, step, unit, gather_reads| {
        gather_action.set_done(step, unit, gather_reads);
        Next::Stay
    });
    assert_eq!(
        (next, state, timer, shown),
        (Ok(Next::Stay), GatherState::Done, Seconds::ZERO, carrying)
    );
}

/// State 0xf3's update (`0x140a8e1d0` case 0xf2) on `gather_action` with `target`, the world's
/// dt `dt` ms, called alone.
fn farm_wait(
    gather_action: &mut GatherAction,
    target: Option<&Victim>,
    dt: u32,
) -> GenieResult<Next> {
    let mut rig = Rig::on_the_path();
    rig.set_load(None, 0.0);
    let gather_reads = GatherReads {
        master: master_id(259),
        ..reads()
    };
    rig.run(gather_action.state, dt, |step, unit, _, _| {
        gather_action.update_farm_wait(step, unit, target, &gather_reads, &roles())
    })
}

/// A fish trap (master 199, class 49, radii 1) that `workers` work.
fn fish_trap(workers: i32) -> Victim {
    Victim {
        id: object_id(12557),
        amount: Amount::new(175.0),
        state: ObjectState::Alive,
        class: class::FARM,
        master_id: ids::FISH_TRAP,
        radius: Footprint::new(1.0, 1.0),
        workers: WorkerCount::of_data(workers),
        ..Victim::default()
    }
}

/// State 0xf3's timer counts the world's dt up while below 0; once at 0 the walks take the
/// target's radius on x, none for a farm of stops (masters 50 and 1187, `0x140a8cd40` case
/// 0xb): a farmer that gathered walks back to reseed within it, one that never gathered walks
/// back to search only at a farm (target type 49) that another farmer works too (`+0x644`
/// above 1), and idles otherwise.
#[test]
fn farm_wait_walks_within_target_radius_or_idles() {
    let mut gather_action = waiting(true);
    assert_eq!(farm_wait(&mut gather_action, None, 400), Ok(Next::Stay));
    assert_eq!(
        (gather_action.state.bare(), gather_action.timer),
        (GatherState::FarmWait, Seconds::new(-1.0 + 0.4))
    );
    let range = |gather_action: &GatherAction| {
        gather_action
            .state
            .move_to()
            .map(|move_to_action| move_to_action.range)
    };
    let mut gather_action = waiting(true);
    assert_eq!(
        farm_wait(&mut gather_action, Some(&fish_trap(1)), 1000),
        Ok(Next::Stay)
    );
    assert_eq!(
        (gather_action.state.bare(), range(&gather_action)),
        (
            GatherState::ReseedWalk { move_to: None },
            Some(Distance::new(1.0) + task().range)
        )
    );
    let farm = Victim {
        master_id: ids::FARM,
        radius: Footprint::new(1.5, 1.5),
        ..fish_trap(1)
    };
    let mut gather_action = waiting(true);
    assert_eq!(
        farm_wait(&mut gather_action, Some(&farm), 1000),
        Ok(Next::Stay)
    );
    assert_eq!(range(&gather_action), Some(task().range));
    // never gathered: at a fish trap two work, the walk back within its radius, the target gone
    let mut gather_action = waiting(false);
    gather_action.target = Some(GatherTargetId::new(object_id(12557)));
    assert_eq!(
        farm_wait(&mut gather_action, Some(&fish_trap(2)), 1000),
        Ok(Next::Stay)
    );
    assert_eq!(
        (
            gather_action.state.bare(),
            range(&gather_action),
            gather_action.target
        ),
        (
            GatherState::WalkBack { move_to: None },
            Some(Distance::new(1.0) + task().range),
            None
        )
    );
    // one worker, or a target type other than 49: the farmer idles
    for (target_class, workers) in [(class::FARM, 1), (class::CLASS_7, 2)] {
        let mut gather_action = waiting(false);
        gather_action.target_class = Some(target_class);
        gather_action.target = Some(GatherTargetId::new(object_id(12557)));
        assert_eq!(
            farm_wait(&mut gather_action, Some(&fish_trap(workers)), 1000),
            Ok(Next::Stay)
        );
        assert_eq!(
            (gather_action.state.bare(), gather_action.target),
            (GatherState::Idle, None)
        );
    }
    // a timer at 0 waits on
    let mut gather_action = waiting(true);
    gather_action.timer = Seconds::ZERO;
    assert_eq!(farm_wait(&mut gather_action, None, 1000), Ok(Next::Stay));
    assert_eq!(
        (gather_action.state.bare(), gather_action.timer),
        (GatherState::FarmWait, Seconds::ZERO)
    );
}
