//! The weapons ([`WeaponInfo`]), the combat part and the projectile part.

use crate::conv::fits;
use crate::sprite::SpriteId;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{MapInto, UnitKindId, read_opt_u16};
use std::io::{Read, Write};

/// One attack or armour value: a class and the amount for it.
#[derive(Debug, Default, Clone, Copy)]
pub struct WeaponInfo {
    /// The attack or armour class.
    pub weapon_kind: i16,
    /// The amount for that class.
    pub value: i16,
}

impl WeaponInfo {
    /// Read an attack or armour value from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        Ok(Self {
            weapon_kind: input.read_i16::<LE>()?,
            value: input.read_i16::<LE>()?,
        })
    }

    /// Write this attack or armour value to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(self, mut output: impl Write) -> GenieResult<()> {
        output.write_i16::<LE>(self.weapon_kind)?;
        output.write_i16::<LE>(self.value)?;
        Ok(())
    }
}

/// The attributes of unit types that fight.
#[derive(Debug, Default, Clone)]
pub struct BaseCombatUnitKindAttributes {
    /// Base armour; stored as a u8 before data version 11.52.
    pub base_armor: u16,
    /// Attack values, one per class.
    pub weapons: Vec<WeaponInfo>,
    /// Armour values, one per class.
    pub armors: Vec<WeaponInfo>,
    /// Terrain restriction whose values scale damage by the terrain the unit stands on (genieutils'
    /// `DefenseTerrainBonus`), or `None`.
    pub defense_terrain_bonus: Option<u16>,
    /// Maximum attack range.
    pub weapon_range_max: f32,
    /// Radius of the area (blast) damage (genieutils' `BlastWidth`).
    pub area_effect_range: f32,
    /// Time between attacks (reload time).
    pub attack_speed: f32,
    /// Projectile unit type fired, or `None`.
    pub missile_id: Option<UnitKindId>,
    /// Chance to hit, in percent (genieutils' `AccuracyPercent`).
    pub base_hit_chance: i16,
    /// genieutils' `BreakOffCombat`; its use is not known yet.
    pub break_off_combat: i8,
    /// Frame of the attack animation at which the attack happens.
    pub frame_delay: i16,
    /// Where the projectile starts, relative to the unit (x, y, z).
    pub weapon_offset: (f32, f32, f32),
    /// Which units the area damage hits (genieutils' `BlastAttackLevel`).
    pub blast_level_offense: i8,
    /// Minimum attack range.
    pub weapon_range_min: f32,
    /// How far missed projectiles scatter (genieutils' `AccuracyDispersion`).
    pub missed_missile_spread: f32,
    /// Sprite shown while attacking, or `None`.
    pub fight_sprite: Option<SpriteId>,
    /// Melee armour shown in the interface.
    pub displayed_armor: i16,
    /// Attack shown in the interface.
    pub displayed_attack: i16,
    /// Range shown in the interface.
    pub displayed_range: f32,
    /// Reload time shown in the interface.
    pub displayed_reload_time: f32,
}

impl BaseCombatUnitKindAttributes {
    /// Read the combat attributes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read, version: f32) -> GenieResult<Self> {
        let mut attrs = Self {
            base_armor: if version < 11.52 {
                input.read_u8()?.into()
            } else {
                input.read_u16::<LE>()?
            },
            ..Default::default()
        };
        let num_weapons = input.read_u16::<LE>()?;
        for _ in 0..num_weapons {
            attrs.weapons.push(WeaponInfo::read_from(&mut input)?);
        }
        let num_armors = input.read_u16::<LE>()?;
        for _ in 0..num_armors {
            attrs.armors.push(WeaponInfo::read_from(&mut input)?);
        }
        attrs.defense_terrain_bonus = read_opt_u16(&mut input)?;
        attrs.weapon_range_max = input.read_f32::<LE>()?;
        attrs.area_effect_range = input.read_f32::<LE>()?;
        attrs.attack_speed = input.read_f32::<LE>()?;
        attrs.missile_id = read_opt_u16(&mut input)?;
        attrs.base_hit_chance = input.read_i16::<LE>()?;
        attrs.break_off_combat = input.read_i8()?;
        attrs.frame_delay = input.read_i16::<LE>()?;
        attrs.weapon_offset = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        attrs.blast_level_offense = input.read_i8()?;
        attrs.weapon_range_min = input.read_f32::<LE>()?;
        attrs.missed_missile_spread = input.read_f32::<LE>()?;
        attrs.fight_sprite = read_opt_u16(&mut input)?;
        attrs.displayed_armor = input.read_i16::<LE>()?;
        attrs.displayed_attack = input.read_i16::<LE>()?;
        attrs.displayed_range = input.read_f32::<LE>()?;
        attrs.displayed_reload_time = input.read_f32::<LE>()?;
        Ok(attrs)
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if `version` is below 11.52 and the base armour does not fit the u8 that such files
    /// store it in.
    // The format stores the attack and armour counts as u16 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version < 11.52 {
            output.write_u8(fits(self.base_armor)?)?;
        } else {
            output.write_u16::<LE>(self.base_armor)?;
        }
        output.write_u16::<LE>(self.weapons.len() as u16)?;
        for weapon in &self.weapons {
            weapon.write_to(&mut output)?;
        }
        output.write_u16::<LE>(self.armors.len() as u16)?;
        for armor in &self.armors {
            armor.write_to(&mut output)?;
        }
        self.write_ranges(&mut output)
    }

    /// The fields after the weapon and armour lists.
    fn write_ranges(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u16::<LE>(self.defense_terrain_bonus.unwrap_or(0xFFFF))?;
        output.write_f32::<LE>(self.weapon_range_max)?;
        output.write_f32::<LE>(self.area_effect_range)?;
        output.write_f32::<LE>(self.attack_speed)?;
        output.write_u16::<LE>(self.missile_id.map_into().unwrap_or(0xFFFF))?;
        output.write_i16::<LE>(self.base_hit_chance)?;
        output.write_i8(self.break_off_combat)?;
        output.write_i16::<LE>(self.frame_delay)?;
        output.write_f32::<LE>(self.weapon_offset.0)?;
        output.write_f32::<LE>(self.weapon_offset.1)?;
        output.write_f32::<LE>(self.weapon_offset.2)?;
        output.write_i8(self.blast_level_offense)?;
        output.write_f32::<LE>(self.weapon_range_min)?;
        output.write_f32::<LE>(self.missed_missile_spread)?;
        output.write_u16::<LE>(self.fight_sprite.map_into().unwrap_or(0xFFFF))?;
        output.write_i16::<LE>(self.displayed_armor)?;
        output.write_i16::<LE>(self.displayed_attack)?;
        output.write_f32::<LE>(self.displayed_range)?;
        output.write_f32::<LE>(self.displayed_reload_time)?;
        Ok(())
    }
}

/// The attributes of projectiles.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissileUnitKindAttributes {
    /// Projectile type (genieutils' `ProjectileType`).
    pub missile_kind: u8,
    /// Whether the projectile aims ahead of moving targets (genieutils' `SmartMode`).
    pub targetting_kind: u8,
    /// What the projectile hits (genieutils' `HitMode`).
    pub missile_hit_info: u8,
    /// Whether the projectile vanishes when it hits (genieutils' `VanishMode`).
    pub missile_die_info: u8,
    /// Area-effect specials (genieutils' `AreaEffectSpecials`); not decoded here.
    pub area_effect_specials: u8,
    /// The arc of the trajectory (genieutils' `ProjectileArc`).
    pub ballistics_ratio: f32,
}

impl MissileUnitKindAttributes {
    /// Read this unit type from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read, _version: f32) -> GenieResult<Self> {
        Ok(Self {
            missile_kind: input.read_u8()?,
            targetting_kind: input.read_u8()?,
            missile_hit_info: input.read_u8()?,
            missile_die_info: input.read_u8()?,
            area_effect_specials: input.read_u8()?,
            ballistics_ratio: input.read_f32::<LE>()?,
        })
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> GenieResult<()> {
        output.write_u8(self.missile_kind)?;
        output.write_u8(self.targetting_kind)?;
        output.write_u8(self.missile_hit_info)?;
        output.write_u8(self.missile_die_info)?;
        output.write_u8(self.area_effect_specials)?;
        output.write_f32::<LE>(self.ballistics_ratio)?;
        Ok(())
    }
}
