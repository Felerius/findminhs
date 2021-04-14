#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use crate::instance::Instance;
use anyhow::{anyhow, Result};
use csv::WriterBuilder;
use log::info;
use rand::{rngs::OsRng, Rng};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    path::PathBuf,
};
use structopt::StructOpt;

mod data_structures;
mod instance;
mod lower_bound;
mod reductions;
mod small_indices;
mod solve;

/// Minimum hitting set solver
#[derive(Debug, StructOpt)]
struct CliOpts {
    /// Input hypergraph file to process
    #[structopt(parse(from_os_str))]
    input_file: PathBuf,

    /// CSV file to append results to
    #[structopt(short, long, parse(from_os_str))]
    csv: Option<PathBuf>,

    /// If given, save the input hypergraph as an ILP to the given file and quit immediately
    #[structopt(long, parse(from_os_str))]
    ilp: Option<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::from_env(env_logger::Env::new().filter_or("FINDMINHS_LOG", "info"))
        .format_timestamp_millis()
        .init();

    let opts = CliOpts::from_args();

    let file_name = opts
        .input_file
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("File name can't be extracted"))?
        .to_string();
    let file = BufReader::new(File::open(&opts.input_file)?);
    let instance = Instance::load(file)?;

    if let Some(ilp_path) = opts.ilp {
        let writer = BufWriter::new(File::create(ilp_path)?);
        instance.export_as_ilp(writer)?;
        return Ok(());
    }

    info!("Solving {:?}", &opts.input_file);
    let seed: u64 = OsRng.gen();
    info!("RNG seed: {:#018x}", seed);

    let solution = solve::solve::<rand_pcg::Pcg64Mcg>(instance, file_name, seed)?;
    info!("Smallest HS has size {}", solution.minimum_hs.len());

    if let Some(csv_file) = opts.csv {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&csv_file)?;
        let write_header = file.metadata()?.len() == 0;
        let mut writer = WriterBuilder::new()
            .has_headers(write_header)
            .from_writer(file);
        writer.serialize(solution)?;
    }

    Ok(())
}
