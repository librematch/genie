use crate::Result;
use crate::element::{ReadableHeaderElement, WritableHeaderElement};
use crate::fits;
use crate::reader::RecordingHeaderReader;
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
pub use genie_dat::unit_type::{AttributeCost, ParseUnitBaseClassError, UnitBaseClass};
pub use genie_support::{StringKey, UnitTypeId};
use std::io::{Read, Write};

/// A player's copy of a unit type as the recording header stores it: only the attributes that can
/// change during a game, grouped by the base class that introduces them.
#[derive(Debug, Clone)]
pub struct CompactUnitType {
    /// The unit type's base class; it decides which of the attribute groups below are present.
    pub unit_base_class: UnitBaseClass,
    /// The attributes every unit type has.
    pub static_: StaticUnitAttributes,
    /// Present from the animated base class on.
    pub animated: Option<AnimatedUnitAttributes>,
    /// Present from the moving base class on.
    pub moving: Option<MovingUnitAttributes>,
    /// Present from the action base class on.
    pub action: Option<ActionUnitAttributes>,
    /// Present from the base combat base class on.
    pub base_combat: Option<BaseCombatUnitAttributes>,
    /// Present from the missile base class on.
    pub missile: Option<MissileUnitAttributes>,
    /// Present from the combat base class on.
    pub combat: Option<CombatUnitAttributes>,
    /// Present for buildings.
    pub building: Option<BuildingUnitAttributes>,
}

impl ReadableHeaderElement for CompactUnitType {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let unit_base_class = fits(input.read_u8()?)?;
        let static_ = StaticUnitAttributes::read_from(input)?;
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
            unit.animated = Some(AnimatedUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Moving {
            unit.moving = Some(MovingUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Action {
            unit.action = Some(ActionUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::BaseCombat {
            unit.base_combat = Some(BaseCombatUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Missile {
            unit.missile = Some(MissileUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Combat {
            unit.combat = Some(CombatUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Building {
            unit.building = Some(BuildingUnitAttributes::read_from(input)?);
        }
        Ok(unit)
    }
}

/// The attributes every unit type has (ids, hit points, line of sight, resources held …).
#[allow(dead_code)]
// The bools are the format's on/off flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct StaticUnitAttributes {
    id: UnitTypeId,
    copy_id: UnitTypeId,
    base_id: UnitTypeId,
    unit_class: u16,
    hotkey_id: u32,
    available: bool,
    death_object_id: Option<UnitTypeId>,
    string_id: Option<StringKey>,
    description_id: Option<StringKey>,
    flags: Option<u32>,
    help_string_id: Option<StringKey>,
    terrain_restriction: Option<u16>,
    hidden_in_editor: bool,
    is_queueable_tech: bool,
    hit_points: u16,
    line_of_sight: f32,
    garrison_capacity: u8,
    radius: (f32, f32),
    attribute_max_amount: u16,
    attribute_amount_held: f32,
    disabled: bool,
}

impl ReadableHeaderElement for StaticUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut attrs = Self {
            id: input.read_u16::<LE>()?.into(),
            copy_id: input.read_u16::<LE>()?.into(),
            base_id: input.read_u16::<LE>()?.into(),
            unit_class: input.read_u16::<LE>()?,
            hotkey_id: input.read_u32::<LE>()?,
            available: input.read_u8()? != 0,
            ..Default::default()
        };
        let hidden_in_editor = input.read_i8()?;
        // UserPatch data
        let hidden_flags = if hidden_in_editor == -16 {
            attrs.read_user_patch(input)?
        } else {
            hidden_in_editor
        };
        attrs.hidden_in_editor = hidden_flags != 0;
        // Community Patch
        attrs.is_queueable_tech = (hidden_flags & 2) == 2;
        attrs.hit_points = input.read_u16::<LE>()?;
        attrs.line_of_sight = input.read_f32::<LE>()?;
        attrs.garrison_capacity = input.read_u8()?;
        attrs.radius = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        attrs.attribute_max_amount = input.read_u16::<LE>()?;
        attrs.attribute_amount_held = input.read_f32::<LE>()?;
        attrs.disabled = input.read_u8()? != 0;
        Ok(attrs)
    }
}

impl StaticUnitAttributes {
    /// `UserPatch`'s fields behind a hidden flag of -16: the death object, the strings, the
    /// flags and the terrain restriction. Gives the hidden flags that follow them.
    fn read_user_patch<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> Result<i8> {
        self.death_object_id = match input.read_i16::<LE>()? {
            -1 => None,
            id => Some(fits(id)?),
        };
        self.string_id = Some(input.read_u16::<LE>()?.into());
        self.description_id = Some(input.read_u16::<LE>()?.into());
        self.flags = Some(input.read_u32::<LE>()?);
        self.help_string_id = Some(input.read_u32::<LE>()?.into());
        self.terrain_restriction = Some(input.read_u16::<LE>()?);
        Ok(input.read_i8()?)
    }
}

/// The attributes the animated base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnimatedUnitAttributes {
    /// The movement speed.
    pub speed: f32,
}

impl ReadableHeaderElement for AnimatedUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let speed = input.read_f32::<LE>()?;
        Ok(Self { speed })
    }
}

impl WritableHeaderElement for AnimatedUnitAttributes {
    fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_f32::<LE>(self.speed)?;
        Ok(())
    }
}

/// The attributes the moving base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct MovingUnitAttributes {
    /// The turning speed.
    pub turn_speed: f32,
}

impl ReadableHeaderElement for MovingUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let turn_speed = input.read_f32::<LE>()?;
        Ok(Self { turn_speed })
    }
}

impl WritableHeaderElement for MovingUnitAttributes {
    fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_f32::<LE>(self.turn_speed)?;
        Ok(())
    }
}

/// The attributes the action base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionUnitAttributes {
    /// The radius within which the unit looks for targets on its own.
    pub search_radius: f32,
    /// The rate at which it works (gathers, builds, repairs, heals …).
    pub work_rate: f32,
}

impl ReadableHeaderElement for ActionUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let search_radius = input.read_f32::<LE>()?;
        let work_rate = input.read_f32::<LE>()?;
        Ok(Self {
            search_radius,
            work_rate,
        })
    }
}

impl WritableHeaderElement for ActionUnitAttributes {
    fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_f32::<LE>(self.search_radius)?;
        output.write_f32::<LE>(self.work_rate)?;
        Ok(())
    }
}

/// One attack or armour value and the attack/armour class it applies to.
#[derive(Debug, Default, Clone, Copy)]
pub struct HitType {
    /// The attack/armour class.
    pub hit_type: u16,
    /// The attack or armour value for that class.
    pub amount: i16,
}

impl ReadableHeaderElement for HitType {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let hit_type = input.read_u16::<LE>()?;
        let amount = input.read_i16::<LE>()?;
        Ok(Self { hit_type, amount })
    }
}

/// The attributes the base combat base class adds: attack and armour.
#[derive(Debug, Default, Clone)]
pub struct BaseCombatUnitAttributes {
    /// The base armour, for attack classes the unit has no armour entry for. Stored as a `u8`
    /// before save version 11.52.
    pub base_armor: u16,
    /// The attack values, one per attack class.
    pub attacks: Vec<HitType>,
    /// The armour values, one per armour class.
    pub armors: Vec<HitType>,
    /// The time between two attacks (reload time).
    pub attack_speed: f32,
    /// The maximum attack range.
    pub weapon_range_max: f32,
    /// The chance to hit, in percent.
    pub base_hit_chance: u16,
    /// The unit type of the projectile it fires, if any.
    pub projectile_object_id: Option<UnitTypeId>,
    /// The terrain table that changes its armour by the terrain it stands on, if any.
    pub defense_terrain_bonus: Option<u16>,
    /// A second maximum attack range. Not known yet how it differs from `weapon_range_max`.
    pub weapon_range_max_2: f32,
    /// The radius of its area (splash) damage.
    pub area_of_effect: f32,
    /// The minimum attack range.
    pub weapon_range_min: f32,
}

impl ReadableHeaderElement for BaseCombatUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut attrs = Self {
            base_armor: if input.version() >= 11.52 {
                input.read_u16::<LE>()?
            } else {
                input.read_u8()?.into()
            },
            ..Default::default()
        };
        let num_attacks = input.read_u16::<LE>()?;
        for _ in 0..num_attacks {
            attrs.attacks.push(HitType::read_from(input)?);
        }
        let num_armors = input.read_u16::<LE>()?;
        for _ in 0..num_armors {
            attrs.armors.push(HitType::read_from(input)?);
        }
        attrs.attack_speed = input.read_f32::<LE>()?;
        attrs.weapon_range_max = input.read_f32::<LE>()?;
        attrs.base_hit_chance = input.read_u16::<LE>()?;
        attrs.projectile_object_id = match input.read_i16::<LE>()? {
            -1 => None,
            id => Some(fits(id)?),
        };
        attrs.defense_terrain_bonus = match input.read_i16::<LE>()? {
            -1 => None,
            id => Some(fits(id)?),
        };
        attrs.weapon_range_max_2 = input.read_f32::<LE>()?;
        attrs.area_of_effect = input.read_f32::<LE>()?;
        attrs.weapon_range_min = input.read_f32::<LE>()?;
        Ok(attrs)
    }
}

/// The attributes the missile (projectile) base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissileUnitAttributes {
    /// How the projectile aims at its target.
    pub targeting_type: u8,
}

impl ReadableHeaderElement for MissileUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let targeting_type = input.read_u8()?;
        Ok(Self { targeting_type })
    }
}

impl WritableHeaderElement for MissileUnitAttributes {
    fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.targeting_type)?;
        Ok(())
    }
}

/// The attributes the combat base class adds: costs, training, and `UserPatch`'s extra data.
#[derive(Debug, Default, Clone)]
pub struct CombatUnitAttributes {
    /// The resource costs, up to three; entries with a negative resource type are left out.
    pub costs: ArrayVec<AttributeCost, 3>,
    /// The training time.
    pub create_time: u16,
    /// `UserPatch` only: the pierce armour before changes.
    pub original_pierce_armor: Option<u16>,
    /// `UserPatch` only: the melee armour before changes.
    pub original_armor: Option<u16>,
    /// `UserPatch` only: the attack before changes.
    pub original_weapon: Option<u16>,
    /// `UserPatch` only: the attack range before changes.
    pub original_weapon_range: Option<f32>,
    /// `UserPatch` only: the area-of-effect (blast) level.
    pub area_effect_level: Option<u8>,
    /// `UserPatch` only: the delay of the attack within its animation.
    pub frame_delay: Option<u16>,
    /// `UserPatch` only: the building that trains the unit.
    pub create_at_building: Option<UnitTypeId>,
    /// `UserPatch` only: the unit's button position in that building's menu.
    pub create_button: Option<i8>,
    /// `UserPatch` only: the damage multiplier for attacks from behind.
    pub rear_attack_modifier: Option<f32>,
    /// `UserPatch` only: the hero flags.
    pub hero_flag: Option<u8>,
    /// The number of projectiles fired per attack.
    pub volley_fire_amount: f32,
    /// The largest number of projectiles per attack (with garrisoned units).
    pub max_attacks_in_volley: u8,
}

impl ReadableHeaderElement for CombatUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut attrs = Self::default();
        for _ in 0..3 {
            let attr = AttributeCost::read_from(&mut *input)?;
            if attr.attribute_type >= 0 {
                attrs.costs.push(attr);
            }
        }
        let create_time = input.read_u16::<LE>()?;
        // UserPatch data
        attrs.create_time = if create_time == u16::MAX - 15 {
            attrs.read_user_patch(input)?
        } else {
            create_time
        };
        attrs.volley_fire_amount = input.read_f32::<LE>()?;
        attrs.max_attacks_in_volley = input.read_u8()?;
        Ok(attrs)
    }
}

impl CombatUnitAttributes {
    /// `UserPatch`'s fields behind a create time of 65520: the original armours and weapon,
    /// the area effect, the delay, the building and button, the rear attack and the hero
    /// flag. Gives the create time that follows them.
    fn read_user_patch<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> Result<u16> {
        self.original_pierce_armor = Some(input.read_u16::<LE>()?);
        self.original_armor = Some(input.read_u16::<LE>()?);
        self.original_weapon = Some(input.read_u16::<LE>()?);
        self.original_weapon_range = Some(input.read_f32::<LE>()?);
        self.area_effect_level = Some(input.read_u8()?);
        self.frame_delay = Some(input.read_u16::<LE>()?);
        self.create_at_building = match input.read_i16::<LE>()? {
            -1 => None,
            id => Some(fits(id)?),
        };
        self.create_button = Some(input.read_i8()?);
        self.rear_attack_modifier = Some(input.read_f32::<LE>()?);
        self.hero_flag = Some(input.read_u8()?);
        Ok(input.read_u16::<LE>()?)
    }
}

/// The attributes the building base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct BuildingUnitAttributes {
    /// The building's facet: which angle of its graphic it shows.
    pub facet: i16,
    // &facet + 62
    needs_name: Option<u8>,
    // &facet + 12
    needs_name_2: Option<u8>,
    /// `UserPatch` only: the rate at which garrisoned units heal.
    pub garrison_heal_rate: Option<f32>,
}

impl ReadableHeaderElement for BuildingUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut attrs = Self::default();
        let facet = input.read_i16::<LE>()?;
        // UserPatch data
        if facet == 32767 {
            attrs.needs_name = Some(input.read_u8()?);
            attrs.needs_name_2 = Some(input.read_u8()?);
            attrs.garrison_heal_rate = Some(input.read_f32::<LE>()?);
            attrs.facet = input.read_i16::<LE>()?;
        } else {
            attrs.facet = facet;
        }
        Ok(attrs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Bytes, read_all};

    /// A unit type's static head up to its hidden flag.
    fn static_head(hidden: u8) -> Bytes {
        Bytes::default()
            .u16(83)
            .u16(83)
            .u16(83)
            .u16(4)
            .i32(16000)
            .u8(1)
            .u8(hidden)
    }

    /// The static tail after the hidden flags: hit points, sight, garrison, radius, the
    /// attribute and the disabled flag.
    fn static_tail(bytes: Bytes) -> Bytes {
        bytes
            .u16(25)
            .floats(&[4.0])
            .u8(0)
            .floats(&[0.2, 0.3])
            .u16(10)
            .floats(&[0.0])
            .u8(0)
    }

    #[test]
    fn static_unit_type_reads_its_hidden_flag() {
        let unit = read_all::<StaticUnitAttributes>(&static_tail(static_head(2)).0, 11.97);
        assert_eq!(u16::from(unit.id), 83);
        assert_eq!((unit.unit_class, unit.hotkey_id), (4, 16000));
        assert!(unit.available && unit.hidden_in_editor && unit.is_queueable_tech);
        assert_eq!(unit.death_object_id, None);
        assert_eq!(unit.string_id, None);
        assert_eq!(unit.hit_points, 25);
        assert_eq!(unit.radius, (0.2, 0.3));
        assert_eq!(unit.attribute_max_amount, 10);
        assert!(!unit.disabled);
    }

    /// A hidden flag of -16 is `UserPatch`'s: its fields, then the real hidden flags.
    #[test]
    fn user_patch_unit_type_reads_its_fields_before_the_hidden_flags() {
        let bytes = static_head(0xF0)
            .u16(3)
            .u16(5001)
            .u16(5002)
            .i32(0x10)
            .i32(5003)
            .u16(7)
            .u8(1);
        let unit = read_all::<StaticUnitAttributes>(&static_tail(bytes).0, 11.97);
        assert_eq!(unit.death_object_id.map(u16::from), Some(3));
        assert_eq!(unit.string_id, Some(StringKey::from(5001u32)));
        assert_eq!(unit.description_id, Some(StringKey::from(5002u32)));
        assert_eq!(unit.flags, Some(0x10));
        assert_eq!(unit.help_string_id, Some(StringKey::from(5003u32)));
        assert_eq!(unit.terrain_restriction, Some(7));
        assert!(unit.hidden_in_editor && !unit.is_queueable_tech);
    }

    /// Three costs (a negative type is none), then the create time.
    fn costs(create_time: u16) -> Bytes {
        Bytes::default()
            .u16(0)
            .u16(60)
            .u16(1)
            .u16(0xFFFF)
            .u16(0)
            .u16(0)
            .u16(3)
            .u16(20)
            .u16(1)
            .u16(create_time)
    }

    #[test]
    fn combat_unit_type_keeps_its_real_costs() {
        let bytes = costs(21).floats(&[1.5]).u8(2).0;
        let unit = read_all::<CombatUnitAttributes>(&bytes, 11.97);
        assert_eq!(
            unit.costs
                .iter()
                .map(|cost| (cost.attribute_type, cost.amount))
                .collect::<Vec<_>>(),
            [(0, 60), (3, 20)]
        );
        assert_eq!(unit.create_time, 21);
        assert_eq!(unit.original_armor, None);
        assert_eq!(unit.volley_fire_amount.to_bits(), 1.5_f32.to_bits());
        assert_eq!(unit.max_attacks_in_volley, 2);
    }

    /// A create time of 65520 is `UserPatch`'s: its fields, then the real create time.
    #[test]
    fn user_patch_combat_unit_type_reads_its_fields_before_the_create_time() {
        let bytes = costs(u16::MAX - 15)
            .u16(1)
            .u16(2)
            .u16(3)
            .floats(&[5.0])
            .u8(1)
            .u16(4)
            .u16(12)
            .u8(6)
            .floats(&[0.5])
            .u8(1)
            .u16(30)
            .floats(&[1.0])
            .u8(1)
            .0;
        let unit = read_all::<CombatUnitAttributes>(&bytes, 11.97);
        assert_eq!(
            (
                unit.original_pierce_armor,
                unit.original_armor,
                unit.original_weapon
            ),
            (Some(1), Some(2), Some(3))
        );
        assert_eq!(
            unit.original_weapon_range.map(f32::to_bits),
            Some(5.0_f32.to_bits())
        );
        assert_eq!(
            (unit.area_effect_level, unit.frame_delay),
            (Some(1), Some(4))
        );
        assert_eq!(unit.create_at_building.map(u16::from), Some(12));
        assert_eq!(unit.create_button, Some(6));
        assert_eq!(
            unit.rear_attack_modifier.map(f32::to_bits),
            Some(0.5_f32.to_bits())
        );
        assert_eq!(unit.hero_flag, Some(1));
        assert_eq!(unit.create_time, 30);
    }

    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    #[test]
    fn cmp_unit_base_class() {
        assert_eq!(UnitBaseClass::Static, UnitBaseClass::Static);
        assert!(UnitBaseClass::Static < UnitBaseClass::Animated);
        assert!(UnitBaseClass::Static < UnitBaseClass::Doppelganger);
        assert!(UnitBaseClass::Animated < UnitBaseClass::Doppelganger);
        assert!(!(UnitBaseClass::Moving < UnitBaseClass::Doppelganger));
        assert!(!(UnitBaseClass::Moving > UnitBaseClass::Doppelganger));
    }
}
