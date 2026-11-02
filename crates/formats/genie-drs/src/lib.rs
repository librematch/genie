//! .drs is the resource archive file format for the Genie Engine, used by Age of Empires 1/2 and
//! Star Wars: Galactic Battlegrounds. .drs files contain tables, each of which contain resources
//! of a single type. Resources are identified by a numeric identifier.
//!
//! ## Example
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::fs::File;
//! use genie_drs::DRSReader;
//!
//! let mut file = File::open("test.drs")?;
//! let drs = DRSReader::new(&mut file)?;
//!
//! for table in drs.tables() {
//!     for resource in table.resources() {
//!         let content = drs.read_resource(&mut file, table.resource_type, resource.id)?;
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
use sorted_vec::SortedVec;
use std::io::{Error, Read, Write};
use std::slice;
use std::str;

mod read;
mod write;

pub use read::DrsReader;
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
/// use genie_drs::ResourceType;
/// let slp = ResourceType::try_from(*b" pls")?;
/// assert_eq!(&slp, "slp", "implements AsRef<str>");
/// assert_eq!(slp.to_string(), "slp", "implements ToString");
/// assert_eq!(slp, ResourceType::try_from(*b" pls")?);
/// let bina = "bina".parse::<ResourceType>()?;
/// assert_ne!(slp, bina);
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceType([u8; 4]);
impl ResourceType {
    #[inline]
    fn write_to<W: Write>(self, output: &mut W) -> Result<(), Error> {
        let mut bytes = [0; 4];
        bytes.copy_from_slice(&self.0);
        bytes.reverse();
        output.write_all(&bytes)?;
        Ok(())
    }
}

impl PartialEq<str> for ResourceType {
    fn eq(&self, ext: &str) -> bool {
        let me: &str = self.as_ref();
        me == ext
    }
}

impl AsRef<str> for ResourceType {
    fn as_ref(&self) -> &str {
        // the constructors take only valid UTF-8, so the text is never empty by a failure
        str::from_utf8(&self.0[..]).map_or("", str::trim)
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// An error occurred while parsing a resource type.
///
/// This may be caused by:
///   - The input not being 4 bytes or fewer long
///   - The input not being valid UTF-8
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid resource type, must be 4 bytes of UTF-8 at most")]
pub struct ParseResourceTypeError;

impl From<ParseResourceTypeError> for Error {
    fn from(e: ParseResourceTypeError) -> Self {
        Self::new(std::io::ErrorKind::InvalidData, e)
    }
}

impl core::str::FromStr for ResourceType {
    type Err = ParseResourceTypeError;
    /// Parse a resource type from a string, with error handling.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.as_bytes().try_into()
    }
}

/// Parse a resource type as the archive stores it: reversed, with a UTF-8 name.
impl TryFrom<[u8; 4]> for ResourceType {
    type Error = ParseResourceTypeError;
    fn try_from(mut u: [u8; 4]) -> Result<Self, Self::Error> {
        u.reverse();
        let _ = str::from_utf8(&u).map_err(|_| ParseResourceTypeError)?;
        Ok(Self(u))
    }
}

/// Parse a resource type from a byte slice of at most 4 bytes of UTF-8.
impl TryFrom<&[u8]> for ResourceType {
    type Error = ParseResourceTypeError;
    fn try_from(u: &[u8]) -> Result<Self, Self::Error> {
        let _ = str::from_utf8(u).map_err(|_| ParseResourceTypeError)?;
        let mut bytes = [b' '; 4];
        bytes
            .get_mut(..u.len())
            .ok_or(ParseResourceTypeError)?
            .copy_from_slice(u);
        Ok(Self(bytes))
    }
}

/// Parse a resource type from a string of at most 4 bytes.
impl TryFrom<&str> for ResourceType {
    type Error = ParseResourceTypeError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.as_bytes().try_into()
    }
}

/// What names a resource type for [`DrsWriter::add`]: a [`ResourceType`] itself, or its name as
/// a string (at most 4 bytes) or as bytes.
pub trait IntoResourceType {
    /// The resource type.
    ///
    /// # Errors
    ///
    /// An `InvalidData` error for a name of more than 4 bytes or one that is not UTF-8.
    fn into_resource_type(self) -> Result<ResourceType, Error>;
}

impl IntoResourceType for ResourceType {
    fn into_resource_type(self) -> Result<ResourceType, Error> {
        Ok(self)
    }
}

impl IntoResourceType for &str {
    fn into_resource_type(self) -> Result<ResourceType, Error> {
        Ok(ResourceType::try_from(self)?)
    }
}

impl IntoResourceType for &[u8] {
    fn into_resource_type(self) -> Result<ResourceType, Error> {
        Ok(ResourceType::try_from(self)?)
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
    num_resource_types: u32,
    /// Size in bytes of the metadata and tables. Resource contents start at this offset.
    directory_size: u32,
}

impl Default for DrsHeader {
    fn default() -> Self {
        Self {
            banner_msg: *b"Copyright (c) 1997 Ensemble Studios.\x1a\x00\x00\x00",
            version: *b"1.00",
            password: *b"tribe\x00\x00\x00\x00\x00\x00\x00",
            num_resource_types: 0,
            directory_size: 0,
        }
    }
}

impl DrsHeader {
    #[inline]
    /// Read a DRS archive header from a `Read`able handle.
    fn from<R: Read>(source: &mut R) -> Result<Self, Error> {
        let mut banner_msg = [0_u8; 40];
        let mut version = [0_u8; 4];
        let mut password = [0_u8; 12];
        source.read_exact(&mut banner_msg)?;
        source.read_exact(&mut version)?;
        source.read_exact(&mut password)?;
        let num_resource_types = source.read_u32::<LE>()?;
        let directory_size = source.read_u32::<LE>()?;
        Ok(Self {
            banner_msg,
            version,
            password,
            num_resource_types,
            directory_size,
        })
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> Result<(), Error> {
        output.write_all(&self.banner_msg)?;
        output.write_all(&self.version)?;
        output.write_all(&self.password)?;
        output.write_u32::<LE>(self.num_resource_types)?;
        output.write_u32::<LE>(self.directory_size)?;
        Ok(())
    }
}

impl std::fmt::Debug for DrsHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DRSHeader {{ banner_msg: '{}', version: '{}', password: '{}', num_resource_types: {}, directory_size: {} }}",
            String::from_utf8_lossy(&self.banner_msg),
            String::from_utf8_lossy(&self.version),
            String::from_utf8_lossy(&self.password),
            self.num_resource_types,
            self.directory_size
        )
    }
}

/// A table containing resource entries.
pub struct DrsTable {
    /// Type of the resource as a little-endian char array.
    pub resource_type: ResourceType,
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
    fn new(resource_type: ResourceType, offset: u32, num_resources: u32) -> Self {
        Self {
            resource_type,
            offset,
            num_resources,
            resources: Vec::new(),
            resource_ids: SortedVec::default(),
        }
    }

    /// Read a DRS table header from a `Read`able handle.
    #[inline]
    fn from<R: Read>(source: &mut R) -> Result<Self, Error> {
        let mut resource_type = [0_u8; 4];
        source.read_exact(&mut resource_type)?;
        let offset = source.read_u32::<LE>()?;
        let num_resources = source.read_u32::<LE>()?;
        Ok(Self::new(resource_type.try_into()?, offset, num_resources))
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> Result<(), Error> {
        self.resource_type.write_to(output)?;
        output.write_u32::<LE>(self.offset)?;
        output.write_u32::<LE>(self.num_resources)?;
        Ok(())
    }

    /// Read the table itself.
    #[inline]
    fn read_resources<R: Read>(&mut self, source: &mut R) -> Result<(), Error> {
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
    pub const fn resource_type(&self) -> ResourceType {
        self.resource_type
    }

    /// Deprecated, use `resource_type()` instead.
    #[inline]
    #[must_use]
    pub fn resource_ext(&self) -> String {
        self.resource_type.to_string()
    }

    /// Add a resource to this table.
    #[inline]
    pub(crate) fn add(&mut self, res: DrsResource) {
        self.resources.push(res);
        self.num_resources += 1;
    }
}

impl std::fmt::Debug for DrsTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DRSTable {{ resource_type: '{}', offset: {}, num_resources: {} }}",
            self.resource_type, self.offset, self.num_resources
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
    fn from<R: Read>(source: &mut R) -> Result<Self, Error> {
        let id = source.read_u32::<LE>()?;
        let offset = source.read_u32::<LE>()?;
        let size = source.read_u32::<LE>()?;
        Ok(Self { id, offset, size })
    }

    #[inline]
    fn write_to<W: Write>(&self, output: &mut W) -> Result<(), Error> {
        output.write_u32::<LE>(self.id)?;
        output.write_u32::<LE>(self.offset)?;
        output.write_u32::<LE>(self.size)?;
        Ok(())
    }
}

/// An iterator over DRS table metadata structs.
pub type DrsTableIterator<'a> = slice::Iter<'a, DrsTable>;
/// An iterator over DRS resource metadata structs.
pub type DrsResourceIterator<'a> = slice::Iter<'a, DrsResource>;

impl From<ParseResourceTypeError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseResourceTypeError) -> Self {
        Self::invalid_input("a resource type that is no 4 characters").with_source(e)
    }
}

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
                let content = drs.read_resource(&mut file, table.resource_type, resource.id)?;
                assert_eq!(
                    expected.remove(0),
                    (table.resource_type, resource.id, content.len())
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn resource_type_error_converts_into_genie_error() {
        let e: genie_error::GenieError = ParseResourceTypeError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a resource type that is no 4 characters"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
    }
}
