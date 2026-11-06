//! Lists, extracts or adds resources in a .drs archive.

use genie_drs::{DrsReader, DrsTable, DrsWriter, ReserveDirectoryStrategy};
use genie_error::{GenieError, GenieResult};
use std::collections::BTreeSet;
use std::fs::rename;
use std::fs::{File, create_dir_all};
use std::io::{self, Write, stdout};
use std::path::PathBuf;
use std::time::SystemTime;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Cli {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
enum Command {
    #[structopt(name = "list")]
    /// List the resources in <file>
    List(List),
    #[structopt(name = "get")]
    /// Get a single resource by ID.
    Get(Get),
    #[structopt(name = "extract")]
    /// Extract the entire archive to a directory.
    Extract(Extract),
    #[structopt(name = "add")]
    /// Add a resource to an existing archive.
    Add(Add),
}

#[derive(StructOpt)]
struct List {
    #[structopt(parse(from_os_str))]
    /// Path to the .drs archive.
    archive: PathBuf,
}

#[derive(StructOpt)]
struct Get {
    /// Path to the .drs archive.
    #[structopt(parse(from_os_str))]
    archive: PathBuf,
    /// The ID of the resource.
    #[structopt(name = "resource")]
    resource_id: u32,
}

#[derive(StructOpt)]
struct Extract {
    /// Path to the .drs archive.
    #[structopt(parse(from_os_str))]
    archive: PathBuf,
    /// Only extract resources from this table.
    #[structopt(long, short = "t")]
    table: Option<String>,
    /// Output directory to place the resources in.
    #[structopt(long, short = "o", parse(from_os_str))]
    out: PathBuf,
}

#[derive(Debug, StructOpt)]
struct Add {
    /// Path to the .drs archive.
    #[structopt(parse(from_os_str))]
    archive: PathBuf,
    /// Path to place the edited .drs archive. If not given, updates the archive in place.
    #[structopt(long, short = "o", parse(from_os_str))]
    output: Option<PathBuf>,
    /// Table to add the file to.
    #[structopt(long, short = "t", number_of_values = 1)]
    table: Vec<String>,
    /// ID of the file.
    #[structopt(long, short = "i", number_of_values = 1)]
    id: Vec<u32>,
    /// Path to the file to add. `-` for standard input.
    #[structopt(parse(from_os_str), default_value = "-")]
    file: Vec<PathBuf>,
}

fn list(args: List) -> GenieResult<()> {
    let mut file = File::open(args.archive)?;
    let drs = DrsReader::new(&mut file)?;

    for table in drs.tables() {
        for resource in table.resources() {
            println!("{}.{}", resource.id, table.resource_ext());
        }
    }

    Ok(())
}

fn get(args: Get) -> GenieResult<()> {
    let mut file = File::open(args.archive)?;
    let drs = DrsReader::new(&mut file)?;

    for table in drs.tables() {
        if let Some(resource) = table.get_resource(args.resource_id) {
            let buf = drs.read_resource(&mut file, table.resource_kind, resource.id)?;
            stdout().write_all(&buf)?;
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Archive does not contain that resource",
    )
    .into())
}

fn extract(args: Extract) -> GenieResult<()> {
    let mut file = File::open(args.archive)?;
    let drs = DrsReader::new(&mut file)?;

    create_dir_all(&args.out)?;

    for table in drs.tables() {
        let table_ext = table.resource_ext();
        if let Some(ref filter_ext) = args.table
            && &table_ext != filter_ext
        {
            continue;
        }

        for resource in table.resources() {
            let buf = drs.read_resource(&mut file, table.resource_kind, resource.id)?;
            let mut outfile =
                File::create(args.out.join(format!("{}.{}", resource.id, table_ext)))?;
            outfile.write_all(&buf)?;
        }
    }

    Ok(())
}

/// A count of tables or files as the archive stores it.
fn count(len: usize) -> GenieResult<u32> {
    u32::try_from(len).map_err(|source| {
        GenieError::invalid_input("a .drs archive holds at most u32::MAX tables and files")
            .with("count", len)
            .with_source(source)
    })
}

fn add(args: Add) -> GenieResult<()> {
    assert_eq!(
        args.file.len(),
        args.table.len(),
        "Must set a --table for every file"
    );
    assert_eq!(
        args.file.len(),
        args.id.len(),
        "Must set an --id for every file"
    );

    let mut input = File::open(&args.archive)?;
    let drs_read = DrsReader::new(&mut input)?;

    let tables = count(drs_read.tables().len())?;
    let files = count(drs_read.tables().map(DrsTable::len).sum::<usize>())?;
    let new_tables = count(args.table.iter().collect::<BTreeSet<_>>().len())?;
    let new_files = count(args.id.len())?;

    let suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or_else(
            |_| "temp".to_string(),
            |duration| format!("{}", duration.as_secs()),
        );
    let mut temp_out = args.output.as_ref().unwrap_or(&args.archive).clone();
    temp_out.set_file_name(format!(
        "{}.{}",
        temp_out.file_name().unwrap().to_str().unwrap(),
        suffix
    ));

    let output = File::create(&temp_out)?;
    let mut drs_write = DrsWriter::new(
        output,
        ReserveDirectoryStrategy::new(tables + new_tables, files + new_files),
    )?;

    for drs_table in drs_read.tables() {
        for drs_resource in drs_table.resources() {
            let reader = drs_read.get_resource_reader(
                &mut input,
                drs_table.resource_kind,
                drs_resource.id,
            )?;
            drs_write.add(drs_table.resource_kind, drs_resource.id, reader)?;
        }
    }

    for (index, path) in args.file.iter().enumerate() {
        drs_write.add(
            args.table[index].parse()?,
            args.id[index],
            File::open(path)?,
        )?;
    }

    // Close the new archive before it replaces the old one.
    drop(drs_write.flush()?);

    rename(
        temp_out,
        if let Some(outfile) = args.output {
            outfile
        } else {
            args.archive
        },
    )?;

    Ok(())
}

fn main() -> GenieResult<()> {
    let args = Cli::from_args();

    match args.command {
        Command::List(args) => list(args),
        Command::Get(args) => get(args),
        Command::Extract(args) => extract(args),
        Command::Add(args) => add(args),
    }
}
