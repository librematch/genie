use crate::{DrsHeader, DrsResource, DrsTable, IntoResourceType, ResourceType};
use byteorder::{LE, WriteBytesExt};
use std::io::{self, Read, Seek, SeekFrom, Write};

/// The error of a count or a size the format cannot hold.
fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_owned())
}

/// Strategy to use when writing files to the archive.
///
/// DRS files contain metadata at the start of the archive, and the size of this metadata depends
/// on the number of files, the different types of files, and the sizes of the files that are added
/// to the archive. To correctly write an archive to disk, we need to know the number and sizes of
/// files before writing the header, and we only know where to put the files once the size of the
/// header is known. In practice, this means we have to keep the entire archive in memory before
/// writing it.
///
/// There are tricks to work around this and reduce memory usage. The `InMemory` strategy keeps the
/// file in memory before writing it and is great for small archives. The other strateg(y|ies) start writing files without needing to keep them entirely in memory, with other tradeoffs.
pub trait Strategy<W>
where
    W: Write + Seek,
{
    /// Start the archive; called once by [`DrsWriter::new`], before any resource is added.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to the output fails.
    fn open(&mut self, drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error>;
    /// Take in the data of one resource of type `table`, and return `resource` with its size
    /// (and its offset, if the strategy already knows it) filled in.
    ///
    /// # Errors
    ///
    /// Returns an error if reading `data` or writing to the output fails.
    fn add_resource(
        &mut self,
        drs: &mut InnerDrsWriter<W>,
        table: ResourceType,
        resource: DrsResource,
        data: &mut dyn Read,
    ) -> Result<DrsResource, io::Error>;
    /// Finish the archive; called once by [`DrsWriter::flush`]. Writes the header, the table
    /// directory and whatever resource data is still pending.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to or seeking in the output fails.
    fn close(&mut self, drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error>;
}

/// Create the entire DRS file in memory first, then flush it to the output.
///
/// This works best for archives with small files.
#[derive(Debug, Default)]
pub struct InMemoryStrategy {
    resources: Vec<(ResourceType, Vec<u8>)>,
}

impl<W> Strategy<W> for InMemoryStrategy
where
    W: Write + Seek,
{
    fn open(&mut self, _drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error> {
        Ok(())
    }

    fn add_resource(
        &mut self,
        _drs: &mut InnerDrsWriter<W>,
        table: ResourceType,
        mut resource: DrsResource,
        data: &mut dyn Read,
    ) -> Result<DrsResource, io::Error> {
        let mut bytes = vec![];
        // The byte count is `bytes.len()`.
        let _ = data.read_to_end(&mut bytes)?;
        resource.size = u32::try_from(bytes.len())
            .ok()
            .filter(|&size| size < u32::MAX)
            .ok_or_else(|| invalid("file too large"))?;
        self.resources.push((table, bytes));
        Ok(resource)
    }

    // The format stores the directory size and offsets as u32; the counts are not checked
    // against that beyond the assert, so the casts keep their truncating behaviour.
    #[allow(clippy::cast_possible_truncation)]
    fn close(&mut self, drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error> {
        if u32::MAX as usize <= self.resources.len() {
            return Err(invalid("too many resources"));
        }

        let num_tables = drs.tables.len();
        let num_resources = drs.tables.iter().fold(0, |acc, t| acc + t.len());
        drs.header.directory_size = 64 + 12 * (num_tables + num_resources) as u32;
        drs.write_header()?;

        // Assign table offsets
        let mut table_offset = 64 + 12 * (drs.tables.len() as u32);
        let mut file_offset = drs.header.directory_size;
        for table in &mut drs.tables {
            table.offset = table_offset;
            table_offset += 12 * table.num_resources;

            // Assign file offsets
            for res in &mut table.resources {
                res.offset = file_offset;
                file_offset += res.size;
            }
        }

        // Write out all the table data
        drs.write_tables()?;

        for table in &drs.tables {
            let mut data = self.resources.iter().filter_map(|(t, bytes)| {
                if table.resource_type == *t {
                    Some(bytes)
                } else {
                    None
                }
            });
            for _ in &table.resources {
                let bytes = data.next().ok_or_else(|| {
                    invalid("mismatch between InMemoryStrategy resources and DRSWriter table data")
                })?;
                drs.output.write_all(bytes)?;
            }
        }

        Ok(())
    }
}

/// Writer strategy that reserve space for metadata for the given amount of tables and files at the top of the file, then fills it in at the end.
#[derive(Debug, Clone, Copy)]
pub struct ReserveDirectoryStrategy {
    reserved_tables: u32,
    file_space_left: u32,
    write_offset: u32,
}

impl ReserveDirectoryStrategy {
    /// Create a write strategy using a reserved metadata block.
    ///
    /// The strategy will support up to `reserved_tables` tables and `reserved_files` files.
    /// The total size of the reserved metadata block will be `12 * (reserved_tables +
    /// reserved_files)` bytes.
    #[inline]
    #[must_use]
    pub const fn new(reserved_tables: u32, reserved_files: u32) -> Self {
        Self {
            reserved_tables,
            file_space_left: reserved_files,
            write_offset: 64 + 12 * (reserved_tables + reserved_files),
        }
    }
}

impl<W> Strategy<W> for ReserveDirectoryStrategy
where
    W: Write + Seek,
{
    fn open(&mut self, drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error> {
        drs.header.directory_size = self.write_offset;
        drs.write_header()?;

        // Write 0 bytes for reserved space at the top of the file.
        let reserved_block = vec![0; self.write_offset as usize - 64];
        drs.output.write_all(&reserved_block)?;
        Ok(())
    }

    fn add_resource(
        &mut self,
        drs: &mut InnerDrsWriter<W>,
        _table: ResourceType,
        mut resource: DrsResource,
        data: &mut dyn Read,
    ) -> Result<DrsResource, io::Error> {
        self.file_space_left = self
            .file_space_left
            .checked_sub(1)
            .ok_or_else(|| invalid("too many files"))?;

        let len = io::copy(data, &mut drs.output)?;
        resource.offset = self.write_offset;
        resource.size = u32::try_from(len)
            .ok()
            .filter(|&size| size < u32::MAX)
            .ok_or_else(|| invalid("file too large"))?;
        self.write_offset += resource.size;
        Ok(resource)
    }

    fn close(&mut self, drs: &mut InnerDrsWriter<W>) -> Result<(), io::Error> {
        if drs.tables.len() > self.reserved_tables as usize {
            return Err(invalid("too many tables"));
        }

        // Update the resource type count
        let _ = drs.output.seek(SeekFrom::Start(56))?;
        drs.output.write_u32::<LE>(drs.header.num_resource_types)?;
        let _ = drs.output.seek(SeekFrom::Current(4))?;

        // Assign table offsets
        let num_tables = u32::try_from(drs.tables.len()).map_err(|_| invalid("too many tables"))?;
        let mut table_offset = 64 + 12 * num_tables;
        for table in &mut drs.tables {
            table.offset = table_offset;
            table_offset += 12 * table.num_resources;
        }

        // Write out all the table data
        drs.write_tables()?;
        let _ = drs.output.seek(SeekFrom::End(0))?;

        Ok(())
    }
}

impl Default for ReserveDirectoryStrategy {
    /// The default strategy reserves space for 65536 files spread across 10 tables, meaning a directory size of about 768KiB.
    #[inline]
    fn default() -> Self {
        Self::new(10, 65536)
    }
}

/// The archive state a [`Strategy`] works on: the output, the header and the tables so far.
#[derive(Debug)]
pub struct InnerDrsWriter<W>
where
    W: Write + Seek,
{
    output: W,
    header: DrsHeader,
    tables: Vec<DrsTable>,
}

impl<W> InnerDrsWriter<W>
where
    W: Write + Seek,
{
    /// Write the .drs archive header.
    fn write_header(&mut self) -> io::Result<()> {
        self.header.write_to(&mut self.output)
    }

    /// Write the table and resource data.
    fn write_tables(&mut self) -> io::Result<()> {
        for table in &self.tables {
            table.write_to(&mut self.output)?;
        }
        for table in &self.tables {
            for resource in table.resources() {
                resource.write_to(&mut self.output)?;
            }
        }
        Ok(())
    }

    /// Consume the writer and return the underlying Write instance.
    fn into_inner(self) -> W {
        self.output
    }
}

/// Generator for .drs archives.
///
/// ```rust
/// use std::{io::Cursor, fs::File};
/// use genie_drs::{DRSWriter, InMemoryStrategy};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let buf = Cursor::new(vec![]);
/// let mut writer = DRSWriter::new(buf, InMemoryStrategy::default())?;
/// writer.add("bina", 50500, "JASC-PAL\r\n0100\r\n...".as_bytes())?;
/// writer.add("slp", 2, &b"some bytes"[..])?;
/// let buf = writer.flush()?;
/// // → a Vec<u8> containing the DRS file
/// # Ok(()) }
/// ```
pub struct DrsWriter<W>
where
    W: Write + Seek,
{
    inner: InnerDrsWriter<W>,
    strategy: Box<dyn Strategy<W>>,
}

impl<W> std::fmt::Debug for DrsWriter<W>
where
    W: Write + Seek + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The strategy is a trait object without `Debug`.
        f.debug_struct("DRSWriter")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<W> DrsWriter<W>
where
    W: Write + Seek,
{
    /// Create a writer with the given strategy.
    ///
    /// # Errors
    ///
    /// Returns an error if the strategy fails to start the archive (writing to `output`).
    pub fn new(output: W, strategy: impl Strategy<W> + 'static) -> io::Result<Self> {
        let header = DrsHeader::default();

        let mut writer = Self {
            inner: InnerDrsWriter {
                output,
                header,
                tables: vec![],
            },
            strategy: Box::new(strategy),
        };

        writer.strategy.open(&mut writer.inner)?;

        Ok(writer)
    }

    /// Add a file to the archive.
    ///
    /// # Errors
    ///
    /// Returns an error if reading `data` or writing to the output fails.
    ///
    /// Returns an `InvalidData` error if the resource type is not 4 bytes of UTF-8 at most, if
    /// the file is 4 GiB or larger, or, with [`ReserveDirectoryStrategy`], if more files are
    /// added than were reserved.
    #[inline]
    pub fn add(&mut self, t: impl IntoResourceType, id: u32, data: impl Read) -> io::Result<()> {
        self.add_inner(t.into_resource_type()?, id, data)
    }

    fn add_inner(&mut self, t: ResourceType, id: u32, mut data: impl Read) -> io::Result<()> {
        let res = DrsResource {
            id,
            offset: 0, // TBD
            size: 0,   // TBD
        };

        let res = self
            .strategy
            .add_resource(&mut self.inner, t, res, &mut data)?;

        if let Some(table) = self
            .inner
            .tables
            .iter_mut()
            .find(|table| table.resource_type == t)
        {
            table.add(res);
        } else {
            let mut table = DrsTable::new(t, 0, 0);
            table.add(res);
            self.inner.tables.push(table);
        }

        Ok(())
    }

    /// Finish any writes that still need to happen and return the file handle.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to or seeking in the output fails, and an `InvalidData` error
    /// if there are `u32::MAX` or more tables or resources, or, with
    /// [`ReserveDirectoryStrategy`], more tables than were reserved.
    pub fn flush(mut self) -> io::Result<W> {
        self.inner.header.num_resource_types = u32::try_from(self.inner.tables.len())
            .ok()
            .filter(|&n| n < u32::MAX)
            .ok_or_else(|| invalid("too many tables"))?;

        self.strategy.close(&mut self.inner)?;

        Ok(self.inner.into_inner())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    /// A drs archive with a single text file containing the ASCII text "example test file".
    ///
    /// Copyright
    /// Version 1.00
    /// Password tribe
    /// Resource types 1
    /// First file offset 88
    /// Table "txt", offset 76, resources 1
    /// File 1, offset 88, size 17
    static ONE_FILE: &[u8] = b"Copyright (c) 1997 Ensemble Studios.\x1a\x00\x00\x001.00tribe\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x58\x00\x00\x00 txt\x4C\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x58\x00\x00\x00\x11\x00\x00\x00example test file";

    fn invalid(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::InvalidData
    }

    /// More files than a reserved directory holds fail to add.
    #[test]
    fn reserved_directory_refuses_one_file_too_many() -> anyhow::Result<()> {
        let mut drs = DrsWriter::new(Cursor::new(vec![]), ReserveDirectoryStrategy::new(2, 1))?;
        drs.add("txt", 1, &b"a"[..])?;
        assert!(invalid(&drs.add("txt", 2, &b"b"[..]).unwrap_err()));
        Ok(())
    }

    /// More tables than a reserved directory holds fail to close.
    #[test]
    fn reserved_directory_refuses_one_table_too_many() -> anyhow::Result<()> {
        let mut drs = DrsWriter::new(Cursor::new(vec![]), ReserveDirectoryStrategy::new(1, 2))?;
        drs.add("txt", 1, &b"a"[..])?;
        drs.add("bin", 2, &b"b"[..])?;
        assert!(invalid(&drs.flush().unwrap_err()));
        Ok(())
    }

    /// A resource type of more than 4 bytes or of bytes that are no UTF-8 fails to add.
    #[test]
    fn bad_resource_type_fails_to_add() -> anyhow::Result<()> {
        let mut drs = DrsWriter::new(Cursor::new(vec![]), InMemoryStrategy::default())?;
        assert!(invalid(&drs.add("toolong", 1, &b"a"[..]).unwrap_err()));
        assert!(invalid(&drs.add(&[0xff_u8][..], 1, &b"a"[..]).unwrap_err()));
        assert!(drs.add(&b"ok"[..], 1, &b"a"[..]).is_ok());
        Ok(())
    }

    /// A resource type reads back as the text it was given, and one stored as no UTF-8 does not
    /// read.
    #[test]
    fn resource_type_reads_back_or_does_not_read() {
        let slp = ResourceType::try_from(*b" pls").unwrap();
        assert_eq!(slp.to_string(), "slp");
        assert!(ResourceType::try_from([0xff, 0, 0, 0]).is_err());
        let mut table = [0_u8; 12];
        table[..4].copy_from_slice(&[0xff, 0, 0, 0]);
        assert!(invalid(&DrsTable::from(&mut &table[..]).unwrap_err()));
    }

    #[test]
    fn one_file_reserve() -> anyhow::Result<()> {
        let output = Cursor::new(vec![]);
        let mut drs = DrsWriter::new(output, ReserveDirectoryStrategy::new(1, 1))?;
        drs.add("txt", 1, &b"example test file"[..])?;
        let output = drs.flush()?.into_inner();
        assert_eq!(output, ONE_FILE.to_vec());
        Ok(())
    }

    #[test]
    fn one_file_memory() -> anyhow::Result<()> {
        let output = Cursor::new(vec![]);
        let mut drs = DrsWriter::new(output, InMemoryStrategy::default())?;
        drs.add("txt", 1, &b"example test file"[..])?;
        let output = drs.flush()?.into_inner();
        assert_eq!(output, ONE_FILE.to_vec());
        Ok(())
    }
}
