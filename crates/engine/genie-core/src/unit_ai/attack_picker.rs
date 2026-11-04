//! What the attack search asks of the world while a unit AI runs ([`AttackPicker`]); a search
//! that picks no attack has no picker ([`crate::map::spatial::SearchQuery`]).

use crate::GenieError;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::unit_ai::Memory;
use crate::unit_ai::best_target::AttackCategory;
use crate::unit_ai::best_target::{Candidate, FoundObject, Searcher};

/// What `bestUnitToAttack` (`FUN_140c44680`, [`crate::unit_ai::best_target::best_unit_to_attack`]) asks
/// of the world while its unit AI runs: the searching unit ([`AttackPicker::searcher`]) and its
/// reads and tests of the objects its search found; the paths (slot 0x3d0) are the search's.
pub trait AttackPicker {
    /// The searching unit as `bestUnitToAttack` reads it, with its AI's `memory`.
    ///
    /// # Errors
    ///
    /// A unit the reads do not model (a computer player's).
    fn searcher(&mut self, memory: Memory<'_>) -> Result<Searcher, GenieError>;

    /// The object `id` as the world holds it, None for one it does not.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn found_object(
        &mut self,
        world: &Spatial,
        id: ObjectId,
    ) -> Result<Option<FoundObject>, GenieError>;

    /// The unit AI's attack check (slot 0xa0 with 0) on object `id`.
    ///
    /// # Errors
    ///
    /// What the attack check does not model.
    fn can_attack(&mut self, world: &Spatial, id: ObjectId) -> Result<bool, GenieError>;

    /// The unit AI's neutral test (slot 0xa8) of an object of `class`.
    ///
    /// # Errors
    ///
    /// A unit-AI type whose test is not modelled.
    fn attacks_neutral(&mut self, class: ClassId) -> Result<bool, GenieError>;

    /// `FUN_14120e4e0` of a unit of the combat type on object `id`: it spares its friends.
    ///
    /// # Errors
    ///
    /// What the scan does not model.
    fn spares_friends(&mut self, world: &Spatial, id: ObjectId) -> Result<bool, GenieError>;

    /// `FUN_140c41690`: a computer player's unit passes object `id` over.
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn shuns(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// What the statistics read of object `id` at `distance` of `category`
    /// ([`Candidate`]).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    fn candidate(
        &mut self,
        world: &Spatial,
        id: ObjectId,
        distance: Distance,
        category: Option<AttackCategory>,
    ) -> Result<Candidate, GenieError>;

    /// The shown armor of object `id` is at least 100 (slot 0x870's first).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn armored(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// Object `id`'s weapon range (slot 0x230).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn weapon_range(&mut self, id: ObjectId) -> Result<Distance, GenieError>;

    /// The current target `id` engages: its attacks sum above 0 (slot 0x228) or its unit AI
    /// could work on the unit (slot 0x1b8 with 1).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    fn engages(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// The unit's quick path to object `id` within `range` (slot 0x3e0).
    ///
    /// # Errors
    ///
    /// What the path does not model.
    fn quick_path(
        &mut self,
        world: &mut Spatial,
        id: ObjectId,
        range: Distance,
    ) -> Result<bool, GenieError>;
}
