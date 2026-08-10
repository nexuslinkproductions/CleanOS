//! Local data root under Application Support/CleanOS.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::error::CleanOsError;

/// Optional override for tests: set CLEANOS_DATA_ROOT to redirect all writes.
pub fn data_root() -> Result<PathBuf, CleanOsError> {
    if let Some(override_root) = std::env::var_os("CLEANOS_DATA_ROOT") {
        return Ok(PathBuf::from(override_root));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| CleanOsError::Io("HOME environment variable is unset".to_string()))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("CleanOS"))
}

pub fn runs_dir() -> Result<PathBuf, CleanOsError> {
    let dir = data_root()?.join("runs");
    fs::create_dir_all(&dir).map_err(|e| CleanOsError::Io(format!("create runs dir: {e}")))?;
    Ok(dir)
}

pub fn reports_dir() -> Result<PathBuf, CleanOsError> {
    let dir = data_root()?.join("reports");
    fs::create_dir_all(&dir).map_err(|e| CleanOsError::Io(format!("create reports dir: {e}")))?;
    Ok(dir)
}

pub fn run_path_for(collected_at: &DateTime<Local>) -> Result<PathBuf, CleanOsError> {
    let name = collected_at.format("%Y%m%d-%H%M%S.json").to_string();
    Ok(runs_dir()?.join(name))
}

pub fn report_path_for_run(run_path: &Path) -> Result<PathBuf, CleanOsError> {
    let stem = run_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("run");
    Ok(reports_dir()?.join(format!("{stem}.report.json")))
}

pub fn resolve_run_arg(arg: Option<&str>) -> Result<PathBuf, CleanOsError> {
    match arg {
        Some(s) => {
            let p = PathBuf::from(s);
            if p.exists() {
                return Ok(p);
            }
            let under = runs_dir()?.join(s);
            if under.exists() {
                return Ok(under);
            }
            let with_json = runs_dir()?.join(format!("{s}.json"));
            if with_json.exists() {
                return Ok(with_json);
            }
            Err(CleanOsError::Usage(format!(
                "run not found: {s}. Pass a path or basename under the runs directory."
            )))
        }
        None => latest_run(),
    }
}

fn latest_run() -> Result<PathBuf, CleanOsError> {
    let dir = runs_dir()?;
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| CleanOsError::Io(format!("read runs dir: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "json")
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    entries.pop().ok_or_else(|| {
        CleanOsError::Usage("no run files found. Run `cleanos collect` first.".into())
    })
}
