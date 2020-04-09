use crate::instance::Instance;
use anyhow::Result;
use std::{env, process};
use std::io::BufReader;
use std::fs::File;
use rand::SeedableRng;

mod data_structures;
mod instance;
mod reductions;
mod small_indices;
mod solve;

fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::new().filter_or("FINDMINHS_LOG", "info"));

    let args: Vec<_> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} input_file", args[0]);
        process::exit(1);
    }

    let file = BufReader::new(File::open(&args[1])?);
    let mut instance = Instance::load(file)?;
    reductions::prune(&mut instance);
    let rng = rand_pcg::Pcg64Mcg::seed_from_u64(13201512356123065126);
    solve::solve(&mut instance, rng);

    Ok(())
}
