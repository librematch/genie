//! The file's version ([`Version`]), its strings and present lists, and the whole file
//! ([`DatFile`]).

use crate::de::{
    Civ, Effect, Graphic, PlayerColour, RandomMaps, Sound, Tech, TechTree, TerrainBlock,
    TerrainRestriction, UnitHeader,
};
use binrw::{BinRead, BinResult, BinWrite, Endian, binrw};
use flate2::read::DeflateDecoder;
use genie_error::{GenieError, GenieResult};
use std::borrow::Cow;
use std::io::Seek as IoSeek;
use std::io::{Cursor, Read, Write};

/// genieutils' `GameVersion` ordinal of a DE file: VER 7.1 is C14, VER 8.9 is C32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u8);

impl Version {
    /// The version of a file version string (`VER 7.1` to `VER 8.9`).
    #[must_use]
    pub fn of(file_version: &[u8; 8]) -> Option<Self> {
        const VERSIONS: [&[u8; 7]; 19] = [
            b"VER 7.1", b"VER 7.2", b"VER 7.3", b"VER 7.4", b"VER 7.5", b"VER 7.6", b"VER 7.7",
            b"VER 7.8", b"VER 7.9", b"VER 8.0", b"VER 8.1", b"VER 8.2", b"VER 8.3", b"VER 8.4",
            b"VER 8.5", b"VER 8.6", b"VER 8.7", b"VER 8.8", b"VER 8.9",
        ];
        VERSIONS
            .iter()
            .zip(14u8..)
            .find(|(magic, _)| file_version[..7] == magic[..])
            .map(|(_, number)| Self(number))
    }

    /// Whether the file is at least genieutils' `GV_C``version`.
    #[must_use]
    pub const fn at_least(self, version: u8) -> bool {
        self.0 >= version
    }
}

/// A string as DE stores it (`serializeDebugString`): the marker 0x0A60, a u16 length, the bytes.
#[binrw]
#[brw(little, magic = 0x0A60u16)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DebugString {
    /// Number of bytes in `bytes`.
    pub len: u16,
    /// The string's bytes, without a terminating NUL.
    #[br(count = len)]
    pub bytes: Vec<u8>,
}

impl DebugString {
    /// The bytes as text; invalid UTF-8 sequences become U+FFFD.
    #[must_use]
    pub fn as_str(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }
}

/// Read `pointers.len()` entries, one for each non-zero pointer (`serializeSubWithPointers`).
pub(super) fn read_present<T, A, R>(
    reader: &mut R,
    endian: Endian,
    pointers: &[i32],
    args: A,
) -> BinResult<Vec<Option<T>>>
where
    T: for<'args> BinRead<Args<'args> = A>,
    A: Clone,
    R: Read + IoSeek,
{
    pointers
        .iter()
        .map(|&pointer| {
            (pointer != 0)
                .then(|| T::read_options(reader, endian, args.clone()))
                .transpose()
        })
        .collect()
}

/// Write the present entries, in order.
pub(super) fn write_present<T, W>(
    list: &[Option<T>],
    writer: &mut W,
    endian: Endian,
) -> BinResult<()>
where
    T: for<'args> BinWrite<Args<'args> = ()>,
    W: Write + IoSeek,
{
    for entry in list.iter().flatten() {
        entry.write_options(writer, endian, ())?;
    }
    Ok(())
}

/// The whole file, inflated.
#[binrw]
#[brw(little)]
#[br(import(version: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct DatFile {
    /// The version string: `VER 7.1` to `VER 8.9`, then a NUL byte.
    pub file_version: [u8; 8],
    /// Number of terrain restrictions.
    pub terrain_restriction_count: i16,
    /// Number of terrains each terrain restriction has an entry for.
    pub terrains_used: i16,
    /// One in-memory pointer per terrain restriction, saved with the file.
    #[br(count = terrain_restriction_count)]
    pub float_ptr_terrain_tables: Vec<i32>,
    /// One in-memory pointer per terrain restriction, saved with the file.
    #[br(count = terrain_restriction_count)]
    pub terrain_pass_graphic_pointers: Vec<i32>,
    /// The terrain restrictions, indexed by restriction id.
    #[br(count = terrain_restriction_count, args { inner: (terrains_used,) })]
    pub terrain_restrictions: Vec<TerrainRestriction>,
    /// Number of player colours.
    pub player_colour_count: i16,
    /// The player colours.
    #[br(count = player_colour_count)]
    pub player_colours: Vec<PlayerColour>,
    /// Number of sounds.
    pub sound_count: i16,
    /// The sounds, indexed by sound id.
    #[br(count = sound_count)]
    pub sounds: Vec<Sound>,
    /// Number of graphic slots.
    pub graphic_count: i16,
    /// One pointer per graphic slot; a zero pointer marks an empty slot, which has no entry in
    /// `graphics`.
    #[br(count = graphic_count)]
    pub graphic_pointers: Vec<i32>,
    /// The graphics, indexed by graphic id; `None` where the slot's pointer is zero.
    #[br(parse_with = |reader, endian, (): ()| read_present(reader, endian, &graphic_pointers, ()))]
    #[bw(write_with = |list, writer, endian, (): ()| write_present(list, writer, endian))]
    pub graphics: Vec<Option<Graphic>>,
    /// The map settings and the terrain definitions.
    #[br(args(version))]
    pub terrain_block: TerrainBlock,
    /// The random map definitions.
    pub random_maps: RandomMaps,
    /// Number of tech effects.
    pub effect_count: i32,
    /// The tech effects, indexed by effect id.
    #[br(count = effect_count)]
    pub effects: Vec<Effect>,
    /// Number of unit headers (one per unit id).
    pub unit_header_count: i32,
    /// The shared task lists, indexed by unit id.
    #[br(count = unit_header_count, args { inner: (version,) })]
    pub unit_headers: Vec<UnitHeader>,
    /// Number of civilizations, Gaia included.
    pub civ_count: i16,
    /// The civilizations, indexed by civilization id (Gaia is 0).
    #[br(count = civ_count, args { inner: (version,) })]
    pub civs: Vec<Civ>,
    /// Number of techs.
    pub tech_count: i16,
    /// The techs, indexed by tech id.
    #[br(count = tech_count, args { inner: (version,) })]
    pub techs: Vec<Tech>,
    /// Not known yet; the first of seven engine-wide integers between the techs and the tech tree.
    pub time_slice: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub unit_kill_rate: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub unit_kill_total: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub unit_hit_point_rate: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub unit_hit_point_total: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub razing_kill_rate: i32,
    /// Not known yet (an engine-wide integer, presumably for score keeping).
    pub razing_kill_total: i32,
    /// The tech tree screen's layout.
    #[br(args(version))]
    pub tech_tree: TechTree,
}

impl DatFile {
    /// Read a compressed file as the game ships it.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorKind::Io`] error if reading or inflating fails, and otherwise the errors
    /// of [`Self::from_inflated`].
    ///
    /// [`ErrorKind::Io`]: genie_error::ErrorKind::Io
    pub fn read_from(input: impl Read) -> GenieResult<Self> {
        let mut data = Vec::new();
        // The byte count is `data.len()`.
        let _ = DeflateDecoder::new(input)
            .read_to_end(&mut data)
            .map_err(|error| {
                GenieError::io("a data file that does not read or inflate").with_source(error)
            })?;
        Self::from_inflated(&data)
    }

    /// Read inflated bytes.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorKind::Unsupported`] error if the data does not start with `VER 7.1` to
    /// `VER 8.9`, and an [`ErrorKind::InvalidInput`] one if the data does not follow the format.
    ///
    /// [`ErrorKind::InvalidInput`]: genie_error::ErrorKind::InvalidInput
    /// [`ErrorKind::Unsupported`]: genie_error::ErrorKind::Unsupported
    pub fn from_inflated(data: &[u8]) -> GenieResult<Self> {
        let version = |bytes: &[u8]| {
            GenieError::unsupported("a data file version other than VER 7.1 to VER 8.9")
                .with("version", String::from_utf8_lossy(bytes))
        };
        let mut start = [0u8; 8];
        start.copy_from_slice(data.get(..8).ok_or_else(|| version(&[]))?);
        let start_version = Version::of(&start).ok_or_else(|| version(&start))?;
        Self::read_le_args(&mut Cursor::new(data), (start_version,)).map_err(|error| {
            GenieError::invalid_input("a data file that does not follow the format")
                .with("binrw", error)
        })
    }

    /// The inflated bytes the file writes.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorKind::Io`] error if a field cannot be written.
    ///
    /// [`ErrorKind::Io`]: genie_error::ErrorKind::Io
    pub fn to_bytes(&self) -> GenieResult<Vec<u8>> {
        let mut out = Cursor::new(Vec::new());
        self.write_le(&mut out).map_err(|error| {
            GenieError::io("a data file that does not write").with("binrw", error)
        })?;
        Ok(out.into_inner())
    }

    /// The file's version.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::InvalidInput`] if `file_version` was changed to a string other than
    /// `VER 7.1` to `VER 8.9`; a file this module read always has a valid one.
    ///
    /// [`ErrorKind::InvalidInput`]: genie_error::ErrorKind::InvalidInput
    pub fn version(&self) -> GenieResult<Version> {
        Version::of(&self.file_version).ok_or_else(|| {
            GenieError::invalid_input("a file_version set outside VER 7.1 to VER 8.9")
                .with("version", String::from_utf8_lossy(&self.file_version))
        })
    }
}
