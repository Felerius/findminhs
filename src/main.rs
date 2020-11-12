#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use crate::instance::Instance;
use crate::solve::SolveResult;
use anyhow::{anyhow, Result};
use csv::WriterBuilder;
use log::info;
use rand::rngs::OsRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

mod activity;
mod data_structures;
mod instance;
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
}

#[derive(Debug, Serialize)]
struct CsvRecord {
    // serde(flatten) unfortunately doesn't work: https://github.com/BurntSushi/rust-csv/issues/98
    file_name: String,
    seed: u64,
    hs_size: usize,
    greedy_size: usize,
    solve_time: f64,
    iterations: usize,
    subsuper_prune_time: f64,
}

impl CsvRecord {
    fn new(input_file: impl AsRef<Path>, seed: u64, results: &SolveResult) -> Result<Self> {
        let file_name = input_file
            .as_ref()
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("File name can't be extracted"))?
            .to_string();
        Ok(Self {
            file_name,
            seed,
            hs_size: results.hs_size,
            greedy_size: results.greedy_size,
            solve_time: results.solve_time,
            iterations: results.stats.iterations,
            subsuper_prune_time: results.stats.reduction_time.as_secs_f64(),
        })
    }
}

fn main() -> Result<()> {
    env_logger::from_env(env_logger::Env::new().filter_or("FINDMINHS_LOG", "info"))
        .format_timestamp_millis()
        .init();

    let opts = CliOpts::from_args();
    info!("Solving {:?}", &opts.input_file);

    let file = BufReader::new(File::open(&opts.input_file)?);
    let instance = Instance::load(file)?;

    let seed: u64 = OsRng.gen();
    info!("RNG seed: {:#018x}", seed);
    let rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let results = solve::solve(instance, rng)?;
    info!("Smallest HS has size {}", results.hs_size);

    if let Some(csv_file) = opts.csv {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&csv_file)?;
        let write_header = file.metadata()?.len() == 0;
        let mut writer = WriterBuilder::new()
            .has_headers(write_header)
            .from_writer(file);
        writer.serialize(CsvRecord::new(&opts.input_file, seed, &results)?)?;
    }

    Ok(())
}
