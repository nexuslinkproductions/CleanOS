//! CleanOS CLI: collect evidence and report ranked findings.

use std::fs;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    long_about = "CleanOS collects a read-only evidence snapshot, classifies findings, ranks them by a documented score, and writes a redacted report."
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
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CleanOsError::Usage(msg)) => {
            eprintln!("error: {msg}");
            eprintln!("Usage: cleanos <collect|report> [args]");
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
    }
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
    fs::write(&path, json).map_err(|e| CleanOsError::Io(format!("write {}: {e}", path.display())))?;
    println!("{}", path.display());
    Ok(())
}

fn cmd_report(run_arg: Option<&str>) -> Result<(), CleanOsError> {
    let run_path = resolve_run_arg(run_arg)?;
    let raw = fs::read_to_string(&run_path)
        .map_err(|e| CleanOsError::Io(format!("read {}: {e}", run_path.display())))?;
    let snapshot: RunSnapshot = serde_json::from_str(&raw)
        .map_err(|e| CleanOsError::Io(format!("parse run JSON: {e}")))?;
    let report = reporter::build_report(&snapshot, &run_path)?;
    let table = reporter::format_report_table(&report);
    print!("{table}");
    let out = reporter::write_report(&report, &report_path_for_run(&run_path)?)?;
    eprintln!("report written: {}", out.display());
    Ok(())
}
