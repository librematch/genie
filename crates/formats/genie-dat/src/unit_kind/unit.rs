//! A unit type ([`UnitKind`]): its base class and the parts it has, read and written.

use crate::unit_kind::{
    ActionUnitKindAttributes, AnimatedUnitKindAttributes, BaseCombatUnitKindAttributes,
    BuildingUnitKindAttributes, CombatUnitKindAttributes, MissileUnitKindAttributes,
    MovingUnitKindAttributes, StaticUnitKindAttributes, UnitBaseClass,
};
use byteorder::{ReadBytesExt, WriteBytesExt};
use genie_error::{GenieResult, invalid_input};
use std::convert::TryInto;
use std::io::{Read, Write};

/// Data for a unit type.
///
/// Unit types have a [base class][] identifier that indicates which data is available for that
/// unit type. Data is split up into several `*Attributes` structs dictated by the unit's base class.
///
/// When editing the unit base class, the available attributes must also be updated. Failing to
/// keep the two in sync will cause a panic if you try to write the unit type data to a file or
/// other output.
///
/// [base class]: ./enum.UnitBaseClass.html
#[derive(Debug, Clone)]
pub struct UnitKind {
    /// The base class for this unit type.
    pub unit_base_class: UnitBaseClass,
    /// The static unit type attributes: these are always available.
    pub static_: StaticUnitKindAttributes,
    /// Animated unit type attributes, available if `self.unit_base_class >= UnitBaseClass::Animated`.
    pub animated: Option<AnimatedUnitKindAttributes>,
    /// Moving unit type attributes, available if `self.unit_base_class >= UnitBaseClass::Moving`.
    pub moving: Option<MovingUnitKindAttributes>,
    /// Action unit type attributes, available if `self.unit_base_class >= UnitBaseClass::Action`.
    pub action: Option<ActionUnitKindAttributes>,
    /// `BaseCombat` unit type attributes, available if `self.unit_base_class >=
    /// UnitBaseClass::BaseCombat`.
    pub base_combat: Option<BaseCombatUnitKindAttributes>,
    /// Missile unit type attributes, available if `self.unit_base_class >= UnitBaseClass::Missile`.
    pub missile: Option<MissileUnitKindAttributes>,
    /// Combat unit type attributes, available if `self.unit_base_class >= UnitBaseClass::Combat`.
    pub combat: Option<CombatUnitKindAttributes>,
    /// Building unit type attributes, available if `self.unit_base_class >=
    /// UnitBaseClass::Building`.
    pub building: Option<BuildingUnitKindAttributes>,
}

/// The unit type's attributes do not match its base class: a class it has lacks them, or a class
/// it lacks has them.
const ATTRIBUTES_MISMATCH: &str = "a unit type whose attributes do not match its base class";

impl UnitKind {
    /// Read a unit type from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    ///
    /// Also fails if the base class is not a known [`UnitBaseClass`], if the name is not valid
    /// UTF-8, or if the garrison sprite ID does not fit a [`SpriteId`].
    ///
    /// [`SpriteId`]: crate::sprite::SpriteId
    pub fn read_from(mut input: impl Read, version: f32) -> GenieResult<Self> {
        let unit_base_class = input.read_u8()?.try_into()?;
        let static_ = StaticUnitKindAttributes::read_from(&mut input, version)?;
        let mut unit = Self {
            unit_base_class,
            static_,
            animated: None,
            moving: None,
            action: None,
            base_combat: None,
            missile: None,
            combat: None,
            building: None,
        };
        if unit_base_class >= UnitBaseClass::Animated {
            unit.animated = Some(AnimatedUnitKindAttributes::read_from(&mut input, version)?);
        }
        if unit_base_class >= UnitBaseClass::Moving {
            unit.moving = Some(MovingUnitKindAttributes::read_from(&mut input, version)?);
        }
        if unit_base_class >= UnitBaseClass::Action {
            unit.action = Some(ActionUnitKindAttributes::read_from(&mut input, version)?);
        }
        if unit_base_class >= UnitBaseClass::BaseCombat {
            unit.base_combat = Some(BaseCombatUnitKindAttributes::read_from(
                &mut input, version,
            )?);
        }
        if unit_base_class >= UnitBaseClass::Missile {
            unit.missile = Some(MissileUnitKindAttributes::read_from(&mut input, version)?);
        }
        if unit_base_class >= UnitBaseClass::Combat {
            unit.combat = Some(CombatUnitKindAttributes::read_from(&mut input, version)?);
        }
        if unit_base_class >= UnitBaseClass::Building {
            unit.building = Some(BuildingUnitKindAttributes::read_from(&mut input, version)?);
        }
        Ok(unit)
    }

    /// The attributes of the class `class` that the base class holds: present for a base class
    /// at or above it, absent below it.
    fn attributes<'unit, T>(
        &self,
        attributes: Option<&'unit T>,
        class: UnitBaseClass,
    ) -> GenieResult<Option<&'unit T>> {
        match (self.unit_base_class >= class, attributes) {
            (true, Some(found)) => Ok(Some(found)),
            (false, None) => Ok(None),
            _ => Err(invalid_input(ATTRIBUTES_MISMATCH)),
        }
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails when the unit type's `unit_base_class` property does not match the available
    /// data attributes. For example, when `self.unit_base_class` is `UnitBaseClass::Animated`,
    /// but `self.animated` is `None`.
    pub fn write_to(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        output.write_u8(self.unit_base_class.into())?;

        self.static_.write_to(&mut output, version)?;

        if let Some(attributes) =
            self.attributes(self.animated.as_ref(), UnitBaseClass::Animated)?
        {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) = self.attributes(self.moving.as_ref(), UnitBaseClass::Moving)? {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) = self.attributes(self.action.as_ref(), UnitBaseClass::Action)? {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) =
            self.attributes(self.base_combat.as_ref(), UnitBaseClass::BaseCombat)?
        {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) = self.attributes(self.missile.as_ref(), UnitBaseClass::Missile)? {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) = self.attributes(self.combat.as_ref(), UnitBaseClass::Combat)? {
            attributes.write_to(&mut output, version)?;
        }
        if let Some(attributes) =
            self.attributes(self.building.as_ref(), UnitBaseClass::Building)?
        {
            attributes.write_to(&mut output, version)?;
        }

        Ok(())
    }
}
