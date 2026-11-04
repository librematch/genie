//! JASC Palette file reader/writer.
//!
//! Parses and stringifes palette files containing any number of colours.
//!
//! JASC palette files follow a simple format:
//! ```txt
//! JASC-PAL
//! 0100
//! $num_colors
//! $rgb...
//! ```
//!
//! Colours are represented using the [`rgb`](https://crates.io/crates/rgb) crate.
//!
//! ## Example
//! ```rust
//! # fn main() -> genie_error::GenieResult<()> {
//! use std::io::Cursor;
//! use jascpal::{Palette, PaletteIndex, Color};
//! let cursor = Cursor::new(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_vec());
//! let pal = Palette::read_from(cursor)?;
//! assert_eq!(pal[PaletteIndex::from(0)], Color { r: 0, g: 0, b: 0 });
//! assert_eq!(pal[PaletteIndex::from(1)], Color { r: 255, g: 255, b: 255 });
//! let mut pal = pal;
//! pal[PaletteIndex::from(1)] = Color { r: 0, g: 255, b: 255 };
//! pal.add(Color { r: 255, g: 0, b: 0 });
//! assert_eq!(
//!     pal.to_string(),
//!     "JASC-PAL\r\n0100\r\n3\r\n0 0 0\r\n0 255 255\r\n255 0 0\r\n".to_string()
//! );
//! # Ok(())
//! # }
//! ```

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]

use genie_error::{GenieError, GenieResult};
use nom::Err as NomErr;
use nom::IResult;
use nom::bytes::complete::tag;
use nom::character::complete::{digit1, one_of};
use nom::combinator::{map, map_res};
use nom::error::{Error as NomError, ErrorKind as NomErrorKind};
use nom::multi::many1;
use rgb::RGB;
use std::convert::{TryFrom, TryInto};

use std::fmt::{Display, Error as FmtError, Formatter};
use std::io::{Read, Write};
use std::ops::{Index, IndexMut};
use std::slice::Iter;
use std::str::{self, FromStr};
use std::vec::IntoIter as VecIntoIter;

/// Represents an RGB colour.
pub type Color = RGB<u8>;

/// A palette index.
///
/// ## Example
/// ```rust
/// use jascpal::{Palette, PaletteIndex, Color};
/// let pal = Palette::default();
/// assert_eq!(pal[PaletteIndex::from(0)], Color { r: 0, g: 0, b: 0 });
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PaletteIndex(u8);
impl From<u8> for PaletteIndex {
    #[inline]
    fn from(index: u8) -> Self {
        Self(index)
    }
}

impl TryFrom<i32> for PaletteIndex {
    type Error = GenieError;
    #[inline]
    fn try_from(index: i32) -> GenieResult<Self> {
        index.try_into().map(Self).map_err(|try_from_int_error| {
            GenieError::invalid_input("a palette index past a byte")
                .with("index", index)
                .with_source(try_from_int_error)
        })
    }
}

impl From<PaletteIndex> for u8 {
    #[inline]
    fn from(palette_index: PaletteIndex) -> Self {
        palette_index.0
    }
}

impl From<PaletteIndex> for i32 {
    #[inline]
    fn from(palette_index: PaletteIndex) -> Self {
        palette_index.0.into()
    }
}

impl From<PaletteIndex> for usize {
    #[inline]
    fn from(palette_index: PaletteIndex) -> Self {
        palette_index.0.into()
    }
}

impl FromStr for PaletteIndex {
    type Err = GenieError;
    #[inline]
    fn from_str(input: &str) -> GenieResult<Self> {
        input.parse().map(Self).map_err(|parse_int_error| {
            GenieError::invalid_input("a palette index that is not a byte")
                .with_source(parse_int_error)
        })
    }
}

/// Eat any amount of whitespace: ASCII spaces and tabs.
fn whitespace(input: &[u8]) -> IResult<&[u8], ()> {
    map(many1(one_of(&b" \t"[..])), |_| ())(input)
}

/// Parse an UTF-8 byte slice using `FromStr`.
fn parse_bytes<Parsed: FromStr>(string: &[u8]) -> Result<Parsed, ()> {
    str::from_utf8(string)
        .map_err(|_| ())
        .and_then(|text| text.parse().map_err(|_| ()))
}

/// Eat and return a number.
fn parse_number<Parsed: FromStr>(input: &[u8]) -> IResult<&[u8], Parsed> {
    map_res(digit1, parse_bytes)(input)
}

/// Eat and return an RGB value (three `u8` components).
fn parse_rgb(input: &[u8]) -> IResult<&[u8], Color> {
    let (input, red) = parse_number(input)?;
    let (input, ()) = whitespace(input)?;
    let (input, green) = parse_number(input)?;
    let (input, ()) = whitespace(input)?;
    let (input, blue) = parse_number(input)?;

    Ok((
        input,
        RGB {
            r: red,
            g: green,
            b: blue,
        },
    ))
}

/// The bytes of the shortest colour line, `0 0 0\r\n`.
const SHORTEST_COLOR: usize = 7;

/// Parse a header and colours from an input stream.
fn parse(input: &[u8]) -> IResult<&[u8], Vec<Color>> {
    let (input, _) = tag(b"JASC-PAL\r\n")(input)?;
    let (input, _) = tag(b"0100\r\n")(input)?;
    let (input, num_colors): (_, i32) = parse_number(input)?;
    let (input, _) = tag(b"\r\n")(input)?;

    // `digit1` takes no sign, so the count is never negative; a colour's line holds at least
    // `0 0 0\r\n`, so a count the bytes left cannot hold fails before room is reserved for it
    let count = usize::try_from(num_colors).unwrap_or(0);
    if input.len() / SHORTEST_COLOR < count {
        return Err(NomErr::Failure(NomError::new(
            input,
            NomErrorKind::TooLarge,
        )));
    }
    let mut colors = Vec::with_capacity(count);
    let mut input = input;
    for _ in 0..num_colors {
        let (remaining, color) = parse_rgb(input)?;
        colors.push(color);
        let (remaining, _) = tag(b"\r\n")(remaining)?;
        input = remaining;
    }

    Ok((input, colors))
}

/// A palette that does not parse.
const BAD_PALETTE: GenieError = GenieError::invalid_input("a palette that does not parse");

/// A Palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    colors: Vec<Color>,
}

impl From<Vec<Color>> for Palette {
    /// Create a palette from a vector of colours.
    #[inline]
    fn from(colors: Vec<Color>) -> Self {
        Self { colors }
    }
}

impl Default for Palette {
    /// Create a palette with 256 colours, all black.
    #[inline]
    fn default() -> Self {
        Self::from(vec![Color::default(); 256])
    }
}

impl Palette {
    /// Create an empty palette.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::from(vec![])
    }

    /// Read a palette from an input stream.
    ///
    /// # Errors
    ///
    /// An `Io` error if reading fails, and an `InvalidInput` one if the input is not a JASC
    /// palette.
    #[inline]
    pub fn read_from(mut input: impl Read) -> GenieResult<Self> {
        let mut buffer = vec![];
        // The byte count is `buffer.len()`.
        let _ = input.read_to_end(&mut buffer)?;
        let (_remaining, pal) = parse(&buffer).map_err(|_| BAD_PALETTE)?;
        Ok(Self::from(pal))
    }

    /// Write a palette to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    #[inline]
    pub fn write_to(&self, mut output: impl Write) -> GenieResult<()> {
        Ok(write!(output, "{self}")?)
    }

    /// Return the colours in a Vec so it can be manipulated.
    ///
    /// It can later be wrapped back into a Palette instance using `Palette::from(vec)`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Vec<Color> {
        self.colors
    }

    /// Return the colours as a slice.
    #[inline]
    #[must_use]
    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    /// Return the colours as a mutable vector.
    #[inline]
    pub const fn colors_mut(&mut self) -> &mut Vec<Color> {
        &mut self.colors
    }

    /// Returns the number of colours in this palette.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.colors.len()
    }

    /// Iterate over the colours.
    #[inline]
    pub fn iter(&self) -> Iter<'_, Color> {
        self.colors.iter()
    }

    /// Returns `true` if this palette contains 0 colours.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Add a colour at the end of the palette.
    #[inline]
    pub fn add(&mut self, color: Color) {
        self.colors.push(color);
    }

    /// Serialize the palette to a byte vector.
    #[inline]
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

impl Index<PaletteIndex> for Palette {
    type Output = Color;
    /// Get the colour at the given index.
    #[inline]
    fn index(&self, index: PaletteIndex) -> &Self::Output {
        let index: usize = index.into();
        &self.colors[index]
    }
}

impl IndexMut<PaletteIndex> for Palette {
    /// Get the colour at the given index.
    #[inline]
    fn index_mut(&mut self, index: PaletteIndex) -> &mut Self::Output {
        let index: usize = index.into();
        &mut self.colors[index]
    }
}

impl FromStr for Palette {
    type Err = GenieError;
    /// Parse a palette from a UTF-8 string.
    #[inline]
    fn from_str(input: &str) -> GenieResult<Self> {
        Self::read_from(input.as_bytes())
    }
}

impl IntoIterator for Palette {
    type Item = Color;
    type IntoIter = VecIntoIter<Color>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.colors.into_iter()
    }
}

impl<'palette> IntoIterator for &'palette Palette {
    type Item = &'palette Color;
    type IntoIter = Iter<'palette, Color>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.colors.iter()
    }
}

impl Display for Palette {
    /// Write the palette in the JASC-PAL text format.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), FmtError> {
        formatter.write_str("JASC-PAL\r\n0100\r\n")?;
        write!(formatter, "{}\r\n", self.colors.len())?;
        for rgb in &self.colors {
            write!(formatter, "{} {} {}\r\n", rgb.r, rgb.g, rgb.b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A count of colours the bytes left cannot hold fails before room is reserved for it.
    #[test]
    fn count_past_the_bytes_fails() {
        assert!(parse(b"JASC-PAL\r\n0100\r\n999999999\r\n0 0 0\r\n").is_err());
    }

    #[test]
    fn parse_test() {
        let (rest, colors) =
            parse(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n").expect("failed to parse");
        assert_eq!(rest, b"");
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn from_str() {
        assert_eq!(
            "JASC-PAL\r\n0100\r\n3\r\n255 0 255\r\n0 0 255\r\n0 255 0\r\n"
                .parse::<Palette>()
                .expect("failed to parse"),
            Palette::from(vec![
                RGB {
                    r: 255,
                    g: 0,
                    b: 255
                },
                RGB { r: 0, g: 0, b: 255 },
                RGB { r: 0, g: 255, b: 0 },
            ])
        );
    }

    #[test]
    fn make() {
        let mut pal = Palette::new();
        for index in 0..=255 {
            pal.add(RGB {
                r: index,
                g: 255 - index,
                b: 127_u8.wrapping_add(index),
            });
        }
        let text = pal.to_string();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("JASC-PAL"));
        assert_eq!(lines.next(), Some("0100"));
        assert_eq!(lines.next(), Some("256"));
        for index in 0..=255 {
            assert_eq!(
                lines.next(),
                Some(format!("{} {} {}", index, 255 - index, 127_u8.wrapping_add(index)).as_str())
            );
        }
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn it_works() {
        let cursor = Cursor::new(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_vec());
        let pal = Palette::read_from(cursor).expect("failed to parse");
        assert_eq!(
            pal.colors,
            vec![
                RGB { r: 0, g: 0, b: 0 },
                RGB {
                    r: 255,
                    g: 255,
                    b: 255
                },
            ]
        );
        assert_eq!(
            pal.to_string(),
            "JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_string()
        );
    }
}
