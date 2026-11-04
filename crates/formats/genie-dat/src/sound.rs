use crate::FileVersion;
use crate::conv::fits;
use arrayvec::ArrayString;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::{GenieResult, invalid_input};
use genie_support::{fallible_try_from, fallible_try_into, infallible_try_into};
use std::io::{Read, Write};

/// An ID identifying a sound.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SoundId(u16);
impl From<u16> for SoundId {
    fn from(n: u16) -> Self {
        Self(n)
    }
}

impl From<SoundId> for u16 {
    fn from(n: SoundId) -> Self {
        n.0
    }
}

impl From<SoundId> for usize {
    fn from(n: SoundId) -> Self {
        n.0.into()
    }
}

fallible_try_into!(SoundId, i16);
infallible_try_into!(SoundId, i32);
infallible_try_into!(SoundId, u32);
fallible_try_from!(SoundId, i16);
fallible_try_from!(SoundId, i32);
fallible_try_from!(SoundId, u32);

/// A "conceptual" sound, consisting of one or a group of sound files.
///
/// Items can be picked depending on the player's civilization, and depending on the probabilities
/// for each file.
#[derive(Debug, Default, Clone)]
pub struct Sound {
    /// Unique ID for this sound.
    pub id: SoundId,
    /// TODO document.
    pub play_delay: i16,
    /// TODO document.
    pub cache_time: i32,
    /// List of sound files in this sound.
    pub items: Vec<SoundItem>,
}

/// A single sound file.
#[derive(Debug, Default, Clone, Copy)]
pub struct SoundItem {
    /// Internal file name for this sound file.
    pub filename: ArrayString<13>,
    /// DRS file ID for this sound file.
    pub resource_id: i32,
    /// The probability out of 100% that this file will be used for any given playback.
    pub probability: i16,
    /// Use this file for this civilization ID only.
    pub civilization: Option<i16>,
    /// File icon set (TODO what does this do?)
    pub icon_set: Option<i16>,
}

impl SoundItem {
    /// Read this sound item from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R, _version: FileVersion) -> GenieResult<Self> {
        let mut item = Self::default();
        // TODO actually use this
        let mut filename = [0u8; 13];
        input.read_exact(&mut filename)?;
        item.resource_id = input.read_i32::<LE>()?;
        item.probability = input.read_i16::<LE>()?;
        // AoK only
        item.civilization = Some(input.read_i16::<LE>()?);
        item.icon_set = Some(input.read_i16::<LE>()?);

        Ok(item)
    }

    /// Write this sound item to an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if `civilization` or `icon_set` is `None`: the format always stores both.
    pub fn write_to<W: Write>(&self, output: &mut W, _version: FileVersion) -> GenieResult<()> {
        output.write_all(&[0; 13])?;
        output.write_i32::<LE>(self.resource_id)?;
        output.write_i16::<LE>(self.probability)?;
        // AoK only, must both be set
        let (Some(civilization), Some(icon_set)) = (self.civilization, self.icon_set) else {
            return Err(invalid_input(
                "a sound item without its civilization or icon set",
            ));
        };
        output.write_i16::<LE>(civilization)?;
        output.write_i16::<LE>(icon_set)?;
        Ok(())
    }
}

impl Sound {
    /// Read this sound from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R, version: FileVersion) -> GenieResult<Self> {
        let mut sound = Self {
            id: input.read_u16::<LE>()?.into(),
            play_delay: input.read_i16::<LE>()?,
            ..Default::default()
        };
        let num_items = input.read_u16::<LE>()?;
        sound.cache_time = input.read_i32::<LE>()?;
        if version.is_de2() {
            let _total_probability = input.read_u16::<LE>()?;
        }
        for _ in 0..num_items {
            sound.items.push(SoundItem::read_from(input, version)?);
        }
        Ok(sound)
    }

    /// Write this sound to an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are more than `u16::MAX` sound files, or if a sound file lacks its
    /// `civilization` or `icon_set` (see [`SoundItem::write_to`]).
    pub fn write_to<W: Write>(&self, output: &mut W, version: FileVersion) -> GenieResult<()> {
        output.write_u16::<LE>(self.id.into())?;
        output.write_i16::<LE>(self.play_delay)?;
        output.write_u16::<LE>(fits(self.len())?)?;
        output.write_i32::<LE>(self.cache_time)?;
        for item in &self.items {
            item.write_to(output, version)?;
        }
        Ok(())
    }

    /// Get the number of sound files that are part of this "conceptual" sound.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns true if there are no sound files.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
