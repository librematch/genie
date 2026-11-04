//! The base combat, missile and combat base classes' attributes.

use crate::ObjectId;
use crate::element::{ReadHeader, WriteHeader};
use crate::reader::RecordingHeaderReader;
use crate::unit::UnitAi;
use byteorder::{LE, ReadBytesExt};
use genie_dat::unit_kind::UnitKind;
use genie_error::GenieResult;
use genie_support::read_opt_u32;
use std::io::Read;

/// The attributes the base combat base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct BaseCombatUnitAttributes {
    /// The formation the unit is in.
    pub formation_id: u8,
    /// The unit's row in the formation.
    pub formation_row: u8,
    /// The unit's column in the formation.
    pub formation_column: u8,
    /// The time until the next attack.
    pub attack_timer: f32,
    /// Not known yet; a capture flag.
    pub capture_flag: u8,
    /// Not known yet.
    pub multi_unified_points: u8,
    /// Not known yet.
    pub large_object_radius: u8,
    /// The number of attacks the unit has made.
    pub attack_count: u32,
}

impl ReadHeader for BaseCombatUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self::default();
        if input.version() >= 9.05 {
            attrs.formation_id = input.read_u8()?;
            attrs.formation_row = input.read_u8()?;
            attrs.formation_column = input.read_u8()?;
        }
        attrs.attack_timer = input.read_f32::<LE>()?;
        if input.version() >= 2.01 {
            attrs.capture_flag = input.read_u8()?;
        }
        if input.version() >= 9.09 {
            attrs.multi_unified_points = input.read_u8()?;
            attrs.large_object_radius = input.read_u8()?;
        }
        if input.version() >= 10.02 {
            attrs.attack_count = input.read_u32::<LE>()?;
        }
        Ok(attrs)
    }
}

impl WriteHeader for BaseCombatUnitAttributes {}

/// The attributes the missile (projectile) base class adds.
#[derive(Debug, Default, Clone)]
pub struct MissileUnitAttributes {
    /// The furthest the projectile flies.
    pub max_range: f32,
    /// The object that fired the projectile.
    pub fired_from_id: ObjectId,
    /// The object's own copy of its unit type, if it has one.
    pub own_base: Option<UnitKind>,
}

impl ReadHeader for MissileUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            max_range: input.read_f32::<LE>()?,
            fired_from_id: input.read_u32::<LE>()?.into(),
            own_base: read_own_base(input)?,
        })
    }
}

/// The unit's own copy of its type, behind a flag: a unit whose type a technology changed
/// for it alone.
fn read_own_base<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Option<UnitKind>> {
    if input.read_u8()? == 0 {
        return Ok(None);
    }
    let version = input.version();
    Ok(Some(UnitKind::read_from(&mut *input, version)?))
}

impl WriteHeader for MissileUnitAttributes {}

/// The attributes the combat base class adds.
#[derive(Debug, Default, Clone)]
pub struct CombatUnitAttributes {
    /// Not known yet; tracks the next volley.
    pub next_volley: u8,
    /// Whether the unit uses its special attack animation.
    pub using_special_attack_animation: u8,
    /// The object's own copy of its unit type, if it has one.
    pub own_base: Option<UnitKind>,
    /// Six attribute amounts; not known yet which.
    pub attribute_amounts: [u16; 6],
    /// The decay timer; stored from save version 9.16 on.
    pub decay_timer: u16,
    /// Not known yet; stored from save version 9.61 on.
    pub raider_build_countdown: u32,
    /// Not known yet; stored from save version 9.65 on.
    pub locked_down_count: u32,
    /// The number of units garrisoned inside; stored from save version 11.56 on.
    pub inside_garrison_count: u8,
    /// The unit's AI, if it has one.
    pub unit_ai: Option<UnitAi>,
    /// The town bell state; stored from save version 10.30 on.
    pub town_bell_flag: i8,
    /// The object the town bell sent the unit to, if any.
    pub town_bell_target_id: Option<ObjectId>,
    /// The position the town bell sent the unit to, if any.
    pub town_bell_target_location: Option<(f32, f32)>,
    /// A second town bell target object; stored from save version 11.71 on.
    pub town_bell_target_id_2: Option<ObjectId>,
    /// The town bell target's type; stored from save version 11.71 on.
    pub town_bell_target_kind: u32,
    /// The town bell action; stored from save version 11.74 on.
    pub town_bell_action: u32,
    /// The hit point regeneration timer (as Berserkers have); stored from save version
    /// 10.42 on.
    pub berserker_timer: f32,
    /// The number of units building the object; stored from save version 10.46 on.
    pub num_builders: u8,
    /// The number of units healing the object; stored from save version 11.69 on.
    pub num_healers: u8,
}

impl ReadHeader for CombatUnitAttributes {
    // Reads the combat unit's fields in file order, section by section.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self {
            next_volley: input.read_u8()?,
            using_special_attack_animation: input.read_u8()?,
            own_base: read_own_base(input)?,
            ..Default::default()
        };
        attrs.read_amounts_and_counters(input)?;
        let has_ai = input.read_u32::<LE>()? != 0;
        if has_ai {
            attrs.unit_ai = Some(UnitAi::read_from(input)?);
        }
        attrs.read_town_bell(input)?;
        attrs.read_last_counters(input)?;
        Ok(attrs)
    }
}

impl CombatUnitAttributes {
    /// The six attribute amounts, then the decay timer (9.16), the raider's build countdown
    /// (9.61), the locked-down count (9.65) and the count inside a garrison (11.56).
    fn read_amounts_and_counters<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        for amount in &mut self.attribute_amounts {
            *amount = input.read_u16::<LE>()?;
        }
        if input.version() >= 9.16 {
            self.decay_timer = input.read_u16::<LE>()?;
        }
        if input.version() >= 9.61 {
            self.raider_build_countdown = input.read_u32::<LE>()?;
        }
        if input.version() >= 9.65 {
            self.locked_down_count = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.56 {
            self.inside_garrison_count = input.read_u8()?;
        }
        Ok(())
    }

    /// The town bell: its flag, target and location (10.30; a negative x is no location),
    /// the second target and its type (11.71) and the action (11.74).
    fn read_town_bell<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        if input.version() >= 10.30 {
            self.town_bell_flag = input.read_i8()?;
            self.town_bell_target_id = read_opt_u32(input)?;
            let location = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
            self.town_bell_target_location = (location.0 >= 0.0).then_some(location);
        }
        if input.version() >= 11.71 {
            self.town_bell_target_id_2 = read_opt_u32(input)?;
            self.town_bell_target_kind = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.74 {
            self.town_bell_action = input.read_u32::<LE>()?;
        }
        Ok(())
    }

    /// The berserker's timer (10.42), the builders (10.46) and the healers (11.69).
    fn read_last_counters<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.version() >= 10.42 {
            self.berserker_timer = input.read_f32::<LE>()?;
        }
        if input.version() >= 10.46 {
            self.num_builders = input.read_u8()?;
        }
        if input.version() >= 11.69 {
            self.num_healers = input.read_u8()?;
        }
        Ok(())
    }
}

impl WriteHeader for CombatUnitAttributes {}

#[cfg(test)]
mod tests;
