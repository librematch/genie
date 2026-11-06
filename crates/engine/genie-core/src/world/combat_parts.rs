//! The combat object's parts (FD-045-D51): its combat layer (`RGE_Combat_Object`, types 50 and
//! up) and a building's layer (`TRIBE_Building_Object`, type 80) by id in the world's tables
//! ([`Parts::combat`], [`Parts::building`]), a new object's where a table holds none, and the
//! repairers or builders that one of the two holds.

use crate::GenieResult;
use crate::construction::BuilderCount;
use crate::objects::ObjectId;
use crate::objects::layers::{Building, Combat};
use crate::unit_ai::UnitAi;
use crate::world::parts::{PartTable, Parts};

impl PartTable<Building> {
    /// Object `id`'s building layer: [`Building::NONE`] where the table holds none.
    #[must_use]
    pub fn of(&self, id: ObjectId) -> &Building {
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        self.get(id).unwrap_or(const { &Building::NONE })
    }

    /// Object `id`'s building layer to change, [`Building::NONE`] put in where the table holds
    /// none. The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    ///
    /// [`World::set_part`]: crate::world::World::set_part
    pub(crate) fn of_mut(&mut self, id: ObjectId) -> GenieResult<&mut Building> {
        self.get_or_insert_with(id, || Building::NONE)
    }
}

impl PartTable<Combat> {
    /// Object `id`'s combat layer: a new object's ([`Combat::NONE`]) where the table holds
    /// none.
    #[must_use]
    pub fn of(&self, id: ObjectId) -> &Combat {
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        self.get(id).unwrap_or(const { &Combat::NONE })
    }

    /// Object `id`'s combat layer to change, a new object's put in where the table holds none.
    /// The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    ///
    /// [`World::set_part`]: crate::world::World::set_part
    pub(crate) fn of_mut(&mut self, id: ObjectId) -> GenieResult<&mut Combat> {
        self.get_or_insert_with(id, Combat::default)
    }
}

/// Object `id`'s repairers or builders to change ([`Parts::repairers_of`]) in the tables
/// `buildings` and `combats`: a building's construction's, else its combat layer's (a new
/// object's put in where the table holds none).
///
/// # Errors
///
/// None (an unreachable error where the combat table lost the part it took).
pub(in crate::world) fn repairers_in<'world>(
    buildings: &'world mut PartTable<Building>,
    combats: &'world mut PartTable<Combat>,
    id: ObjectId,
) -> GenieResult<&'world mut BuilderCount> {
    if let Some(construction) = buildings
        .get_mut(id)
        .and_then(|building| building.construction.as_mut())
    {
        return Ok(&mut construction.builders);
    }
    Ok(&mut combats.of_mut(id)?.builders)
}

impl Parts {
    /// Object `id`'s building layer: [`Building::NONE`] where the table holds none.
    #[must_use]
    pub fn building_of(&self, id: ObjectId) -> &Building {
        self.building.of(id)
    }

    /// Object `id`'s building layer to change, [`Building::NONE`] put in where the table holds
    /// none. The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    ///
    /// [`World::set_part`]: crate::world::World::set_part
    pub(crate) fn building_mut(&mut self, id: ObjectId) -> GenieResult<&mut Building> {
        self.building.of_mut(id)
    }

    /// Object `id`'s combat layer: a new object's ([`Combat::default`]) where the table holds
    /// none.
    #[must_use]
    pub fn combat_of(&self, id: ObjectId) -> &Combat {
        self.combat.of(id)
    }

    /// Object `id`'s combat layer to change, a new object's put in where the table holds none.
    /// The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    ///
    /// [`World::set_part`]: crate::world::World::set_part
    pub(crate) fn combat_mut(&mut self, id: ObjectId) -> GenieResult<&mut Combat> {
        self.combat.of_mut(id)
    }

    /// Object `id`'s repairers or builders (`+0x9e8`, `num_builders`): a building's in its
    /// construction, any other's in its combat layer.
    #[must_use]
    pub fn repairers_of(&self, id: ObjectId) -> BuilderCount {
        self.building_of(id).construction.map_or_else(
            || self.combat_of(id).builders,
            |construction| construction.builders,
        )
    }

    /// [`Parts::repairers_of`] to change ([`repairers_in`]).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a table lost the part it took).
    pub(crate) fn repairers_mut(&mut self, id: ObjectId) -> GenieResult<&mut BuilderCount> {
        repairers_in(&mut self.building, &mut self.combat, id)
    }

    /// Object `id`'s unit AI (none without one) and its combat layer ([`Parts::combat_mut`]),
    /// to change together.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn ai_and_combat_mut(
        &mut self,
        id: ObjectId,
    ) -> GenieResult<(Option<&mut UnitAi>, &mut Combat)> {
        let combat = self.combat.of_mut(id)?;
        Ok((self.unit_ai.get_mut(id), combat))
    }
}
