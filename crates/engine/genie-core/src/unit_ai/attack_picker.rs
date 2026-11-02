//! What the attack search asks of the world while a unit AI runs ([`AttackPicker`]), and the
//! picker of searches that pick no attack ([`NoAttack`]).

use crate::GenieError;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::unit_ai::Memory;
use crate::unit_ai::best_target::AttackCategory;
use crate::unit_ai::best_target::{Candidate, Searcher, Thing};

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

    /// [`crate::unit_ai::best_target::Battlefield::thing`].
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    fn thing(&mut self, world: &Spatial, id: ObjectId) -> Result<Option<Thing>, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::can_attack`].
    ///
    /// # Errors
    ///
    /// What the attack check does not model.
    fn can_attack(&mut self, world: &Spatial, id: ObjectId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::attacks_neutral`].
    ///
    /// # Errors
    ///
    /// A unit-AI type whose test is not modelled.
    fn attacks_neutral(&mut self, class: ClassId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::spares_friends`].
    ///
    /// # Errors
    ///
    /// What the scan does not model.
    fn spares_friends(&mut self, world: &Spatial, id: ObjectId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::shuns`].
    ///
    /// # Errors
    ///
    /// What the test does not model.
    fn shuns(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::candidate`].
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

    /// [`crate::unit_ai::best_target::Battlefield::armored`].
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn armored(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::weapon_range`].
    ///
    /// # Errors
    ///
    /// What the read does not model.
    fn weapon_range(&mut self, id: ObjectId) -> Result<Distance, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::engages`].
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    fn engages(&mut self, id: ObjectId) -> Result<bool, GenieError>;

    /// [`crate::unit_ai::best_target::Battlefield::quick_path`].
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

/// A search that picks no attack, asked about one ([`NoAttack`]).
pub(super) const NO_ATTACK: GenieError =
    GenieError::unsupported("an attack test in a search that picks no attack");

/// An [`AttackPicker`] for searches that pick no attack: every read fails.
#[derive(Clone, Copy, Debug)]
pub struct NoAttack;

impl AttackPicker for NoAttack {
    fn searcher(&mut self, _: Memory<'_>) -> Result<Searcher, GenieError> {
        Err(NO_ATTACK)
    }

    fn thing(&mut self, _: &Spatial, _: ObjectId) -> Result<Option<Thing>, GenieError> {
        Err(NO_ATTACK)
    }

    fn can_attack(&mut self, _: &Spatial, _: ObjectId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn attacks_neutral(&mut self, _: ClassId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn spares_friends(&mut self, _: &Spatial, _: ObjectId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn shuns(&mut self, _: ObjectId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn candidate(
        &mut self,
        _: &Spatial,
        _: ObjectId,
        _: Distance,
        _: Option<AttackCategory>,
    ) -> Result<Candidate, GenieError> {
        Err(NO_ATTACK)
    }

    fn armored(&mut self, _: ObjectId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn weapon_range(&mut self, _: ObjectId) -> Result<Distance, GenieError> {
        Err(NO_ATTACK)
    }

    fn engages(&mut self, _: ObjectId) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }

    fn quick_path(
        &mut self,
        _: &mut Spatial,
        _: ObjectId,
        _: Distance,
    ) -> Result<bool, GenieError> {
        Err(NO_ATTACK)
    }
}
