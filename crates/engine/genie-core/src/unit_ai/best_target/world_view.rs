//! What `bestUnitToAttack` reads of the unit whose AI searches, of its arguments and of the
//! world ([`Battlefield`]).

use crate::map::Distance;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::unit_ai::best_target::Candidate;
use crate::unit_ai::best_target::OrderWeight;
use crate::unit_ai::best_target::{AttackCategory, ByCategory};
use crate::unit_ai::{AiAction, OrderKind};
use crate::world::WorldTime;
use crate::world::{Percent, TargetingMode};

/// What `bestUnitToAttack` reads of the unit whose AI searches, and of that AI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Searcher {
    /// The unit.
    pub id: ObjectId,
    /// Its unit AI's current target (`+0x48`).
    pub target: Option<ObjectId>,
    /// The AI's target's class (`+0x4c`) and its last target's (`+0x70`).
    pub target_classes: [Option<ClassId>; 2],
    /// The AI's order (`+0x3c`) and action (`+0x44`).
    pub order: Option<OrderKind>,
    /// The AI's action (`+0x44`).
    pub action: Option<AiAction>,
    /// The unit's master's id, class and type (`+0x5d7`, `+0x98f`, the master's `+0x8`).
    pub master: MasterId,
    /// Its master's class.
    pub class: Option<ClassId>,
    /// Its master's type (`'F'`, 70: the combat type).
    pub kind: MasterKind,
    /// Its master's `attack_priority` (`+0x92c`, `FUN_141202220`).
    pub attack_priority: AttackPriority,
    /// The owner's type (`player+0x10`; [`PlayerType::is_computer`] is `FUN_141659cf0`).
    pub player_type: PlayerType,
    /// Its owner may attack walls (`player+0x611`, the AI's slot 0x18's player).
    pub walls_allowed: bool,
    /// The AI's slot 0x1d0 with −1: the unit may move to attack.
    pub can_move: bool,
    /// `FUN_140c47f90`: the farthest it looks (a human player's twice its line of sight).
    pub sight: Distance,
    /// Its weapon range against its current target (slot 0x238 with it, or with none).
    pub range: Distance,
    /// Its weapon range (slot 0x230) and least range (slot 0x240).
    pub weapon_range: Distance,
    /// Its least range (slot 0x240).
    pub min_range: Distance,
    /// Its top speed (slot 0x208).
    pub speed: Speed,
    /// The game mode is a scenario's (`FUN_141302540`).
    pub scenario: bool,
    /// Its owner's targeting mode and factor (`player+0x614`, `+0x618`).
    pub targeting: (TargetingMode, Percent),
    /// The order history's weights ([`order_weights`]).
    ///
    /// [`order_weights`]: crate::unit_ai::best_target::statistics::order_weights
    pub weights: ByCategory<OrderWeight>,
    /// The AI's retarget entries' targets (`+0x1e0`).
    pub retries: Vec<ObjectId>,
    /// The world's time (`FUN_1415300d0`).
    pub now: WorldTime,
}

/// `bestUnitToAttack`'s arguments past its reference point (the search's, the caller's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    /// The reference distance (0: none).
    pub distance: Distance,
    /// An idle check: the current target narrows nothing and is no shortcut.
    pub idle: bool,
    /// The path checks: 1 on, 0 off (the shortcuts answer without a path).
    pub paths: u8,
    /// Extra objects for the statistics (`int*`, count).
    pub extra: Vec<ObjectId>,
    /// Any class: walls, gates, buildings, towers and packed units are not passed over.
    pub any_class: bool,
    /// The unit is in an attack work state: a current target on top answers at once.
    pub work_state: bool,
}

/// What `bestUnitToAttack` reads of an object of the world (`FUN_1415417a0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Thing {
    /// Its owner's id.
    pub owner: PlayerId,
    /// The unit's owner counts its owner an ally (`FUN_141663d90` with the ally flag).
    pub allied: bool,
    /// `+0x640`.
    pub state: ObjectState,
    /// It has a unit AI (`+0x6b0`).
    pub has_ai: bool,
    /// It is inside another (`+0x2f0`).
    pub inside: bool,
    /// Its master's id, class and type.
    pub master: MasterId,
    /// Its master's class.
    pub class: ClassId,
    /// Its master's type.
    pub kind: MasterKind,
    /// Its `+0x924` (`FUN_14107cc90`): others take it over by seeing it.
    pub capture: bool,
    /// Its targeting flag (`+0x689`, `FUN_141188bb0`).
    pub targetable: bool,
    /// Its attackable flag (`+0x68a`, `FUN_141188b10`).
    pub attackable: bool,
    /// What it holds (`+0x310`).
    pub held: Amount,
    /// The footprints' gap to the unit (`FUN_14118f680`).
    pub distance: Distance,
    /// Its top speed (slot 0x208).
    pub speed: Speed,
}

/// What `bestUnitToAttack` asks of the world while its unit AI runs.
pub trait Battlefield {
    /// The object `id` as the world holds it, None for one it does not.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn thing(&mut self, id: ObjectId) -> Result<Option<Thing>, crate::GenieError>;

    /// The unit AI's attack check (slot 0xa0 with 0) on object `id`.
    ///
    /// # Errors
    ///
    /// What the check does not model.
    fn can_attack(&mut self, id: ObjectId) -> Result<bool, crate::GenieError>;

    /// The unit AI's neutral test (slot 0xa8) of an object of `class`.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn attacks_neutral(&mut self, class: ClassId) -> Result<bool, crate::GenieError>;

    /// `FUN_14120e4e0` of a unit of the combat type on object `id`: it spares its friends.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn spares_friends(&mut self, id: ObjectId) -> Result<bool, crate::GenieError>;

    /// `FUN_140c41690`: a computer player's unit passes object `id` over.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn shuns(&mut self, id: ObjectId) -> Result<bool, crate::GenieError>;

    /// What the statistics read of object `id` at `distance` of `category`
    /// ([`Candidate`]).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    fn candidate(
        &mut self,
        id: ObjectId,
        distance: Distance,
        category: Option<AttackCategory>,
    ) -> Result<Candidate, crate::GenieError>;

    /// The shown armor of object `id` is at least 100 (slot 0x870's first).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn armored(&mut self, id: ObjectId) -> Result<bool, crate::GenieError>;

    /// Object `id`'s weapon range (slot 0x230).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn weapon_range(&mut self, id: ObjectId) -> Result<Distance, crate::GenieError>;

    /// The current target `id` engages: its attacks sum above 0 (slot 0x228) or its unit AI
    /// could work on the unit (slot 0x1b8 with 1).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    fn engages(&mut self, id: ObjectId) -> Result<bool, crate::GenieError>;

    /// The unit's quick path to object `id` within `range` (slot 0x3e0).
    ///
    /// # Errors
    ///
    /// What the path does not model.
    fn quick_path(&mut self, id: ObjectId, range: Distance) -> Result<bool, crate::GenieError>;

    /// The unit's path to object `id` within `range` (slot 0x3d0).
    ///
    /// # Errors
    ///
    /// What the path does not model.
    fn path(&mut self, id: ObjectId, range: Distance) -> Result<bool, crate::GenieError>;
}
