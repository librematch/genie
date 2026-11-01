//! What the unit AI asks of the world ([`UnitSide`]) and the commands it gives its unit.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::unit_ai::{
    Attacked, Duel, Effect, Found, Notify, PatrolPath, Search, Seeker, UnitAi, UnitSeen,
};

/// The caller's answer to a [`Search`].
/// What the unit AI asks of its unit and the world while it runs, each answered at once (the game
/// calls them from inside the AI): its searches over the objects-in-sight registry
/// ([`Search`]) and the unit's own commands. A closure answers the searches alone.
pub trait UnitSide {
    /// A search (the unit AI's `FUN_140c63e20` with its pick).
    ///
    /// # Errors
    ///
    /// What the search does not model.
    fn closest(&mut self, search: &Search<'_>) -> Result<Found, GenieError>;

    /// The unit's attack command on object `target` (slot 0x338, `FUN_14120cea0`), forced or
    /// not, while its unit AI stands as `ai`: true when the unit attacks it, with the notifies
    /// the command posts to the unit AI (kind and parameters), which the AI queues.
    ///
    /// # Errors
    ///
    /// What the command does not model.
    fn attack(
        &mut self,
        target: ObjectId,
        force: bool,
        ai: &UnitAi,
    ) -> Result<Attacked, GenieError>;

    /// The unit's `+0x685`, `underAttackValue`, becomes `value` (`FUN_14119d2d0`).
    fn set_under_attack(&mut self, value: bool);

    /// The unit's call its unit AI makes ([`Effect`]: its stop, a move, a command of its own), at
    /// once where the game's AI calls the unit's slot. The call runs on the unit holding its AI as
    /// it stands (`ai`, which the call reads and changes as the game's does) and with the AI
    /// stream (`rng`), from which the routines it runs draw as the game's do.
    ///
    /// # Errors
    ///
    /// A side without its unit, and what the call does not model.
    fn effect_now(
        &mut self,
        _e: Effect,
        _ai: &mut UnitAi,
        _rng: &mut Well512,
    ) -> Result<(), GenieError> {
        Err(WITHOUT_UNIT)
    }

    /// The unit's slot 0x430 as it stands now (`FUN_14106d0e0`; its slot 0x818, `FUN_14106d100`,
    /// is 0): its action list holds an action, the stops and commands its AI made in the update
    /// included ([`Surroundings::busy`] at the update's start).
    ///
    /// # Errors
    ///
    /// A side without its unit.
    ///
    /// [`Surroundings::busy`]: crate::unit_ai::Surroundings::busy
    fn busy_now(&mut self) -> Result<bool, GenieError> {
        Err(WITHOUT_UNIT)
    }

    /// The unit's slot 0x2d0 as it stands now: its top action may be interrupted
    /// ([`Surroundings::interruptible`] at the update's start).
    ///
    /// # Errors
    ///
    /// A side without its unit, and an action whose slot 0x2d0 the slice does not model.
    ///
    /// [`Surroundings::interruptible`]: crate::unit_ai::Surroundings::interruptible
    fn interruptible_now(&mut self) -> Result<bool, GenieError> {
        Err(WITHOUT_UNIT)
    }

    /// `FUN_141658e20` from the unit's place: the drop-site table's nearest building of the
    /// owner for what the unit carries (`+0x308`) from its class; None without one.
    ///
    /// # Errors
    ///
    /// A side without its unit.
    fn table_drop_site(&mut self) -> Result<Option<ObjectId>, GenieError> {
        Err(WITHOUT_UNIT)
    }

    /// The unit's build command's answer (slot 0x380, forced) on `foundation` before it runs:
    /// false for a foundation the world does not hold or no task for it (its 0).
    ///
    /// # Errors
    ///
    /// A side without its unit, and what the task search does not model.
    fn builds(&self, _foundation: ObjectId) -> Result<bool, GenieError> {
        Err(WITHOUT_UNIT)
    }

    /// `AttackObject`'s test of object `target` (`FUN_140c43720`): the world holds it and the unit
    /// AI's attack check (slot 0xa0, [`can_attack`], as `seeker` stands) passes; then its master's
    /// class and its place.
    ///
    /// # Errors
    ///
    /// What the attack check does not model.
    ///
    /// [`can_attack`]: crate::unit_ai::can_attack
    fn may_attack(
        &mut self,
        target: ObjectId,
        seeker: &Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError>;

    /// The explorer's point to explore (the base handler's case 0x202 for the auto scout): the
    /// search around its town centre (`FUN_140c65490`), else its unit AI's (`ai`) slot 0x90 with
    /// a draw of the AI stream (`rng`); None when neither finds one.
    ///
    /// # Errors
    ///
    /// What the searches do not model.
    fn seek_unexplored(
        &mut self,
        ai: &UnitAi,
        rng: &mut Well512,
    ) -> Result<Option<Tile>, GenieError>;

    /// The auto scout's last search (`FUN_140c4b130`, [`crate::movement::explore::beyond_sight`]), drawing
    /// from the AI stream (`rng`); None when it finds no point.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    fn seek_beyond_sight(&mut self, rng: &mut Well512) -> Result<Option<Tile>, GenieError>;

    /// The unit AI hands the notify it handled to its owner (slot 0x48, then the owner's slot
    /// 0x2e8, whose hear logic a computer player's AI runs). A human player's owner does nothing
    /// the simulation reads; a side of searches alone has no owner.
    ///
    /// # Errors
    ///
    /// What the AI's hear logic does not model.
    fn owner_hears(&mut self, _n: Notify) -> Result<(), GenieError> {
        Ok(())
    }

    /// The player's scout map, made when the auto scout's last search found nothing
    /// (`FUN_140c589c0`'s case 0x202).
    ///
    /// # Errors
    ///
    /// A unit without its player's sight.
    fn make_scout_map(&mut self) -> Result<(), GenieError>;

    /// The unit's slot 0x468 (`FUN_141157c50`, `getPatrolInformation`): a copy of its top path
    /// (`FUN_140f960f0`; `FUN_140f95c00` into a new path: its points, its length, its flags) and
    /// the waypoint of its top level (`+0x24`); None without one.
    fn patrol_information(&self) -> Option<(PatrolPath, i32)>;

    /// The builder's search for another foundation (`FUN_140c1c7c0`), None when it finds none.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    fn foundation_search(&mut self) -> Result<Option<ObjectId>, GenieError> {
        Err(GenieError::unsupported(
            "a builder's search without the world",
        ))
    }

    /// Slot 0x3d0: whether the unit reaches object `target` within `range`.
    ///
    /// # Errors
    ///
    /// What the reach test does not model.
    fn reaches(&mut self, _target: ObjectId, _range: Distance) -> Result<bool, GenieError> {
        Err(GenieError::unsupported("a reach test without the world"))
    }

    /// What the base handler's case 0x216 reads ([`OffFootprint`]) for a unit sent off the
    /// footprint of `site` (its master's radii `radius`).
    ///
    /// # Errors
    ///
    /// What the spot's search does not model.
    fn off_footprint(
        &mut self,
        _site: ObjectId,
        _radius: Footprint,
    ) -> Result<OffFootprint, GenieError> {
        Err(GenieError::unsupported(
            "a unit sent off a footprint without the world",
        ))
    }

    /// The unit's slot 0x278: whether it could stand at `point`.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn can_stand(&mut self, _point: Point) -> Result<bool, GenieError> {
        Err(GenieError::unsupported("a stand test without the world"))
    }

    /// `AskForHelp`'s walk (`FUN_140c42ce0`) of the lists its search left of the unit's player's
    /// own objects and its friends' (`DAT_143f69180`, the relation kinds 1 and 2, each in its
    /// order) for helpers against `attacker`: the unit itself, an object without a unit AI or
    /// owner, a dead one (state 3 and up), one of the classes 1 to 3, 18 to 21, 27 and 39
    /// (`0x80083c000e`), the siege tower master 1105 (`FUN_141194f10`), a villager (class 4)
    /// against an attacker that is not Gaia's, one of a player that does not count the unit's
    /// as an ally (`FUN_141663d90` with 1) and one farther than 3 tiles (the squared footprint
    /// gap, `FUN_14118f8c0`, above 9) do not help. These tests read nothing the helper's attack
    /// check (slot 0xa0) between them changes.
    ///
    /// # Errors
    ///
    /// A helper that passes them ([`helper_answers`]).
    ///
    /// [`helper_answers`]: crate::unit_ai::helper_answers
    fn call_for_help(&mut self, _attacker: ObjectId) -> Result<(), GenieError> {
        Err(GenieError::unsupported("a call for help without the world"))
    }

    /// The object `id` as the unit AI reads it ([`UnitSeen`]): None when the world does not
    /// hold it (`FUN_1415417a0`).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn seen_unit(&mut self, _id: ObjectId) -> Result<Option<UnitSeen>, GenieError> {
        Err(GenieError::unsupported(
            "an object's look without the world",
        ))
    }

    /// What a computer player's unit weighs of object `other` (its current target or its
    /// attacker) on a damage notify ([`Duel`], [`UnitAi::damaged`]).
    ///
    /// # Errors
    ///
    /// An object the side does not hold, and a side without the world.
    fn duel(&mut self, _other: ObjectId) -> Result<Duel, GenieError> {
        Err(GenieError::missing_data("a duel without the world"))
    }

    /// The villager's test of its attacker on a damage notify: the attacker shows too little
    /// attack, armor and range to run from, and is fought back as Gaia's are
    /// ([`UnitAi::villager_damaged`]).
    ///
    /// # Errors
    ///
    /// An object the side does not hold, and a side without the world.
    fn harmless(&mut self, _attacker: ObjectId) -> Result<bool, GenieError> {
        Err(GenieError::missing_data(
            "an attacker's shown values without the world",
        ))
    }

    /// The attacker of object `target` nearest to it (its unit AI's slot 0x78, `FUN_140c48610`:
    /// of its attackers in their order, those the world holds, the least footprint gap,
    /// `FUN_14118f680`, the first of a tie; [`NearestAttacker`], [`UnitAi::defend_misc`]).
    ///
    /// # Errors
    ///
    /// A side without the world.
    fn defended_attacker(&mut self, _target: ObjectId) -> Result<NearestAttacker, GenieError> {
        Err(GenieError::missing_data(
            "a defended unit's attackers without the world",
        ))
    }

    /// The unit AI's `ClosestResourceObject` (slot 0x80, `FUN_140c49760`) as `search` asks it:
    /// what it found, None for nothing.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    fn closest_resource(
        &mut self,
        _search: ResourceSearch,
    ) -> Result<Option<Gatherable>, GenieError> {
        Err(GenieError::unsupported(
            "a resource search without the world",
        ))
    }

    /// The class of master `master` of player `player` (`FUN_14152fe40`, the player's master
    /// table `+0x28`): None for a player or master the game does not hold.
    ///
    /// # Errors
    ///
    /// A side without the players' masters.
    fn master_class(
        &mut self,
        _player: PlayerId,
        _master: MasterId,
    ) -> Result<Option<ClassId>, GenieError> {
        Err(GenieError::unsupported(
            "a master's class without the world",
        ))
    }

    /// The unit's slot 0x388 (`FUN_141226fc0`) on object `target`: when the world holds it
    /// (`FUN_1415417a0`) and the unit's slot 0x820 finds a task on it (any action type, with the
    /// master switch), a new `TRIBE_Action_Wait_Solidify` on that task
    /// ([`crate::construction::wait_solidify`]) replaces the unit's actions (`FUN_140a98620`, slot 0x808);
    /// true then.
    ///
    /// # Errors
    ///
    /// What the task search does not model.
    fn wait_solidify(&mut self, _target: ObjectId) -> Result<bool, GenieError> {
        Err(GenieError::unsupported(
            "a wait for a foundation without the world",
        ))
    }

    /// Slot 400 (`FUN_14106d090`) as the full update's tail reads it: the target of the unit's
    /// top action as it stands now (`FUN_140a78450`: an object the world holds), the commands the
    /// unit took inside the update included (its attack command, [`UnitSide::attack`]); None
    /// without one. A side without a unit (the searches' closures, the tests' sides) has no
    /// action list.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn top_target_now(&mut self) -> Result<Option<ObjectId>, GenieError> {
        Ok(None)
    }

    /// What the full update's tail tests of object `id`, the target of its unit's top action
    /// ([`TargetFlags`]); None when the world does not hold it. A side without the world (the
    /// searches' closures, the tests' sides) holds no object.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn target_flags(&mut self, _id: ObjectId) -> Result<Option<TargetFlags>, GenieError> {
        Ok(None)
    }

    /// A command of the unit's that the unit AI's order slots give ([`UnitCommand`]): true when
    /// the unit takes it (the slot's 1).
    ///
    /// # Errors
    ///
    /// The commands the slice holds no action for.
    fn command(&mut self, _command: UnitCommand) -> Result<bool, GenieError> {
        Err(UNMODELLED_COMMAND)
    }

    /// The soldier's `AttackObject` (`FUN_140c17550`) after the attack, for a unit whose group
    /// the world holds: its future path data goes (slot 0x550, `FUN_14114d6a0`), and a unit
    /// that moves (slot 0x5c8) or speeds up (slot 0x5a0, its velocity data's forward
    /// acceleration above 0) stops (slot 0x5d0).
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    fn group_attack_stop(&mut self) -> Result<(), GenieError> {
        Err(GenieError::unsupported(
            "a soldier's attack in a group without its unit",
        ))
    }
}

/// A unit AI's call on its unit from a side that holds none (a search's closure).
pub(super) const WITHOUT_UNIT: GenieError =
    GenieError::unsupported("a unit AI's call on its unit from a side without it");

/// What a defend target's unit AI names as its nearest attacker ([`UnitSide::defended_attacker`]):
/// the defender attacks it, or with none runs its idle check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NearestAttacker {
    /// The world does not hold the target, or it has no unit AI: no slot 0x78 to ask.
    NoUnitAi,
    /// Its unit AI holds no attacker the world holds (slot 0x78's −1).
    NoAttacker,
    /// The attacker nearest to it.
    Attacker(ObjectId),
}

/// A unit command the slice holds no action for ([`UnitSide::command`]).
pub(crate) const UNMODELLED_COMMAND: GenieError = GenieError::unsupported(
    "a unit command the slice does not model (heal, conversion, enter, transport, unload)",
);

/// The unit's commands the unit AI's order slots give ([`UnitSide::command`]), each the unit's
/// slot of `TRIBE_Combat_Object`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnitCommand {
    /// The heal of an object (slot 0x108, `FUN_141214040`), forced.
    Heal(ObjectId),
    /// The conversion of an object (slot 0x370, `FUN_141210510`), forced.
    Convert(ObjectId),
    /// The repair of an object (slot 0x378, `FUN_141217c00`), forced.
    Repair(ObjectId),
    /// The enter into an object (slot 0x3a0), forced.
    Enter(ObjectId),
    /// The transport to a point (slot 0x3b8, `FUN_14108ce60`), forced.
    Transport(Position),
    /// The unload at a point at the unit's height (slot 0x3a8, `FUN_14108d050`), or over a wall
    /// (slot 0x3b0, `FUN_14108d560`), forced.
    Unload {
        /// The point.
        to: Position,
        /// Over a wall.
        over_wall: bool,
    },
}

/// What the full update's tail (`0x140c66b10` after the look-around check) tests of the target of
/// its unit's top action ([`UnitSide::target_flags`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetFlags {
    /// Its owner's id (`+0x138`, `player+0x718`); Gaia is 0.
    pub owner: PlayerId,
    /// Its type (`+0xf`).
    pub kind: MasterKind,
    /// `capture_flag` (`+0x924`, `FUN_14107cc90`).
    pub capture: u8,
    /// `+0x790`, `parent_pasture_id`: the pasture it is in.
    pub pasture: Option<ObjectId>,
    /// `+0x689`, `mUnitTargetingFlag`, is 0 (`FUN_141188bb0`).
    pub untargetable: bool,
    /// `+0x68a`, `unit_attackable_flag` (`FUN_141188b10`).
    pub attackable: bool,
}

/// What the unit AI asks `ClosestResourceObject` (slot 0x80, `FUN_140c49760`;
/// [`UnitSide::closest_resource`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceSearch {
    /// The gather group (`param_2`).
    pub kind: GatherGroup,
    /// Fewer workers than this (`param_4`; −1 any).
    pub cap: crate::objects::WorkerCount,
    /// The drop site the search starts from (`param_3`).
    pub site: Option<ObjectId>,
    /// The unit AI's current target (`+0x48`), which the search passes over.
    pub current: Option<ObjectId>,
    /// The point the search starts from without a site (`param_7`, `param_8`; (−1, −1) for the
    /// unit's place).
    pub point: Point,
    /// Below this state the unit finds nothing (`param_6`; its −1 is 0 here).
    pub min_state: crate::objects::ObjectState,
}

/// [`ResourceSearch::point`]'s (−1, −1): the unit's own place (`DAT_14330ec20`).
pub const NO_POINT: Point = Point::new(-1.0, -1.0);

/// What the unit AI's `ClosestResourceObject` found ([`UnitSide::closest_resource`]): the object,
/// its place and its master's class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gatherable {
    /// The object.
    pub id: ObjectId,
    /// Its place.
    pub position: Position,
    /// Its master's class, or the class the hunter's or gatherer's search took.
    pub class: ClassId,
}

/// An object as an order in the queue names it: its id, master, owner and the first two parts
/// of its slot 0x520 vector (its x and its height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Named {
    /// The object.
    pub id: ObjectId,
    /// Its master.
    pub master: MasterId,
    /// Its owner.
    pub owner: PlayerId,
    /// Its slot 0x520 vector's x and second part (the height), which the game's orders read as
    /// a point's x and y.
    pub vector: Point,
}

/// What a unit sent off a footprint finds (the base handler's case 0x216).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffFootprint {
    /// The closest spot beside the footprint (`FUN_1411566b0` with 0), None when there is none
    /// (or no foundation).
    pub spot: Option<Point>,
    /// The unit's height.
    pub z: f32,
    /// Its master's class.
    pub class: ClassId,
    /// Its owner is a computer player (`FUN_141659cf0`).
    pub computer: bool,
    /// Its owner's id.
    pub player: PlayerId,
    /// The foundation, while the world holds it.
    pub site: Option<Named>,
    /// The target of its current action (`FUN_140a986f0`, `FUN_140a78450`).
    pub action_target: Option<Named>,
}

impl<F: FnMut(&Search<'_>) -> Result<Found, GenieError>> UnitSide for F {
    fn closest(&mut self, search: &Search<'_>) -> Result<Found, GenieError> {
        self(search)
    }

    fn attack(&mut self, _: ObjectId, _: bool, _: &UnitAi) -> Result<Attacked, GenieError> {
        Err(GenieError::unsupported(
            "a unit AI's attack without its unit",
        ))
    }

    fn set_under_attack(&mut self, _: bool) {}

    fn may_attack(
        &mut self,
        _: ObjectId,
        _: &Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        Err(GenieError::unsupported("an attack test without the world"))
    }

    fn seek_unexplored(&mut self, _: &UnitAi, _: &mut Well512) -> Result<Option<Tile>, GenieError> {
        Err(GenieError::unsupported(
            "an explorer's search without its unit",
        ))
    }

    fn seek_beyond_sight(&mut self, _: &mut Well512) -> Result<Option<Tile>, GenieError> {
        Err(GenieError::unsupported(
            "an explorer's search without its unit",
        ))
    }

    fn make_scout_map(&mut self) -> Result<(), GenieError> {
        Err(GenieError::unsupported("a scout map without the world"))
    }

    /// A side of searches alone stands for no unit with movement state, whose slot 0x468 (the
    /// static object's) holds no path.
    fn patrol_information(&self) -> Option<(PatrolPath, i32)> {
        None
    }
}
