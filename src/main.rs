//! CleanOS CLI: collect evidence, report ranked findings, run benchmarks.

use std::fs;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use cleanos::bench;
use cleanos::doctor;
use cleanos::error::CleanOsError;
use cleanos::model::RunSnapshot;
use cleanos::paths::{report_path_for_run, resolve_run_arg, run_path_for};
use cleanos::probes;
use cleanos::reporter;

#[derive(Parser, Debug)]
#[command(
    name = "cleanos",
    version,
    about = "CleanOS measures macOS bottlenecks and ranks reversible findings by evidence.",
    long_about = "CleanOS collects a read-only evidence snapshot, classifies findings, ranks them by a documented score, and writes a redacted report. It also runs a bounded benchmark suite with stored results and deltas."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run all probes and write the run JSON.
    Collect,
    /// Classify and rank a run, print the table, write the report JSON.
    Report {
        /// Path or basename of a run JSON under the runs directory.
        run: Option<String>,
    },
    /// Run the bounded benchmark suite and store the result as JSON.
    Bench(BenchArgs),
    /// Diagnostics against the live machine (the verification mechanic).
    Doctor {
        /// Include the bench tolerance check (cpu burst vs baseline).
        #[arg(long)]
        bench: bool,
    },
}

#[derive(Args, Debug)]
struct BenchArgs {
    /// Run only the CPU burst probe, one run.
    #[arg(long)]
    quick: bool,
    /// Include the powermetrics power probe (requires root).
    #[arg(long)]
    power: bool,
    /// Number of CPU burst runs (default 3).
    #[arg(long)]
    runs: Option<u32>,
    #[command(subcommand)]
    command: Option<BenchCommand>,
}

#[derive(Subcommand, Debug)]
enum BenchCommand {
    /// Compare two stored benchmark results and print deltas.
    Compare {
        /// Path or basename of a stored result under the benchmarks directory.
        reference: Option<String>,
        /// Write the compare table as JSON under the benchmarks directory.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CleanOsError::Usage(msg)) => {
            eprintln!("error: {msg}");
            eprintln!("Usage: cleanos <collect|report|bench> [args]");
            ExitCode::from(2)
        }
        Err(CleanOsError::ProbeFatal(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
        Err(CleanOsError::Io(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), CleanOsError> {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            e.print().ok();
            if e.use_stderr() {
                std::process::exit(2);
            }
            return Ok(());
        }
    };

    match cli.command {
        Commands::Collect => cmd_collect(),
        Commands::Report { run } => cmd_report(run.as_deref()),
        Commands::Bench(args) => match args.command {
            None => bench::cmd_bench(args.quick, args.power, args.runs),
            Some(BenchCommand::Compare { reference, json }) => {
                bench::cmd_compare(reference.as_deref(), json)
            }
        },
        Commands::Doctor { bench } => cmd_doctor(bench),
    }
}

fn cmd_doctor(include_bench: bool) -> Result<(), CleanOsError> {
    let diags = doctor::run(include_bench)?;
    print!("{}", doctor::format_diagnostics(&diags));
    if diags.iter().any(|d| !d.ok) {
        return Err(CleanOsError::ProbeFatal(
            "diagnostics failed; see the FAIL rows above".to_string(),
        ));
    }
    Ok(())
}

fn cmd_collect() -> Result<(), CleanOsError> {
    let snapshot = probes::collect_run();
    let path = {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&snapshot.collected_at) {
            let local: chrono::DateTime<chrono::Local> = dt.with_timezone(&chrono::Local);
            run_path_for(&local)?
        } else {
            run_path_for(&chrono::Local::now())?
        }
    };
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CleanOsError::Io(format!("serialize run: {e}")))?;
    fs::write(&path, json)
        .map_err(|e| CleanOsError::Io(format!("write {}: {e}", path.display())))?;
    println!("{}", path.display());
    Ok(())
}

fn cmd_report(run_arg: Option<&str>) -> Result<(), CleanOsError> {
    let run_path = resolve_run_arg(run_arg)?;
    let raw = fs::read_to_string(&run_path)
        .map_err(|e| CleanOsError::Io(format!("read {}: {e}", run_path.display())))?;
    let snapshot: RunSnapshot =
        serde_json::from_str(&raw).map_err(|e| CleanOsError::Io(format!("parse run JSON: {e}")))?;
    let report = reporter::build_report(&snapshot, &run_path)?;
    let table = reporter::format_report_table(&report);
    print!("{table}");
    let out = reporter::write_report(&report, &report_path_for_run(&run_path)?)?;
    eprintln!("report written: {}", out.display());
    Ok(())
}
