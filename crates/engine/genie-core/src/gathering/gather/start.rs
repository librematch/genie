//! The Gather's constructor and start, its retarget, the walk, the drop and a move's target,
//! the idle and the halt; a farm's stops and their productivity.

use crate::gathering::DropSiteId;
use crate::gathering::FarmStops;
use crate::gathering::StopIndex;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::Stand;
use crate::gathering::gather::point_walks::walk_back_radius;
use crate::gathering::gather::{
    FARM_STOPS, GatherAction, GatherReads, Resource, STOP_DRAWS, Walks,
};
use crate::gathering::hunt::HuntTask;
use crate::gathering::{GatherTargetId, load_for_task};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::Mover;
use crate::movement::move_to::{TargetMoved, WalkTo};
use crate::objects::ObjectId;
use crate::objects::WorkRate;
use crate::objects::WorkerCount;
use crate::objects::actions::LAST_TAKEN;
use crate::objects::lifecycle::Held;
use crate::objects::masters::WorkValue;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::GatherTuning;
use crate::world::layer::generated::MoveToTuning;
use crate::world::layer::generated::Tuning;
use crate::world::settings::FarmMode;
use crate::world::settings::Settings;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// What a gather's start takes besides its mover and stream ([`GatherAction::start`]'s
/// arguments).
#[derive(Debug)]
pub struct GatherStart<'world> {
    /// The target.
    pub(crate) target: &'world Resource,
    /// The owner.
    pub(crate) player: PlayerId,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
    /// The game settings, whose test `FUN_141302150` ([`Settings::carry_rule`]) the load reads.
    pub(crate) settings: &'world Settings,
    /// What the gatherer carries, its resource kind and amount.
    pub(crate) held: &'world mut Held,
    /// What its `MoveTo`s' constructors read of the gatherer; its load is `held`'s.
    pub(crate) walks: Walks<'world>,
}

/// How the Gather opens: what its start ([`GatherAction::start`], `0x140a8b3a0`) leaves to its
/// caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum GatherOpening {
    /// It approaches: walks to its target (`set_state(4)`), with its load to its drop site (8)
    /// or to a farm's point (11).
    Approaches,
    /// It holds no target: `set_state(3)`, the search, which reads the gatherer's step
    /// ([`Unit::gather_begin_search`]).
    Searches,
}

/// What a gather's retarget takes besides its mover and stream ([`GatherAction::retarget`]'s
/// arguments).
#[derive(Clone, Copy, Debug)]
pub struct GatherRetarget<'world> {
    /// The work command's new target.
    pub(crate) target: &'world Resource,
    /// The owner.
    pub(crate) player: PlayerId,
    /// What the gatherer carries and the sprites its `MoveTo`s take for none.
    pub(crate) walks: Walks<'world>,
    /// The game's farm setting: walkable, a stop farm or a fish trap keeps its place as the
    /// farm's.
    pub(crate) farm_mode: FarmMode,
    /// What times a farm's stops.
    pub(crate) rate: StopRate,
}

/// What the constructor reads to time a farm's stops: the gatherer's work rate (its master's
/// `+0x798` plus its powerup, slot 0x6e8), its capacity (`+0x61c`) and the player's attribute the
/// task's `attribute_type2` names (none: 1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StopRate {
    /// The work rate.
    pub(crate) work_rate: WorkRate,
    /// The capacity.
    pub(crate) capacity: Amount,
    /// The productivity.
    pub(crate) productivity: Amount,
}

impl StopRate {
    /// The time per stop (`+0x70`): the capacity times the productivity over the work a second,
    /// shared out over the stops.
    #[must_use]
    pub fn time_per_stop(self, work_value: WorkValue, stops: FarmStops) -> Seconds {
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
    pub(crate) id: ObjectId,
    /// Its owner.
    pub(crate) player: PlayerId,
    /// The gatherer's master drops its loads there (its main building's master for an annex,
    /// `+0xbb8`; `FUN_14109d4f0`).
    pub(crate) drop_site: bool,
    /// Its place.
    pub(crate) position: Position,
    /// The footprint gap from the gatherer to it.
    pub(crate) gap: Distance,
}

/// The productivity the time per stop reads (the constructor's and the update's): the player's
/// attribute the task's `attribute_type2` names, as it is, or 1.0 for none (`DAT_14330eb8c`).
#[must_use]
pub fn productivity_of(
    attribute_kind2: Option<AttributeId>,
    attributes: &[Amount],
    gather_tuning: &GatherTuning,
) -> Amount {
    let Some(attribute) = attribute_kind2 else {
        return Amount::new(gather_tuning.default_productivity);
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
pub fn stop_point(
    position: Position,
    radius: Footprint,
    last: Point,
    rng: &mut Well512,
    gather_tuning: &GatherTuning,
) -> Point {
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
        x = ((rng.rand15() as i32) % ix + left) as f32 + gather_tuning.stop_centre;
        y = ((rng.rand15() as i32) % iy + bottom) as f32 + gather_tuning.stop_centre;
        draws += 1;
    }
    Point::new(x, y)
}

impl GatherAction {
    /// The constructor with a target (`FUN_140a88310`), the gather command's
    /// (`FUN_141085030`) and the factory's (`FUN_140a97e00` case 5): an empty Gather in state 0
    /// (the base constructor `FUN_140a73e90` with 1: no target, no sub-action) on `task`. It
    /// holds `target` (`FUN_140a7a230`) unless it is past [`LAST_TAKEN`]; the take of a held
    /// target (slot 0x178) is the caller's. Held or not, the target's place is the point and its
    /// class the target type (`+0x68`); a farm (class 49) or masters 357 and 1188 keep their
    /// place as the farm's (`+0x84`, else the allocation's 0). One stop, `mGatherX` and
    /// `mGatherY` at −1. A farm of master 50 or 1187 with the game's farm setting has four
    /// stops, each [`StopRate::time_per_stop`] long.
    ///
    /// The routine also takes a null target, which leaves the point, the farm's place and the
    /// target type (−1) unset, and a null task, which keeps one stop. No caller passes either:
    /// the gather command makes the Gather only on an object the world holds and a task slot
    /// 0x820 found, and the factory calls this constructor only with a target, its task the one
    /// it switches on (`+0xe`); without a target it calls `FUN_140a88260`.
    #[must_use]
    pub fn new(
        task: HuntTask,
        target: &Resource,
        farm_mode: FarmMode,
        rate: StopRate,
        roles: &Roles,
    ) -> Self {
        let farm = if roles.of_class(target.class).contains(ClassRoles::FARM)
            || roles
                .of_master(target.master)
                .contains(MasterRoles::DEAD_FARM_SITE)
        {
            target.position.ground()
        } else {
            Point::ORIGIN
        };
        let (stops, time_per_stop) = if farm_mode == FarmMode::Walkable
            && roles.of_class(target.class).contains(ClassRoles::FARM)
            && roles
                .of_master(target.master)
                .contains(MasterRoles::STOP_FARM)
        {
            (FARM_STOPS, rate.time_per_stop(task.work_value, FARM_STOPS))
        } else {
            (FarmStops::ONE, Seconds::ZERO)
        };
        Self {
            target: (target.state <= LAST_TAKEN).then_some(GatherTargetId::new(target.id)),
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
        }
    }

    /// The install's start (slot 5 with 1, `0x140a8b3a0`): the gatherer takes the task's load
    /// ([`load_for_task`] on `held`). A loaded gatherer sent to one of its own drop sites carries
    /// the load there: the target becomes the second target (the caller takes it, then releases
    /// it as the first), the point clears and `set_state(8)` walks there. Else `set_state(4)`
    /// walks to the target. A farm another farmer works (with this one, two or more) is walked
    /// to only as a point (`set_state(11)`, [`GatherAction::set_walk_back`]) and let go. A Gather
    /// whose constructor held no target (one past state 6) searches (`set_state(3)`), which the
    /// caller runs on the gatherer's step ([`GatherOpening::Searches`]).
    ///
    /// # Errors
    ///
    /// What the start does not model.
    pub fn start(
        &mut self,
        start: GatherStart<'_>,
        mover: &mut Mover,
        rng: &mut Well512,
        roles: &Roles,
        tuning: &Tuning,
    ) -> GenieResult<GatherOpening> {
        let move_to_tuning = &tuning.move_to;
        let GatherStart {
            target,
            player,
            player_kind,
            settings,
            held,
            walks,
        } = start;
        *held = load_for_task(*held, self.task.resource, settings, player_kind);
        // the farm's test and the walks read the target: without one, set_state(3)
        if self.target.is_none() {
            return Ok(GatherOpening::Searches);
        }
        let walks = Walks {
            held: *held,
            ..walks
        };
        // a farm another farmer works (`+0x644` above 1 with this one, which the constructor
        // took): set_state(11) to its point, then the farm goes (slot 0x68 with none; the caller
        // lets it go)
        if roles
            .of_some_class(self.target_class)
            .contains(ClassRoles::FARM)
            && WorkerCount::ONE < target.workers.plus(1)
        {
            let radius = walk_back_radius(target.master, target.radius, roles);
            self.set_walk_back(radius, mover, &walks, move_to_tuning)?;
            self.target = None;
            return Ok(GatherOpening::Approaches);
        }
        if target.player == player && target.drop_site && Amount::ZERO < held.amount {
            self.target = None;
            self.point = Position::from([-1.0; 3]);
            self.carry_to = Some(DropSiteId::new(target.id));
            self.set_drop(target.position, target.gap, mover, &walks, move_to_tuning)?;
            return Ok(GatherOpening::Approaches);
        }
        self.set_walk(target.walk_to(), target.radius, mover, rng, &walks, tuning)?;
        Ok(GatherOpening::Approaches)
    }

    /// Slot 0x48's test of its present target (`0x140a90496`): whether the Gather keeps `target`
    /// as it is, its target (`+0x20`) in state 6, 7 or 15, or in state 4 or 11 for a computer
    /// player (`player_kind`).
    #[must_use]
    pub fn keeps(&self, target: ObjectId, player_kind: PlayerKind) -> bool {
        self.target.map(GatherTargetId::id) == Some(target)
            && match self.state {
                GatherState::Face | GatherState::Work | GatherState::Moved { .. } => true,
                GatherState::Walk { .. } | GatherState::WalkBack { .. } => {
                    player_kind.is_computer()
                }
                _ => false,
            }
    }

    /// Slot 0x48 (`0x140a903a0`) on a new `target` that its tests let through (the caller's:
    /// none and an Unpack's building walk to a point, a target it keeps stays, a building other
    /// than a farm is refused, its player's foundation is built). A loaded gatherer (`held`) sent
    /// to one of its player's drop sites carries its load there: the timer clears, the target goes
    /// (slot 0x68 with none) with the point, the drop site becomes the second target
    /// (`FUN_140a7a190`) and `set_state(8)` walks there. Any other target becomes the target
    /// (slot 0x68: none past [`LAST_TAKEN`]) with its place as the point and its class as the
    /// target type, the second target goes and `set_state(4)` walks to it; the caller takes and
    /// lets go the targets that changed. With the game's farm setting a fish trap (master 199)
    /// keeps its place as the farm's, and a stop farm (masters 50, 1187) too, with its four stops
    /// from the first and the time per stop of `rate`, its time at the stop cleared. A farm
    /// another farmer works (with this one, two or more) is walked to as a point
    /// (`set_state(11)`) and let go again (slot 0x68 with none, then `FUN_140a7a190` with none):
    /// the caller takes it and lets it go.
    ///
    /// # Errors
    ///
    /// What it does not model; a unit that cannot move.
    pub fn retarget(
        &mut self,
        retarget: GatherRetarget<'_>,
        mover: &mut Mover,
        rng: &mut Well512,
        roles: &Roles,
        tuning: &Tuning,
    ) -> GenieResult<()> {
        let move_to_tuning = &tuning.move_to;
        let GatherRetarget {
            target,
            player,
            walks,
            farm_mode,
            rate,
        } = retarget;
        let held = walks.held.amount;
        // COMISS and JBE at 0x140a907f9: a NaN load takes the target
        if target.player == player && target.drop_site && Amount::ZERO < held {
            self.timer = Seconds::ZERO;
            self.target = None;
            self.point = Position::from([-1.0; 3]);
            self.carry_to = Some(DropSiteId::new(target.id));
            self.set_drop(target.position, target.gap, mover, &walks, move_to_tuning)?;
            return Ok(());
        }
        // slot 0x68 takes the target, none past state 6: its workers count it once it is new
        let taken = (target.state <= LAST_TAKEN).then_some(GatherTargetId::new(target.id));
        let workers = target.workers.plus(i32::from(taken != self.target));
        self.target = taken;
        self.point = target.position;
        self.target_class = target.class.or_none();
        if farm_mode == FarmMode::Walkable {
            if roles
                .of_master(target.master)
                .contains(MasterRoles::STOP_FARM)
            {
                self.farm = target.position.ground();
                self.stops = FARM_STOPS;
                self.current_stop = StopIndex::FIRST;
                self.time_per_stop = rate.time_per_stop(self.task.work_value, FARM_STOPS);
                self.time_at_stop = Seconds::ZERO;
            } else if roles
                .of_master(target.master)
                .contains(MasterRoles::FISH_TRAP)
            {
                self.farm = target.position.ground();
            }
        }
        if roles.of_class(target.class).contains(ClassRoles::FARM)
            && taken.is_some()
            && WorkerCount::ONE < workers
        {
            let radius = walk_back_radius(target.master, target.radius, roles);
            self.set_walk_back(radius, mover, &walks, move_to_tuning)?;
            self.target = None;
            self.carry_to = None;
            return Ok(());
        }
        self.carry_to = None;
        self.set_walk(target.walk_to(), target.radius, mover, rng, &walks, tuning)?;
        Ok(())
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
        tuning: &Tuning,
    ) -> GenieResult<()> {
        let (gather_tuning, move_to_tuning) = (&tuning.gather, &tuning.move_to);
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
                let point = stop_point(position, radius, self.last_stop, rng, gather_tuning);
                (point.x(), point.y())
            };
            self.last_stop = Point::new(x, y);
            let to = Position::new(x, y, position.z());
            self.move_to_point(
                to,
                self.task.range,
                TargetMoved::AsksAgain,
                mover,
                walks,
                move_to_tuning,
            )?
        } else {
            self.move_to_object(to, mover, walks, move_to_tuning)?
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
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
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
        let move_to = self.move_to_object(to, mover, walks, move_to_tuning)?;
        self.state = GatherState::Drop { move_to };
        Ok(())
    }

    /// Slot 0x40 (`0x140a8b740`) on a move to `point`, `target` the object moved to, if any: a
    /// loaded gatherer (`walks`) moved to one of its player's drop sites carries its load there
    /// (nothing changes when that is already the drop site it walks to in state 8): the target
    /// goes (slot 0x68 with none) with the point, the site becomes the second target
    /// (`FUN_140a7a190`) and `set_state(8)` walks there. Any other move becomes the point: the
    /// target goes and `set_state(15)` walks there ([`GatherAction::walk_to_moved_point`]).
    /// Always taken (it returns 1); the caller takes and lets go the targets that changed.
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
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        let held = walks.held.amount;
        match target.filter(|move_target| {
            move_target.player == player && move_target.drop_site && Amount::ZERO < held
        }) {
            Some(target)
                if Some(target.id) == self.carry_to.map(DropSiteId::id)
                    && matches!(self.state, GatherState::Drop { .. }) =>
            {
                Ok(())
            }
            Some(move_target) => {
                self.target = None;
                self.point = Position::from([-1.0; 3]);
                self.carry_to = Some(DropSiteId::new(move_target.id));
                self.set_drop(
                    move_target.position,
                    move_target.gap,
                    mover,
                    walks,
                    move_to_tuning,
                )
            }
            None => self.walk_to_moved_point(point, mover, walks, move_to_tuning),
        }
    }

    /// The Gather walks to `point` without a target: the point becomes `point`, the target goes
    /// (slot 0x68 with none; the caller lets it go) and `set_state(15)` walks there
    /// ([`GatherAction::set_moved`]). Slot 0x40 does so on a move to no drop site, slot 0x48
    /// (`0x140a90440`) without a target or on a building under its Unpack.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn walk_to_moved_point(
        &mut self,
        point: Position,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.point = point;
        self.target = None;
        self.set_moved(mover, walks, move_to_tuning)
    }

    /// `set_state(15)` (`0x140a8cd40` case 0xf, as 11): the sub-actions go, the timer clears and
    /// a quiet `MoveTo` to the point within the target's radius plus the task's range starts
    /// ([`GatherAction::move_to_point`]); with no target (a move lets it go) the radius is 0.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn set_moved(
        &mut self,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.tear_down(Some(mover))?;
        self.state = GatherState::Moved { move_to: None };
        self.timer = Seconds::ZERO;
        if self.target.is_some() {
            return Err(GenieError::unsupported(
                "a gather's walk to a point with a target (its radius)",
            ));
        }
        let range = self.task.range;
        let move_to = self.move_to_point(
            self.point,
            range,
            TargetMoved::Follows,
            mover,
            walks,
            move_to_tuning,
        )?;
        self.state = GatherState::Moved { move_to };
        Ok(())
    }

    /// `set_state(2)` (case 2): the timer clears, the unit stands ([`GatherAction::stand_sprite`]),
    /// the target and the second target go (slot 0x68, `FUN_140a7a190` with none; the caller
    /// lets them go) and the point is −1.
    pub(super) fn set_idle(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
    ) {
        self.state = GatherState::Idle;
        self.timer = Seconds::ZERO;
        Self::show(step, unit, self.stand_sprite(unit, gather_reads));
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
        stand: Stand<'_>,
    ) -> GenieResult<[Option<ObjectId>; 2]> {
        let mut mover = mover;
        self.tear_down(mover.as_deref_mut())?;
        self.state = GatherState::Idle;
        self.timer = Seconds::ZERO;
        stand.show(mover.map(|mover| &mut mover.stats));
        let released = self.targets();
        self.target = None;
        self.carry_to = None;
        self.point = Position::from([-1.0; 3]);
        Ok(released)
    }
}
