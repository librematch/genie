//! What `bestUnitToAttack` reads of the unit whose AI searches, of its arguments and of the
//! world ([`AttackField`]).

use crate::GenieResult;
use crate::map::Distance;
use crate::map::spatial::{Reach, Spatial};
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::best_target::Candidate;
use crate::unit_ai::best_target::OrderWeight;
use crate::unit_ai::best_target::{AttackCategory, ByCategory};
use crate::unit_ai::world_view::WorldPicker;
use crate::unit_ai::{AiAction, OrderKind};
use crate::world::WorldTime;
use crate::world::{Percent, TargetingMode};

/// What `bestUnitToAttack` reads of the unit whose AI searches, and of that AI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Searcher {
    /// The unit.
    pub(crate) id: ObjectId,
    /// Its unit AI's current target (`+0x48`).
    pub(crate) target: Option<ObjectId>,
    /// The AI's target's class (`+0x4c`) and its last target's (`+0x70`).
    pub(crate) target_classes: [Option<ClassId>; 2],
    /// The AI's order (`+0x3c`) and action (`+0x44`).
    pub(crate) order: Option<OrderKind>,
    /// The AI's action (`+0x44`).
    pub(crate) action: Option<AiAction>,
    /// The unit's master's id, class and type (`+0x5d7`, `+0x98f`, the master's `+0x8`).
    pub(crate) master: MasterId,
    /// Its master's class.
    pub(crate) class: Option<ClassId>,
    /// Its master's type (`'F'`, 70: the combat type).
    pub(crate) kind: MasterKind,
    /// Its master's `attack_priority` (`+0x92c`, `FUN_141202220`).
    pub(crate) attack_priority: AttackPriority,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
    /// Its owner may attack walls (`player+0x611`, the AI's slot 0x18's player).
    pub(crate) walls_allowed: bool,
    /// The AI's slot 0x1d0 with −1: the unit may move to attack.
    pub(crate) can_move: bool,
    /// `FUN_140c47f90`: the farthest it looks (a human player's twice its line of sight).
    pub(crate) sight: Distance,
    /// Its weapon range against its current target (slot 0x238 with it, or with none).
    pub(crate) range: Distance,
    /// Its weapon range (slot 0x230) and least range (slot 0x240).
    pub(crate) weapon_range: Distance,
    /// Its least range (slot 0x240).
    pub(crate) min_range: Distance,
    /// Its top speed (slot 0x208).
    pub(crate) speed: Speed,
    /// The game mode is a scenario's (`FUN_141302540`).
    pub(crate) scenario: bool,
    /// Its owner's targeting mode and factor (`player+0x614`, `+0x618`).
    pub(crate) targeting: (TargetingMode, Percent),
    /// The order history's weights ([`order_weights`]).
    ///
    /// [`order_weights`]: crate::unit_ai::best_target::statistics::order_weights
    pub(crate) weights: ByCategory<OrderWeight>,
    /// The AI's retarget entries' targets (`+0x1e0`).
    pub(crate) retries: Vec<ObjectId>,
    /// The world's time (`FUN_1415300d0`).
    pub(crate) now: WorldTime,
}

/// `bestUnitToAttack`'s arguments past its reference point (the search's, the caller's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    /// The reference distance (0: none).
    pub(crate) distance: Distance,
    /// An idle check: the current target narrows nothing and is no shortcut.
    pub(crate) idle: bool,
    /// The path checks: 1 on, 0 off (the shortcuts answer without a path).
    pub(crate) paths: u8,
    /// Extra objects for the statistics (`int*`, count).
    pub(crate) extra: Vec<ObjectId>,
    /// Any class: walls, gates, buildings, towers and packed units are not passed over.
    pub(crate) any_class: bool,
    /// The unit is in an attack work state: a current target on top answers at once.
    pub(crate) work_state: bool,
}

bitflags::bitflags! {
    /// What `bestUnitToAttack` reads of an object as yes or no ([`FoundObject`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FoundFlags: u8 {
        /// The unit's owner counts its owner an ally (`FUN_141663d90` with the ally flag).
        const ALLIED = 1;
        /// It has a unit AI (`+0x6b0`).
        const HAS_AI = 1 << 1;
        /// It is inside another (`+0x2f0`).
        const INSIDE = 1 << 2;
        /// Its targeting flag (`+0x689`, `FUN_141188bb0`).
        const TARGETABLE = 1 << 3;
        /// Its attackable flag (`+0x68a`, `FUN_141188b10`).
        const ATTACKABLE = 1 << 4;
    }
}

/// What `bestUnitToAttack` reads of an object of the world (`FUN_1415417a0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoundObject {
    /// Its owner's id.
    pub(crate) owner: PlayerId,
    /// Its alliance, unit AI, container, targeting and attackable reads.
    pub(crate) flags: FoundFlags,
    /// `+0x640`.
    pub(crate) state: ObjectState,
    /// Its master's id, class and type.
    pub(crate) master: MasterId,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its master's type.
    pub(crate) kind: MasterKind,
    /// Its `+0x924` (`FUN_14107cc90`): how others take it over.
    pub(crate) capture: CaptureKind,
    /// What it holds (`+0x310`).
    pub(crate) held: Amount,
    /// The footprints' gap to the unit (`FUN_14118f680`).
    pub(crate) distance: Distance,
    /// Its top speed (slot 0x208).
    pub(crate) speed: Speed,
}

/// What `bestUnitToAttack` asks of the world while its unit AI runs: the unit AI's picker for
/// its reads and tests, the search's reachability for its paths (slot 0x3d0).
pub(crate) struct AttackField<'field, 'env, 'world> {
    /// The unit AI's reads and tests of the unit and the objects.
    pub(crate) picker: &'field WorldPicker<'world>,
    /// The unit's path to an object (slot 0x3d0).
    pub(crate) reach: &'field mut Reach<'env>,
    /// The spatial index the search and the picker's reads go through.
    pub(crate) world: &'field mut Spatial,
}

impl AttackField<'_, '_, '_> {
    /// The object `id` as the world holds it, None for one it does not.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    pub(crate) fn found_object(&self, id: ObjectId) -> GenieResult<Option<FoundObject>> {
        self.picker.found_object(self.world, id)
    }

    /// The unit AI's attack check (slot 0xa0 with 0) on object `id`.
    ///
    /// # Errors
    ///
    /// What the check does not model.
    pub(crate) fn can_attack(&self, id: ObjectId) -> GenieResult<bool> {
        self.picker.can_attack(self.world, id)
    }

    /// The unit AI's neutral test (slot 0xa8) of an object of `class`.
    pub(crate) fn attacks_neutral(&self, class: ClassId) -> bool {
        self.picker.attacks_neutral(class)
    }

    /// `FUN_14120e4e0` of a unit of the combat type on object `id`: it spares its friends.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    pub(crate) fn spares_friends(&self, id: ObjectId) -> GenieResult<bool> {
        self.picker.spares_friends(self.world, id)
    }

    /// `FUN_140c41690`: a computer player's unit passes object `id` over.
    pub(crate) const fn shuns(&self, id: ObjectId) -> bool {
        self.picker.shuns(id)
    }

    /// What the statistics read of object `id` at `distance` of `category`
    /// ([`Candidate`]).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    pub(crate) fn candidate(
        &self,
        id: ObjectId,
        distance: Distance,
        category: Option<AttackCategory>,
    ) -> GenieResult<Candidate> {
        self.picker.candidate(self.world, id, distance, category)
    }

    /// The shown armor of object `id` is at least 100 (slot 0x870's first).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    pub(crate) fn armored(&self, id: ObjectId) -> GenieResult<bool> {
        self.picker.armored(id)
    }

    /// Object `id`'s weapon range (slot 0x230).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    pub(crate) fn weapon_range(&self, id: ObjectId) -> GenieResult<Distance> {
        self.picker.weapon_range(id)
    }

    /// The current target `id` engages: its attacks sum above 0 (slot 0x228) or its unit AI
    /// could work on the unit (slot 0x1b8 with 1).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    pub(crate) fn engages(&self, id: ObjectId) -> GenieResult<bool> {
        self.picker.engages(id)
    }

    /// The unit's quick path to object `id` within `range` (slot 0x3e0).
    ///
    /// # Errors
    ///
    /// What the path does not model.
    pub(crate) fn quick_path(&mut self, id: ObjectId, range: Distance) -> GenieResult<bool> {
        self.picker.quick_path(self.world, id, range)
    }

    /// The unit's path to object `id` within `range` (slot 0x3d0).
    ///
    /// # Errors
    ///
    /// What the path does not model.
    pub(crate) fn path(&mut self, id: ObjectId, range: Distance) -> GenieResult<bool> {
        (self.reach)(self.world, id, range)
    }
}
