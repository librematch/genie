#![cfg(test)]
//! The tests' sides that record the unit's calls ([`UnitSide::effect_now`]): a list of them
//! standing for a side without a unit, and [`With`] around another test side.

use crate::GenieError;
use crate::construction::FoundationId;
use crate::gathering::DropSiteId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::rng::Well512;
use crate::movement::WaypointIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::SurroundingFlags;
use crate::unit_ai::{
    AiCommand, Attacked, Duel, Effect, Found, Gatherable, NearestAttacker, OffFootprint,
    PatrolPath, ResourceSearch, Search, Seeker, Surroundings, TargetFlags, UnitAi, UnitCommand,
    UnitSeen, UnitSide,
};

/// What calls leave of the unit's top action's target ([`top_after`]).
#[derive(Clone, Copy)]
enum TopLeft {
    /// None of them replaces the actions.
    Untouched,
    /// A stop, or an action without an object.
    NoObject,
    /// An action on the object.
    On(ObjectId),
}

/// What the calls `fx` leave of the unit's top action, the last that replaces its actions
/// deciding: a stop leaves none, a move to a point or an explore one without an object, a move
/// to an object, a hunt, gather, build, work or drop one on it.
fn top_after(fx: &[Effect]) -> TopLeft {
    fx.iter()
        .rev()
        .find_map(|e| match *e {
            Effect::Stop
            | Effect::Move(_)
            | Effect::Command(AiCommand::MoveAside(_))
            | Effect::Explore(_)
            | Effect::RestorePatrol => Some(TopLeft::NoObject),
            Effect::MoveToObject { target, .. }
            | Effect::Command(
                AiCommand::HuntObject(target)
                | AiCommand::GatherObject(target)
                | AiCommand::WorkOn { target, .. },
            ) => Some(TopLeft::On(target)),
            Effect::Command(AiCommand::BuildObject(target)) => Some(TopLeft::On(target.id())),
            Effect::Command(AiCommand::DropElsewhere { site, .. }) => Some(TopLeft::On(site.id())),
            Effect::CapHumanOrder(_) | Effect::OrderOnTarget(_) | Effect::ChangeOwner(_) => None,
        })
        .unwrap_or(TopLeft::Untouched)
}

/// Whether the calls `fx` leave the unit an action: the last that changes its actions decides
/// (a stop leaves none); None when none of them changes them.
fn busy_after(fx: &[Effect]) -> Option<bool> {
    fx.iter().rev().find_map(|e| match e {
        Effect::Stop => Some(false),
        Effect::CapHumanOrder(_) | Effect::OrderOnTarget(_) | Effect::ChangeOwner(_) => None,
        _ => Some(true),
    })
}

const NO_WORLD: GenieError = GenieError::unsupported("a test side without the world");

/// A side without a unit or the world that keeps the unit's calls in their order; the unit's
/// action list is as they leave it.
impl UnitSide for Vec<Effect> {
    fn closest(&mut self, _: &Search<'_>) -> Result<Found, GenieError> {
        Err(NO_WORLD)
    }

    fn attack(&mut self, _: ObjectId, _: OrderForce, _: &UnitAi) -> Result<Attacked, GenieError> {
        Err(NO_WORLD)
    }

    fn set_under_attack(&mut self, _: bool) {}

    fn effect_now(&mut self, e: Effect, _: &mut UnitAi, _: &mut Well512) -> Result<(), GenieError> {
        self.push(e);
        Ok(())
    }

    fn busy_now(&mut self) -> Result<bool, GenieError> {
        Ok(busy_after(self).unwrap_or(false))
    }

    fn interruptible_now(&mut self) -> Result<bool, GenieError> {
        Ok(true)
    }

    fn top_target_now(&mut self) -> Result<Option<ObjectId>, GenieError> {
        Ok(match top_after(self) {
            TopLeft::On(target) => Some(target),
            TopLeft::NoObject | TopLeft::Untouched => None,
        })
    }

    fn may_attack(
        &mut self,
        _: ObjectId,
        _: &Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        Err(NO_WORLD)
    }

    fn seek_unexplored(&mut self, _: &UnitAi, _: &mut Well512) -> Result<Option<Tile>, GenieError> {
        Err(NO_WORLD)
    }

    fn seek_beyond_sight(&mut self, _: &mut Well512) -> Result<Option<Tile>, GenieError> {
        Err(NO_WORLD)
    }

    fn make_scout_map(&mut self) -> Result<(), GenieError> {
        Err(NO_WORLD)
    }

    fn patrol_information(&self) -> Option<(PatrolPath, WaypointIndex)> {
        None
    }
}

/// A test side `side` whose unit's calls are kept in `fx`, in their order; the unit's action
/// list is as they leave it, else as a command `side` took (an attack, a unit command, a wait
/// for a foundation) leaves it, else as the update read it at its start (`busy`,
/// `interruptible`, the top action's target `top`).
pub(super) struct With<'s> {
    side: &'s mut dyn UnitSide,
    fx: &'s mut Vec<Effect>,
    busy: bool,
    interruptible: bool,
    top: Option<ObjectId>,
    commanded: bool,
}

impl<'s> With<'s> {
    /// `side` keeping the calls in `fx`, with a unit without an action.
    pub(super) fn new(side: &'s mut dyn UnitSide, fx: &'s mut Vec<Effect>) -> Self {
        Self {
            side,
            fx,
            busy: false,
            interruptible: false,
            top: None,
            commanded: false,
        }
    }

    /// [`With::new`] with a unit whose action list stands as `at` read it.
    pub(super) fn at(
        side: &'s mut dyn UnitSide,
        fx: &'s mut Vec<Effect>,
        at: &Surroundings,
    ) -> Self {
        Self {
            busy: at.flags.contains(SurroundingFlags::BUSY),
            interruptible: at.flags.contains(SurroundingFlags::INTERRUPTIBLE),
            top: at.top_target.map(|t| t.id),
            ..Self::new(side, fx)
        }
    }
}

impl UnitSide for With<'_> {
    fn closest(&mut self, search: &Search<'_>) -> Result<Found, GenieError> {
        self.side.closest(search)
    }

    fn attack(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        ai: &UnitAi,
    ) -> Result<Attacked, GenieError> {
        let attacked = self.side.attack(target, force, ai)?;
        self.commanded |= attacked.attacks;
        Ok(attacked)
    }

    fn set_under_attack(&mut self, value: bool) {
        self.side.set_under_attack(value);
    }

    fn effect_now(&mut self, e: Effect, _: &mut UnitAi, _: &mut Well512) -> Result<(), GenieError> {
        self.fx.push(e);
        Ok(())
    }

    fn busy_now(&mut self) -> Result<bool, GenieError> {
        Ok(busy_after(self.fx).unwrap_or(self.commanded || self.busy))
    }

    fn interruptible_now(&mut self) -> Result<bool, GenieError> {
        Ok(self.interruptible || self.fx.contains(&Effect::Stop))
    }

    fn table_drop_site(&mut self) -> Result<Option<DropSiteId>, GenieError> {
        self.side.table_drop_site()
    }

    fn builds(&self, foundation: FoundationId) -> Result<bool, GenieError> {
        self.side.builds(foundation)
    }

    fn may_attack(
        &mut self,
        target: ObjectId,
        seeker: &Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        self.side.may_attack(target, seeker)
    }

    fn seek_unexplored(
        &mut self,
        ai: &UnitAi,
        rng: &mut Well512,
    ) -> Result<Option<Tile>, GenieError> {
        self.side.seek_unexplored(ai, rng)
    }

    fn seek_beyond_sight(&mut self, rng: &mut Well512) -> Result<Option<Tile>, GenieError> {
        self.side.seek_beyond_sight(rng)
    }

    fn make_scout_map(&mut self) -> Result<(), GenieError> {
        self.side.make_scout_map()
    }

    fn patrol_information(&self) -> Option<(PatrolPath, WaypointIndex)> {
        self.side.patrol_information()
    }

    fn foundation_search(&mut self) -> Result<Option<ObjectId>, GenieError> {
        self.side.foundation_search()
    }

    fn reaches(&mut self, target: ObjectId, range: Distance) -> Result<bool, GenieError> {
        self.side.reaches(target, range)
    }

    fn off_footprint(
        &mut self,
        site: FoundationId,
        radius: Footprint,
    ) -> Result<OffFootprint, GenieError> {
        self.side.off_footprint(site, radius)
    }

    fn can_stand(&mut self, point: Point) -> Result<bool, GenieError> {
        self.side.can_stand(point)
    }

    fn call_for_help(&mut self, attacker: ObjectId) -> Result<(), GenieError> {
        self.side.call_for_help(attacker)
    }

    fn seen_unit(&mut self, id: ObjectId) -> Result<Option<UnitSeen>, GenieError> {
        self.side.seen_unit(id)
    }

    fn duel(&mut self, other: ObjectId) -> Result<Duel, GenieError> {
        self.side.duel(other)
    }

    fn harmless(&mut self, attacker: ObjectId) -> Result<bool, GenieError> {
        self.side.harmless(attacker)
    }

    fn defended_attacker(&mut self, target: ObjectId) -> Result<NearestAttacker, GenieError> {
        self.side.defended_attacker(target)
    }

    fn closest_resource(
        &mut self,
        search: ResourceSearch,
    ) -> Result<Option<Gatherable>, GenieError> {
        self.side.closest_resource(search)
    }

    fn master_class(
        &mut self,
        player: PlayerId,
        master: MasterId,
    ) -> Result<Option<ClassId>, GenieError> {
        self.side.master_class(player, master)
    }

    fn wait_solidify(&mut self, target: ObjectId) -> Result<Answer, GenieError> {
        let waits = self.side.wait_solidify(target)?;
        self.commanded |= waits == Answer::Taken;
        Ok(waits)
    }

    fn top_target_now(&mut self) -> Result<Option<ObjectId>, GenieError> {
        match top_after(self.fx) {
            TopLeft::On(target) => Ok(Some(target)),
            TopLeft::NoObject => Ok(None),
            TopLeft::Untouched => Ok(self.side.top_target_now()?.or(self.top)),
        }
    }

    fn target_flags(&mut self, id: ObjectId) -> Result<Option<TargetFlags>, GenieError> {
        self.side.target_flags(id)
    }

    fn command(&mut self, command: UnitCommand) -> Result<Answer, GenieError> {
        let takes = self.side.command(command)?;
        self.commanded |= takes == Answer::Taken;
        Ok(takes)
    }

    fn group_attack_stop(&mut self) -> Result<(), GenieError> {
        self.side.group_attack_stop()
    }
}
