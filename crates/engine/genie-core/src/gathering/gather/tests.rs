use super::*;
use crate::map::DistanceSquared;
use crate::math::Angle;
use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::move_to::{
    MoveDefaults, MoveTo, MoveToAction, MoveToReads, MoveWait, SpriteReads, TargetMoved, WalkTo,
};
use crate::movement::{Me, Mover};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::lifecycle::Held;
use crate::objects::masters::TaskId;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::attribute::{self, AttributeId};
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::unit_ai::UnitStance;
use crate::world::WorldTime;
use crate::world::step::Step;
use crate::world::{Player, World};

/// What a human player's villager's Gather reads (master 83, class 4): work rate 0.31, capacity
/// 10, no productivity attribute, no sprites, outside the game's farm setting.
fn reads() -> GatherReads {
    GatherReads {
        work_rate: WorkRate::new(0.31),
        capacity: Amount::new(10.0),
        productivity: 1.0,
        sprites: HuntSprites::default(),
        class: class::VILLAGER,
        defaults: MoveDefaults::default(),
        master: ids::VILLAGER_MALE,
        stance: UnitStance::AGGRESSIVE,
        farm_stops: false,
        computer: false,
        stop_productivity: 1.0,
        food_sold: 0.0,
    }
}

/// The sprites' table of a world without sprites.
static NO_GRAPHICS: std::collections::BTreeMap<i16, Graphic> = std::collections::BTreeMap::new();

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
    t: World,
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
        self.t.players[0].objects.push(house);
        self
    }

    /// The rig with `n` attributes for the player.
    fn with_attributes(n: usize) -> Self {
        let owner = Player {
            attributes: vec![Amount::ZERO; n],
            ..Player::default()
        };
        let mut rig = Self {
            t: crate::movement::tests::path_world(owner),
        };
        rig.mover_mut().animated.speed = 1.0;
        let unit = rig.unit_mut();
        unit.life.master_id = ids::VILLAGER_MALE;
        (unit.life.attribute_type, unit.life.amount) =
            (Some(AttributeId::numbered(17)), Amount::new(3.0));
        rig
    }

    /// `f` on the villager's step of `dt` ms, in action state `state`.
    fn run<R>(
        &mut self,
        state: GatherState,
        dt: u32,
        f: impl FnOnce(&mut Step<'_>, &mut Unit, Me) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        crate::world::tests::in_update(
            &mut self.t,
            (PlayerId::new(1), ListIndex::new(0)),
            dt,
            |u, c| {
                let me = Me {
                    id: u.id,
                    group: None,
                    action_state: state.get(),
                };
                f(c, u, me)
            },
        )
    }

    /// The villager.
    fn unit(&self) -> &Unit {
        &self.t.players[1].objects[ListIndex::new(0)]
    }

    /// The villager to change.
    fn unit_mut(&mut self) -> &mut Unit {
        &mut self.t.players[1].objects[ListIndex::new(0)]
    }

    /// Its movement state.
    fn mover_mut(&mut self) -> &mut Mover {
        self.unit_mut()
            .body
            .mover_mut()
            .expect("the rig's unit moves")
    }

    /// What it carries: the resource and how much.
    fn load(&self) -> Held {
        self.unit().life.held()
    }

    /// It carries `held` of resource `kind`.
    fn set_load(&mut self, kind: Option<AttributeId>, held: f32) {
        let life = &mut self.unit_mut().life;
        (life.attribute_type, life.amount) = (kind, Amount::new(held));
    }

    /// Its animation stopped (`+0x7b8` 0).
    fn anim_stopped(&self) -> bool {
        self.unit().body.anim_speed() == Some(0.0)
    }

    /// The next draw of the world's Action stream, less [`WAIT_SHIFT`]: a random wait's timer.
    fn next_wait(&self) -> f32 {
        let mut action = self.t.streams.clone();
        action.get(Stream::Action).float01() - WAIT_SHIFT
    }
}

/// A forager's task: food (kind 17, berries), range 0.25, work value 1 (the 4v4's 31139).
fn task() -> HuntTask {
    HuntTask {
        id: TaskId::test(2),
        range: Distance::new(0.25),
        resource: Some(AttributeId::numbered(17)),
        sprites: [-1; 4],
        work_value: 1.0,
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
        workers: 0,
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
        productivity: 1.0,
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
        workers: 0,
        drop_site: false,
        gap: Distance::ZERO,
        state: ObjectState::Alive,
        radius: Footprint::new(1.5, 1.5),
    }
}

#[test]
fn a_farm_s_stop_is_a_tile_centre_of_its_footprint_other_than_the_last() {
    let mut rng = Well512::default();
    rng.set_seed(29910);
    let mut last = Point::ORIGIN;
    for _ in 0..50 {
        let [x, y] = stop_point(
            Position::new(112.5, 173.5, 3.0),
            Footprint::new(1.5, 1.5),
            last,
            &mut rng,
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
    );
    assert_eq!((at, rng.draws), (Point::new(10.5, 20.5), 20));
}

#[test]
fn under_the_farm_setting_a_farm_s_gather_goes_in_four_timed_stops() {
    let rate = StopRate {
        work_rate: WorkRate::new(0.33),
        capacity: Amount::new(10.0),
        productivity: 1.0,
    };
    let g = GatherAction::new(task(), &farm(), true, rate).unwrap();
    assert_eq!(
        (g.stops, g.time_per_stop, g.farm),
        (
            4,
            Seconds::new(((10.0f32 * 1.0) / (0.33 * 1.0)) / 4.0),
            Point::new(112.5, 173.5)
        )
    );
    // without the setting, or on a fish trap, one stop
    let one = GatherAction::new(task(), &farm(), false, rate).unwrap();
    let trap = Resource {
        master: ids::FISH_TRAP,
        ..farm()
    };
    let trap = GatherAction::new(task(), &trap, true, rate).unwrap();
    assert_eq!(
        (one.stops, one.time_per_stop, trap.stops),
        (1, Seconds::ZERO, 1)
    );
}

/// Slot 0x38 (`FUN_140a8e1b0`, `set_state(2)`): the `MoveTo` goes and stops the unit, the
/// timer clears, the unit stands, the target and the drop site go (returned for the
/// caller's release) and the point is −1.
#[test]
fn a_stop_idles_the_gather_and_lets_its_targets_go() {
    let mut mover = crate::gathering::hunt::tests::walker();
    let mut g = GatherAction {
        state: GatherState::Walk,
        target: Some(object_id(31)),
        carry_to: Some(object_id(9)),
        point: Position::new(5.0, 6.0, 0.0),
        timer: Seconds::new(2.0),
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
    };
    let stand = Graphic {
        id: 4,
        frames: 1,
        facets: 1,
        speed_multiplier: 1.0,
        frame_duration: 0.1,
        replay_delay: 0.0,
        flags: 0,
    };
    let (mut sprite, mut rng) = (None, Well512::default());
    let released = g
        .halt(Some(&mut mover), Some(stand), &mut sprite, &mut rng)
        .unwrap();
    assert_eq!(released, [Some(object_id(31)), Some(object_id(9))]);
    assert_eq!(
        (g.state, g.timer, g.target, g.carry_to, g.point, g.move_to),
        (
            GatherState::Idle,
            Seconds::ZERO,
            None,
            None,
            Position::from([-1.0; 3]),
            None
        )
    );
    assert_eq!(sprite.map(|s| s.graphic.id), Some(4));
    assert_eq!(mover.velocity.unwrap().velocity.y(), 0.0);
}

/// `FUN_140aa0df0` takes a target in state 6 or below and holds a later one as none
/// (`FUN_140a7a230`): the `MoveTo` has no target and walks to its place as to a point, far.
#[test]
fn a_gather_s_move_to_refuses_a_target_past_state_6() {
    let mover = crate::gathering::hunt::tests::walker();
    let g = GatherAction::new(task(), &farm(), true, rate()).unwrap();
    let to = |state| WalkTo {
        id: Some(object_id(31)),
        at: Position::new(5.0, 6.0, 0.0),
        gap: Distance::ZERO,
        state: Some(state),
    };
    let walk = |state| {
        g.move_to_object(to(state), &mover, &walks(Held::default()))
            .unwrap()
            .map(|m| (ObjectId::field(m.target), m.point, m.state))
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
fn past_its_time_per_stop_a_farmer_walks_to_the_next_stop() {
    let mut mover = crate::gathering::hunt::tests::walker();
    let mut g = GatherAction {
        state: GatherState::Work,
        time_per_stop: Seconds::new(1.0),
        time_at_stop: Seconds::new(0.95),
        ..GatherAction::new(task(), &farm(), true, rate()).unwrap()
    };
    let mut rng = Well512::default();
    let at = stop_point(
        Position::new(112.5, 173.5, 3.0),
        Footprint::new(1.5, 1.5),
        Point::ORIGIN,
        &mut rng.clone(),
    );
    let walks = walks(Held::default());
    let to = WalkTo {
        id: g.target,
        at: Position::new(112.5, 173.5, 3.0),
        gap: Distance::ZERO,
        state: Some(ObjectState::Alive),
    };
    g.set_walk(to, Footprint::new(1.5, 1.5), &mut mover, &mut rng, &walks)
        .unwrap();
    // a farm's stop: a quiet MoveTo to a point of the farm within the task's range, which asks
    // again when it moves (`+0x61` 1)
    let to_stop = MoveToAction {
        on_target_moved: TargetMoved::AsksAgain,
        ..MoveToAction::sub_to_point(at.with_height(3.0), task().range, None)
    };
    assert_eq!(
        (g.state, g.last_stop, g.move_to),
        (GatherState::Walk, at, Some(to_stop))
    );
}

#[test]
fn a_farm_s_food_sold_at_once_goes_to_the_player_and_the_gathering_goes_on() {
    let g = GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
    };
    let mut rig = Rig::with_attributes(250);
    rig.set_load(Some(attribute::FARM_FOOD), 0.0);
    let mut field = Victim {
        class: class::FARM,
        master_id: ids::FARM,
        ..target(15.0)
    };
    let r = GatherReads {
        work_rate: WorkRate::new(0.5),
        food_sold: 1.0,
        ..reads()
    };
    let gathered = rig.run(GatherState::Work, 500, |c, u, _| {
        g.gather(c, u, &mut field, &r)
    });
    assert_eq!(gathered, Ok(Next::Stay));
    // the player gains it as food at once (resource 0 and its gathered total, attribute 166)
    let owner = &rig.t.players[1];
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
fn a_gatherer_moved_walks_to_the_point_or_carries_its_load_to_the_drop_site() {
    let mut mover = crate::gathering::hunt::tests::walker();
    let gatherer = || GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    // a move to a point (or an object that is no drop site): state 15 to the point
    let mut g = gatherer();
    let loaded = walks(carrying(Some(AttributeId::numbered(17)), 5.0));
    g.take_move(
        None,
        Position::new(30.0, 40.0, 0.0),
        PlayerId::new(7),
        &mut mover,
        &loaded,
    )
    .unwrap();
    // a quiet MoveTo to the point within the task's range; its `+0x61` stays 0
    let to_point = MoveToAction::sub_to_point(Position::new(30.0, 40.0, 0.0), task().range, None);
    assert_eq!(
        (g.state, g.target, g.point, g.move_to),
        (
            GatherState::Moved,
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
    let mut g = gatherer();
    g.take_move(
        Some(mill),
        Position::from([1.0; 3]),
        PlayerId::new(7),
        &mut mover,
        &loaded,
    )
    .unwrap();
    assert_eq!(
        (g.state, g.target, g.carry_to, g.point),
        (
            GatherState::Drop,
            None,
            Some(object_id(500)),
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
        quiet: true,
        on_target_moved: TargetMoved::AsksAgain,
        wait: MoveWait::NEW,
    };
    assert_eq!(g.move_to, Some(to_mill));
    // already on its way there, nothing changes; empty-handed it walks to the point
    let walking = g.move_to;
    g.take_move(
        Some(mill),
        Position::from([1.0; 3]),
        PlayerId::new(7),
        &mut mover,
        &loaded,
    )
    .unwrap();
    assert_eq!((g.state, g.move_to), (GatherState::Drop, walking));
    let mut g = gatherer();
    g.take_move(
        Some(mill),
        Position::from([1.0; 3]),
        PlayerId::new(7),
        &mut mover,
        &walks(carrying(Some(AttributeId::numbered(17)), 0.0)),
    )
    .unwrap();
    assert_eq!((g.state, g.carry_to), (GatherState::Moved, None));
}

#[test]
fn at_its_point_a_human_s_moved_gatherer_idles_and_a_computer_s_is_done() {
    let run = |computer: bool| {
        let mut g = GatherAction {
            state: GatherState::Moved,
            target: None,
            move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
            ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
        };
        let mut rig = Rig::on_the_path();
        let r = GatherReads {
            computer,
            farm_stops: true,
            ..reads()
        };
        let moved = rig.run(GatherState::Moved, 100, |c, u, me| {
            g.update_moved(c, u, me, &r)
        });
        assert_eq!(moved, Ok(Next::Stay));
        (g.state, g.point, g.move_to)
    };
    assert_eq!(
        run(false),
        (GatherState::Idle, Position::from([-1.0; 3]), None)
    );
    assert_eq!(run(true).0, GatherState::Done);
}

#[test]
fn a_gatherer_without_a_target_or_a_load_asks_for_another_every_ten_seconds() {
    // the 4v4's forager 31237 at update 10820: its bush went while it walked there
    let run = |master: MasterId, g: &mut GatherAction, dt: u32| {
        let mut rig = Rig::on_the_path();
        rig.set_load(None, 0.0);
        let r = GatherReads { master, ..reads() };
        rig.run(g.state, dt, |c, u, me| g.update(c, u, me, None, &r))
    };
    let mut g = GatherAction {
        state: GatherState::Walk,
        target: None,
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        ..GatherAction::new(task(), &bush(), false, rate()).unwrap()
    };
    assert_eq!(run(master_id(120), &mut g, 20), Ok(Next::Search));
    assert_eq!(
        (g.state, g.timer, g.move_to),
        (GatherState::Search, Seconds::new(-10.0), None)
    );
    // state 3 counts the world's dt up to 0, then waits 10 seconds again and asks again
    g.timer = Seconds::new(-0.03);
    assert_eq!(run(master_id(120), &mut g, 20), Ok(Next::Stay));
    assert_eq!(run(master_id(120), &mut g, 20), Ok(Next::Search));
    assert_eq!(g.timer, Seconds::new(-10.0));
    // a farmer waits a second for another farm instead (set_state 0xf3), its unit AI asked
    let mut g = GatherAction {
        state: GatherState::Work,
        target: None,
        ..GatherAction::new(task(), &bush(), false, rate()).unwrap()
    };
    assert_eq!(run(master_id(259), &mut g, 20), Ok(Next::Search));
    assert_eq!(
        (g.state, g.timer),
        (GatherState::FarmWait, Seconds::new(-1.0))
    );
}

/// A farmer's (master 259) update in its state with `target`, the world's dt `dt` ms, a
/// human's unless `computer`, empty-handed: what it answers, with the gather after it.
fn farm_update(
    g: &mut GatherAction,
    target: Option<&mut Victim>,
    dt: u32,
    computer: bool,
) -> Result<Next, GenieError> {
    let mut rig = Rig::on_the_path();
    rig.set_load(None, 0.0);
    let r = GatherReads {
        master: master_id(259),
        computer,
        ..reads()
    };
    rig.run(g.state, dt, |c, u, me| g.update(c, u, me, target, &r))
}

/// Case 5 (`FUN_141162db0`): a gatherer with a turn time turns a facet a turn time; not yet
/// facing its target, the switch breaks and it stays in state 6 for the next update.
#[test]
fn a_gatherer_turning_over_time_faces_its_target_later() {
    let mut g = GatherAction {
        state: GatherState::Face,
        target: Some(object_id(28222)),
        ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
    };
    let mut mover = crate::gathering::hunt::tests::walker();
    mover.stats.turn_time = Seconds::new(1.0);
    mover.stats.facets = Some(crate::objects::facing::SpriteFacets {
        count: 16,
        flag_1: true,
    });
    mover.angle = Angle::ZERO;
    let before = mover.angle;
    let mut rig = Rig::on_the_path();
    rig.t.world_time = WorldTime::from_millis(5000);
    rig.set_load(None, 0.0);
    rig.unit_mut().body =
        crate::objects::layers::Body::new(Position::new(30.0, 40.0, 0.0), None, Some(mover));
    let r = GatherReads {
        master: master_id(259),
        ..reads()
    };
    // the bush straight behind the unit (the angle 0 faces +x)
    let mut bush = Victim {
        position: Position::new(20.0, 40.0, 0.0),
        ..target(10.0)
    };
    let faced = rig.run(GatherState::Face, 100, |c, u, me| {
        g.update(c, u, me, Some(&mut bush), &r)
    });
    assert_eq!(faced, Ok(Next::Stay));
    assert_eq!(g.state, GatherState::Face);
    assert_ne!(rig.unit().body.mover().unwrap().angle, before);
}

/// State 8's drop-off (`0x140a8e1d0` case 7) by its `MoveTo`: the timer's wait (below 0 it
/// counts up, at 0 the drop-off starts again); a waiting `MoveTo` starts again past its
/// delay; a failed one waits a random while, the animation stopped; an arrived one stops the
/// unit, then a site gone starts again, a site that does not take the load idles, one out of
/// reach waits, and one in reach takes the load, as its player's resource kind.
#[test]
fn a_gatherer_s_drop_off_waits_walks_and_drops_its_load_at_the_site() {
    let dropping = |move_to: Option<MoveTo>, timer| GatherAction {
        state: GatherState::Drop,
        carry_to: Some(object_id(10028)),
        move_to: move_to.map(MoveToAction::sub_in),
        timer,
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let site = |takes, gap_squared| Some(crate::gathering::hunt::DropOff { takes, gap_squared });
    let drop = |g: &mut GatherAction, rig: &mut Rig, site| {
        rig.run(GatherState::Drop, 100, |c, u, me| {
            g.update_drop(c, u, me, &reads(), site)
        })
    };
    let mut rig = Rig::on_the_path();
    let mut g = dropping(Some(MoveTo::Arrived), Seconds::new(-0.15));
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Again));
    assert_eq!(g.timer, Seconds::ZERO);
    let mut g = dropping(Some(MoveTo::Waiting), Seconds::ZERO);
    g.move_to = g.move_to.map(|m| MoveToAction {
        wait: MoveWait {
            delay: Seconds::new(0.15),
            ..m.wait
        },
        ..m
    });
    let waiting = |g: &GatherAction| g.move_to.map(|m| (m.state, m.wait.timer));
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(waiting(&g), Some((MoveTo::Waiting, Seconds::new(0.1))));
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(waiting(&g), Some((MoveTo::Start, Seconds::ZERO)));
    let mut g = dropping(Some(MoveTo::Failed), Seconds::ZERO);
    let draw = rig.next_wait();
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Going));
    assert_eq!(g.timer, Seconds::new(draw));
    assert!(rig.anim_stopped());
    let mut g = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Again));
    assert_eq!(g.move_to, None);
    let mut g = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(
        drop(&mut g, &mut rig, site(false, DistanceSquared::ZERO)),
        Ok(Dropped::Idle)
    );
    assert_eq!(g.state, GatherState::Idle);
    let mut g = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    let far = DROP_REACH + 1.0;
    assert_eq!(
        drop(&mut g, &mut rig, site(true, DistanceSquared::new(far))),
        Ok(Dropped::Going)
    );
    assert!(g.timer < Seconds::ZERO);
    let mut g = dropping(Some(MoveTo::Arrived), Seconds::ZERO);
    assert_eq!(
        drop(&mut g, &mut rig, site(true, DistanceSquared::ZERO)),
        Ok(Dropped::Load(
            crate::gathering::hunt::resource_kind(Some(AttributeId::numbered(17)), 4),
            Amount::new(3.0)
        ))
    );
    assert_eq!(rig.load(), Held::default());
    // an empty list answers 1 ("There is no list."): the drop site is looked for again
    let mut g = dropping(None, Seconds::ZERO);
    assert_eq!(drop(&mut g, &mut rig, None), Ok(Dropped::Again));
}

/// The sub-action list's codes in state 8 (`0x140a8e1d0` case 7): the `MoveTo` to the drop site
/// let its target go (code 5: its check of the target found it past state 6) waits a random
/// while as a failed one does; the `MoveTo` then walks on to the point it held.
#[test]
fn a_drop_off_whose_site_was_let_go_waits_a_random_while() {
    let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
    let mut g = GatherAction {
        state: GatherState::Drop,
        carry_to: Some(object_id(10028)),
        move_to: Some(to_house(MoveTo::Moving)),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let draw = rig.next_wait();
    let dropped = rig.run(GatherState::Drop, 100, |c, u, me| {
        g.update_drop(c, u, me, &reads(), None)
    });
    assert_eq!(dropped, Ok(Dropped::Going));
    assert_eq!(g.timer, Seconds::new(draw));
    assert_eq!(g.move_to.map(|m| ObjectId::field(m.target)), Some(-1));
    assert!(rig.anim_stopped());
}

/// State 8's `MoveTo` to the drop site (`0x140a8e1d0` case 7, `FUN_140a992b0`): it starts
/// toward the site, asking for a path; moving, it walks on while the site stays on its point
/// and starts again once the site left it.
#[test]
fn a_gatherer_walks_to_its_drop_site_and_starts_again_when_it_moved() {
    let dropping = |move_to| GatherAction {
        state: GatherState::Drop,
        carry_to: Some(object_id(10028)),
        move_to: Some(to_house(move_to)),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let drop = |g: &mut GatherAction, rig: &mut Rig| {
        rig.run(GatherState::Drop, 100, |c, u, me| {
            g.update_drop(c, u, me, &reads(), None)
        })
    };
    let state = |g: &GatherAction| g.move_to.map(|m| m.state);
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    rig.mover_mut().paths.clear();
    let mut g = dropping(MoveTo::Start);
    assert_eq!(drop(&mut g, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&g), Some(MoveTo::Moving));
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    let mut g = dropping(MoveTo::Moving);
    assert_eq!(drop(&mut g, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&g), Some(MoveTo::Moving));
    // the site left the point the MoveTo holds: it starts again
    g.move_to = g.move_to.map(|m| MoveToAction {
        point: Position::new(35.0, 88.0, 0.0),
        ..m
    });
    assert_eq!(drop(&mut g, &mut rig), Ok(Dropped::Going));
    assert_eq!(state(&g), Some(MoveTo::Start));
}

/// State 4 (case 3) by its `MoveTo` ([`GatherAction::update_walk`]): it starts toward the
/// target, asking for a path; moving, it walks on while the target stays on its point and
/// starts again once it left; a waiting one starts again past its delay; an arrived one stops
/// the unit and faces the target (`set_state(6)`); a failed one near a target of more than 3
/// workers searches (`set_state(3)`), else waits a random while; none is not modelled.
#[test]
fn a_gatherer_walks_to_its_target_and_faces_it_on_arrival() {
    let walking = |move_to: Option<MoveTo>| GatherAction {
        state: GatherState::Walk,
        target: Some(object_id(10028)),
        move_to: move_to.map(to_house),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let state = |g: &GatherAction| g.move_to.map(|m| m.state);
    let house = |workers| Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        workers,
        ..target(10.0)
    };
    let walk = |g: &mut GatherAction, rig: &mut Rig, workers| {
        let mut house = house(workers);
        rig.run(GatherState::Walk, 100, |c, u, me| {
            g.update(c, u, me, Some(&mut house), &reads())
        })
    };
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    rig.mover_mut().paths.clear();
    let mut g = walking(Some(MoveTo::Start));
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!(
        (g.state, state(&g)),
        (GatherState::Walk, Some(MoveTo::Moving))
    );
    assert_eq!(rig.mover_mut().paths.len(), 2);
    let mut rig = Rig::on_the_path().with_house(ObjectState::Alive);
    let mut g = walking(Some(MoveTo::Moving));
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!(state(&g), Some(MoveTo::Moving));
    // the target left the point the MoveTo holds: it starts again
    g.move_to = g.move_to.map(|m| MoveToAction {
        point: Position::new(35.0, 88.0, 0.0),
        ..m
    });
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!(state(&g), Some(MoveTo::Start));
    let mut g = walking(Some(MoveTo::Waiting));
    g.move_to = g.move_to.map(|m| MoveToAction {
        wait: MoveWait {
            delay: Seconds::new(0.05),
            ..m.wait
        },
        ..m
    });
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!(state(&g), Some(MoveTo::Start));
    let mut g = walking(Some(MoveTo::Arrived));
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!(
        (g.state, g.move_to, g.timer),
        (GatherState::Face, None, Seconds::ZERO)
    );
    let mut g = walking(Some(MoveTo::Failed));
    assert_eq!(walk(&mut g, &mut rig, 4), Ok(Next::Search));
    assert_eq!((g.state, g.timer), (GatherState::Search, SEARCH_WAIT));
    let mut rig = Rig::on_the_path();
    let mut g = walking(Some(MoveTo::Failed));
    let draw = rig.next_wait();
    assert_eq!(walk(&mut g, &mut rig, 3), Ok(Next::Stay));
    assert_eq!(g.timer, Seconds::new(draw));
    assert!(rig.anim_stopped());
    // an empty list answers 1 ("There is no list."): the unit faces its target
    let mut g = walking(None);
    assert_eq!(walk(&mut g, &mut rig, 0), Ok(Next::Stay));
    assert_eq!((g.state, g.move_to), (GatherState::Face, None));
}

/// State 4's code 5 (`0x140a8e1d0` case 3: the `MoveTo`'s check found its target past state 6):
/// the Gather's target goes (slot 0x68 with none) and the gatherer walks back to its point
/// within the task's range (`set_state(11)`, no target's radius), a `MoveTo` to the point whose
/// `+0x61` stays 0.
#[test]
fn a_gatherer_whose_move_to_lost_its_target_walks_back_to_its_point() {
    let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
    let mut g = GatherAction {
        state: GatherState::Walk,
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        move_to: Some(to_house(MoveTo::Moving)),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let mut house = Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        ..target(10.0)
    };
    let next = rig.run(GatherState::Walk, 100, |c, u, me| {
        g.update(c, u, me, Some(&mut house), &reads())
    });
    assert_eq!(next, Ok(Next::Stay));
    assert_eq!((g.state, g.target), (GatherState::WalkBack, None));
    let to_point = MoveToAction::sub_to_point(Position::new(30.0, 88.0, 0.0), task().range, None);
    assert_eq!(g.move_to, Some(to_point));
}

/// The code 5 of the walks to a point: state 11 searches (`set_state(3)`), state 15 idles a
/// human's gatherer and ends a computer's, state 0xf2 ends as an arrived walk does; code 3 is
/// failed in states 11 and 15, and a quiet `MoveTo` never gives the codes above 5.
#[test]
fn a_walk_to_a_point_ends_on_its_target_let_go_as_on_its_arrival() {
    let at = |state, target| GatherAction {
        state,
        target: None,
        move_to: Some(MoveToAction {
            target: ObjectId::of_field(target),
            ..MoveToAction::sub_in(MoveTo::Moving)
        }),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let run = |g: &mut GatherAction, computer: bool| {
        let mut rig = Rig::on_the_path().with_house(ObjectState::Removed);
        rig.set_load(None, 0.0);
        let r = GatherReads {
            computer,
            ..reads()
        };
        rig.run(g.state, 100, |c, u, me| match g.state {
            GatherState::WalkBack => g.update_walk_back(c, u, me, &r),
            GatherState::Moved => g.update_moved(c, u, me, &r),
            _ => g.update_reseed_walk(c, u, me, None, &r),
        })
    };
    let mut g = at(GatherState::WalkBack, 10028);
    assert_eq!(run(&mut g, false), Ok(Next::Search));
    assert_eq!((g.state, g.move_to), (GatherState::Search, None));
    let mut g = at(GatherState::Moved, 10028);
    assert_eq!(run(&mut g, false), Ok(Next::Stay));
    assert_eq!((g.state, g.move_to), (GatherState::Idle, None));
    let mut g = at(GatherState::Moved, 10028);
    assert_eq!(run(&mut g, true), Ok(Next::Stay));
    assert_eq!(g.state, GatherState::Done);
    let mut g = at(GatherState::ReseedWalk, 10028);
    assert_eq!(run(&mut g, false), Ok(Next::Reseed { arrived: true }));
    assert_eq!(g.move_to, None);
    // a failed walk: state 15 idles a computer's too
    for (state, computer, then) in [
        (GatherState::WalkBack, false, GatherState::Search),
        (GatherState::Moved, true, GatherState::Idle),
    ] {
        let mut g = at(state, -1);
        g.move_to = Some(MoveToAction::sub_in(MoveTo::Failed));
        let _ = run(&mut g, computer);
        assert_eq!(g.state, then);
    }
}

/// Case 5 facing its target at once (no turn time): with something left in the target it
/// works (`set_state(7)`): the gathering starts at the unit's place, a load of another kind
/// gives way to the task's, and the unit shows its working sprite.
#[test]
fn a_gatherer_facing_its_target_works_it() {
    let working = Graphic {
        id: 7,
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: 0.1,
        replay_delay: 0.0,
        flags: 0,
    };
    let mut rig = Rig::on_the_path();
    rig.mover_mut().stats.turn_time = Seconds::ZERO;
    rig.set_load(Some(attribute::STONE), 4.0);
    let mut g = GatherAction {
        state: GatherState::Face,
        target: Some(object_id(10028)),
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let mut house = Victim {
        id: object_id(10028),
        position: Position::new(30.0, 88.0, 0.0),
        ..target(10.0)
    };
    let mut r = reads();
    r.sprites.working = Some(working);
    let next = rig.run(GatherState::Face, 100, |c, u, me| {
        g.update(c, u, me, Some(&mut house), &r)
    });
    assert_eq!(next, Ok(Next::Stay));
    assert_eq!(
        (g.state, g.worked, g.gather_at, g.time_at_stop),
        (
            GatherState::Work,
            true,
            Point::new(28.0, 89.0),
            Seconds::ZERO
        )
    );
    assert_eq!(rig.load(), carrying(Some(AttributeId::numbered(17)), 0.0));
    assert_eq!(rig.unit().sprite.map(|s| s.graphic.id), Some(7));
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
        ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
    }
}

/// State 0xf3 counts the world's dt up to 0; then a farmer that never gathered idles, one
/// that gathered walks back to where it gathered (state 0xf2).
#[test]
fn a_farmer_without_its_farm_waits_a_second_then_walks_back_to_reseed() {
    let mut g = waiting(true);
    assert_eq!(farm_update(&mut g, None, 600, false), Ok(Next::Stay));
    assert_eq!(
        (g.state, g.timer),
        (GatherState::FarmWait, Seconds::new(-1.0 + 0.6))
    );
    assert_eq!(farm_update(&mut g, None, 600, false), Ok(Next::Stay));
    assert_eq!(g.state, GatherState::ReseedWalk);
    assert_eq!(g.point.ground(), Point::new(112.0, 173.0));
    // a quiet MoveTo to the point within the task's range (`+0x61` 0), its start `0x140aa2100`
    let to_point = MoveToAction::sub_to_point(Position::new(112.0, 173.0, 3.0), task().range, None);
    assert_eq!(g.move_to, Some(to_point));
    // without a gather point the walk goes to the point it holds
    let mut g = waiting(true);
    g.gather_at = Point::new(-1.0, -1.0);
    g.point = Position::new(5.0, 6.0, 0.0);
    assert_eq!(farm_update(&mut g, None, 1000, false), Ok(Next::Stay));
    assert_eq!(
        (g.state, g.point),
        (GatherState::ReseedWalk, Position::new(5.0, 6.0, 0.0))
    );
    // a farmer that never gathered idles
    let mut g = waiting(false);
    assert_eq!(farm_update(&mut g, None, 1000, false), Ok(Next::Stay));
    assert_eq!(
        (g.state, g.target, g.point),
        (GatherState::Idle, None, Position::from([-1.0; 3]))
    );
    // a timer at 0 or more waits on
    let mut g = waiting(true);
    g.timer = Seconds::ZERO;
    assert_eq!(farm_update(&mut g, None, 1000, false), Ok(Next::Stay));
    assert_eq!(g.state, GatherState::FarmWait);
}

/// A farmer that never gathered, at a farm another farmer works too, walks back to its
/// point to search (state 11, the farm's radius none) and lets the farm go.
#[test]
fn a_farmer_at_a_farm_another_works_walks_back_to_search() {
    let mut g = waiting(false);
    g.target = Some(object_id(12557));
    let mut farm = Victim {
        id: object_id(12557),
        amount: Amount::new(175.0),
        state: ObjectState::Alive,
        class: class::FARM,
        master_id: ids::FARM,
        radius: Footprint::new(1.5, 1.5),
        workers: 2,
        ..Victim::default()
    };
    assert_eq!(
        farm_update(&mut g, Some(&mut farm), 1000, false),
        Ok(Next::Stay)
    );
    assert_eq!((g.state, g.target), (GatherState::WalkBack, None));
    assert_eq!(g.move_to.map(|m| m.range), Some(task().range));
}

/// State 0xf2's walk: once it arrived without a target, or failed, the caller reseeds; a
/// target standing (state 2) is faced again.
#[test]
fn at_the_end_of_its_walk_back_a_farmer_reseeds() {
    let walking = |move_to| GatherAction {
        state: GatherState::ReseedWalk,
        target: None,
        move_to: Some(MoveToAction::sub_in(move_to)),
        ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
    };
    let mut g = walking(MoveTo::Arrived);
    assert_eq!(
        farm_update(&mut g, None, 100, false),
        Ok(Next::Reseed { arrived: true })
    );
    assert_eq!(g.move_to, None);
    let mut g = walking(MoveTo::Failed);
    assert_eq!(
        farm_update(&mut g, None, 100, false),
        Ok(Next::Reseed { arrived: false })
    );
    // the farm still standing: the farmer faces it (set_state 6)
    let mut g = walking(MoveTo::Arrived);
    g.target = Some(object_id(28222));
    let mut bush = target(10.0);
    assert_eq!(
        farm_update(&mut g, Some(&mut bush), 100, false),
        Ok(Next::Stay)
    );
    assert_eq!(g.state, GatherState::Face);
    // a dying one (state 3) does not hold it: the reseed
    let mut g = walking(MoveTo::Arrived);
    g.target = Some(object_id(28222));
    let mut dying = Victim {
        state: ObjectState::Dying,
        ..target(10.0)
    };
    assert_eq!(
        farm_update(&mut g, Some(&mut dying), 100, false),
        Ok(Next::Reseed { arrived: true })
    );
}

/// Without a reseed a computer's farmer whose walk arrived is done; any other idles and
/// lets its targets go.
#[test]
fn without_a_reseed_the_farmer_idles_or_a_computer_s_is_done() {
    let after = |arrived: bool, computer: bool| {
        let mut g = GatherAction {
            state: GatherState::ReseedWalk,
            target: Some(object_id(5)),
            carry_to: Some(object_id(6)),
            move_to: Some(MoveToAction::sub_in(MoveTo::Failed)),
            ..GatherAction::new(task(), &farm(), false, rate()).unwrap()
        };
        let mut mover = crate::gathering::hunt::tests::walker();
        let (mut sprite, mut rng) = (None, Well512::default());
        let released = g
            .after_reseed(
                arrived,
                computer,
                Some(&mut mover),
                None,
                &mut sprite,
                &mut rng,
            )
            .unwrap();
        (g.state, released, g.target, g.move_to)
    };
    assert_eq!(
        after(true, true),
        (GatherState::Done, [None, None], Some(object_id(5)), None)
    );
    assert_eq!(
        after(true, false),
        (
            GatherState::Idle,
            [Some(object_id(5)), Some(object_id(6))],
            None,
            None
        )
    );
    assert_eq!(
        after(false, true),
        (
            GatherState::Idle,
            [Some(object_id(5)), Some(object_id(6))],
            None,
            None
        )
    );
}

#[test]
fn a_forager_walks_to_its_bush() {
    let mut mover = crate::gathering::hunt::tests::walker();
    let mut g = GatherAction::new(task(), &bush(), true, rate()).unwrap();
    assert_eq!(
        (g.stops, g.gather_at, g.target_class),
        (1, Point::from([-1.0; 2]), class_of(7))
    );
    let mut held = carrying(Some(attribute::STONE), 0.0);
    let carried = walks(held);
    g.start(
        GatherStart {
            target: &bush(),
            player: PlayerId::new(7),
            computer: false,
            carry_rule: false,
            held: &mut held,
            walks: carried,
        },
        &mut mover,
        &mut Well512::default(),
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
        (g.state, g.move_to, held),
        (
            GatherState::Walk,
            Some(to_bush),
            carrying(Some(attribute::STONE), 0.0)
        )
    );
    // a load of another food becomes the task's kind
    let mut g = GatherAction::new(task(), &bush(), true, rate()).unwrap();
    let mut held = carrying(Some(attribute::MEAT), 3.0);
    let carried = walks(held);
    g.start(
        GatherStart {
            target: &bush(),
            player: PlayerId::new(7),
            computer: false,
            carry_rule: false,
            held: &mut held,
            walks: carried,
        },
        &mut mover,
        &mut Well512::default(),
    )
    .unwrap();
    assert_eq!(held, carrying(Some(AttributeId::numbered(17)), 3.0));
}

/// Slot 0x48 on the work command's target: the 4v4's forager 31156 walking to bush 28222 (state
/// 4) is sent to it again in update 5026 and walks to it anew (`set_state(4)`); facing or
/// gathering it, it keeps on; a computer player's walking one too. A loaded one sent to its
/// player's drop site takes its load there (`set_state(8)`, the site its second target).
#[test]
fn a_gatherer_sent_again_to_its_bush_walks_to_it_anew() {
    let mut mover = crate::gathering::hunt::tests::walker();
    let walking = |state| GatherAction {
        state,
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
        worked: true,
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let mut g = walking(GatherState::Walk);
    assert_eq!(
        g.retarget(
            GatherRetarget {
                target: &bush(),
                player: PlayerId::new(7),
                computer: false,
                walks: walks(carrying(Some(AttributeId::numbered(17)), 0.0)),
                farm_stops: true,
                rate: rate()
            },
            &mut mover,
            &mut Well512::default()
        ),
        Ok(true)
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
            g.state,
            g.move_to,
            g.target,
            g.carry_to,
            g.point,
            g.target_class
        ),
        (
            GatherState::Walk,
            Some(to_bush),
            Some(object_id(28222)),
            None,
            Position::new(23.5, 93.5, 0.0),
            class_of(7)
        )
    );
    // the gathering is not started any more (+0x80 cleared first)
    assert!(!g.worked);
    for (state, computer) in [
        (GatherState::Face, false),
        (GatherState::Work, false),
        (GatherState::Walk, true),
    ] {
        let mut g = walking(state);
        assert_eq!(
            g.retarget(
                GatherRetarget {
                    target: &bush(),
                    player: PlayerId::new(7),
                    computer,
                    walks: walks(carrying(Some(AttributeId::numbered(17)), 0.0)),
                    farm_stops: true,
                    rate: rate()
                },
                &mut mover,
                &mut Well512::default()
            ),
            Ok(true)
        );
        assert_eq!(
            (g.state, g.move_to),
            (state, Some(MoveToAction::sub_in(MoveTo::Moving)))
        );
    }
    let mill = Resource {
        id: object_id(500),
        player: PlayerId::new(7),
        drop_site: true,
        ..bush()
    };
    let mut g = walking(GatherState::Walk);
    assert_eq!(
        g.retarget(
            GatherRetarget {
                target: &mill,
                player: PlayerId::new(7),
                computer: false,
                walks: walks(carrying(Some(AttributeId::numbered(17)), 3.0)),
                farm_stops: true,
                rate: rate()
            },
            &mut mover,
            &mut Well512::default()
        ),
        Ok(true)
    );
    assert_eq!(
        (g.state, g.target, g.carry_to, g.point),
        (
            GatherState::Drop,
            None,
            Some(object_id(500)),
            Position::from([-1.0; 3])
        )
    );
}

/// The forager's gathering: `(work rate) × work_val1 × dt` a tick into its load, the bush
/// losing as much; the last bit below 0.1 goes with a thousandth of the master's storage; a
/// load that would pass the capacity asks for the drop-off.
#[test]
fn a_forager_gathers_until_its_load_is_full() {
    let g = GatherAction {
        state: GatherState::Work,
        ..GatherAction::new(task(), &bush(), true, rate()).unwrap()
    };
    let mut rig = Rig::on_the_path();
    rig.set_load(Some(AttributeId::numbered(17)), 0.0);
    let r = GatherReads {
        farm_stops: true,
        ..reads()
    };
    let gather = |rig: &mut Rig, bush: &mut Victim| {
        rig.run(GatherState::Work, 500, |c, u, _| g.gather(c, u, bush, &r))
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
    rig.set_load(Some(AttributeId::numbered(17)), 9.9);
    assert_eq!(gather(&mut rig, &mut target(100.0)), Ok(Next::Drop));
    assert_eq!(rig.load().amount, Amount::new(9.9));
}
