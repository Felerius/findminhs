#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use crate::{instance::Instance, report::IlpReductionReport};
use anyhow::{anyhow, Result};
use log::info;
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, BufReader, BufWriter},
    path::PathBuf,
    time::Instant,
};
use structopt::StructOpt;

mod data_structures;
mod instance;
mod lower_bound;
mod reductions;
mod report;
mod small_indices;
mod solve;

#[derive(Debug, StructOpt)]
enum CliOpts {
    /// Solve a given instance using the solver
    Solve(SolveOpts),

    /// Convert a given instance into an equivalent ILP
    Ilp(IlpOpts),
}

#[derive(Debug, StructOpt)]
struct IlpOpts {
    /// Input hypergraph file to convert
    #[structopt(parse(from_os_str))]
    hypergraph: PathBuf,

    /// Reduce the hypergraph first by applying vertex and edge domination rules
    #[structopt(long)]
    reduced: bool,

    /// Write a json report about the applied reductions to this file
    #[structopt(long, parse(from_os_str), requires("reduced"))]
    report: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
struct SolveOpts {
    /// Hypergraph to solve
    #[structopt(parse(from_os_str))]
    hypergraph: PathBuf,

    /// File containing solver settings
    #[structopt(parse(from_os_str))]
    settings: PathBuf,

    /// File to write an optional, JSON-formatted report into
    #[structopt(short, long, parse(from_os_str))]
    report: Option<PathBuf>,
}

fn solve(opts: SolveOpts) -> Result<()> {
    let file_name = opts
        .hypergraph
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("File name can't be extracted"))?
        .to_string();
    let instance = {
        let reader = BufReader::new(File::open(&opts.hypergraph)?);
        Instance::load(reader)?
    };
    let settings = {
        let reader = BufReader::new(File::open(&opts.settings)?);
        serde_json::from_reader(reader)?
    };

    info!("Solving {:?}", &opts.hypergraph);
    let report = solve::solve(instance, file_name, settings);
    info!("Smallest HS has size {}", report.opt);

    if let Some(report_file) = opts.report {
        let writer = BufWriter::new(File::create(&report_file)?);
        serde_json::to_writer(writer, &report)?;
    }

    Ok(())
}

fn convert_to_ilp(opts: IlpOpts) -> Result<()> {
    let reader = BufReader::new(File::open(&opts.hypergraph)?);
    let mut instance = Instance::load(reader)?;

    if opts.reduced {
        let time_before = Instant::now();
        let (reduced_vertices, reduced_edges) = reductions::reduce_for_ilp(&mut instance);
        if let Some(report_file) = opts.report {
            let report = IlpReductionReport {
                runtime: time_before.elapsed(),
                reduced_vertices,
                reduced_edges,
            };
            let log_writer = BufWriter::new(File::create(&report_file)?);
            serde_json::to_writer(log_writer, &report)?;
        }
    }

    let stdout = io::stdout();
    instance.export_as_ilp(stdout.lock())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::new().filter_or("FINDMINHS_LOG", "info"))
        .format_timestamp_millis()
        .init();

    let opts = CliOpts::from_args();
    match opts {
        CliOpts::Solve(solve_opts) => solve(solve_opts),
        CliOpts::Ilp(ilp_opts) => convert_to_ilp(ilp_opts),
    }
}
