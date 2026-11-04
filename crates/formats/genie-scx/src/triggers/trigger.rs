//! A trigger ([`Trigger`]) and the trigger system that orders them ([`TriggerSystem`]).

use crate::Result;
use crate::triggers::{TriggerCondition, TriggerEffect, write_opt_string_key};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{
    ReadStringsExt, StringKey, read_le, read_opt_u32, write_i32_str, write_opt_i32_str,
};
use std::io::{Read, Write};
use std::iter::repeat;

/// A trigger, describing automatic interactive behaviours in a scenario.
// Each bool is a flag the format stores.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Trigger {
    pub(super) enabled: bool,
    pub(super) looping: bool,
    pub(super) name_id: i32,
    pub(super) is_objective: bool,
    pub(super) objective_order: i32,
    pub(super) start_time: u32,
    pub(super) description: Option<String>,
    pub(super) short_description_id: Option<StringKey>,
    pub(super) short_description: Option<String>,
    pub(super) display_short_description: bool,
    pub(super) short_description_state: u8,
    pub(super) mute_objective: bool,
    pub(super) name: Option<String>,
    pub(super) effects: Vec<TriggerEffect>,
    pub(super) effect_order: Vec<i32>,
    pub(super) conditions: Vec<TriggerCondition>,
    pub(super) condition_order: Vec<i32>,
    pub(super) make_header: bool,
}

impl Trigger {
    /// Read a trigger from an input stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or a string cannot be decoded.
    pub fn read_from(mut input: impl Read, version: f64) -> Result<Self> {
        let enabled = input.read_i32::<LE>()? != 0;
        let looping = input.read_i8()? != 0;
        let name_id = input.read_i32::<LE>()?;
        let is_objective = input.read_i8()? != 0;
        let objective_order = input.read_i32::<LE>()?;

        let mut make_header = false;
        let mut short_description_id = None;
        let mut short_description_state = 0;
        let mut display_short_description = false;
        let mut mute_objective = false;
        let start_time;
        if version >= 1.8 {
            make_header = input.read_u8()? != 0;
            short_description_id = read_opt_u32(&mut input)?;
            display_short_description = input.read_u8()? != 0;
            short_description_state = input.read_u8()?;
            start_time = input.read_u32::<LE>()?;
            mute_objective = input.read_u8()? != 0;
        } else {
            start_time = input.read_u32::<LE>()?;
        }

        let description = input.read_u32_length_prefixed_str()?;
        let name = input.read_u32_length_prefixed_str()?;
        let short_description = if version >= 1.8 {
            input.read_u32_length_prefixed_str()?
        } else {
            None
        };

        let num_effects = input.read_i32::<LE>()?;
        let mut effects = vec![];
        let mut effect_order = vec![];
        for _ in 0..num_effects {
            effects.push(TriggerEffect::read_from(&mut input, version)?);
        }
        for _ in 0..num_effects {
            effect_order.push(input.read_i32::<LE>()?);
        }

        let num_conditions = input.read_i32::<LE>()?;
        let mut conditions = vec![];
        let mut condition_order = vec![];
        for _ in 0..num_conditions {
            conditions.push(TriggerCondition::read_from(&mut input, version)?);
        }
        for _ in 0..num_conditions {
            condition_order.push(input.read_i32::<LE>()?);
        }

        Ok(Self {
            enabled,
            looping,
            name_id,
            is_objective,
            objective_order,
            start_time,
            description,
            short_description_id,
            short_description,
            display_short_description,
            short_description_state,
            mute_objective,
            name,
            effects,
            effect_order,
            conditions,
            condition_order,
            make_header,
        })
    }

    /// Write this trigger to an output stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, a string cannot be encoded, or the short description
    /// key is a name or does not fit a u32.
    // The format stores the effect and condition counts as u32.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write, version: f64) -> Result<()> {
        output.write_i32::<LE>(i32::from(self.enabled))?;
        output.write_i8(i8::from(self.looping))?;
        output.write_i32::<LE>(self.name_id)?;
        output.write_i8(i8::from(self.is_objective))?;
        output.write_i32::<LE>(self.objective_order)?;

        if version >= 1.8 {
            output.write_u8(u8::from(self.make_header))?;
            write_opt_string_key(&mut output, self.short_description_id.as_ref())?;
            output.write_u8(u8::from(self.display_short_description))?;
            output.write_u8(self.short_description_state)?;
            output.write_u32::<LE>(self.start_time)?;
            output.write_u8(u8::from(self.mute_objective))?;
        } else {
            output.write_u32::<LE>(self.start_time)?;
        }

        write_opt_i32_str(&mut output, &self.description)?;
        write_opt_i32_str(&mut output, &self.name)?;
        if version >= 1.8 {
            write_opt_i32_str(&mut output, &self.short_description)?;
        }

        output.write_u32::<LE>(self.effects.len() as u32)?;
        for effect in &self.effects {
            effect.write_to(&mut output, version)?;
        }
        for order in &self.effect_order {
            output.write_i32::<LE>(*order)?;
        }
        output.write_u32::<LE>(self.conditions.len() as u32)?;
        for condition in &self.conditions {
            condition.write_to(&mut output, version)?;
        }
        for order in &self.condition_order {
            output.write_i32::<LE>(*order)?;
        }

        Ok(())
    }

    /// Get the conditions in this trigger, in display order.
    // The order entries are indices, never negative in valid data.
    #[allow(clippy::cast_sign_loss)]
    pub fn conditions(&self) -> impl Iterator<Item = &TriggerCondition> {
        self.condition_order
            .iter()
            .map(move |index| &self.conditions[*index as usize])
    }

    /// Get the conditions in this trigger, unordered.
    pub fn conditions_unordered_mut(&mut self) -> impl Iterator<Item = &mut TriggerCondition> {
        self.conditions.iter_mut()
    }

    /// Get the effects in this trigger, in display order.
    // The order entries are indices, never negative in valid data.
    #[allow(clippy::cast_sign_loss)]
    pub fn effects(&self) -> impl Iterator<Item = &TriggerEffect> {
        self.effect_order
            .iter()
            .map(move |index| &self.effects[*index as usize])
    }

    /// Get the effects in this trigger, unordered.
    pub fn effects_unordered_mut(&mut self) -> impl Iterator<Item = &mut TriggerEffect> {
        self.effects.iter_mut()
    }
}

/// The trigger system maintains an ordered list  of triggers.
#[derive(Debug, Clone)]
pub struct TriggerSystem {
    pub(super) version: f64,
    pub(super) objectives_state: i8,
    pub(super) triggers: Vec<Trigger>,
    pub(super) trigger_order: Vec<i32>,
    pub(super) enabled_techs: Vec<u32>,
    pub(super) variable_values: Vec<u32>,
    pub(super) variable_names: Vec<String>,
}

impl Default for TriggerSystem {
    fn default() -> Self {
        Self {
            version: 1.6,
            objectives_state: 0,
            triggers: vec![],
            trigger_order: vec![],
            enabled_techs: vec![],
            variable_values: vec![0; 256],
            variable_names: vec![String::new(); 256],
        }
    }
}

impl TriggerSystem {
    /// Read a trigger system from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or a string cannot be decoded.
    ///
    /// # Panics
    ///
    /// Panics if a variable name has a variable number of 256 or more.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let version = input.read_f64::<LE>()?;
        log::debug!("Trigger system version {version}");
        let objectives_state = if version >= 1.5 { input.read_i8()? } else { 0 };

        let num_triggers = input.read_i32::<LE>()?;
        log::debug!("{num_triggers} triggers");

        let mut triggers = vec![];
        let mut trigger_order = vec![];
        for _ in 0..num_triggers {
            triggers.push(Trigger::read_from(&mut input, version)?);
        }
        if version >= 1.4 {
            for _ in 0..num_triggers {
                trigger_order.push(input.read_i32::<LE>()?);
            }
        } else {
            for i in 0..num_triggers {
                trigger_order.push(i);
            }
        }

        let (variable_values, enabled_techs, variable_names) = if version >= 2.2 {
            let mut variable_values = vec![0; 256];
            input.read_u32_into::<LE>(&mut variable_values)?;
            let enabled_techs = {
                let num_enabled_techs = input.read_u32::<LE>()?;
                read_le(&mut input, num_enabled_techs as usize, u32::from_le_bytes)?
            };
            let variable_names = {
                let num_var_names = input.read_u32::<LE>()?;
                let mut variable_names = vec![String::new(); 256];
                for _ in 0..num_var_names {
                    let id = input.read_u32::<LE>()?;
                    assert!(
                        id < 256,
                        "Unexpected variable number, this is probably a genie-scx bug"
                    );
                    variable_names[id as usize] =
                        input.read_u32_length_prefixed_str()?.unwrap_or_default();
                }
                variable_names
            };
            (variable_values, enabled_techs, variable_names)
        } else {
            (vec![], vec![], vec![])
        };

        Ok(Self {
            version,
            objectives_state,
            triggers,
            trigger_order,
            enabled_techs,
            variable_values,
            variable_names,
        })
    }

    /// Write the trigger system to an output stream with the given system version.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, a string cannot be encoded, or a trigger's short
    /// description key is a name or does not fit a u32.
    ///
    /// # Panics
    ///
    /// Fails if there are more than `u32::MAX` triggers.
    // The format stores the technology and variable name counts and the variable numbers as u32.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write, version: f64) -> Result<()> {
        output.write_f64::<LE>(version)?;
        if version >= 1.5 {
            output.write_i8(self.objectives_state)?;
        }
        output.write_u32::<LE>(
            u32::try_from(self.triggers.len()).map_err(|_| crate::Error::TooManyTriggersError)?,
        )?;
        for trigger in &self.triggers {
            trigger.write_to(&mut output, version)?;
        }
        if version >= 1.4 {
            for order in &self.trigger_order {
                output.write_i32::<LE>(*order)?;
            }
        }
        if version >= 2.2 {
            let padded_values = self
                .variable_values
                .iter()
                .copied()
                .chain(repeat(0))
                .take(256);
            for value in padded_values {
                output.write_u32::<LE>(value)?;
            }
            output.write_u32::<LE>(self.enabled_techs.len() as u32)?;
            for id in &self.enabled_techs {
                output.write_u32::<LE>(*id)?;
            }

            let custom_names = self
                .variable_names
                .iter()
                .enumerate()
                .filter(|(_index, name)| !name.is_empty());
            let num_custom_names = custom_names.clone().count();
            output.write_u32::<LE>(num_custom_names as u32)?;
            for (index, name) in custom_names {
                output.write_u32::<LE>(index as u32)?;
                write_i32_str(&mut output, name)?;
            }
        }
        Ok(())
    }

    /// Get the version of the trigger system data.
    #[must_use]
    pub const fn version(&self) -> f64 {
        self.version
    }

    /// Get the number of triggers in the trigger system.
    // The format stores the trigger count as a u32.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    pub const fn num_triggers(&self) -> u32 {
        self.triggers.len() as u32
    }

    /// Iterate over all triggers, in order.
    // The order entries are indices, never negative in valid data.
    #[allow(clippy::cast_sign_loss)]
    pub fn triggers(&self) -> impl Iterator<Item = &Trigger> {
        self.trigger_order
            .iter()
            .map(move |index| &self.triggers[*index as usize])
    }

    /// Iterate over all triggers, mutably and unordered.
    pub fn triggers_unordered_mut(&mut self) -> impl Iterator<Item = &mut Trigger> {
        self.triggers.iter_mut()
    }
}
