//! Lists or extracts the scenario files in a campaign file.

use genie_formats::Campaign;
use std::env::current_dir;
use std::fs::write;
use std::{fs::File, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct List {
    /// Campaign file.
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

#[derive(Debug, StructOpt)]
struct Extract {
    /// Campaign file.
    #[structopt(parse(from_os_str))]
    input: PathBuf,
    /// Output folder, defaults to cwd.
    #[structopt(parse(from_os_str))]
    output: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "extractcpx", about = "Campaign file manager")]
enum Cli {
    /// List the scenario files in the campaign file.
    #[structopt(name = "list")]
    List(List),
    /// Extract scenario files from the campaign file.
    #[structopt(name = "extract")]
    Extract(Extract),
}

fn list(args: List) {
    let file = File::open(args.input).expect("could not open file");
    let mut campaign = Campaign::read_from(file).expect("not a campaign file");

    println!("Name: {}", campaign.name());
    println!("Version: {}", String::from_utf8_lossy(&campaign.version()));
    println!("Scenarios: ({})", campaign.len());

    let names = campaign
        .entries()
        .map(|entry| entry.filename.clone())
        .collect::<Vec<String>>();

    (0..campaign.len()).for_each(|i| {
        let bytes = campaign.by_index_raw(i).expect("missing scenario data");
        println!("- {} ({})", names[i], format_bytes(bytes.len()));
    });
}

fn extract(args: Extract) {
    let dir = args
        .output
        .unwrap_or_else(|| current_dir().expect("invalid cwd"));

    let file = File::open(args.input).expect("could not open file");
    let mut campaign = Campaign::read_from(file).expect("not a campaign file");

    let names = campaign
        .entries()
        .map(|entry| entry.filename.clone())
        .collect::<Vec<String>>();

    (0..campaign.len()).for_each(|i| {
        let bytes = campaign.by_index_raw(i).expect("missing scenario data");
        println!("{}", names[i]);
        write(dir.join(&names[i]), bytes).expect("failed to write");
    });
}

/// Derived from <https://github.com/banyan/rust-pretty-bytes/blob/master/src/converter.rs>
fn format_bytes(num: usize) -> String {
    let units = ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    if num < 1 {
        return format!("{} {}", num, "B");
    }
    let delimiter = 1000;
    // floor(log1000(num)), capped at the largest unit
    let mut pretty_bytes = num;
    let mut exponent = 0;
    while pretty_bytes >= delimiter && exponent < units.len() - 1 {
        pretty_bytes /= delimiter;
        exponent += 1;
    }
    let unit = units[exponent];
    format!("{pretty_bytes:.2} {unit}")
}

fn main() {
    match Cli::from_args() {
        Cli::List(args) => list(args),
        Cli::Extract(args) => extract(args),
    }
}
