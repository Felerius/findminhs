#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use crate::instance::Instance;
use anyhow::Result;
use log::info;
use rand::rngs::OsRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::BufReader;
use std::{env, process};

mod activity;
mod data_structures;
mod instance;
mod reductions;
mod small_indices;
mod solve;

fn main() -> Result<()> {
    env_logger::from_env(env_logger::Env::new().filter_or("FINDMINHS_LOG", "info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<_> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} input_file", args[0]);
        process::exit(1);
    }

    let file = BufReader::new(File::open(&args[1])?);
    let mut instance = Instance::load(file)?;
    reductions::prune(&mut instance);

    let seed: u64 = OsRng.gen();
    info!("RNG seed: {}", seed);
    let rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed);
    let smallest_size = solve::solve(&mut instance, rng)?;
    info!("Smallest HS: {}", smallest_size);

    Ok(())
}
