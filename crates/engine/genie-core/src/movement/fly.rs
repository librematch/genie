//! A bird's Fly action (type 10, vtable `0x142ed4c88`; update `0x140a84b40`, `set_state`
//! `0x140a84a50`): the bird rests (state 2), picks a point to fly to from the Action stream
//! (state 3) and flies there with a `MoveTo` sub-action (state 4) until its flight time runs out or
//! the `MoveTo` ends; then it rests for its task's wait time, or picks the next flight at once.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::{cos, sin};
use crate::math::rng::{Stream, Well512};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::movement::move_to::{MoveToAction, graphic_of};
use crate::movement::{Me, Mover};
use crate::objects::SpriteId;
use crate::objects::masters::ActionType;
use crate::objects::masters::FlyMode;
use crate::objects::masters::Task;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::step::Step;
use genie_diff::Diff;

/// The Fly action's type, which word 9 adds.
pub const FLY: ActionType = ActionType::named(10);

pub use crate::movement::fly_state::FlyState;

/// Below this (`DAT_142eb0c60`, 1e-7) a task value counts as none.
const EPSILON: f32 = crate::math::image::fly::EPSILON;
/// Degrees to radians (`DAT_142eb568c`).
const DEGREE: f32 = crate::math::image::fly::DEGREE;
/// The flight angle's step between tries, in degrees, and the tries (`FUN_140a84b40`, state 3).
const ANGLE_STEP: i32 = 80;
const TRIES: i32 = 12;

/// A bird without movement state, whose flight reads its speed and place.
const NO_MOVER: GenieError = GenieError::unsupported("a bird without movement state");

/// A bird's Fly action (type 10).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Fly {
    /// `+0x18`.
    pub state: FlyState,
    /// `+0x44`, in seconds: the flight's time left, or the rest's.
    pub timer: Seconds,
    /// Its `MoveTo` sub-action to the flight's point (not quiet).
    pub move_to: Option<MoveToAction>,
    /// `+0x38`, `+0x3c`: the point the bird keeps within its task's leash.
    pub home: Point,
    /// The action's task (`+0x48`): its leash (`+0x19`, tiles), rest (`+0x1c`), distance factor
    /// (`+0x28`), avoided class and range (`+0x2c`, `+0x30`), terrain (`+0x16`) and sprites.
    pub task: Task,
}

/// The sprites a flight's `MoveTo` takes (`0x140a84b40` state 3): the master's walking sprite
/// (`+0x748`) for a task without one, and the sprites' table.
#[derive(Clone, Copy, Debug)]
pub struct FlightSprites<'a> {
    /// The bird's master's walking sprite, None for none.
    pub walking: Option<SpriteId>,
    /// The sprites' table.
    pub graphics: &'a std::collections::BTreeMap<SpriteId, Graphic>,
}

/// The height a bird of fly mode 1 flies at once its Fly starts (`FUN_140a84900`).
pub const HEIGHT: f32 = crate::math::image::fly::HEIGHT;

impl Fly {
    /// The Fly a bird's default task starts (constructor `FUN_140a845c0`, then its first, slot
    /// 0x28 `FUN_140a84900` with 1, as the action list adds it): its home the bird's place
    /// (`position`), then `set_state(3)` draws the flight's time from the Action stream. The
    /// first's other parts are the caller's: a player's bird whose task has `+0x15` set goes to
    /// state 1 first, and a master of fly mode 1 lifts the bird to [`HEIGHT`] (slot 0x58).
    pub fn start(task: Task, position: Position, action: &mut Well512) -> Self {
        let mut fly = Self {
            state: FlyState::New,
            timer: Seconds::ZERO,
            move_to: None,
            home: position.ground(),
            task,
        };
        let _ = fly.set_state(FlyState::Choosing, action);
        fly
    }

    /// `set_state` (`0x140a84a50`): 2 and 3 drop the `MoveTo` (its teardown stops the bird, the
    /// caller's: the returned flag); 2 gives the bird its master's sprite (slot 0x68, not held
    /// for a bird); 3 draws the flight's time, `r * 10 / 0x7fff + 5` seconds, from the Action
    /// stream.
    fn set_state(&mut self, state: FlyState, action: &mut Well512) -> bool {
        self.state = state;
        if state == FlyState::Flying {
            return false;
        }
        let walked = self.move_to.take().is_some();
        if state == FlyState::Choosing {
            let r = action.rand15() as i32;
            self.timer = Seconds::new(((r * 10) / 0x7fff + 5) as f32);
        }
        walked
    }

    /// One update (`0x140a84b40`) of bird `bird` (`me`) through its step: the timer counts
    /// down by dt; resting, a bird whose timer ran out chooses (`set_state` 3); choosing, it
    /// picks its flight ([`Fly::choose`]); flying, a timer run out rests it (`set_state` 2, the
    /// rest time set), and once the `MoveTo` is done or gone (its list's update returns 1) it
    /// rests for the rest time more, or chooses at once. A `set_state` that drops the `MoveTo`
    /// stops the bird there (the teardown's slot 0x5d0, [`stop_if`]); the failed `MoveTo`'s
    /// update posts notify 0x1f9 with the move action's value to the bird's unit AI.
    ///
    /// # Errors
    ///
    /// A bird without movement state, and the states and tasks the slice does not model.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        bird: &mut Unit,
        me: Me,
    ) -> Result<(), GenieError> {
        let _ = bird.body.mover().ok_or(NO_MOVER)?;
        if Seconds::ZERO < self.timer {
            self.timer -= c.dt.seconds();
        }
        let rest = self.task.wait_time;
        match self.state {
            FlyState::Resting => {
                if Seconds::ZERO < self.timer {
                    return Ok(());
                }
                self.set_state_of(bird, FlyState::Choosing, c)
            }
            FlyState::Choosing => {
                let mover = bird.body.mover().ok_or(NO_MOVER)?;
                let sprites = FlightSprites {
                    walking: bird.master(&c.w.masters, c.player)?.move_sprite,
                    graphics: &c.w.graphics,
                };
                let action = c.w.streams.get(Stream::Action);
                let dropped = self.choose(mover, c.w.spatial.map, action, &sprites)?;
                stop_if(bird, dropped)
            }
            FlyState::Flying => {
                if self.timer <= Seconds::ZERO {
                    self.set_state_of(bird, FlyState::Resting, c)?;
                    if EPSILON < rest.get() {
                        self.timer = rest;
                    }
                }
                // the sub-action list's update (FUN_140a992b0): none left (1), the MoveTo's end
                // (1) or its failure (3) end the flight. A bird is no villager: a motion step
                // that gives up fails its MoveTo (set_state 13, LAB_140aa320c)
                // (its set_state(4) shows its sprite, +0x58, before the motion step)
                let code =
                    update_sub_actions(&mut self.move_to, c, bird, me, ListUpdate::default())?;
                if code == 0 {
                    return Ok(());
                }
                if EPSILON < rest.get() {
                    self.timer += rest;
                    return self.set_state_of(bird, FlyState::Resting, c);
                }
                self.set_state_of(bird, FlyState::Choosing, c)
            }
            _ => Err(GenieError::unsupported(
                "a Fly action in a state the slice does not model",
            )),
        }
    }

    /// [`Fly::set_state`] of `bird`'s Fly with the step's Action stream; a dropped `MoveTo` stops
    /// the bird at once ([`stop_if`]).
    fn set_state_of(
        &mut self,
        bird: &mut Unit,
        state: FlyState,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let dropped = self.set_state(state, c.w.streams.get(Stream::Action));
        stop_if(bird, dropped)
    }

    /// State 3: an angle drawn from the Action stream (`0..360`, `FUN_140810fc0`), a distance of
    /// speed × time left × the task's distance factor (1 for none); from the bird the point that
    /// far along the angle (the y axis turned by it), tried at most 12 times, 80° on each time:
    /// it must lie inside the map and, with a leash, the bird must be within it of its centre or
    /// the point nearer the centre than the bird. A terrain the point must have and objects of a
    /// class to keep away from are not modelled. The flight's sprite ([`Fly::flight_sprite`]):
    /// a task with sprites draws one, else the master's walking sprite. The `MoveTo` to the point
    /// at the bird's height within 0 with that sprite starts (state 3) and the bird flies
    /// (`set_state` 4); with no distance or no point it rests.
    fn choose(
        &mut self,
        mover: &Mover,
        map: GridSize,
        action: &mut Well512,
        sprites: &FlightSprites<'_>,
    ) -> Result<bool, GenieError> {
        let t = self.task;
        let (speed, position) = (mover.stats.speed, mover.position());
        let radius = mover.stats.radius.ground();
        let leash = f32::from(t.holding_attr) * f32::from(t.holding_attr);
        let mut angle = action.below_checked(360) as i32;
        let factor = if t.work_val1.get() < EPSILON {
            1.0
        } else {
            t.work_val1.get()
        };
        let distance = (speed * self.timer * factor).get();
        let (x, y, z) = (position.x(), position.y(), position.z());
        let (w, h) = (map.width(), map.height());
        let mut tries = 0;
        let point = loop {
            if distance < EPSILON {
                return Ok(self.set_state(FlyState::Resting, action));
            }
            let theta = angle as f32 * DEGREE;
            let (c, s) = (cos(theta), sin(theta));
            let dx = c * 0.0 - s;
            let dy = s * 0.0 + c;
            let py = dy * distance + y;
            let px = dx * distance + x;
            angle += ANGLE_STEP;
            tries += 1;
            let inside = 0.0 < px && px <= (w - 1) as f32 && 0.0 < py && py <= (h - 1) as f32;
            if inside
                && (leash <= EPSILON || self.near_home(position, radius, Point::new(px, py), leash))
            {
                break [px, py];
            }
            if TRIES - 1 < tries {
                return Ok(self.set_state(FlyState::Resting, action));
            }
        };
        if !t.terrain.is_none() {
            return Err(GenieError::unsupported(
                "a flight to a terrain of its task's",
            ));
        }
        if t.work_val2.get() as i32 as i16 != -1 && Distance::ZERO < t.work_range {
            return Err(GenieError::unsupported(
                "a flight keeping away from a class",
            ));
        }
        if EPSILON < t.work_val1.get() {
            return Err(GenieError::unsupported(
                "a flight's animation speed (+0x7b8)",
            ));
        }
        let sprite = graphic_of(sprites.graphics, Self::flight_sprite(t, action, sprites)?)?;
        // FUN_140aa0d30 to the point within 0 with the flight's sprite, not quiet
        self.move_to = Some(MoveToAction {
            quiet: false,
            ..MoveToAction::sub_to_point(
                Position::new(point[0], point[1], z),
                Distance::ZERO,
                sprite,
            )
        });
        let _ = self.set_state(FlyState::Flying, action);
        Ok(false)
    }

    /// The sprite of state 3's `MoveTo`: a task with a moving (`+0x40`), proceeding (`+0x48`) or
    /// carrying (`+0x58`) sprite draws `r % 3` from the Action stream and takes the first there
    /// from that index on among its moving, working (`+0x50`) and proceeding sprites; a task with
    /// none of the three takes the master's walking sprite (`+0x748`).
    ///
    /// # Errors
    ///
    /// A task with only a carrying sprite (the game's draw loops on).
    fn flight_sprite(
        t: Task,
        action: &mut Well512,
        sprites: &FlightSprites<'_>,
    ) -> Result<Option<SpriteId>, GenieError> {
        let TaskSprites {
            moving,
            proceeding,
            working,
            carrying,
        } = t.sprites;
        if moving.is_none() && carrying.is_none() && proceeding.is_none() {
            return Ok(sprites.walking);
        }
        let first = action.rand15() % 3;
        let drawn = [moving, working, proceeding]
            .into_iter()
            .cycle()
            .skip(first as usize)
            .take(3)
            .find(Option::is_some);
        drawn.ok_or(GenieError::unsupported(
            "a flight whose task has only a carrying sprite",
        ))
    }

    /// The leash test of state 3: the bird within it of the centre (the squared footprint
    /// distance, `FUN_140a847c0`), or the point within it, or no farther than the bird.
    fn near_home(&self, at: Position, radius: Footprint, point: Point, leash: f32) -> bool {
        let (hx, hy) = (self.home.x(), self.home.y());
        let gap = |d: f32, r: f32| {
            let g = d.abs() - r;
            if 0.0 <= g { g } else { 0.0 }
        };
        let gx = gap(hx - at.x(), radius.x());
        let gy = gap(hy - at.y(), radius.y());
        let bird = gy * gy + gx * gx;
        if bird <= leash {
            return true;
        }
        let ey = point.y() - hy;
        let ex = point.x() - hx;
        let d = ex * ex + ey * ey;
        d < leash || d <= bird
    }
}

/// The teardown of a dropped `MoveTo` (slot 0x5d0, [`Mover::stop`]): the bird stops when it
/// has movement state.
///
/// # Errors
///
/// What the stop does not model.
fn stop_if(bird: &mut Unit, dropped: bool) -> Result<(), GenieError> {
    match bird.body.mover_mut() {
        Some(mover) if dropped => mover.stop(),
        _ => Ok(()),
    }
}

impl Unit {
    /// The default task's Fly ([`crate::movement::fly::Fly::start`]): a player's bird (its task's
    /// `+0x15`, not held) is not modelled; a master of fly mode 1 lifts the bird to the fly
    /// height (slot 0x58, the position setter: its tile stays, so do its counts).
    pub(crate) fn start_fly(&mut self, c: &mut Step<'_>, task: Task) -> Result<(), GenieError> {
        if c.player != PlayerId::GAIA {
            return Err(GenieError::unsupported(
                "a player's bird's default Fly (its task's +0x15)",
            ));
        }
        let fly_mode = self.master(&c.w.masters, c.player)?.fly_mode;
        self.actions.set_fly(Some(Fly::start(
            task,
            self.position(),
            c.w.streams.get(Stream::Action),
        )));
        if fly_mode == FlyMode::ABOVE {
            self.lift_to_fly_height(c)?;
        }
        Ok(())
    }

    /// The bird goes to the fly height on its tile (slot 0x58, the position setter), which marks
    /// the move (`+0x824`, `+0x870`).
    fn lift_to_fly_height(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let (x, y) = (self.position().x(), self.position().y());
        let at = Position::new(x, y, HEIGHT);
        self.new_position(at, (&mut c.w.spatial, Some(&mut c.w.pathing)))?;
        if let Some(m) = self.body.mover_mut() {
            m.mark_move(at, c.w.world_time);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Radii;
    use crate::movement::Speed;
    use crate::movement::move_to::MoveTo;
    use crate::objects::ActionState;
    use crate::objects::ListIndex;
    use crate::objects::ObjectState;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::layers::Body;
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::TaskTerrain;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::unit_ai::UnitAi;
    use crate::world::Millis;
    use crate::world::WorldTime;
    use crate::world::{Player, World};

    /// The hawk's place.
    const AT: Position = Position::new(57.845_688, 104.499_275, 9.0);

    /// The hawk's movement state: top speed `speed`, radii 0.2, waiting to move (a stop clears
    /// it).
    fn wings(speed: f32) -> Mover {
        let mut m = crate::gathering::hunt::tests::walker();
        m.stats.speed = Speed::new(speed);
        m.stats.radius = Radii::new(0.2, 0.2, 0.5);
        m
    }

    /// A flight's `MoveTo` in `state` to `point` within 0, not quiet, without a sprite.
    fn flight(state: MoveTo, point: Position) -> MoveToAction {
        MoveToAction {
            state,
            quiet: false,
            ..MoveToAction::sub_to_point(point, Distance::ZERO, None)
        }
    }

    /// MarvinBlue/426048731's hawk 9925 (master 96, HAWKX): its task has no leash, rest or
    /// distance factor, and three sprites.
    fn hawk(state: FlyState, timer: f32) -> Fly {
        Fly {
            state,
            timer: Seconds::new(timer),
            move_to: None,
            home: Point::new(78.5, 103.5),
            task: Task {
                action_type: FLY,
                object_group: None,
                work_val2: crate::objects::masters::WorkValue::new(0.0),
                sprites: TaskSprites::of_data([846, 1595, 846, -1]),
                ..Task::default()
            },
        }
    }

    /// A map of 120 × 120 tiles whose Gaia holds the hawk at its place with `fly`, a unit AI
    /// and movement state of top speed `speed`; the Action stream seeded with `seed`.
    fn sky_world(fly: Fly, speed: f32, seed: u32) -> World {
        let mut bird = Unit {
            id: object_id(9925),
            body: Body::new(AT, None, Some(wings(speed))),
            life: Lifecycle {
                state: ObjectState::Alive,
                master_id: master_id(96),
                ..Lifecycle::default()
            },
            ai: Some(UnitAi::default()),
            ..Unit::default()
        };
        bird.actions.set_fly(Some(fly));
        let gaia = Player {
            objects: vec![bird].into(),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![gaia],
        );
        t.spatial = crate::map::spatial::Spatial::new(GridSize::new(120, 120));
        t.streams.get(Stream::Action).set_seed(seed);
        t.graphics = hawk_graphics();
        t.masters = vec![crate::objects::masters::Masters::new([
            crate::objects::masters::Master {
                id: master_id(96),
                move_sprite: Some(SpriteId::named(846)),
                ..crate::objects::masters::Master::default()
            },
        ])];
        t
    }

    /// The hawk's sprites: 846 (its task's moving and working one, its master's walking one)
    /// and 1595 (the task's proceeding one).
    fn hawk_graphics() -> std::collections::BTreeMap<SpriteId, Graphic> {
        [(846, 0.0625), (1595, 0.0625)]
            .map(|(id, frame_duration)| {
                let g = Graphic {
                    id: SpriteId::named(id),
                    frames: crate::objects::Frame::of_data(60),
                    facets: crate::objects::FacetCount::of_data(16),
                    speed_multiplier: SpeedMultiplier::ONE,
                    frame_duration: Seconds::new(frame_duration),
                    replay_delay: Seconds::ZERO,
                    flags: SpriteFlags::ANIMATED | SpriteFlags::TURNS,
                };
                (g.id, g)
            })
            .into()
    }

    /// The hawk's update of 21 ms through its step: its Fly after it.
    fn flown(t: &mut World) -> Result<Fly, GenieError> {
        let me = Me {
            id: object_id(9925),
            group: None,
            action_state: ActionState::NO_ACTION,
        };
        crate::world::tests::in_update(t, (PlayerId::new(0), ListIndex::new(0)), 21, |u, c| {
            u.fly_part(c, me)
        })?;
        Ok(*t.players[0].objects[ListIndex::new(0)]
            .actions
            .fly()
            .unwrap())
    }

    /// The hawk stopped (slot 0x5d0 clears its wait to move).
    fn stopped(t: &World) -> bool {
        t.players[0].objects[ListIndex::new(0)]
            .body
            .mover()
            .unwrap()
            .waiting
            == 0
    }

    /// The notifies 0x1f9 the hawk's unit AI holds.
    fn failures(t: &World) -> usize {
        let ai = t.players[0].objects[ListIndex::new(0)].ai.as_ref().unwrap();
        let failed = ai.notifies.iter();
        failed
            .filter(|n| n.kind == crate::unit_ai::NotifyKind::ACTION_FAILED)
            .count()
    }

    #[test]
    fn a_flight_whose_time_runs_out_chooses_the_next_at_once_without_a_rest() {
        // update 717: the timer of 0.016 runs out: set_state(2) drops the MoveTo (the bird
        // stops), the empty list's update is 1 and the task has no rest: set_state(3) draws the
        // next flight's time
        let fly = Fly {
            move_to: Some(flight(
                MoveTo::Moving,
                Position::new(57.844_986, 104.49953, 9.0),
            )),
            ..hawk(FlyState::Flying, 0.015_977_083)
        };
        let mut t = sky_world(fly, 1.32, 1);
        let mut e = t.streams.get(Stream::Action).clone();
        let fly = flown(&mut t).unwrap();
        assert!(stopped(&t));
        assert_eq!(failures(&t), 0);
        let time = ((e.rand15() as i32 * 10) / 0x7fff + 5) as f32;
        assert_eq!(
            (fly.state, fly.timer, fly.move_to),
            (FlyState::Choosing, Seconds::new(time), None)
        );
        assert_eq!(t.streams.get(Stream::Action).draws, 1);
    }

    /// State 3's choice (`FUN_140b03a00` case 3): no time left (no distance) or no point on the
    /// map in 12 tries rests; a task without sprites draws none and flies; a task's terrain,
    /// a class to keep away from, a distance factor (the animation speed) and only a carrying
    /// sprite are not modelled.
    #[test]
    fn a_choice_without_a_distance_or_a_point_rests() {
        let mover = Body::new(AT, None, Some(wings(1.32)));
        let mover = mover.mover().unwrap();
        let choose = |fly: &mut Fly, map: GridSize| {
            let mut r = Well512::default();
            r.set_seed(7);
            let graphics = hawk_graphics();
            let sprites = FlightSprites {
                walking: None,
                graphics: &graphics,
            };
            let got = fly.choose(mover, map, &mut r, &sprites);
            (got, r.draws)
        };
        let mut fly = hawk(FlyState::Choosing, 0.0);
        let _ = choose(&mut fly, GridSize::new(120, 120)).0.unwrap();
        assert_eq!(fly.state, FlyState::Resting);
        let mut fly = hawk(FlyState::Choosing, 10.0);
        let _ = choose(&mut fly, GridSize::new(1, 1)).0.unwrap();
        assert_eq!((fly.state, fly.move_to), (FlyState::Resting, None));
        let plain = |task: Task| Fly {
            task,
            ..hawk(FlyState::Choosing, 10.0)
        };
        let unsprited = Task {
            sprites: TaskSprites::NONE,
            ..hawk(FlyState::Choosing, 10.0).task
        };
        let mut fly = plain(unsprited);
        let (got, draws) = choose(&mut fly, GridSize::new(120, 120));
        assert_eq!((got, draws), (Ok(false), 1));
        assert_eq!(fly.state, FlyState::Flying);
        for task in [
            Task {
                terrain: TaskTerrain::of_data(2),
                ..unsprited
            },
            Task {
                work_val2: crate::objects::masters::WorkValue::new(4.0),
                work_range: Distance::new(3.0),
                ..unsprited
            },
            Task {
                work_val1: crate::objects::masters::WorkValue::new(2.0),
                ..unsprited
            },
            Task {
                sprites: TaskSprites::of_data([-1, -1, -1, 5]),
                ..unsprited
            },
        ] {
            assert!(choose(&mut plain(task), GridSize::new(120, 120)).0.is_err());
        }
    }

    /// State 3's sprite (`0x140a84b40`): `r % 3` picks among the moving, working and proceeding
    /// sprites and takes the first there from that index on; a task with no moving, proceeding
    /// or carrying sprite takes the master's walking one without a draw; only a carrying sprite
    /// is not modelled (the game's draw loops on).
    #[test]
    fn a_flight_s_sprite_is_the_first_of_the_task_s_from_the_drawn_index_on() {
        let graphics = hawk_graphics();
        let sprites = FlightSprites {
            walking: Some(SpriteId::named(846)),
            graphics: &graphics,
        };
        let with = |s: [i16; 4]| Task {
            sprites: TaskSprites::of_data(s),
            ..hawk(FlyState::Choosing, 0.0).task
        };
        // the draw for each seed, then the sprite it gives: [moving, proceeding, working, carrying]
        for seed in 1..40 {
            let mut r = Well512::default();
            r.set_seed(seed);
            let first = r.clone().rand15() % 3;
            // moving 1, working 3, proceeding 2: index 0 → 1, 1 → 3, 2 → 2
            let task = with([1, 2, 3, -1]);
            let got = Fly::flight_sprite(task, &mut r, &sprites).unwrap();
            assert_eq!(got, Some(SpriteId::named([1, 3, 2][first as usize])));
            assert_eq!(r.draws, 1);
            // only the proceeding sprite: every index comes round to it
            let mut r2 = Well512::default();
            r2.set_seed(seed);
            assert_eq!(
                Fly::flight_sprite(with([-1, 2, -1, -1]), &mut r2, &sprites),
                Ok(Some(SpriteId::named(2)))
            );
        }
        let mut r = Well512::default();
        assert_eq!(
            Fly::flight_sprite(with([-1, -1, 5, -1]), &mut r, &sprites),
            Ok(Some(SpriteId::named(846)))
        );
        assert_eq!(r.draws, 0);
        assert!(Fly::flight_sprite(with([-1, -1, -1, 7]), &mut r, &sprites).is_err());
    }

    #[test]
    fn a_choice_flies_speed_times_time_along_a_drawn_angle() {
        // update 718: 10 s left less dt, 1.32 a second: 13.17 tiles at the drawn angle, then
        // the sprite draw; the MoveTo starts at the bird's height
        let mut t = sky_world(hawk(FlyState::Choosing, 10.0), 1.32, 7);
        let mut e = t.streams.get(Stream::Action).clone();
        let fly = flown(&mut t).unwrap();
        let angle = e.below_checked(360) as i32;
        let d = 1.32 * (10.0f32 - 0.021) * 1.0;
        let theta = angle as f32 * DEGREE;
        let (c, s) = (cos(theta), sin(theta));
        let point = Point::new((c * 0.0 - s) * d + AT.x(), (s * 0.0 + c) * d + AT.y());
        let inside = |p: Point| 0.0 < p.x() && p.x() <= 119.0 && 0.0 < p.y() && p.y() <= 119.0;
        assert!(inside(point), "the seed's first angle lands on the map");
        // the sprite draw: the task's moving 846, working 846, proceeding 1595 from r % 3 on
        let drawn = [846, 846, 1595][e.rand15() as usize % 3];
        let to = point.with_height(9.0);
        let sprite = hawk_graphics().get(&SpriteId::named(drawn)).copied();
        assert_eq!(
            (fly.state, fly.move_to),
            (
                FlyState::Flying,
                Some(MoveToAction {
                    sprite,
                    ..flight(MoveTo::Start, to)
                })
            )
        );
        assert_eq!(t.streams.get(Stream::Action).draws, 2);
        assert!(!stopped(&t));
    }

    #[test]
    fn a_failed_flight_posts_its_failure_and_rests() {
        // records/425906646's update 2025: the hawk 30594's MoveTo failed (state 13) the update
        // before; its update returns 3, the flight ends as an arrived one does
        let fly = Fly {
            move_to: Some(flight(
                MoveTo::Failed,
                Position::new(57.844_986, 104.49953, 9.0),
            )),
            ..hawk(FlyState::Flying, 5.0)
        };
        let mut t = sky_world(fly, 1.32, 1);
        let fly = flown(&mut t).unwrap();
        assert_eq!(failures(&t), 1);
        assert!(stopped(&t));
        assert_eq!(fly.move_to, None);
    }

    /// The motion step gives up in the update the flight's start runs (a bird without speed
    /// here): the bird's `MoveTo` fails there, as in its moving state; the next update posts it.
    #[test]
    fn a_flight_the_motion_step_gives_up_at_its_start_fails() {
        let fly = Fly {
            move_to: Some(flight(MoveTo::Start, Position::new(60.5, 104.5, 9.0))),
            ..hawk(FlyState::Flying, 5.0)
        };
        let mut t = sky_world(fly, 0.0, 1);
        let fly = flown(&mut t).unwrap();
        assert_eq!(
            (fly.move_to.map(|m| m.state), failures(&t)),
            (Some(MoveTo::Failed), 0)
        );
        assert!(!stopped(&t));
        let _ = flown(&mut t).unwrap();
        assert_eq!(failures(&t), 1);
        assert!(stopped(&t));
    }

    #[test]
    fn a_resting_bird_chooses_once_its_rest_ran_out() {
        let mut t = sky_world(hawk(FlyState::Resting, 0.5), 1.32, 1);
        let fly = flown(&mut t).unwrap();
        assert_eq!(
            (fly.state, t.streams.get(Stream::Action).draws),
            (FlyState::Resting, 0)
        );
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .fly_mut()
            .unwrap()
            .timer = Seconds::new(0.01);
        let fly = flown(&mut t).unwrap();
        assert_eq!(
            (fly.state, t.streams.get(Stream::Action).draws),
            (FlyState::Choosing, 1)
        );
    }

    /// A bird without movement state fails its update, whatever its state.
    #[test]
    fn a_bird_without_movement_state_does_not_fly() {
        let mut t = sky_world(hawk(FlyState::Resting, 0.5), 1.32, 1);
        t.players[0].objects[ListIndex::new(0)].body = Body::at(AT);
        assert_eq!(flown(&mut t), Err(NO_MOVER));
    }

    #[test]
    fn a_leash_keeps_the_point_near_home() {
        let fly = Fly {
            home: Point::new(10.0, 10.0),
            ..hawk(FlyState::Choosing, 1.0)
        };
        // the bird 5 tiles off with a leash of 3 (9 squared): a point nearer home passes, one
        // farther does not
        assert!(fly.near_home(
            Position::new(15.0, 10.0, 0.0),
            Footprint::ZERO,
            Point::new(12.0, 10.0),
            9.0
        ));
        assert!(!fly.near_home(
            Position::new(15.0, 10.0, 0.0),
            Footprint::ZERO,
            Point::new(16.0, 10.0),
            9.0
        ));
        assert!(fly.near_home(
            Position::new(12.0, 10.0, 0.0),
            Footprint::ZERO,
            Point::new(40.0, 10.0),
            9.0
        ));
    }
}
