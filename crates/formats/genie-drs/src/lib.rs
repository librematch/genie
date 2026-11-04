//! .drs is the resource archive file format for the Genie Engine, used by Age of Empires 1/2 and
//! Star Wars: Galactic Battlegrounds. .drs files contain tables, each of which contain resources
//! of a single type. Resources are identified by a numeric identifier.
//!
//! ## Example
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::fs::File;
//! use genie_drs::DrsReader;
//!
//! let mut file = File::open("test.drs")?;
//! let drs = DrsReader::new(&mut file)?;
//!
//! for table in drs.tables() {
//!     for resource in table.resources() {
//!         let content = drs.read_resource(&mut file, table.resource_kind, resource.id)?;
//!         println!("{}: {:?}", resource.id, std::str::from_utf8(&content)?);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]

use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::{GenieError, GenieResult};
use sorted_vec::SortedVec;
use std::io::{Read, Write};
use std::slice;
use std::str;

mod read;
mod write;

use core::str::FromStr;
pub use read::DrsReader;
use std::fmt::{Debug, Display, Error as FmtError, Formatter};
pub use write::{DrsWriter, InMemoryStrategy, ReserveDirectoryStrategy, Strategy as WriteStrategy};

/// A DRS version string.
type DrsVersion = [u8; 4];

/// A resource type name.
///
/// In a .drs archive, type names are represented as 4 bytes. They are laid out in reverse order and
/// padded with ASCII space characters (`' '`). For example, the "slp" resource type is stored as `" pls"`.
///
/// ## Examples
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use genie_drs::ResourceKind;
/// let slp = ResourceKind::try_from(*b" pls")?;
/// assert_eq!(&slp, "slp", "implements AsRef<str>");
/// assert_eq!(slp.to_string(), "slp", "implements ToString");
/// assert_eq!(slp, ResourceKind::try_from(*b" pls")?);
/// let bina = "bina".parse::<ResourceKind>()?;
/// assert_ne!(slp, bina);
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceKind([u8; 4]);
impl ResourceKind {
    #[inline]
    fn write_to<W: Write>(self, output: &mut W) -> GenieResult<()> {
        let mut bytes = [0; 4];
        bytes.copy_from_slice(&self.0);
        bytes.reverse();
        output.write_all(&bytes)?;
        Ok(())
    }
}

impl PartialEq<str> for ResourceKind {
    fn eq(&self, ext: &str) -> bool {
        let me: &str = self.as_ref();
        me == ext
    }
}

impl AsRef<str> for ResourceKind {
    fn as_ref(&self) -> &str {
        // the constructors take only valid UTF-8, so the text is never empty by a failure
        str::from_utf8(&self.0[..]).map_or("", str::trim)
    }
}

impl Display for ResourceKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), FmtError> {
        formatter.write_str(self.as_ref())
    }
}

/// A resource type that is not at most 4 bytes of UTF-8.
const BAD_RESOURCE_KIND: GenieError =
    GenieError::invalid_input("a resource type that is not at most 4 bytes of UTF-8");

impl FromStr for ResourceKind {
    type Err = GenieError;
    /// Parse a resource type from a string, with error handling.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.as_bytes().try_into()
    }
}

/// Parse a resource type as the archive stores it: reversed, with a UTF-8 name.
impl TryFrom<[u8; 4]> for ResourceKind {
    type Error = GenieError;
    fn try_from(mut bytes: [u8; 4]) -> Result<Self, Self::Error> {
        bytes.reverse();
        let _ = str::from_utf8(&bytes).map_err(|_| BAD_RESOURCE_KIND)?;
        Ok(Self(bytes))
    }
}

/// Parse a resource type from a byte slice of at most 4 bytes of UTF-8.
impl TryFrom<&[u8]> for ResourceKind {
    type Error = GenieError;
    fn try_from(raw: &[u8]) -> Result<Self, Self::Error> {
        let _ = str::from_utf8(raw).map_err(|_| BAD_RESOURCE_KIND)?;
        let mut bytes = [b' '; 4];
        bytes
            .get_mut(..raw.len())
            .ok_or(BAD_RESOURCE_KIND)?
            .copy_from_slice(raw);
        Ok(Self(bytes))
    }
}

/// Parse a resource type from a string of at most 4 bytes.
impl TryFrom<&str> for ResourceKind {
    type Error = GenieError;
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        text.as_bytes().try_into()
    }
}

/// What names a resource type for [`DrsWriter::add`]: a [`ResourceKind`] itself, or its name as
/// a string (at most 4 bytes) or as bytes.
pub trait IntoResourceKind {
    /// The resource type.
    ///
    /// # Errors
    ///
    /// An `InvalidData` error for a name of more than 4 bytes or one that is not UTF-8.
    fn into_resource_kind(self) -> GenieResult<ResourceKind>;
}

impl IntoResourceKind for ResourceKind {
    fn into_resource_kind(self) -> GenieResult<ResourceKind> {
        Ok(self)
    }
}

impl IntoResourceKind for &str {
    fn into_resource_kind(self) -> GenieResult<ResourceKind> {
        ResourceKind::try_from(self)
    }
}

impl IntoResourceKind for &[u8] {
    fn into_resource_kind(self) -> GenieResult<ResourceKind> {
        ResourceKind::try_from(self)
    }
}

/// The DRS archive header.
#[derive(Clone, Copy)]
pub struct DrsHeader {
    /// A copyright message.
    banner_msg: [u8; 40],
    /// File version. (always "1.00")
    version: DrsVersion,
    /// File password / identifier.
    password: [u8; 12],
    /// The amount of resource types (tables).
    num_resource_kinds: u32,
    /// Size in bytes of the metadata and tables. Resource contents start at this offset.
    directory_size: u32,
}

impl Default for DrsHeader {
    fn default() -> Self {
        Self {
            banner_msg: *b"Copyright (c) 1997 Ensemble Studios.\x1a\x00\x00\x00",
            version: *b"1.00",
            password: *b"tribe\x00\x00\x00\x00\x00\x00\x00",
            num_resource_kinds: 0,
            directory_size: 0,
        }
    }
}

impl DrsHeader {
    #[inline]
    /// Read a DRS archive header from a `Read`able handle.
    fn from<R: Read>(source: &mut R) -> GenieResult<Self> {
        let mut banner_msg = [0_u8; 40];
        let mut version = [0_u8; 4];
        let mut password = [0_u8; 12];
        source.read_exact(&mut banner_msg)?;
        source.read_exact(&mut version)?;
        source.read_exact(&mut password)?;
        let num_resource_kinds = source.read_u32::<LE>()?;
        let directory_size = source.read_u32::<LE>()?;
        Ok(Self {
            banner_msg,
            version,
            password,
            num_resource_kinds,
            directory_size,
        })
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_all(&self.banner_msg)?;
        output.write_all(&self.version)?;
        output.write_all(&self.password)?;
        output.write_u32::<LE>(self.num_resource_kinds)?;
        output.write_u32::<LE>(self.directory_size)?;
        Ok(())
    }
}

impl Debug for DrsHeader {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(
            formatter,
            "DRSHeader {{ banner_msg: '{}', version: '{}', password: '{}', num_resource_types: {}, directory_size: {} }}",
            String::from_utf8_lossy(&self.banner_msg),
            String::from_utf8_lossy(&self.version),
            String::from_utf8_lossy(&self.password),
            self.num_resource_kinds,
            self.directory_size
        )
    }
}

/// A table containing resource entries.
pub struct DrsTable {
    /// Type of the resource as a little-endian char array.
    pub resource_kind: ResourceKind,
    /// Offset in the DRS archive where this table's resource entries can be found.
    offset: u32,
    /// Number of resource entries in this table.
    num_resources: u32,
    /// Resources.
    resources: Vec<DrsResource>,
    /// Resource IDs.
    resource_ids: SortedVec<u32>,
}

impl DrsTable {
    fn new(resource_kind: ResourceKind, offset: u32, num_resources: u32) -> Self {
        Self {
            resource_kind,
            offset,
            num_resources,
            resources: Vec::new(),
            resource_ids: SortedVec::default(),
        }
    }

    /// Read a DRS table header from a `Read`able handle.
    #[inline]
    fn from<R: Read>(source: &mut R) -> GenieResult<Self> {
        let mut resource_kind = [0_u8; 4];
        source.read_exact(&mut resource_kind)?;
        let offset = source.read_u32::<LE>()?;
        let num_resources = source.read_u32::<LE>()?;
        Ok(Self::new(resource_kind.try_into()?, offset, num_resources))
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        self.resource_kind.write_to(output)?;
        output.write_u32::<LE>(self.offset)?;
        output.write_u32::<LE>(self.num_resources)?;
        Ok(())
    }

    /// Read the table itself.
    #[inline]
    fn read_resources<R: Read>(&mut self, source: &mut R) -> GenieResult<()> {
        for _ in 0..self.num_resources {
            let resource = DrsResource::from(source)?;
            let _discard = self.resource_ids.insert(resource.id);
            self.resources.push(resource);
        }
        Ok(())
    }

    /// Get the number of resources in this table.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.num_resources as usize
    }

    /// Check if the table contains no resources.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.num_resources == 0
    }

    /// Iterate over the resources in this table.
    #[inline]
    pub fn resources(&self) -> DrsResourceIterator<'_> {
        self.resources.iter()
    }

    /// Find a resource by ID.
    #[inline]
    #[must_use]
    pub fn get_resource(&self, id: u32) -> Option<&DrsResource> {
        self.resource_ids
            .binary_search(&id)
            .ok()
            .map(|index| &self.resources[index])
    }

    /// Get the resource type this table contains.
    #[inline]
    #[must_use]
    pub const fn resource_kind(&self) -> ResourceKind {
        self.resource_kind
    }

    /// Deprecated, use `resource_kind()` instead.
    #[inline]
    #[must_use]
    pub fn resource_ext(&self) -> String {
        self.resource_kind.to_string()
    }

    /// Add a resource to this table.
    #[inline]
    pub(crate) fn add(&mut self, res: DrsResource) {
        self.resources.push(res);
        self.num_resources += 1;
    }
}

impl Debug for DrsTable {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(
            formatter,
            "DRSTable {{ resource_type: '{}', offset: {}, num_resources: {} }}",
            self.resource_kind, self.offset, self.num_resources
        )
    }
}

/// A single resource in a DRS archive.
#[derive(Debug, Clone, Copy)]
pub struct DrsResource {
    /// The resource ID.
    pub id: u32,
    /// The offset into the DRS archive where the resource can be found.
    offset: u32,
    /// The size in bytes of the resource.
    pub size: u32,
}

impl DrsResource {
    /// Read DRS resource metadata from a `Read`able handle.
    #[inline]
    fn from<R: Read>(source: &mut R) -> GenieResult<Self> {
        let id = source.read_u32::<LE>()?;
        let offset = source.read_u32::<LE>()?;
        let size = source.read_u32::<LE>()?;
        Ok(Self { id, offset, size })
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.id)?;
        output.write_u32::<LE>(self.offset)?;
        output.write_u32::<LE>(self.size)?;
        Ok(())
    }
}

/// An iterator over DRS table metadata structs.
pub type DrsTableIterator<'archive> = slice::Iter<'archive, DrsTable>;
/// An iterator over DRS resource metadata structs.
pub type DrsResourceIterator<'archive> = slice::Iter<'archive, DrsResource>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn it_works() -> anyhow::Result<()> {
        let mut file = File::open("test.drs")?;
        let drs = DrsReader::new(&mut file)?;
        let mut expected = vec![
            // (reversed_type, id, size)
            ("js".parse()?, 1, 632),
            ("js".parse()?, 2, 452),
            ("js".parse()?, 3, 38),
            ("json".parse()?, 4, 710),
        ];

        for table in drs.tables() {
            for resource in table.resources() {
                let content = drs.read_resource(&mut file, table.resource_kind, resource.id)?;
                assert_eq!(
                    expected.remove(0),
                    (table.resource_kind, resource.id, content.len())
                );
            }
        }

        Ok(())
    }
}
