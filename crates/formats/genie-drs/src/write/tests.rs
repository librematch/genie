use super::*;
use crate::DrsReader;
use genie_error::ErrorKind;
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

fn invalid(error: &GenieError) -> bool {
    error.kind() == ErrorKind::InvalidInput
}

/// The resource type of the name `text`.
fn kind(text: &str) -> ResourceKind {
    text.parse().unwrap()
}

/// More files than a reserved directory holds fail to add.
#[test]
fn reserved_directory_refuses_one_file_too_many() -> GenieResult<()> {
    let mut drs = DrsWriter::new(Cursor::new(vec![]), ReserveDirectoryStrategy::new(2, 1))?;
    drs.add(kind("txt"), 1, &b"a"[..])?;
    assert!(invalid(&drs.add(kind("txt"), 2, &b"b"[..]).unwrap_err()));
    Ok(())
}

/// More tables than a reserved directory holds fail to close.
#[test]
fn reserved_directory_refuses_one_table_too_many() -> GenieResult<()> {
    let mut drs = DrsWriter::new(Cursor::new(vec![]), ReserveDirectoryStrategy::new(1, 2))?;
    drs.add(kind("txt"), 1, &b"a"[..])?;
    drs.add(kind("bin"), 2, &b"b"[..])?;
    assert!(invalid(&drs.flush().unwrap_err()));
    Ok(())
}

/// A resource type of more than 4 bytes or of bytes that are no UTF-8 does not parse; one
/// of at most 4 bytes of UTF-8 parses, padded with spaces.
#[test]
fn resource_kind_parses_only_from_at_most_4_bytes_of_utf8() -> GenieResult<()> {
    assert!(invalid(&ResourceKind::try_from("toolong").unwrap_err()));
    assert!(invalid(
        &ResourceKind::try_from(&[0xff_u8][..]).unwrap_err()
    ));
    assert_eq!(ResourceKind::try_from(&b"ok"[..])?, ResourceKind(*b"ok  "));
    assert_eq!("bina".parse::<ResourceKind>()?, ResourceKind(*b"bina"));
    Ok(())
}

/// A file added under the resource type of a table read from an archive writes that table.
#[test]
fn file_added_under_the_kind_of_the_read_table_writes_that_table() -> GenieResult<()> {
    let read = DrsReader::new(&mut Cursor::new(ONE_FILE))?;
    let text = read.tables().next().unwrap().resource_kind;
    let mut drs = DrsWriter::new(Cursor::new(vec![]), InMemoryStrategy::default())?;
    drs.add(text, 1, &b"example test file"[..])?;
    assert_eq!(drs.flush()?.into_inner(), ONE_FILE.to_vec());
    Ok(())
}

/// A resource type reads back as the text it was given, and one stored as no UTF-8 does not
/// read.
#[test]
fn resource_kind_reads_back_or_does_not_read() {
    let slp = ResourceKind::try_from(*b" pls").unwrap();
    assert_eq!(slp.to_string(), "slp");
    assert!(ResourceKind::try_from([0xff, 0, 0, 0]).is_err());
    let mut table = [0_u8; 12];
    table[..4].copy_from_slice(&[0xff, 0, 0, 0]);
    assert!(invalid(&DrsTable::from(&mut &table[..]).unwrap_err()));
}

#[test]
fn one_file_archive_writes_its_bytes_with_reserved_directory() -> GenieResult<()> {
    let output = Cursor::new(vec![]);
    let mut drs = DrsWriter::new(output, ReserveDirectoryStrategy::new(1, 1))?;
    drs.add(kind("txt"), 1, &b"example test file"[..])?;
    let output = drs.flush()?.into_inner();
    assert_eq!(output, ONE_FILE.to_vec());
    Ok(())
}

#[test]
fn one_file_archive_writes_its_bytes_in_memory() -> GenieResult<()> {
    let output = Cursor::new(vec![]);
    let mut drs = DrsWriter::new(output, InMemoryStrategy::default())?;
    drs.add(kind("txt"), 1, &b"example test file"[..])?;
    let output = drs.flush()?.into_inner();
    assert_eq!(output, ONE_FILE.to_vec());
    Ok(())
}
