#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use crate::{instance::Instance, report::IlpReductionReport};
use anyhow::{anyhow, Result};
use log::{debug, info};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, BufReader, BufWriter},
    path::PathBuf,
    time::Instant,
};
use structopt::{clap::AppSettings, StructOpt};

mod data_structures;
mod instance;
mod lower_bound;
mod reductions;
mod report;
mod small_indices;
mod solve;

const APP_SETTINGS: &[AppSettings] = &[
    AppSettings::DisableHelpSubcommand,
    AppSettings::SubcommandRequiredElseHelp,
    AppSettings::VersionlessSubcommands,
];
const GLOBAL_APP_SETTINGS: &[AppSettings] =
    &[AppSettings::ColoredHelp, AppSettings::UnifiedHelpMessage];

#[derive(Debug, StructOpt)]
#[structopt(settings = APP_SETTINGS, global_settings = GLOBAL_APP_SETTINGS)]
enum CliOpts {
    /// Run the solver on a given hypergraph
    Solve(SolveOpts),

    /// Convert a hypergraph into an equivalent ILP
    Ilp(IlpOpts),
}

#[derive(Debug, StructOpt)]
struct CommonOpts {
    /// Input hypergraph
    #[structopt(parse(from_os_str), value_name = "hypergraph-file")]
    hypergraph: PathBuf,

    /// Use the json format for the input hypergraph rather than the text-based one.
    #[structopt(short, long)]
    json: bool,
}

impl CommonOpts {
    fn load_instance(&self) -> Result<Instance> {
        let reader = BufReader::new(File::open(&self.hypergraph)?);
        if self.json {
            Instance::load_from_json(reader)
        } else {
            Instance::load_from_text(reader)
        }
    }
}

#[derive(Debug, StructOpt)]
struct IlpOpts {
    #[structopt(flatten)]
    common: CommonOpts,

    /// Reduce the hypergraph first by applying vertex and edge domination rules
    #[structopt(long)]
    reduced: bool,

    /// Write a json report about the applied reductions to this file
    #[structopt(
        short,
        long,
        parse(from_os_str),
        requires("reduced"),
        value_name = "file"
    )]
    report: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
struct SolveOpts {
    #[structopt(flatten)]
    common: CommonOpts,

    /// Solver settings
    #[structopt(parse(from_os_str), value_name = "settings-file")]
    settings: PathBuf,

    /// Write the final hitting set to this file as a json array
    #[structopt(short, long, parse(from_os_str), value_name = "file")]
    solution: Option<PathBuf>,

    /// Write a detailed statistics report to this file formatted as json
    #[structopt(short, long, parse(from_os_str), value_name = "file")]
    report: Option<PathBuf>,
}

fn solve(opts: SolveOpts) -> Result<()> {
    let file_name = opts
        .common
        .hypergraph
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("File name can't be extracted"))?
        .to_string();
    let instance = opts.common.load_instance()?;
    let settings = {
        let reader = BufReader::new(File::open(&opts.settings)?);
        serde_json::from_reader(reader)?
    };

    info!("Solving {:?}", &opts.common.hypergraph);
    let (final_hs, report) = solve::solve(instance, file_name, settings)?;

    if let Some(solution_file) = opts.solution {
        debug!("Writing solution to {}", solution_file.display());
        let writer = BufWriter::new(File::create(&solution_file)?);
        serde_json::to_writer(writer, &final_hs)?;
    }
    if let Some(report_file) = opts.report {
        debug!("Writing report to {}", report_file.display());
        let writer = BufWriter::new(File::create(&report_file)?);
        serde_json::to_writer(writer, &report)?;
    }

    Ok(())
}

fn convert_to_ilp(opts: IlpOpts) -> Result<()> {
    let mut instance = opts.common.load_instance()?;

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
