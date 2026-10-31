//! The attribute setters by master type (slots 0x18, 0x20 and 0x28), the master copy (slot 8),
//! the loader's defaults and the selected entries.

use crate::movement::Speed;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::{
    AttributeWorld, DamageGraphic, Master, TrainLocation, attribute, master_type,
};
use crate::players::Amount;

impl Master {
    /// Slot 0x18 (`set`): attribute `attribute` of this master takes `v`, by its type's setter
    /// (`FUN_14123d310` for a building, `FUN_1412661d0` a combat master, `FUN_1410d5a20` a
    /// missile, `FUN_1410c12b0` an RGE combat master, `FUN_14109fbb0` an action master,
    /// `FUN_1410e1c00` a moving one, `FUN_1410a8b00` an animated one or a doppelganger,
    /// `FUN_141126400` the rest), each falling back on its base's. Each first records the master
    /// for the spectator (`FUN_1417785e0`, not simulation). The interface's values (icons, names,
    /// tooltips, hotkey actions) and the sounds are not held. The followers of the values
    /// (`Master::follow`) are the caller's.
    pub fn set_attribute(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match self.kind {
            master_type::BUILDING => self.set_building(attribute, v, world),
            master_type::COMBAT => self.set_tribe_combat(attribute, v, world),
            master_type::MISSILE => self.set_missile(attribute, v, world),
            master_type::RGE_COMBAT => self.set_combat(attribute, v, world),
            master_type::ACTION => self.set_action(attribute, v, world),
            master_type::MOVING => self.set_moving(attribute, v, world),
            master_type::ANIMATED | master_type::DOPPELGANGER => {
                self.set_animated(attribute, v, world);
            }
            _ => self.set_static(attribute, v, world),
        }
    }

    /// Slot 0x20 (`add`): `v` adds to attribute `attribute`, by its type's setter
    /// (`FUN_14123d530`, `FUN_141266a60`, `FUN_1410d5ab0`, `FUN_1410c16e0`, `FUN_14109fdb0`,
    /// `FUN_1410e1da0`, `FUN_1410a8b60`, `FUN_141126af0`), as [`Master::set_attribute`].
    pub fn add_attribute(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match self.kind {
            master_type::BUILDING => self.add_building(attribute, v, world),
            master_type::COMBAT => self.add_tribe_combat(attribute, v, world),
            master_type::MISSILE => self.add_missile(attribute, v, world),
            master_type::RGE_COMBAT => self.add_combat(attribute, v, world),
            master_type::ACTION => self.add_action(attribute, v, world),
            master_type::MOVING => self.add_moving(attribute, v, world),
            master_type::ANIMATED | master_type::DOPPELGANGER => {
                self.add_animated(attribute, v, world);
            }
            _ => self.add_static(attribute, v, world),
        }
    }

    /// Slot 0x28 (`multiply`): attribute `attribute` is multiplied by `v`, by its type's setter
    /// (`FUN_14123d730`, `FUN_141267040`, `FUN_1410d5b50`, `FUN_1410c1bf0`, `FUN_14109fea0`,
    /// `FUN_1410e1f60`, `FUN_1410a8c00`, `FUN_1411271e0`), as [`Master::set_attribute`].
    pub fn multiply_attribute(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match self.kind {
            master_type::BUILDING => self.multiply_building(attribute, v, world),
            master_type::COMBAT => self.multiply_tribe_combat(attribute, v, world),
            master_type::MISSILE => self.multiply_missile(attribute, v, world),
            master_type::RGE_COMBAT => self.multiply_combat(attribute, v, world),
            master_type::ACTION => self.multiply_action(attribute, v, world),
            master_type::MOVING => self.multiply_moving(attribute, v, world),
            master_type::ANIMATED | master_type::DOPPELGANGER => {
                self.multiply_animated(attribute, v, world);
            }
            _ => self.multiply_static(attribute, v, world),
        }
    }

    /// The master copy, slot 8 (`FUN_1411252b0` and its overrides up to the building's
    /// `FUN_14123c4e0`), as an upgrade of a master to another of its type runs it: this master
    /// takes `other`'s values (its tasks, costs, train locations, damage sprites and every other
    /// list copied) but keeps its id, its class, its availability, its disabling and its base id
    /// (`+0x260`; its `+0x5ec`, `+0x738` and, keeping its id, the interface's names: not held). The
    /// action master's work rate takes `other`'s times this one's handicapped rate (`+0x79c`)
    /// when the two differ: not held, taken equal.
    pub fn copy_from(&mut self, other: &Self) {
        let (id, class, available, disabled) = (self.id, self.class, self.available, self.disabled);
        let base_id = self.base_id;
        *self = other.clone();
        self.id = id;
        self.base_id = base_id;
        self.class = class;
        self.available = available;
        self.disabled = disabled;
    }

    /// The value attribute `attribute` changes that the masters' loader left at its default
    /// while what the slice derives from it holds another (the speed, `+0x740`, under a moving
    /// master's speeds; the turn speed, `+0x758`, under its turn time; the train locations under
    /// their ids; the damage sprites under the flag that it has some): the change would derive
    /// from the default. None when the master holds it.
    #[must_use]
    pub fn unloaded_for(&self, attribute: MasterAttribute) -> Option<&'static str> {
        use crate::objects::masters::attribute as a;
        let moving = self.moving.as_ref();
        match attribute {
            a::MOVEMENT_SPEED | a::STANDING_GRAPHIC | a::WALKING_GRAPHIC | a::RUNNING_GRAPHIC
                if self.max_speed == Speed::ZERO
                    && moving.is_some_and(|m| m.speed != Speed::ZERO) =>
            {
                Some("a master's speed (+0x740) the loader did not fill")
            }
            a::ROTATION_SPEED
                if self.turn_speed == 0.0 && moving.is_some_and(|m| m.turn_time != 0.0) =>
            {
                Some("a master's turn speed (+0x758) the loader did not fill")
            }
            a::TRAIN_LOCATION
            | a::TRAIN_BUTTON
            | a::HOTKEY_ID
            | a::TRAIN_TIME
            | a::TRAIN_LOCATIONS_ENTRY_MOD
            | a::TRAIN_LOCATIONS_TOTAL_NUM
                if self.train_locations.is_empty() && !self.trained_at.is_empty() =>
            {
                Some("a master's train locations (+0x898) the loader did not fill")
            }
            a::DAMAGE_GRAPHIC
            | a::DAMAGE_GRAPHICS_ENTRY_MOD
            | a::DAMAGE_GRAPHICS_TOTAL_NUM
            | a::DAMAGE_GRAPHIC_PERCENT
            | a::DAMAGE_GRAPHIC_APPLY_MODE
                if self.damage_graphics.is_empty() && self.damage_sprites =>
            {
                Some("a master's damage sprites (+0x638) the loader did not fill")
            }
            _ => None,
        }
    }

    /// The resource storage `attribute` (21, 26, 27) names: the first, second or third entry.
    pub(super) fn storage_mut(&mut self, attribute: MasterAttribute) -> Option<&mut Amount> {
        let i = match attribute {
            attribute::STORAGE_1 => 0,
            attribute::STORAGE_2 => 1,
            _ => 2,
        };
        self.storage.get_mut(i).map(|e| &mut e.1)
    }

    /// The selected damage sprite (`+0x731` below the count, an unsigned compare).
    pub(super) fn damage_graphic_mut(&mut self) -> Option<&mut DamageGraphic> {
        self.damage_graphics
            .get_mut(usize::from(self.damage_graphic_entry))
    }

    /// The selected train location (`FUN_1412669d0`: `+0x940` sign-extended, below the count as
    /// an unsigned value, so a negative entry selects none).
    pub(super) fn train_location_mut(&mut self) -> Option<&mut TrainLocation> {
        let i = usize::try_from(i64::from(self.train_location_entry) as u64).ok()?;
        self.train_locations.get_mut(i)
    }
}
