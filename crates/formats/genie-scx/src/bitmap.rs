//! Handles bitmap files embedded in the scenario file.

use crate::Result;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use rgb::RGBA8;
use std::io::{Read, Write};

/// Bitmap header info.
#[derive(Debug, Default, Clone)]
pub struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    xpels_per_meter: i32,
    ypels_per_meter: i32,
    clr_used: u32,
    clr_important: u32,
    colors: Vec<RGBA8>,
}

impl BitmapInfo {
    /// Read a bitmap header info structure from a byte stream.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut bitmap = Self {
            size: input.read_u32::<LE>()?,
            width: input.read_i32::<LE>()?,
            height: input.read_i32::<LE>()?,
            planes: input.read_u16::<LE>()?,
            bit_count: input.read_u16::<LE>()?,
            compression: input.read_u32::<LE>()?,
            size_image: input.read_u32::<LE>()?,
            xpels_per_meter: input.read_i32::<LE>()?,
            ypels_per_meter: input.read_i32::<LE>()?,
            clr_used: input.read_u32::<LE>()?,
            clr_important: input.read_u32::<LE>()?,
            ..Default::default()
        };

        for _ in 0..256 {
            let r = input.read_u8()?;
            let g = input.read_u8()?;
            let b = input.read_u8()?;
            let a = input.read_u8()?;
            bitmap.colors.push(RGBA8 { r, g, b, a });
        }

        Ok(bitmap)
    }

    #[allow(unused)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        assert_eq!(self.colors.len(), 256);

        output.write_u32::<LE>(self.size)?;
        output.write_i32::<LE>(self.width)?;
        output.write_i32::<LE>(self.height)?;
        output.write_u16::<LE>(self.planes)?;
        output.write_u16::<LE>(self.bit_count)?;
        output.write_u32::<LE>(self.compression)?;
        output.write_u32::<LE>(self.size_image)?;
        output.write_i32::<LE>(self.xpels_per_meter)?;
        output.write_i32::<LE>(self.ypels_per_meter)?;
        output.write_u32::<LE>(self.clr_used)?;
        output.write_u32::<LE>(self.clr_important)?;
        for color in &self.colors {
            output.write_u8(color.r)?;
            output.write_u8(color.g)?;
            output.write_u8(color.b)?;
            output.write_u8(color.a)?;
        }

        Ok(())
    }
}

/// A Genie-style bitmap file: a typical BMP with some metadata.
#[derive(Debug)]
pub struct Bitmap {
    own_memory: u32,
    width: u32,
    height: u32,
    orientation: u16,
    info: BitmapInfo,
    pixels: Vec<u8>,
}

impl Bitmap {
    pub fn read_from(mut input: impl Read) -> Result<Option<Self>> {
        let own_memory = input.read_u32::<LE>()?;
        let width = input.read_u32::<LE>()?;
        let height = input.read_u32::<LE>()?;
        let orientation = input.read_u16::<LE>()?;

        if width > 0 && height > 0 {
            let info = BitmapInfo::read_from(&mut input)?;
            let aligned_width = height * ((width + 3) & !3);
            let mut pixels = vec![0u8; aligned_width as usize];
            input.read_exact(&mut pixels)?;
            Ok(Some(Self {
                own_memory,
                width,
                height,
                orientation,
                info,
                pixels,
            }))
        } else {
            Ok(None)
        }
    }

    #[allow(unused)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(self.own_memory)?;
        output.write_u32::<LE>(self.width)?;
        output.write_u32::<LE>(self.height)?;
        output.write_u16::<LE>(self.orientation)?;
        self.info.write_to(&mut output)?;
        output.write_all(&self.pixels)?;
        Ok(())
    }

    #[allow(unused)]
    pub fn write_empty(mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(0)?;
        output.write_u32::<LE>(0)?;
        output.write_u32::<LE>(0)?;
        output.write_u16::<LE>(0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bitmap with a size: its header, the info with its 256 colours and the rows of pixels
    /// padded to 4 bytes; written back byte for byte. One without a size has no more.
    #[test]
    fn bitmap_writes_back_the_bytes_it_read() {
        let mut bytes = vec![];
        for word in [1u32, 2, 3] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(1u16.to_le_bytes());
        bytes.extend((0..40u8).map(|b| b.wrapping_mul(7)));
        bytes.extend((0..1024u32).map(|i| u8::try_from(i % 251).unwrap()));
        bytes.extend([9u8; 12]);
        let bitmap = Bitmap::read_from(bytes.as_slice()).unwrap().unwrap();
        assert_eq!(bitmap.info.colors.len(), 256);
        assert_eq!(bitmap.pixels.len(), 12);
        let mut out = vec![];
        bitmap.write_to(&mut out).unwrap();
        assert_eq!(out, bytes);
        let mut empty = vec![];
        Bitmap::write_empty(&mut empty).unwrap();
        assert!(Bitmap::read_from(empty.as_slice()).unwrap().is_none());
    }
}
