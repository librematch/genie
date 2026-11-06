use crate::civ::CivilizationId;
use crate::unit_kind::UnitKindId;
use arrayvec::{ArrayString, ArrayVec};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use encoding_rs::WINDOWS_1252;
use genie_error::{GenieResult, invalid_input};
pub use genie_support::TechId;
use genie_support::{MapIntoExt, StringKey, read_bytes, read_opt_u16, read_opt_u32};
use std::io::{Read, Write};

/// An effect command specifies an attribute change when a tech effect is triggered.
#[derive(Debug, Default, Clone, Copy)]
pub struct EffectCommand {
    /// The command.
    pub command_kind: u8,
    /// Command-dependent parameters.
    pub params: (i16, i16, i16, f32),
}

type TechEffectName = ArrayString<31>;

/// A tech effect's name takes more than the 31 bytes its field holds.
const NAME_TOO_LONG: &str = "a tech effect name longer than its field";

/// A tech effect is a group of attribute changes that are applied when the effect is triggered.
#[derive(Debug, Default, Clone)]
pub struct TechEffect {
    /// Name for the effect.
    name: TechEffectName,
    /// Attribute commands to execute when this effect is triggered.
    pub commands: Vec<EffectCommand>,
}

/// One of a tech's three resource costs (genieutils' `ResearchResourceCost`).
///
/// Despite the name, it holds a cost, not a reference to a [`TechEffect`].
#[derive(Debug, Default, Clone, Copy)]
pub struct TechEffectRef {
    /// The resource type paid; 0xFFFF marks an unused slot.
    pub effect_kind: u16,
    /// The amount of that resource.
    pub amount: u16,
    /// Whether the cost is deducted from the player's stockpile.
    pub enabled: bool,
}

/// A technology (research): its prerequisites, costs, where and how long it is researched, and
/// its strings and icon.
#[derive(Debug, Default, Clone)]
pub struct Tech {
    required_techs: ArrayVec<TechId, 6>,
    effects: ArrayVec<TechEffectRef, 3>,
    civilization_id: Option<CivilizationId>,
    full_tech_mode: u16,
    location: Option<UnitKindId>,
    language_dll_name: Option<StringKey>,
    language_dll_description: Option<StringKey>,
    time: u16,
    time2: u16,
    kind: u16,
    icon_id: Option<u16>,
    button_id: u8,
    language_dll_help: Option<StringKey>,
    help_page_id: u32,
    hotkey: Option<u32>,
    name: String,
}

impl EffectCommand {
    /// Read an effect command from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let command_kind = input.read_u8()?;
        let params = (
            input.read_i16::<LE>()?,
            input.read_i16::<LE>()?,
            input.read_i16::<LE>()?,
            input.read_f32::<LE>()?,
        );
        Ok(Self {
            command_kind,
            params,
        })
    }

    /// Write this effect command to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u8(self.command_kind)?;
        output.write_i16::<LE>(self.params.0)?;
        output.write_i16::<LE>(self.params.1)?;
        output.write_i16::<LE>(self.params.2)?;
        output.write_f32::<LE>(self.params.3)?;
        Ok(())
    }
}

impl TechEffect {
    /// Get the name of this effect.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Set the name of this effect.
    ///
    /// # Errors
    ///
    /// Returns an error if `name` requires more than 31 bytes of storage.
    pub fn set_name(&mut self, name: &str) -> GenieResult<()> {
        self.name = TechEffectName::from(name).map_err(|_| invalid_input(NAME_TOO_LONG))?;
        Ok(())
    }

    /// Read a tech effect from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the name, decoded from WINDOWS-1252, takes more than 31 bytes as UTF-8.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let mut effect = Self::default();
        let mut bytes = [0; 31];
        input.read_exact(&mut bytes)?;
        let bytes = &bytes[..bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len())];
        let (name, _encoding, _failed) = WINDOWS_1252.decode(bytes);
        effect.name = TechEffectName::from(&name).map_err(|_| invalid_input(NAME_TOO_LONG))?;

        let num_commands = input.read_u16::<LE>()?;
        for _ in 0..num_commands {
            effect.commands.push(EffectCommand::read_from(input)?);
        }

        Ok(effect)
    }

    /// Write this tech effect to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    // The format stores the command count as a u16; longer lists are truncated, as before.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        let mut buffer = [0; 31];
        buffer[..self.name.len()].copy_from_slice(self.name.as_bytes());
        output.write_all(&buffer)?;

        output.write_u16::<LE>(self.commands.len() as u16)?;
        for effect in &self.commands {
            effect.write_to(output)?;
        }
        Ok(())
    }
}

impl TechEffectRef {
    /// Read a resource cost from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        Ok(Self {
            effect_kind: input.read_u16::<LE>()?,
            amount: input.read_u16::<LE>()?,
            enabled: input.read_u8()? != 0,
        })
    }

    /// Write this resource cost to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(self, output: &mut W) -> GenieResult<()> {
        output.write_u16::<LE>(self.effect_kind)?;
        output.write_u16::<LE>(self.amount)?;
        output.write_u8(u8::from(self.enabled))?;
        Ok(())
    }
}

impl Tech {
    /// Get the name of this tech.
    #[must_use]
    pub const fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Read a tech from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let mut tech = Self::default();
        for _ in 0..6 {
            // 4 on some versions
            if let Some(tech_id) = read_opt_u16(&mut input)? {
                tech.required_techs.push(tech_id);
            }
        }
        for _ in 0..3 {
            let effect = TechEffectRef::read_from(&mut input)?;
            if effect.effect_kind != 0xFFFF {
                tech.effects.push(effect);
            }
        }
        let _num_required_techs = input.read_u16::<LE>()?;
        tech.civilization_id = read_opt_u16(&mut input)?;
        tech.full_tech_mode = input.read_u16::<LE>()?;
        tech.location = read_opt_u16(&mut input)?;
        tech.language_dll_name = read_opt_u16(&mut input)?;
        tech.language_dll_description = read_opt_u16(&mut input)?;
        tech.time = input.read_u16::<LE>()?;
        tech.time2 = input.read_u16::<LE>()?;
        tech.kind = input.read_u16::<LE>()?;
        tech.icon_id = read_opt_u16(&mut input)?;
        tech.button_id = input.read_u8()?;
        tech.language_dll_help = read_opt_u32(&mut input)?;
        tech.help_page_id = input.read_u32::<LE>()?;
        tech.hotkey = read_opt_u32(&mut input)?;
        tech.name = {
            let name_len = input.read_u16::<LE>()?;
            let bytes = read_bytes(&mut input, usize::from(name_len))?;
            let bytes = &bytes[..bytes
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(bytes.len())];
            let (name, _encoding, _failed) = WINDOWS_1252.decode(bytes);
            name.to_string()
        };
        Ok(tech)
    }

    /// Write this tech to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if a string key is a name rather than a number.
    // The format stores the name length as u16 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> GenieResult<()> {
        self.write_requirements(&mut output)?;
        output.write_u16::<LE>(self.civilization_id.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(self.full_tech_mode)?;
        output.write_u16::<LE>(self.location.map_into().unwrap_or(0xFFFF))?;
        output.write_u16::<LE>(dll_id_u16(self.language_dll_name.as_ref())?)?;
        output.write_u16::<LE>(dll_id_u16(self.language_dll_description.as_ref())?)?;
        output.write_u16::<LE>(self.time)?;
        output.write_u16::<LE>(self.time2)?;
        output.write_u16::<LE>(self.kind)?;
        output.write_u16::<LE>(self.icon_id.map_into().unwrap_or(0xFFFF))?;
        output.write_u8(self.button_id)?;
        output.write_u32::<LE>(match self.language_dll_help {
            Some(StringKey::Num(id)) => id,
            Some(_) => return Err(invalid_input(NAMED_STRING_KEY)),
            None => 0xFFFF_FFFF,
        })?;
        output.write_u32::<LE>(self.help_page_id)?;
        output.write_u32::<LE>(self.hotkey.map_into().unwrap_or(0xFFFF_FFFF))?;
        let (encoded, _encoding, _failed) = WINDOWS_1252.encode(&self.name);
        output.write_u16::<LE>(encoded.len() as u16)?;
        output.write_all(encoded.as_ref())?;
        Ok(())
    }

    /// The six required techs' slots (-1 for none), the three effects' (type 0xFFFF for none)
    /// and the count of required techs.
    // The count of required techs is at most 6.
    #[allow(clippy::cast_possible_truncation)]
    fn write_requirements(&self, mut output: impl Write) -> GenieResult<()> {
        for index in 0..6 {
            match self.required_techs.get(index) {
                Some(&id) => output.write_u16::<LE>(id.into())?,
                None => output.write_i16::<LE>(-1)?,
            }
        }
        for index in 0..3 {
            match self.effects.get(index) {
                Some(effect) => effect.write_to(&mut output)?,
                None => TechEffectRef {
                    effect_kind: 0xFFFF,
                    amount: 0,
                    enabled: false,
                }
                .write_to(&mut output)?,
            }
        }
        output.write_u16::<LE>(self.required_techs.len() as u16)?;
        Ok(())
    }
}

/// What a named string key is: a data file cannot hold one.
const NAMED_STRING_KEY: &str = "a named string key in a data file";

/// A language string's id as the format's u16, 0xFFFF for none.
///
/// # Errors
///
/// A named string key, which a data file cannot hold.
// The format stores the string IDs as u16 (truncated, as before).
#[allow(clippy::cast_possible_truncation)]
const fn dll_id_u16(key: Option<&StringKey>) -> GenieResult<u16> {
    match key {
        Some(StringKey::Num(id)) => Ok(*id as u16),
        Some(_) => Err(invalid_input(NAMED_STRING_KEY)),
        None => Ok(0xFFFF),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A language string's id fits the format's u16, none is 0xFFFF.
    #[test]
    fn language_string_id_is_its_number() {
        assert_eq!(dll_id_u16(Some(&StringKey::from(7198u32))).unwrap(), 7198);
        assert_eq!(dll_id_u16(None).unwrap(), 0xFFFF);
    }

    /// A named string key has no place in a data file.
    #[test]
    fn named_string_key_cannot_be_written() {
        let error = dll_id_u16(Some(&StringKey::from("NAME"))).unwrap_err();
        assert_eq!(error.kind(), genie_error::ErrorKind::InvalidInput);
    }

    /// The requirements: six tech slots (-1 for none), three effect slots (type 0xFFFF for
    /// none) and the count of required techs.
    #[test]
    fn tech_requirements_fill_their_slots() {
        let tech = Tech::default();
        let mut bytes = vec![];
        tech.write_requirements(&mut bytes).unwrap();
        assert_eq!(bytes[..12], [0xff; 12]);
        let effect_len = (bytes.len() - 12 - 2) / 3;
        assert_eq!(bytes[12..14], [0xff, 0xff]);
        assert_eq!(bytes[12 + effect_len..14 + effect_len], [0xff, 0xff]);
        assert_eq!(bytes[bytes.len() - 2..], [0, 0]);
    }
}
