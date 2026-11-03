//! The orders, actions, timers and reaches the unit AI names, its factory types and stances.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::math::image::unit_ai::{INVALID_POINT as UNIT_AI_INVALID_POINT, RUN_ONE};
use crate::math::rng::Well512;
use crate::objects::ActionState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObjectFlags;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::Posture;
use crate::unit_ai::OrderForce;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    ARTIFACT_AI, AiAction, AiMove, BOAR_AI, CIVILIAN, FISHING_AI, PRIEST_AI, QueuedOrder, SIEGE_AI,
    SIEGE_TOWER_AI, SOLDIER_AI, Surroundings, TOWER_AI, TRADE_AI,
};
use crate::world::Millis;
use crate::world::WorldTime;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

/// What slot 73 does for a factory type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Retarget {
    /// The base slot 73: the retarget check.
    Base,
    /// An override that never reaches the check: livestock (19, `0x140c21f20`: 9).
    Never,
    /// The villagers' override (8, `0x140c21da0`, [`UnitAi::civilian_misc`]), which never
    /// reaches the check either.
    ///
    /// [`UnitAi::civilian_misc`]: crate::unit_ai::UnitAi::civilian_misc
    Civilian,
    /// The artifact's override (14, `FUN_140c21d40`, [`UnitAi::artifact_misc`]): its
    /// processIdle, no check.
    ///
    /// [`UnitAi::artifact_misc`]: crate::unit_ai::UnitAi::artifact_misc
    Artifact,
    /// The priest's override (9, `FUN_140c21f30`): the base's check but on the order on a target
    /// (`0x2db`) and for a computer player's priest, which return 9.
    Priest,
}

pub(super) const fn retarget_kind(ai_kind: UnitAiKind) -> Result<Retarget, GenieError> {
    match ai_kind {
        // villagers (0x140c21da0) and livestock (0x140c21f20) return 9 from slot 73
        UnitAiKind::CIVILIAN => Ok(Retarget::Civilian),
        UnitAiKind::LIVESTOCK => Ok(Retarget::Never),
        ARTIFACT_AI => Ok(Retarget::Artifact),
        PRIEST_AI => Ok(Retarget::Priest),
        // hunted animal (2, slot 72 FUN_140c1eea0), boar (4, slot 72 FUN_140c1e630), lion (5,
        // slot 72 FUN_140c1fb70), trade ship (0xa, slot 72 FUN_140c21930), transport ship (11,
        // slot 72 0x140c21d20), fishing ship (0xc, slot 72 FUN_140c1eab0), soldier (13, slot 72
        // FUN_140c20db0), tower (15, the base's slot 72 FUN_140c564c0), building (17, slot 72
        // 0x140c1e600, which does nothing), siege unit (0x10, slot 72 FUN_140c206b0), siege tower
        // (0x12, slot 72 FUN_140c20690)
        UnitAiKind::HUNTED
        | UnitAiKind::BOAR
        | UnitAiKind::LION
        | TRADE_AI
        | UnitAiKind::TRANSPORT
        | FISHING_AI
        | UnitAiKind::SOLDIER
        | UnitAiKind::TOWER
        | SIEGE_AI
        | UnitAiKind::BUILDING
        | SIEGE_TOWER_AI => Ok(Retarget::Base),
        _ => Err(GenieError::unsupported("unit-AI type not modelled")),
    }
}

/// The base idle timeout (`+0x168`) the constructor sets; the adjusted one is drawn around it.
pub(super) const IDLE_TIMEOUT: Millis = Millis::new(3000);
/// The tower's idle timeout (`FUN_140c12960`'s `FUN_140c41bc0` with 1000).
pub(super) const TOWER_IDLE_TIMEOUT: Millis = Millis::new(1000);
/// The artifact's idle timeout (`FUN_140c11340`'s `FUN_140c41bc0` with 5000).
pub(super) const ARTIFACT_IDLE_TIMEOUT: Millis = Millis::new(5000);
/// The desired target distance (`+0x5c`) the constructor sets.
pub(super) const DESIRED_DISTANCE: Distance = Distance::new(2.0);

/// The boar's idle check (`FUN_140c1e630`): it stays within a draw of `0..81` plus `DAT_142eb5bbc`
/// (144) of its origin, squared; within `DAT_142eb0f48` (4) it wanders `-3..=2` tiles each way
/// (`(r % 6) - 3`), once in 7 checks; its moves come within `DAT_142e7f384` (0.5).
pub(super) const BOAR_LEASH_DRAW: u32 = 81;
pub(super) const BOAR_LEASH: f32 = 144.0;
pub(super) const BOAR_HOME: f32 = 4.0;
pub(super) const BOAR_WANDER: u32 = 7;
pub(super) const BOAR_STEP: u32 = 6;
pub(super) const BOAR_OFFSET: i32 = 3;
pub(super) const AI_MOVE_RANGE: Distance = Distance::new(0.5);

/// A villager's notify, or a part of one, the slice does not model.
pub(super) const VILLAGER_NOTIFY: GenieError =
    GenieError::unsupported("a villager's notify the slice does not model");

/// A villager's damage notify that reads its animation speed (slot 0x1f8) while the slice does
/// not know it ([`Surroundings::anim_speed`]).
pub(super) const NO_ANIM_SPEED: GenieError =
    GenieError::unsupported("a moving villager's damage notify without its animation speed");
/// The target point's x that marks it unset (`DAT_14330ec20`, −1), which the defend order fills.
pub(super) const INVALID_POINT: f32 = UNIT_AI_INVALID_POINT;

/// An order on an object run without the unit AI's update (its surroundings and side), as
/// [`UnitAi::take_order_now`] runs one.
///
/// [`UnitAi::take_order_now`]: crate::unit_ai::UnitAi::take_order_now
pub(super) const OUTSIDE_ORDER: GenieError =
    GenieError::unsupported("an order on an object run outside the unit AI's update");

/// The move an order to a point gives ([`UnitAi::process_order`]'s move and attack move): the
/// order's point and range, forced and walking.
///
/// [`UnitAi::process_order`]: crate::unit_ai::UnitAi::process_order
pub(super) const fn order_move(o: &QueuedOrder) -> AiMove {
    AiMove {
        point: o.point,
        range: o.range,
        force: OrderForce::Forced,
        walk: true,
    }
}

/// The hunted animal's idle check (`FUN_140c1eea0`): away from its origin by more than
/// `DAT_142eb0f48` (4, squared) it goes home once in 7 checks; otherwise it wanders 1 to 3 tiles
/// each way (`HUNTED_STEP`), in mode 1 once in 28 and by a tile. Its search reaches its line of
/// sight plus `DAT_14330eb8c` (1), or its search radius if that is as far, plus `+0x1e8` (unsaved,
/// 0 in every unit probed).
pub(super) const HUNTED_HOME: f32 = 4.0;
pub(super) const HUNTED_STEP: i32 = 3;
pub(super) const HUNTED_SIGHT_GROWTH: Distance = Distance::new(1.0);
/// What an idle check lowers the unit's last human order time to (`FUN_140fe0d10`), and below
/// which the moving object's update counts it down.
pub const HUMAN_ORDER_CAP: WorldTime = WorldTime::from_millis(5000);

/// The lion's idle timeout (`FUN_140c1fb70`, as its constructor `FUN_140c11bd0`).
pub(super) const LION_IDLE_TIMEOUT: Millis = Millis::new(6000);
/// The lion's idle timeout once its attack completed (`FUN_140c266f0`).
pub(super) const LION_ATTACKED_TIMEOUT: Millis = Millis::new(1000);

/// The walk back's gap and range (`FUN_140c5e7d0`: `DAT_14330eb8c`, 1.0, whose image role in
/// the unit AI's table is [`RUN_ONE`]'s).
///
/// [`RUN_ONE`]: crate::unit_ai::RUN_ONE
pub(super) const WALK_BACK: f32 = RUN_ONE;

/// The action values the full update resets once the unit's actions ended (`0x140c66b10`, the
/// unit's slot 0x430 false).
pub(super) const ENDED_ACTIONS: [AiAction; 9] = [
    AiAction::Attack,
    AiAction::Gather,
    AiAction::Move,
    AiAction::Hunt,
    AiAction::Enter,
    AiAction::Make,
    AiAction::Research,
    AiAction::BoarLuring,
    AiAction::MoveAside,
];

/// What the retarget's search reads of its checks ([`UnitAi::retarget_outcome`]): the current
/// target is a building's, wall's or trebuchet's (or one off the attack order), and the top
/// action is in an attack's work state (6 or 7).
///
/// [`UnitAi::retarget_outcome`]: crate::unit_ai::UnitAi::retarget_outcome
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RetargetSearch {
    pub(super) building: bool,
    pub(super) work_state: bool,
}

/// The unit AI type the factory (`FUN_141215a90`) gives a new object of master `master`, by
/// the master's id `id` ([`factory_kind`]).
#[must_use]
pub fn unit_ai_kind(master: &Master, id: MasterId, roles: &Roles) -> Option<UnitAiKind> {
    factory_kind(
        master.class,
        id,
        master.creatable_part().volley_missile,
        master.combat_level,
        roles,
    )
}

/// The unit-AI type the factory (`FUN_141215a90`) gives a new object of a master of `class`
/// with the volley missile `missile` (`+0x8d4`, −1 for none) and the combat level
/// `combat_level` (`+0x62c`). The types are those `FUN_141215560`'s switch names for its
/// constructors: masters 1105 (`FUN_140c11fb0`, 0x12) and 1744, 1746 (`FUN_140c12040`, 0x10) by
/// their id; then none for classes 5, 7, 8, 11, 14, 15, 16, 30 to 34, 38, 42, 48 and 63; 0xe for
/// class 1; 0xa for 2 and 19; the building's (0x11) for classes 3, 27, 39, 49 and 60 without a
/// volley missile, the tower's (0xf) for those with one and for 52; the civilian's (8) for 4;
/// the hunted animal's (2) for 9 and 59; 0x10 for 13, 51, 54 and 55; 9 for 18, 43 and 53; 0xb for
/// 20; 0xc for 21; 0x13 for 29 and 58; the soldier's (0xd) for any other. Class 10 takes the
/// boar's (4, `FUN_140c11720`) when its master is of class 10 and combat level 1
/// (`FUN_140fde4f0`), else the lion's (5, `FUN_140c11bd0`).
#[must_use]
pub fn factory_kind(
    class: ClassId,
    master: MasterId,
    missile: Option<MasterId>,
    combat_level: CombatLevel,
    roles: &Roles,
) -> Option<UnitAiKind> {
    {
        let held = roles.of_master(master);
        if held.contains(MasterRoles::SIEGE_TOWER) {
            return Some(UnitAiKind::SIEGE_TOWER);
        } else if held.intersects(MasterRoles::ARMORED_ELEPHANT | MasterRoles::SIEGE_ELEPHANT) {
            return Some(UnitAiKind::SIEGE);
        }
    }
    Some({
        let held = roles.of_class(class);
        if held.intersects(
            ClassRoles::OCEAN_FISH
                | ClassRoles::FORAGE_BUSH
                | ClassRoles::STONE_MINE
                | ClassRoles::MISCELLANEOUS
                | ClassRoles::TERRAIN
                | ClassRoles::TREE
                | ClassRoles::TREE_STUMP
                | ClassRoles::FLAG
                | ClassRoles::DEEP_SEA_FISH
                | ClassRoles::GOLD_MINE
                | ClassRoles::SHORE_FISH
                | ClassRoles::CLIFF
                | ClassRoles::BIRD
                | ClassRoles::RELIC
                | ClassRoles::ORE_MINE
                | ClassRoles::GOLD_FISH,
        ) {
            return None;
        } else if held.contains(ClassRoles::ARTIFACT) {
            UnitAiKind::ARTIFACT
        } else if held.intersects(ClassRoles::TRADE_BOAT | ClassRoles::TRADE_CART) {
            UnitAiKind::TRADE
        } else if held.intersects(
            ClassRoles::BUILDING
                | ClassRoles::WALL
                | ClassRoles::GATE
                | ClassRoles::FARM
                | ClassRoles::MISC_BUILDING,
        ) && (missile.is_none())
        {
            UnitAiKind::BUILDING
        } else if held.intersects(
            ClassRoles::BUILDING
                | ClassRoles::WALL
                | ClassRoles::GATE
                | ClassRoles::FARM
                | ClassRoles::TOWER
                | ClassRoles::MISC_BUILDING,
        ) {
            TOWER_AI
        } else if held.contains(ClassRoles::VILLAGER) {
            CIVILIAN
        } else if held.intersects(ClassRoles::PREY | ClassRoles::KING) {
            UnitAiKind::HUNTED
        } else if held.contains(ClassRoles::PREDATOR)
            && (combat_level.get() == CombatLevel::ANIMAL.get())
        {
            BOAR_AI
        } else if held.contains(ClassRoles::PREDATOR) {
            UnitAiKind::LION
        } else if held.intersects(
            ClassRoles::SIEGE
                | ClassRoles::PACKED_TREBUCHET
                | ClassRoles::UNPACKED_TREBUCHET
                | ClassRoles::SCORPION,
        ) {
            UnitAiKind::SIEGE
        } else if held
            .intersects(ClassRoles::MONK | ClassRoles::MONK_WITH_RELIC | ClassRoles::BOARDING_SHIP)
        {
            UnitAiKind::PRIEST
        } else if held.contains(ClassRoles::TRANSPORT) {
            UnitAiKind::TRANSPORT
        } else if held.contains(ClassRoles::FISHING_SHIP) {
            UnitAiKind::FISHING
        } else if held.intersects(ClassRoles::DOMESTIC_ANIMAL | ClassRoles::LIVESTOCK) {
            UnitAiKind::LIVESTOCK
        } else {
            SOLDIER_AI
        }
    })
}

/// What a unit AI's constructor reads besides its type (`FUN_141215a90`'s and
/// `FUN_141215560`'s constructors): the new object's place, its master's fields and its
/// owner's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NewObject {
    /// The object's place (`+0x3c8`, `+0x4c8`, `+0x5a8`): the state position, the hunted
    /// animal's and the boar's origin.
    pub position: Position,
    /// Its master's id (`+0x58`).
    pub master: MasterId,
    /// Its master's `copy_id` (`+0x158`).
    pub copy_id: Option<MasterId>,
    /// Its master's class (`+0x410`).
    pub class: ClassId,
    /// Its master's `object_flags` (`+0x654`).
    pub object_flags: ObjectFlags,
    /// Its master's `hero_flag` (`+0x8d9`).
    pub hero_flag: HeroFlags,
    /// The object's slot 0x7b0 (`+0xb74`, which the action object's constructor set before
    /// through slot 0x8a8, `FUN_141206d50`: the work range of its master's last task of action
    /// type 6, else 0): the hunted animal's leash.
    pub hunt_range: Distance,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub player_kind: PlayerKind,
    /// The owner's posture (`+0x71c`, its slot 0x90 `FUN_1416224f0`).
    pub posture: Posture,
    /// The owner's attribute 6, the current age (`player+0x100` at `+0x30`).
    pub age: Amount,
}

impl NewObject {
    /// The object at `position` of master `made` (id `master`), its owner's fields left at a
    /// human player's in the Dark Age with posture 0 ([`NewObject::player_kind`],
    /// [`NewObject::posture`], [`NewObject::age`] are the caller's).
    #[must_use]
    pub fn of(position: Position, master: MasterId, made: &Master) -> Self {
        Self {
            position,
            master,
            copy_id: made.copy_id,
            class: made.class,
            object_flags: made.object_flags,
            hero_flag: made.creatable_part().hero_flag,
            // FUN_141206d50: each task of action type 6 in turn sets `+0xb74`
            hunt_range: made
                .action_part()
                .tasks
                .iter()
                .rev()
                .find(|t| t.action_kind == ActionKind::HUNT_RANGE)
                .map_or(Distance::ZERO, |t| t.work_range),
            player_kind: PlayerKind::Human,
            posture: Posture::Aggressive,
            age: Amount::ZERO,
        }
    }
}

/// The hunted animal's idle timeout (`FUN_140c11920`'s `FUN_140c41bc0` with 4000).
pub(super) const HUNTED_IDLE_TIMEOUT: Millis = Millis::new(4000);
/// The priest's idle timeout (`FUN_140c11e30`'s `FUN_140c41bc0` with 500).
pub(super) const PRIEST_IDLE_TIMEOUT: Millis = Millis::new(500);
/// The soldier's idle timeout (`FUN_140c12300`'s `FUN_140c41bc0`): 500, a computer player's
/// 1500.
pub(super) const SOLDIER_IDLE_TIMEOUT: Millis = Millis::new(500);
pub(super) const COMPUTER_SOLDIER_IDLE_TIMEOUT: Millis = Millis::new(1500);
/// The siege unit's idle timeout (`FUN_140c12040`'s `FUN_140c41bc0` with 1000).
pub(super) const SIEGE_IDLE_TIMEOUT: Millis = Millis::new(1000);

/// The soldier's stance (`+0x1a9`) its constructor (`FUN_140c12300`) gives a new object: 2 (stand
/// ground) to a human player's object of a master with object flag 0x10 that is its own copy
/// (`copy_id`, `+0x158`, its id) while the player's current age (attribute 6) is 0; 1 (defensive)
/// to a human player's of masters 527, 528 and 1104; 3 (no attack) to a human player's of classes
/// 35 and 64; then 1 to a hero (its `hero_flag` grants [`HeroFlags::DEFENSIVE`]); 3 to class 61;
/// else the owner's posture (slot 0x90, `+0x71c`, its low byte).
pub(super) fn soldier_stance(new: &NewObject, roles: &Roles) -> UnitStance {
    // UCOMISS with JP, JNZ: the age equals 0 (a NaN does not)
    if !new.player_kind.is_computer()
        && new.object_flags.contains(ObjectFlags::SCOUT)
        && new.copy_id == Some(new.master)
        && new.age == Amount::ZERO
    {
        return UnitStance::StandGround;
    }
    if !new.player_kind.is_computer()
        && roles
            .of_master(new.master)
            .contains(MasterRoles::DEFENSIVE_SOLDIER)
    {
        return UnitStance::Defensive;
    }
    if !new.player_kind.is_computer() && roles.of_class(new.class).contains(ClassRoles::NO_ATTACK) {
        return UnitStance::NoAttack;
    }
    // TEST 9, then a signed byte at least 0 or bit 3 clear
    if new.hero_flag.grants(HeroFlags::DEFENSIVE) {
        return UnitStance::Defensive;
    }
    if roles
        .of_class(new.class)
        .contains(ClassRoles::NO_ATTACK_UNLESS_HERO)
    {
        return UnitStance::NoAttack;
    }
    new.posture.stance()
}

/// The siege unit's stance (`+0x1a9`) its constructor (`FUN_140c12040`) gives a new object: 0
/// for a computer player's and for classes 51 and 54 (`-0xe79`, `-0xe76` raw); the owner's
/// posture (slot 0x90, its low byte) for class 55 (`-0xe75`); 1 for any other.
pub(super) fn siege_stance(new: &NewObject, roles: &Roles) -> UnitStance {
    if new.player_kind.is_computer()
        || roles
            .of_class(new.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
    {
        return UnitStance::Aggressive;
    }
    if roles.of_class(new.class).contains(ClassRoles::SCORPION) {
        return new.posture.stance();
    }
    UnitStance::Defensive
}

/// A ship's draw on a missile's warning (`0x20f`, `processNotify`): `r * 100 / 0x7fff` below
/// its owner's `chance_to_dodge_missiles` ([`Surroundings::dodge_chance`]) dodges it.
pub(super) fn dodges(at: &Surroundings, ai: &mut Well512) -> bool {
    // IMUL by 100, then IDIV by 0x7fff: a 15-bit draw stays positive
    at.dodge_chance.passes(ai.rand15())
}

/// The owner's slot 0x2e8 with an alert of its unit's (0x201 under attack, 0x209 dead; 0x206
/// and 0x207 from a transport, units left inside or no unload there; `FUN_14162dab0`): a human
/// player's alert list or interface message and the old pather's hulled-area reset, not held;
/// a computer player's AI hears it (slot 0x3c8), which is not modelled.
pub(crate) const fn alert_owner(player_kind: PlayerKind) -> Result<(), GenieError> {
    if player_kind.is_computer() {
        return Err(GenieError::unsupported(
            "a computer player's AI hearing its unit's alert (slot 0x2e8, then slot 0x3c8)",
        ));
    }
    Ok(())
}

/// `AskForHelp`'s helper that passes every test ([`crate::unit_ai::world_view::UnitView::helper_free`]):
/// its reach to the attacker (`FUN_14104c220` within its weapon range), its own
/// `bestUnitToAttack` (`FUN_140c44680` from its place) and its `AttackObject` (slot 0xb8, any)
/// on the nearer of that and the attacker, which run as the helper's, are not modelled.
pub(crate) const fn helper_answers() -> Result<(), GenieError> {
    Err(GenieError::unsupported(
        "a helper that answers a call for help (its reach, bestUnitToAttack and AttackObject)",
    ))
}

/// The unit AI's slot 0x90 (`closestUndiscoveredTile`, `FUN_140c4bd40`) for an explorer other
/// than the auto scout, then its explore there (slot 0x100); not modelled.
pub(super) const fn closest_undiscovered_tile() -> Result<(), GenieError> {
    Err(GenieError::unsupported(
        "an explorer's search for the closest undiscovered tile (slot 0x90)",
    ))
}

/// The state of the unit's top action (slot 0x2b0) as [`Surroundings::action_state`] holds it,
/// for an action the slice holds.
pub(super) const fn held_action_state(
    state: Option<ActionState>,
) -> Result<ActionState, GenieError> {
    match state {
        Some(s) => Ok(s),
        None => Err(GenieError::unsupported(
            "an action the slice does not hold (its state, slot 0x2b0)",
        )),
    }
}

/// The classes of a target for which the retarget check of a unit on the attack order searches
/// again (3, 27, 39, 49, 52, 51 and 54).
pub(crate) fn searches_on_class(class: ClassId, roles: &Roles) -> bool {
    roles.of_class(class).intersects(
        ClassRoles::BUILDING
            | ClassRoles::WALL
            | ClassRoles::GATE
            | ClassRoles::FARM
            | ClassRoles::TOWER
            | ClassRoles::PACKED_TREBUCHET
            | ClassRoles::UNPACKED_TREBUCHET,
    )
}
