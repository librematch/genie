//! A trigger effect ([`TriggerEffect`]): the response when a trigger fires.

use crate::UnitKindId;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{ReadStringsExt, reserved, write_opt_i32_str};
use std::convert::TryInto;
use std::io::{Read, Write};

/// A trigger effect, describing the response when a trigger fires.
#[derive(Debug, Default, Clone)]
pub struct TriggerEffect {
    pub(super) effect_kind: i32,
    pub(super) properties: Vec<i32>,
    pub(super) chat_text: Option<String>,
    pub(super) audio_file: Option<String>,
    pub(super) objects: Vec<i32>,
}

impl TriggerEffect {
    /// Read a trigger effect from an input stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or a string cannot be decoded.
    // The format stores the property count as an i32; valid files never make it negative.
    #[allow(clippy::cast_sign_loss)]
    pub fn read_from(mut input: impl Read, version: f64) -> GenieResult<Self> {
        let effect_kind = input.read_i32::<LE>()?;
        let num_properties = if version > 1.0 {
            input.read_i32::<LE>()?
        } else {
            16
        };
        let mut properties = reserved(num_properties as usize);
        for _ in 0..num_properties {
            properties.push(input.read_i32::<LE>()?);
        }

        while properties.len() < 24 {
            properties.push(-1);
        }

        let chat_text = input.read_u32_length_prefixed_str()?;
        let audio_file = input.read_u32_length_prefixed_str()?;
        let mut objects = vec![];

        if version > 1.1 {
            for _ in 0..properties[4] {
                objects.push(input.read_i32::<LE>()?);
            }
        } else {
            objects.push(properties[4]);
            properties[4] = 1;
        }

        Ok(Self {
            effect_kind,
            properties,
            chat_text,
            audio_file,
            objects,
        })
    }

    /// Write a trigger effect to an output stream, with the given trigger system version.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or a string cannot be encoded.
    // The format stores the property count as an i32; the object count is never negative in
    // valid data.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub fn write_to(&self, mut output: impl Write, version: f64) -> GenieResult<()> {
        output.write_i32::<LE>(self.effect_kind)?;
        output.write_i32::<LE>(self.properties.len() as i32)?;
        for value in &self.properties {
            output.write_i32::<LE>(*value)?;
        }

        write_opt_i32_str(&mut output, &self.chat_text)?;
        write_opt_i32_str(&mut output, &self.audio_file)?;

        if version > 1.1 {
            for i in 0..self.num_objects() {
                output.write_i32::<LE>(*self.objects.get(i as usize).unwrap_or(&-1))?;
            }
        }

        Ok(())
    }

    /// Get the "AI Goal" value for this trigger effect.
    #[must_use]
    pub fn ai_goal(&self) -> i32 {
        self.properties[0]
    }

    /// Set the "AI Goal" value for this trigger effect.
    pub fn set_ai_goal(&mut self, ai_goal: i32) {
        self.properties[0] = ai_goal;
    }

    /// Get the "Amount" value for this trigger effect.
    #[must_use]
    pub fn amount(&self) -> i32 {
        self.properties[1]
    }

    /// Set the "Amount" value for this trigger effect.
    pub fn set_amount(&mut self, amount: i32) {
        self.properties[1] = amount;
    }

    /// Get the "Resource" value for this trigger effect.
    #[must_use]
    pub fn resource(&self) -> i32 {
        self.properties[2]
    }

    /// Set the "Resource" value for this trigger effect.
    pub fn set_resource(&mut self, resource: i32) {
        self.properties[2] = resource;
    }

    /// Get the "Diplomacy" value for this trigger effect.
    #[must_use]
    pub fn diplomacy(&self) -> i32 {
        self.properties[3]
    }

    /// Set the "Diplomacy" value for this trigger effect.
    pub fn set_diplomacy(&mut self, diplomacy: i32) {
        self.properties[3] = diplomacy;
    }

    /// Get the "Number of Objects" value for this trigger effect.
    #[must_use]
    pub fn num_objects(&self) -> i32 {
        self.properties[4]
    }

    /// Set the "Number of Objects" value for this trigger effect.
    pub fn set_num_objects(&mut self, num_objects: i32) {
        self.properties[4] = num_objects;
    }

    /// Get the "Object ID" value for this trigger effect.
    #[must_use]
    pub fn object_id(&self) -> i32 {
        self.properties[5]
    }

    /// Set the "Object ID" value for this trigger effect.
    pub fn set_object_id(&mut self, object_id: i32) {
        self.properties[5] = object_id;
    }

    /// Get the "Unit Type" value for this trigger effect.
    ///
    /// None if the stored value is not a unit type ID, e.g. -1 when none is set.
    #[must_use]
    pub fn unit_kind(&self) -> Option<UnitKindId> {
        self.properties[6].try_into().ok()
    }

    /// Set the "Unit Type" value for this trigger effect.
    pub fn set_unit_kind(&mut self, unit_kind: UnitKindId) {
        self.properties[6] = i32::from(unit_kind);
    }

    /// Get the "Source Player" value for this trigger effect.
    #[must_use]
    pub fn source_player_id(&self) -> i32 {
        self.properties[7]
    }

    /// Set the "Source Player" value for this trigger effect.
    pub fn set_source_player_id(&mut self, source_player_id: i32) {
        self.properties[7] = source_player_id;
    }

    /// Get the "Target Player" value for this trigger effect.
    #[must_use]
    pub fn target_player_id(&self) -> i32 {
        self.properties[8]
    }

    /// Set the "Target Player" value for this trigger effect.
    pub fn set_target_player_id(&mut self, target_player_id: i32) {
        self.properties[8] = target_player_id;
    }

    /// Get the "Tech ID" value for this trigger effect.
    #[must_use]
    pub fn technology_id(&self) -> i32 {
        self.properties[9]
    }

    /// Set the "Tech ID" value for this trigger effect.
    pub fn set_technology_id(&mut self, technology_id: i32) {
        self.properties[9] = technology_id;
    }

    /// Get the "Text ID" value for this trigger effect.
    #[must_use]
    pub fn text_id(&self) -> i32 {
        self.properties[10]
    }

    /// Set the "Text ID" value for this trigger effect.
    pub fn set_text_id(&mut self, text_id: i32) {
        self.properties[10] = text_id;
    }

    /// Get the "Sound ID" value for this trigger effect.
    #[must_use]
    pub fn sound_id(&self) -> i32 {
        self.properties[11]
    }

    /// Set the "Sound ID" value for this trigger effect.
    pub fn set_sound_id(&mut self, sound_id: i32) {
        self.properties[11] = sound_id;
    }

    /// Get the "Timer" value for this trigger effect.
    #[must_use]
    pub fn timer(&self) -> i32 {
        self.properties[12]
    }

    /// Set the "Timer" value for this trigger effect.
    pub fn set_timer(&mut self, timer: i32) {
        self.properties[12] = timer;
    }

    /// Get the "Trigger ID" value for this trigger effect.
    #[must_use]
    pub fn trigger_id(&self) -> i32 {
        self.properties[13]
    }

    /// Set the "Trigger ID" value for this trigger effect.
    pub fn set_trigger_id(&mut self, trigger_id: i32) {
        self.properties[13] = trigger_id;
    }

    /// Get the location this trigger effect applies to.
    #[must_use]
    pub fn location(&self) -> (i32, i32) {
        (self.properties[14], self.properties[15])
    }

    /// Set the location this trigger effect applies to.
    pub fn set_location(&mut self, location: (i32, i32)) {
        self.properties[14] = location.0;
        self.properties[15] = location.1;
    }

    /// Get the area this trigger effect applies to.
    #[must_use]
    pub fn area(&self) -> (i32, i32, i32, i32) {
        (
            self.properties[16],
            self.properties[17],
            self.properties[18],
            self.properties[19],
        )
    }

    /// Set the area this trigger effect applies to.
    pub fn set_area(&mut self, area: (i32, i32, i32, i32)) {
        self.properties[16] = area.0;
        self.properties[17] = area.1;
        self.properties[18] = area.2;
        self.properties[19] = area.3;
    }

    /// Get the "Object Group" value for this trigger effect.
    #[must_use]
    pub fn object_group(&self) -> i32 {
        self.properties[20]
    }

    /// Set the "Object Group" value for this trigger effect.
    pub fn set_object_group(&mut self, object_group: i32) {
        self.properties[20] = object_group;
    }

    /// Get the "Object Type" value for this trigger effect.
    ///
    /// None if the stored value is not a unit type ID, e.g. -1 when none is set.
    #[must_use]
    pub fn object_kind(&self) -> Option<UnitKindId> {
        self.properties[21].try_into().ok()
    }

    /// Set the "Object Type" value for this trigger effect.
    pub fn set_object_kind(&mut self, object_kind: UnitKindId) {
        self.properties[21] = i32::from(object_kind);
    }

    /// Get the "Line ID" value for this trigger effect.
    #[must_use]
    pub fn line_id(&self) -> i32 {
        self.properties[22]
    }

    /// Set the "Line ID" value for this trigger effect.
    pub fn set_line_id(&mut self, line_id: i32) {
        self.properties[22] = line_id;
    }

    /// Get the "Stance" value for this trigger effect.
    #[must_use]
    pub fn stance(&self) -> i32 {
        self.properties[23]
    }

    /// Set the "Stance" value for this trigger effect.
    pub fn set_stance(&mut self, stance: i32) {
        self.properties[23] = stance;
    }
}
