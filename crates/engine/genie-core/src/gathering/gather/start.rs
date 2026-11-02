//! The Gather's constructor and start, its retarget, the walk, the drop and a move's target,
//! the idle and the halt; a farm's stops and their productivity.

use crate::GenieError;
use crate::gathering::DropSiteId;
use crate::gathering::FarmStops;
use crate::gathering::StopIndex;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{
    DEFAULT_PRODUCTIVITY, FARM_STOPS, GatherAction, GatherReads, Resource, SPECIAL_TARGETS,
    STOP_CENTRE, STOP_DRAWS, STOP_FARMS, Walks,
};
use crate::gathering::hunt::HuntTask;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::Mover;
use crate::movement::move_to::{TargetMoved, WalkTo};
use crate::objects::ObjectId;
use crate::objects::WorkRate;
use crate::objects::lifecycle::Held;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::step::Step;

/// What a gather's start takes besides its mover and stream ([`GatherAction::start`]'s
/// arguments).
#[derive(Debug)]
pub struct GatherStart<'a> {
    /// The target.
    pub target: &'a Resource,
    /// The owner.
    pub player: PlayerId,
    /// The owner's type (`player+0x10`; [`PlayerType::is_computer`] is `FUN_141659cf0`).
    pub player_type: PlayerType,
    /// The game settings' test `FUN_141302150`.
    pub carry_rule: bool,
    /// What the gatherer carries, its resource kind and amount.
    pub held: &'a mut Held,
    /// What its `MoveTo`s' constructors read of the gatherer; its load is `held`'s.
    pub walks: Walks<'a>,
}

/// What a gather's retarget takes besides its mover and stream ([`GatherAction::retarget`]'s
/// arguments).
#[derive(Clone, Copy, Debug)]
pub struct GatherRetarget<'a> {
    /// The work command's new target.
    pub target: &'a Resource,
    /// The owner.
    pub player: PlayerId,
    /// The owner's type (`player+0x10`; [`PlayerType::is_computer`] is `FUN_141659cf0`).
    pub player_type: PlayerType,
    /// What the gatherer carries and the sprites its `MoveTo`s take for none.
    pub walks: Walks<'a>,
    /// The game's farm setting: a stop farm or a fish trap keeps its place as the farm's.
    pub farm_stops: bool,
    /// What times a farm's stops.
    pub rate: StopRate,
}

/// What the constructor reads to time a farm's stops: the gatherer's work rate (its master's
/// `+0x798` plus its powerup, slot 0x6e8), its capacity (`+0x61c`) and the player's attribute the
/// task's `attribute_type2` names (none: 1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StopRate {
    /// The work rate.
    pub work_rate: WorkRate,
    /// The capacity.
    pub capacity: Amount,
    /// The productivity.
    pub productivity: Amount,
}

impl StopRate {
    /// The time per stop (`+0x70`): the capacity times the productivity over the work a second,
    /// shared out over the stops.
    #[must_use]
    pub fn time_per_stop(
        self,
        work_value: crate::objects::masters::WorkValue,
        stops: FarmStops,
    ) -> Seconds {
        Seconds::new(
            ((self.capacity.get() * self.productivity.get()) / (self.work_rate * work_value).get())
                / stops.as_f32(),
        )
    }
}

/// What slot 0x40 reads of a move's target object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveTarget {
    /// The object.
    pub id: ObjectId,
    /// Its owner.
    pub player: PlayerId,
    /// The gatherer's master drops its loads there (its main building's master for an annex,
    /// `+0xbb8`; `FUN_14109d4f0`).
    pub drop_site: bool,
    /// Its place.
    pub position: Position,
    /// The footprint gap from the gatherer to it.
    pub gap: Distance,
}

/// The productivity the time per stop reads (the constructor's and the update's): the player's
/// attribute the task's `attribute_type2` names, as it is, or 1.0 for none (`DAT_14330eb8c`).
#[must_use]
pub fn productivity_of(attribute_type2: Option<AttributeId>, attributes: &[Amount]) -> Amount {
    let Some(attribute) = attribute_type2 else {
        return DEFAULT_PRODUCTIVITY;
    };
    attributes
        .get(attribute.index())
        .copied()
        .unwrap_or(Amount::ZERO)
}

/// `set_state(4)`'s point of a farm's next stop (`0x140a8cd40` case 4): a tile of the farm's
/// footprint (its place `position` less its radii `radius`, plus a draw of the Action stream
/// modulo twice each radius, its centre 0.5 in), drawn again while it is the last stop's, at
/// most ten times.
#[must_use]
pub fn stop_point(position: Position, radius: Footprint, last: Point, rng: &mut Well512) -> Point {
    let (mut ix, mut iy) = (
        (radius.x() + radius.x()) as i32,
        (radius.y() + radius.y()) as i32,
    );
    if ix == 0 || iy == 0 {
        (ix, iy) = (1, 1);
    }
    let (left, bottom) = (
        (position.x() - radius.x()) as i32,
        (position.y() - radius.y()) as i32,
    );
    let (mut x, mut y) = (last.x(), last.y());
    let mut draws = 0;
    while x == last.x() && y == last.y() && draws < STOP_DRAWS {
        x = ((rng.rand15() as i32) % ix + left) as f32 + STOP_CENTRE;
        y = ((rng.rand15() as i32) % iy + bottom) as f32 + STOP_CENTRE;
        draws += 1;
    }
    Point::new(x, y)
}

impl GatherAction {
    /// The factory's constructor with a target (`FUN_140a88310`; the target's add, slot 0x178, is
    /// the caller's): the target's place is the point and its class the target type; a farm (class
    /// 49) or masters 357 and 1188 keep their place as the farm's. One stop, `mGatherX` and
    /// `mGatherY` at −1. A farm of master 50 or 1187 with the game's farm setting has four stops,
    /// each [`StopRate::time_per_stop`] long.
    ///
    /// # Errors
    ///
    /// None yet.
    pub fn new(
        task: HuntTask,
        target: &Resource,
        farm_stops: bool,
        rate: StopRate,
    ) -> Result<Self, GenieError> {
        let farm = if target.class == class::FARM || SPECIAL_TARGETS.contains(&target.master) {
            target.position.ground()
        } else {
            Point::ORIGIN
        };
        let (stops, time_per_stop) =
            if farm_stops && target.class == class::FARM && STOP_FARMS.contains(&target.master) {
                (FARM_STOPS, rate.time_per_stop(task.work_value, FARM_STOPS))
            } else {
                (FarmStops::ONE, Seconds::ZERO)
            };
        Ok(Self {
            target: Some(crate::gathering::GatherTargetId::new(target.id)),
            carry_to: None,
            state: GatherState::New,
            point: target.position,
            timer: Seconds::ZERO,
            task,
            target_class: target.class.or_none(),
            stops,
            current_stop: StopIndex::FIRST,
            time_per_stop,
            time_at_stop: Seconds::ZERO,
            last_stop: Point::ORIGIN,
            worked: false,
            farm,
            gather_at: Point::from([-1.0; 2]),
        })
    }

    /// The install's start (slot 5 with 1, `0x140a8b3a0`): a gatherer carrying another food keeps
    /// the amount as the task's kind (`FUN_14119d310`); one carrying another resource drops it
    /// when the game settings' test `FUN_141302150` (`carry_rule`) says so or it is a computer
    /// player's carrying nothing. A loaded gatherer sent to one of its own drop sites carries the
    /// load there: the target becomes the second target (the caller takes it, then releases it as
    /// the first), the point clears and `set_state(8)` walks there. Else `set_state(4)` walks to
    /// the target. A farm another farmer works (with this one, two or more) is walked to only as
    /// a point (`set_state(11)`, [`GatherAction::set_walk_back`]) and let go.
    ///
    /// # Errors
    ///
    /// What the start does not model.
    pub fn start(
        &mut self,
        start: GatherStart<'_>,
        mover: &mut Mover,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let GatherStart {
            target,
            player,
            player_type,
            carry_rule,
            held,
            walks,
        } = start;
        let kind = self.task.resource;
        let both_food = crate::gathering::both_food(kind, held.kind);
        let drops = player_type.is_computer() && held.amount == Amount::ZERO;
        if (!carry_rule && !drops) || held.kind == kind || both_food {
            if held.kind != kind && both_food {
                held.kind = kind;
            }
        } else {
            *held = Held {
                kind,
                amount: Amount::ZERO,
            };
        }
        let walks = Walks {
            held: *held,
            ..walks
        };
        // a farm another farmer works (`+0x644` above 1 with this one, which the constructor
        // took): set_state(11) to its point, then the farm goes (slot 0x68 with none; the caller
        // lets it go)
        if self.target_class == Some(class::FARM)
            && crate::objects::WorkerCount::ONE < target.workers.plus(1)
        {
            let radius = if STOP_FARMS.contains(&target.master) {
                0.0
            } else {
                target.radius.x()
            };
            self.set_walk_back(Distance::new(radius), mover, &walks)?;
            self.target = None;
            return Ok(());
        }
        if target.player == player && target.drop_site && Amount::ZERO < held.amount {
            self.target = None;
            self.point = Position::from([-1.0; 3]);
            self.carry_to = Some(DropSiteId::new(target.id));
            return self.set_drop(target.position, target.gap, mover, &walks);
        }
        self.set_walk(target.walk_to(), target.radius, mover, rng, &walks)
    }

    /// Slot 0x48 (`0x140a903a0`) on the work command's new `target` (one the world holds that is
    /// no building, or a farm; the caller's tests): the gathering is not started any more
    /// (`+0x80` cleared first, whatever follows). Its present target in state 6, 7 or 15, or in
    /// 4 or 11 for a computer player, it keeps as it is. A loaded gatherer (`held`) sent to one of
    /// its player's drop sites carries its load there: the timer clears, the target goes (slot
    /// 0x68 with none) with the point, the drop site becomes the second target (`FUN_140a7a190`)
    /// and `set_state(8)` walks there. Any other target becomes the target (slot 0x68) with its
    /// place as the point and its class as the target type, the second target goes and
    /// `set_state(4)` walks to it; the caller takes and lets go the targets that changed. With
    /// the game's farm setting a fish trap (master 199) keeps its place as the farm's, and a stop
    /// farm (masters 50, 1187) too, with its four stops from the first and the time per stop of
    /// `rate`, its time at the stop cleared. A farm another farmer works (with this one, two or
    /// more) is walked to as a point (`set_state(11)`) and let go again (slot 0x68 with none, then
    /// `FUN_140a7a190` with none): the caller takes it and lets it go. Always true: the gather
    /// took the target.
    ///
    /// # Errors
    ///
    /// What it does not model; a unit that cannot move.
    pub fn retarget(
        &mut self,
        retarget: GatherRetarget<'_>,
        mover: &mut Mover,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        let GatherRetarget {
            target,
            player,
            player_type,
            walks,
            farm_stops,
            rate,
        } = retarget;
        let held = walks.held.amount;
        self.worked = false;
        if self.target.map(crate::gathering::GatherTargetId::id) == Some(target.id) {
            match self.state {
                GatherState::Face | GatherState::Work | GatherState::Moved { .. } => {
                    return Ok(true);
                }
                GatherState::Walk { .. } | GatherState::WalkBack { .. }
                    if player_type.is_computer() =>
                {
                    return Ok(true);
                }
                _ => {}
            }
        }
        if target.player == player && target.drop_site && Amount::ZERO < held {
            self.timer = Seconds::ZERO;
            self.target = None;
            self.point = Position::from([-1.0; 3]);
            self.carry_to = Some(DropSiteId::new(target.id));
            self.set_drop(target.position, target.gap, mover, &walks)?;
            return Ok(true);
        }
        // slot 0x68 takes the target: its workers count it once it is new
        let taken = crate::gathering::GatherTargetId::new(target.id);
        let workers = target.workers.plus(i32::from(Some(taken) != self.target));
        self.target = Some(taken);
        self.point = target.position;
        self.target_class = target.class.or_none();
        if farm_stops {
            if STOP_FARMS.contains(&target.master) {
                self.farm = target.position.ground();
                self.stops = FARM_STOPS;
                self.current_stop = StopIndex::FIRST;
                self.time_per_stop = rate.time_per_stop(self.task.work_value, FARM_STOPS);
                self.time_at_stop = Seconds::ZERO;
            } else if target.master == ids::FISH_TRAP {
                self.farm = target.position.ground();
            }
        }
        if target.class == class::FARM && crate::objects::WorkerCount::ONE < workers {
            let radius = if STOP_FARMS.contains(&target.master) {
                0.0
            } else {
                target.radius.x()
            };
            self.set_walk_back(Distance::new(radius), mover, &walks)?;
            self.target = None;
            self.carry_to = None;
            return Ok(true);
        }
        self.carry_to = None;
        self.set_walk(target.walk_to(), target.radius, mover, rng, &walks)?;
        Ok(true)
    }

    /// `set_state(4)` (`0x140a8cd40` case 4) with its target at `position` (its master's radii
    /// `radius`), `gap` from the unit: the sub-actions go (a `MoveTo`'s teardown stops the unit),
    /// the timer clears and the target's place becomes the point. With one stop a quiet `MoveTo`
    /// to the target within the task's range starts ([`GatherAction::move_to_object`]). A
    /// farm's stops (the game's `+0xc14` being 0) walk to a point of the farm instead
    /// ([`stop_point`]), which becomes the last stop: a quiet `MoveTo` to it within the task's
    /// range ([`GatherAction::move_to_point`], `+0x61` 1). `walks` is what the constructors read
    /// of the unit.
    ///
    /// # Errors
    ///
    /// A gather without its target (the `MoveTo`'s state 13); a sprite the table lacks.
    pub fn set_walk(
        &mut self,
        to: WalkTo,
        radius: Footprint,
        mover: &mut Mover,
        rng: &mut Well512,
        walks: &Walks<'_>,
    ) -> Result<(), GenieError> {
        let position = to.at;
        self.tear_down(Some(mover))?;
        self.state = GatherState::Walk { move_to: None };
        self.timer = Seconds::ZERO;
        if self.target.is_none() {
            return Err(GenieError::unsupported(
                "a gather's walk without its target (set_state 13)",
            ));
        }
        self.point = position;
        let walk = if self.stops.several() {
            let (x, y) = {
                let p = stop_point(position, radius, self.last_stop, rng);
                (p.x(), p.y())
            };
            self.last_stop = Point::new(x, y);
            let to = Position::new(x, y, position.z());
            self.move_to_point(to, self.task.range, TargetMoved::AsksAgain, mover, walks)?
        } else {
            self.move_to_object(to, mover, walks)?
        };
        self.state = GatherState::Walk { move_to: walk };
        Ok(())
    }

    /// `set_state(8)` (`0x140a8cd40` case 8) with a load and its drop site (`carry_to`, the
    /// caller's search, `FUN_1411671e0`) at `site`, `gap` from the unit: the current stop clears,
    /// the sub-actions go and a quiet `MoveTo` to the drop site within the task's range starts
    /// ([`GatherAction::move_to_object`]).
    ///
    /// # Errors
    ///
    /// No drop site (the unit AI's part); a sprite the table lacks.
    pub fn set_drop(
        &mut self,
        site: Position,
        gap: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
    ) -> Result<(), GenieError> {
        self.tear_down(Some(mover))?;
        self.state = GatherState::Drop { move_to: None };
        self.current_stop = StopIndex::FIRST;
        if self.carry_to.is_none() {
            return Err(GenieError::unsupported(
                "a gather's drop-off without a drop site (notify 0x203)",
            ));
        }
        let to = WalkTo {
            id: self.carry_to.map(DropSiteId::id),
            at: site,
            gap,
            state: None,
        };
        let move_to = self.move_to_object(to, mover, walks)?;
        self.state = GatherState::Drop { move_to };
        Ok(())
    }

    /// Slot 0x40 (`0x140a8b740`) on a move to `point`, `target` the object moved to, if any: a
    /// loaded gatherer (`walks`) moved to one of its player's drop sites carries its load there
    /// (nothing changes when that is already the drop site it walks to in state 8): the target
    /// goes (slot 0x68 with none) with the point, the site becomes the second target
    /// (`FUN_140a7a190`) and `set_state(8)` walks there. Any other move becomes the point: the
    /// target goes and `set_state(15)` walks there ([`GatherAction::set_moved`]). Always taken
    /// (it returns 1); the caller takes and lets go the targets that changed.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn take_move(
        &mut self,
        target: Option<MoveTarget>,
        point: Position,
        player: PlayerId,
        mover: &mut Mover,
        walks: &Walks<'_>,
    ) -> Result<(), GenieError> {
        let held = walks.held.amount;
        match target.filter(|t| t.player == player && t.drop_site && Amount::ZERO < held) {
            Some(t)
                if Some(t.id) == self.carry_to.map(DropSiteId::id)
                    && matches!(self.state, GatherState::Drop { .. }) =>
            {
                Ok(())
            }
            Some(t) => {
                self.target = None;
                self.point = Position::from([-1.0; 3]);
                self.carry_to = Some(DropSiteId::new(t.id));
                self.set_drop(t.position, t.gap, mover, walks)
            }
            None => {
                self.point = point;
                self.target = None;
                self.set_moved(mover, walks)
            }
        }
    }

    /// `set_state(15)` (`0x140a8cd40` case 0xf, as 11): the sub-actions go, the timer clears and
    /// a quiet `MoveTo` to the point within the target's radius plus the task's range starts
    /// ([`GatherAction::move_to_point`]); with no target (a move lets it go) the radius is 0.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn set_moved(&mut self, mover: &mut Mover, walks: &Walks<'_>) -> Result<(), GenieError> {
        self.tear_down(Some(mover))?;
        self.state = GatherState::Moved { move_to: None };
        self.timer = Seconds::ZERO;
        if self.target.is_some() {
            return Err(GenieError::unsupported(
                "a gather's walk to a point with a target (its radius)",
            ));
        }
        let range = self.task.range;
        let move_to = self.move_to_point(self.point, range, TargetMoved::Follows, mover, walks)?;
        self.state = GatherState::Moved { move_to };
        Ok(())
    }

    /// `set_state(2)` (case 2): the timer clears, the unit stands ([`GatherAction::stand_sprite`]),
    /// the target and the second target go (slot 0x68, `FUN_140a7a190` with none; the caller
    /// lets them go) and the point is −1.
    pub(super) fn set_idle(&mut self, c: &mut Step<'_>, unit: &mut Unit, r: &GatherReads) {
        self.state = GatherState::Idle;
        self.timer = Seconds::ZERO;
        Self::show(c, unit, self.stand_sprite(unit, r));
        self.target = None;
        self.carry_to = None;
        self.point = Position::from([-1.0; 3]);
    }

    /// Slot 0x38 (`FUN_140a8e1b0`, as `TRIBE_Action_Gather`'s `FUN_140abf3c0`) as the unit's
    /// stop calls it: `set_state(2)` (`0x140a8cd40` case 2: the sub-actions go first, a
    /// `MoveTo`'s teardown stopping the unit; then [`GatherAction::set_idle`] with `stand`, the
    /// sprite `FUN_140a8b6b0` gives), true. Returns the target and the second target it let go
    /// (slot 0x68 and `FUN_140a7a190` with none), which the caller releases.
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub(crate) fn halt(
        &mut self,
        mover: Option<&mut Mover>,
        stand: Option<crate::objects::sprite::Graphic>,
        sprite: &mut Option<ActiveSprite>,
        rng: &mut Well512,
    ) -> Result<[Option<ObjectId>; 2], GenieError> {
        let mut mover = mover;
        self.tear_down(mover.as_deref_mut())?;
        self.state = GatherState::Idle;
        self.timer = Seconds::ZERO;
        if let Some(s) = stand {
            new_sprite(sprite, mover.map(|m| &mut m.stats), s, rng);
        }
        let released = self.targets();
        self.target = None;
        self.carry_to = None;
        self.point = Position::from([-1.0; 3]);
        Ok(released)
    }
}
