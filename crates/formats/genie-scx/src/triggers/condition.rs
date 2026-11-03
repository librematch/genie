//! A trigger condition ([`TriggerCondition`]): when a trigger can fire.

use crate::{Result, UnitKindId};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use std::convert::TryInto;
use std::io::{Read, Write};

/// A trigger condition, describing when a trigger can fire.
#[derive(Debug, Default, Clone)]
pub struct TriggerCondition {
    pub(super) condition_kind: i32,
    pub(super) properties: Vec<i32>,
}

impl TriggerCondition {
    /// Read a trigger condition from an input stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    // The format stores the property count as an i32; valid files never make it negative.
    #[allow(clippy::cast_sign_loss)]
    pub fn read_from(mut input: impl Read, version: f64) -> Result<Self> {
        let condition_kind = input.read_i32::<LE>()?;
        let num_properties = if version > 1.0 {
            input.read_i32::<LE>()?
        } else {
            13
        };
        let mut properties = Vec::with_capacity(num_properties as usize);
        for _ in 0..num_properties {
            properties.push(input.read_i32::<LE>()?);
        }

        while properties.len() < 18 {
            properties.push(-1);
        }

        Ok(Self {
            condition_kind,
            properties,
        })
    }

    /// Write this trigger condition to an output stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    // The format stores the property count as an i32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn write_to(&self, mut output: impl Write, version: f64) -> Result<()> {
        output.write_i32::<LE>(self.condition_kind)?;
        if version > 1.0 {
            output.write_i32::<LE>(self.properties.len() as i32)?;
            for value in &self.properties {
                output.write_i32::<LE>(*value)?;
            }
        } else {
            for i in 0..13 {
                output.write_i32::<LE>(*self.properties.get(i).unwrap_or(&0))?;
            }
        }

        Ok(())
    }

    /// Get the "amount" value for this trigger condition.
    #[must_use]
    pub fn amount(&self) -> i32 {
        self.properties[0]
    }

    /// Set the "amount" value for this trigger condition.
    pub fn set_amount(&mut self, amount: i32) {
        self.properties[0] = amount;
    }

    /// Get the "resource" value for this trigger condition.
    #[must_use]
    pub fn resource(&self) -> i32 {
        self.properties[1]
    }

    /// Set the "resource" value for this trigger condition.
    pub fn set_resource(&mut self, resource: i32) {
        self.properties[1] = resource;
    }

    /// Get the "primary object" value for this trigger condition.
    #[must_use]
    pub fn primary_object(&self) -> i32 {
        self.properties[2]
    }

    /// Set the "primary object" value for this trigger condition.
    pub fn set_primary_object(&mut self, primary_object: i32) {
        self.properties[2] = primary_object;
    }

    /// Get the "secondary object" value for this trigger condition.
    #[must_use]
    pub fn secondary_object(&self) -> i32 {
        self.properties[3]
    }

    /// Set the "secondary object" value for this trigger condition.
    pub fn set_secondary_object(&mut self, secondary_object: i32) {
        self.properties[3] = secondary_object;
    }

    /// Get the raw "unit type" value for this trigger condition.
    #[must_use]
    pub fn raw_unit_kind(&self) -> i32 {
        self.properties[4]
    }

    /// Set the raw "unit type" value for this trigger condition.
    pub fn set_raw_unit_kind(&mut self, unit_kind: i32) {
        self.properties[4] = unit_kind;
    }

    /// Get the "unit type" value for this trigger condition.
    ///
    /// None if the stored value is not a unit type ID, e.g. -1 when none is set.
    #[must_use]
    pub fn unit_kind(&self) -> Option<UnitKindId> {
        self.properties[4].try_into().ok()
    }

    /// Set the "unit type" value for this trigger condition.
    pub fn set_unit_kind(&mut self, unit_kind: UnitKindId) {
        self.properties[4] = unit_kind.into();
    }

    /// Get the "player ID" value for this trigger condition.
    #[must_use]
    pub fn player_id(&self) -> i32 {
        self.properties[5]
    }

    /// Set the "player ID" value for this trigger condition.
    pub fn set_player_id(&mut self, player_id: i32) {
        self.properties[5] = player_id;
    }

    /// Get the "Tech ID" value for this trigger condition.
    #[must_use]
    pub fn technology_id(&self) -> i32 {
        self.properties[6]
    }

    /// Set the "Tech ID" value for this trigger condition.
    pub fn set_technology_id(&mut self, technology_id: i32) {
        self.properties[6] = technology_id;
    }

    /// Get the "Timer" value for this trigger condition.
    #[must_use]
    pub fn timer(&self) -> i32 {
        self.properties[7]
    }

    /// Set the "Timer" value for this trigger condition.
    pub fn set_timer(&mut self, timer: i32) {
        self.properties[7] = timer;
    }

    /// Get the "Trigger ID" value for this trigger condition.
    #[must_use]
    pub fn trigger_id(&self) -> i32 {
        self.properties[8]
    }

    /// Set the "Trigger ID" value for this trigger condition.
    pub fn set_trigger_id(&mut self, trigger_id: i32) {
        self.properties[8] = trigger_id;
    }

    /// Get the area this trigger condition applies to.
    ///
    /// -1 values indicate no area is set.
    #[must_use]
    pub fn area(&self) -> (i32, i32, i32, i32) {
        (
            self.properties[9],
            self.properties[10],
            self.properties[11],
            self.properties[12],
        )
    }

    /// Set the area this trigger condition applies to.
    ///
    /// -1 values indicate no area is set.
    pub fn set_area(&mut self, area: (i32, i32, i32, i32)) {
        self.properties[9] = area.0;
        self.properties[10] = area.1;
        self.properties[11] = area.2;
        self.properties[12] = area.3;
    }

    /// Get the "unit group" value for this trigger condition.
    #[must_use]
    pub fn unit_group(&self) -> i32 {
        self.properties[13]
    }

    /// Set the "unit group" value for this trigger condition.
    pub fn set_unit_group(&mut self, unit_group: i32) {
        self.properties[13] = unit_group;
    }

    /// Get the "Object Type" value for this trigger condition.
    ///
    /// None if the stored value is not a unit type ID, e.g. -1 when none is set.
    #[must_use]
    pub fn object_kind(&self) -> Option<UnitKindId> {
        self.properties[14].try_into().ok()
    }

    /// Set the "Object Type" value for this trigger condition.
    pub fn set_object_kind(&mut self, object_kind: UnitKindId) {
        self.properties[14] = i32::from(object_kind);
    }

    /// Get the "AI Signal" value for this trigger condition.
    #[must_use]
    pub fn ai_signal(&self) -> i32 {
        self.properties[15]
    }

    /// Set the "AI Signal" value for this trigger condition.
    pub fn set_ai_signal(&mut self, ai_signal: i32) {
        self.properties[15] = ai_signal;
    }

    /// Get the "Inverted" value for this trigger condition.
    #[must_use]
    pub fn inverted(&self) -> bool {
        self.properties[16] == 1
    }

    /// Set the "Inverted" value for this trigger condition.
    pub fn set_inverted(&mut self, inverted: i32) {
        self.properties[16] = inverted;
    }
}
