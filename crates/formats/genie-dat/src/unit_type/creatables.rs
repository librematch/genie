//! The costs, the creatable part, the annexes and the building part.

use crate::conv::fits;
use crate::sound::SoundId;
use crate::sprite::SpriteId;
use crate::terrain::TerrainId;
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{MapInto, TechId, UnitTypeId, read_opt_u16, read_opt_u32};
use std::io::{Read, Result, Write};

/// Resource cost for a unit.
#[derive(Debug, Default, Clone, Copy)]
pub struct AttributeCost {
    /// The player attribute type to give/take.
    pub attribute_type: i16,
    /// The amount of that attribute that should be taken/given.
    pub amount: i16,
    /// Flag determining how and when this cost is counted.
    ///
    /// TODO make this an enum
    pub flag: u8,
}

impl AttributeCost {
    /// Read a cost (and its padding byte) from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let cost = Self {
            attribute_type: input.read_i16::<LE>()?,
            amount: input.read_i16::<LE>()?,
            flag: input.read_u8()?,
        };
        let _padding = input.read_u8()?;
        Ok(cost)
    }

    /// Write this cost (and a zero padding byte) to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(self, mut output: impl Write) -> Result<()> {
        output.write_i16::<LE>(self.attribute_type)?;
        output.write_i16::<LE>(self.amount)?;
        output.write_u8(self.flag)?;
        output.write_u8(0)?;
        Ok(())
    }
}

/// The attributes of combat unit types that players create (genieutils' `Creatable`).
#[derive(Debug, Default, Clone)]
pub struct CombatUnitTypeAttributes {
    /// The costs of creating a unit of this type.
    pub costs: ArrayVec<AttributeCost, 3>,
    /// Time to create the unit.
    pub create_time: u16,
    /// Unit type ID of the building or unit where this unit can be created.
    pub create_at_building: Option<UnitTypeId>,
    /// Button location index where the button to create this unit should be shown when a
    /// `create_at_building` unit is selected.
    pub create_button: i8,
    /// Damage multiplier for attacks from behind.
    pub rear_attack_modifier: f32,
    /// Damage multiplier for attacks from the side.
    pub flank_attack_modifier: f32,
    /// Is this unit a hero unit?
    ///
    /// TODO what is special about hero units? Does it just opt into the healing behaviour?
    pub hero_flag: u8,
    /// Sprite shown while units are garrisoned, or `None`.
    pub garrison_sprite: Option<SpriteId>,
    /// Projectiles fired per attack (genieutils' `TotalProjectiles`).
    pub volley_fire_amount: f32,
    /// The most projectiles fired per attack (genieutils' `MaxTotalProjectiles`).
    pub max_attacks_in_volley: i8,
    /// Width and length of the area the projectiles start from (genieutils'
    /// `ProjectileSpawningArea`).
    pub volley_spread: (f32, f32),
    /// Randomness of the projectiles' start (the third value of genieutils'
    /// `ProjectileSpawningArea`).
    pub volley_start_spread_adjustment: f32,
    /// Unit type of the extra projectiles (genieutils' `SecondaryProjectileUnit`), or `None`.
    pub volley_missile: Option<UnitTypeId>,
    /// Sprite of the special attack (genieutils' `SpecialGraphic`), or `None`.
    pub special_attack_sprite: Option<SpriteId>,
    /// Kind of special ability (genieutils' `SpecialAbility`).
    pub special_attack_flag: i8,
    /// Pierce armour shown in the interface.
    pub displayed_pierce_armor: i16,
}

impl CombatUnitTypeAttributes {
    /// Read this unit type from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    ///
    /// Also fails if the garrison sprite ID does not fit a [`SpriteId`].
    pub fn read_from(mut input: impl Read, _version: f32) -> Result<Self> {
        let mut attrs = Self::default();
        for _ in 0..3 {
            let attr = AttributeCost::read_from(&mut input)?;
            if attr.attribute_type >= 0 {
                attrs.costs.push(attr);
            }
        }
        attrs.create_time = input.read_u16::<LE>()?;
        attrs.create_at_building = read_opt_u16(&mut input)?;
        attrs.create_button = input.read_i8()?;
        attrs.rear_attack_modifier = input.read_f32::<LE>()?;
        attrs.flank_attack_modifier = input.read_f32::<LE>()?;
        let _tribe_unit_type = input.read_u8()?;
        attrs.hero_flag = input.read_u8()?;
        attrs.garrison_sprite = {
            let n = input.read_i32::<LE>()?;
            if n < 0 { None } else { Some(fits(n)?) }
        };
        attrs.volley_fire_amount = input.read_f32::<LE>()?;
        attrs.max_attacks_in_volley = input.read_i8()?;
        attrs.volley_spread = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        attrs.volley_start_spread_adjustment = input.read_f32::<LE>()?;
        attrs.volley_missile = read_opt_u32(&mut input)?;
        attrs.special_attack_sprite = read_opt_u32(&mut input)?;
        attrs.special_attack_flag = input.read_i8()?;
        attrs.displayed_pierce_armor = input.read_i16::<LE>()?;
        Ok(attrs)
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> Result<()> {
        for i in 0..3 {
            match self.costs.get(i) {
                Some(cost) => cost.write_to(&mut output)?,
                None => AttributeCost {
                    attribute_type: -1,
                    amount: 0,
                    flag: 0,
                }
                .write_to(&mut output)?,
            }
        }
        output.write_u16::<LE>(self.create_time)?;
        output.write_u16::<LE>(self.create_at_building.map_into().unwrap_or(0xFFFF))?;
        output.write_i8(self.create_button)?;
        output.write_f32::<LE>(self.rear_attack_modifier)?;
        output.write_f32::<LE>(self.flank_attack_modifier)?;
        output.write_u8(0)?;
        output.write_u8(self.hero_flag)?;
        output.write_u32::<LE>(self.garrison_sprite.map_into().unwrap_or(0xFFFF_FFFF))?;
        output.write_f32::<LE>(self.volley_fire_amount)?;
        output.write_i8(self.max_attacks_in_volley)?;
        output.write_f32::<LE>(self.volley_spread.0)?;
        output.write_f32::<LE>(self.volley_spread.1)?;
        output.write_f32::<LE>(self.volley_start_spread_adjustment)?;
        output.write_u32::<LE>(self.volley_missile.map_into().unwrap_or(0xFFFF_FFFF))?;
        output.write_u32::<LE>(self.special_attack_sprite.map_into().unwrap_or(0xFFFF_FFFF))?;
        output.write_i8(self.special_attack_flag)?;
        output.write_i16::<LE>(self.displayed_pierce_armor)?;
        Ok(())
    }
}

/// A linked, or "Annex" building. These allow for buildings made up of multiple pieces
/// with different behaviour, like the Town Centre with some walkable tiles and some non-walkable
/// tiles.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinkedBuilding {
    /// Unit type ID for this linked building.
    pub unit_id: UnitTypeId,
    /// X offset in tiles from the centre of the "owner" building.
    pub x_offset: f32,
    /// Y offset in tiles from the centre of the "owner" building.
    pub y_offset: f32,
}

impl LinkedBuilding {
    /// Read an annex from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        Ok(Self {
            unit_id: input.read_u16::<LE>()?.into(),
            x_offset: input.read_f32::<LE>()?,
            y_offset: input.read_f32::<LE>()?,
        })
    }

    /// Write this annex to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(self.unit_id.into())?;
        output.write_f32::<LE>(self.x_offset)?;
        output.write_f32::<LE>(self.y_offset)?;
        Ok(())
    }
    pub(super) fn write_empty(mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(0xFFFF)?;
        output.write_f32::<LE>(0.0)?;
        output.write_f32::<LE>(0.0)?;
        Ok(())
    }
}

/// Unit type class for buildings.
#[derive(Debug, Default, Clone)]
pub struct BuildingUnitTypeAttributes {
    /// Sprite to use during construction.
    pub construction_sprite: Option<SpriteId>,
    /// Sprite to use when this building is finished and built on snow.
    pub snow_sprite: Option<SpriteId>,
    /// TODO document
    pub connect_flag: u8,
    /// TODO document
    pub facet: i16,
    /// Whether the building should be immediately destroyed on completion.
    pub destroy_on_build: bool,
    /// Unit to spawn at the build site on completion.
    pub on_build_make_unit: Option<UnitTypeId>,
    /// Change the underlying terrain to this terrain ID on completion.
    pub on_build_make_tile: Option<TerrainId>,
    /// TODO document
    pub on_build_make_overlay: i16,
    /// Research this tech on completion.
    pub on_build_make_tech: Option<TechId>,
    /// Whether this building…can burn?
    ///
    /// TODO document the details
    pub can_burn: bool,
    /// Annex buildings, at most 4; unused slots are dropped when reading.
    pub linked_buildings: ArrayVec<LinkedBuilding, 4>,
    /// The head unit of a building made of several units (genieutils' `HeadUnit`), or `None`.
    pub construction_unit: Option<UnitTypeId>,
    /// Unit type the building transforms into (genieutils' `TransformUnit`), or `None`.
    pub transform_unit: Option<UnitTypeId>,
    /// Sound played when the building transforms, or `None`.
    pub transform_sound: Option<SoundId>,
    /// Sound played while the building is built, or `None`.
    pub construction_sound: Option<SoundId>,
    /// Which kinds of unit can garrison inside (genieutils' `GarrisonType`, a bit mask).
    pub garrison_type: i8,
    /// Rate at which garrisoned units heal.
    pub garrison_heal_rate: f32,
    /// Rate at which the building is repaired by garrisoned units (genieutils'
    /// `GarrisonRepairRate`).
    pub garrison_repair_rate: f32,
    /// Unit type left behind when the building is destroyed (genieutils' `PileUnit`), or `None`.
    pub salvage_unit: Option<UnitTypeId>,
    /// The six values of genieutils' `LootingTable`: what is looted from the building.
    pub salvage_attributes: ArrayVec<i8, 6>,
}

impl BuildingUnitTypeAttributes {
    /// Read this unit type from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read, version: f32) -> Result<Self> {
        let mut attrs = Self {
            construction_sprite: read_opt_u16(&mut input)?,
            snow_sprite: if version < 11.53 {
                None
            } else {
                read_opt_u16(&mut input)?
            },
            connect_flag: input.read_u8()?,
            facet: input.read_i16::<LE>()?,
            destroy_on_build: input.read_u8()? != 0,
            on_build_make_unit: read_opt_u16(&mut input)?,
            on_build_make_tile: read_opt_u16(&mut input)?,
            on_build_make_overlay: input.read_i16::<LE>()?,
            on_build_make_tech: read_opt_u16(&mut input)?,
            can_burn: input.read_u8()? != 0,
            ..Default::default()
        };
        for _ in 0..attrs.linked_buildings.capacity() {
            let link = LinkedBuilding::read_from(&mut input)?;
            if link.unit_id != 0xFFFF.into() {
                attrs.linked_buildings.push(link);
            }
        }

        attrs.construction_unit = read_opt_u16(&mut input)?;
        attrs.transform_unit = read_opt_u16(&mut input)?;
        attrs.transform_sound = read_opt_u16(&mut input)?;
        attrs.construction_sound = read_opt_u16(&mut input)?;
        attrs.garrison_type = input.read_i8()?;
        attrs.garrison_heal_rate = input.read_f32::<LE>()?;
        attrs.garrison_repair_rate = input.read_f32::<LE>()?;
        attrs.salvage_unit = read_opt_u16(&mut input)?;
        for _ in 0..attrs.salvage_attributes.capacity() {
            let attr = input.read_i8()?;
            attrs.salvage_attributes.push(attr);
        }
        Ok(attrs)
    }

    /// Write the unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write, version: f32) -> Result<()> {
        output.write_u16::<LE>(self.construction_sprite.map_into().unwrap_or(0xFFFF))?;
        if version >= 11.53 {
            output.write_u16::<LE>(self.snow_sprite.map_into().unwrap_or(0xFFFF))?;
        }
        output.write_u8(self.connect_flag)?;
        output.write_i16::<LE>(self.facet)?;
        output.write_u8(u8::from(self.destroy_on_build))?;
        output.write_u16::<LE>(self.on_build_make_unit.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(self.on_build_make_tile.map_into().unwrap_or(0xFFFF))?;
        output.write_i16::<LE>(self.on_build_make_overlay)?;
        output.write_u16::<LE>(self.on_build_make_tech.map_into().unwrap_or(0xFFFF))?;
        output.write_u8(u8::from(self.can_burn))?;
        for i in 0..self.linked_buildings.capacity() {
            match self.linked_buildings.get(i) {
                Some(link) => link.write_to(&mut output)?,
                None => LinkedBuilding::write_empty(&mut output)?,
            }
        }
        output.write_u16::<LE>(self.construction_unit.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(self.transform_unit.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(self.transform_sound.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(self.construction_sound.map_into().unwrap_or(0xFFFF))?;
        output.write_i8(self.garrison_type)?;
        output.write_f32::<LE>(self.garrison_heal_rate)?;
        output.write_f32::<LE>(self.garrison_repair_rate)?;
        output.write_u16::<LE>(self.salvage_unit.map_into().unwrap_or(0xFFFF))?;
        for attr in &self.salvage_attributes {
            output.write_i8(*attr)?;
        }
        Ok(())
    }
}
