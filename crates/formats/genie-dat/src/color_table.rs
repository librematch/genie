use crate::conv::fits;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
pub use jascpal::PaletteIndex;
use std::io::{Read, Write};

/// Player colour data.
#[derive(Debug, Clone, Copy)]
pub struct ColorTable {
    /// ID of this player colour.
    pub id: i32,
    /// Base palette index for this player colour.
    pub base: PaletteIndex,
    /// The palette index to use for unit outlines when they are obscured by buildings or trees.
    pub unit_outline_color: PaletteIndex,
    /// The two palette indices of the selection outline drawn around this player's units.
    pub unit_selection_colors: (PaletteIndex, PaletteIndex),
    /// Palette indices for this colour on the minimap.
    pub minimap_colors: (PaletteIndex, PaletteIndex, PaletteIndex),
    /// Color table to use for this player colour in the in-game statistics in the bottom right.
    pub statistics_text_color: i32,
}

impl ColorTable {
    /// Read a colour table from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if a palette index is outside 0 to 255.
    pub fn read_from<R: Read>(input: &mut R) -> GenieResult<Self> {
        let id = input.read_i32::<LE>()?;
        let base = fits(input.read_i32::<LE>()?)?;
        let unit_outline_color = fits(input.read_i32::<LE>()?)?;
        let unit_selection_colors = (
            fits(input.read_i32::<LE>()?)?,
            fits(input.read_i32::<LE>()?)?,
        );
        let minimap_colors = (
            fits(input.read_i32::<LE>()?)?,
            fits(input.read_i32::<LE>()?)?,
            fits(input.read_i32::<LE>()?)?,
        );
        let statistics_text_color = input.read_i32::<LE>()?;

        Ok(Self {
            id,
            base,
            unit_outline_color,
            unit_selection_colors,
            minimap_colors,
            statistics_text_color,
        })
    }

    /// Write this colour table to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_i32::<LE>(self.id)?;
        output.write_i32::<LE>(self.base.into())?;
        output.write_i32::<LE>(self.unit_outline_color.into())?;
        output.write_i32::<LE>(self.unit_selection_colors.0.into())?;
        output.write_i32::<LE>(self.unit_selection_colors.1.into())?;
        output.write_i32::<LE>(self.minimap_colors.0.into())?;
        output.write_i32::<LE>(self.minimap_colors.1.into())?;
        output.write_i32::<LE>(self.minimap_colors.2.into())?;
        output.write_i32::<LE>(self.statistics_text_color)?;
        Ok(())
    }
}
