//! Launch-item inventory for the report section (no classification).

use std::fs;
use std::path::PathBuf;

use crate::model::LaunchDirInventory;
use crate::redaction::redact_text;

const DIRS: &[&str] = &[
    "Library/LaunchAgents",
    "Library/LaunchAgents",
    "Library/LaunchDaemons",
];

fn absolute_dirs() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        out.push((
            "~/Library/LaunchAgents".to_string(),
            PathBuf::from(&home).join("Library").join("LaunchAgents"),
        ));
    }
    out.push((
        "/Library/LaunchAgents".to_string(),
        PathBuf::from("/Library/LaunchAgents"),
    ));
    out.push((
        "/Library/LaunchDaemons".to_string(),
        PathBuf::from("/Library/LaunchDaemons"),
    ));
    let _ = DIRS;
    out
}

fn labels_in_dir(dir: &PathBuf) -> Vec<String> {
    let mut labels = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return labels;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("plist") {
            continue;
        }
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        labels.push(redact_text(&label));
    }
    labels.sort();
    labels
}

pub fn collect_launch_inventory() -> Vec<LaunchDirInventory> {
    let mut out = Vec::new();
    for (display, path) in absolute_dirs() {
        let labels = labels_in_dir(&path);
        let count = labels.len() as u32;
        let top_labels: Vec<String> = labels.into_iter().take(10).collect();
        out.push(LaunchDirInventory {
            path: display,
            count,
            top_labels,
        });
    }
    out
}
