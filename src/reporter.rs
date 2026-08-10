//! Report builder and table renderer.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Local;

use crate::classifier;
use crate::error::CleanOsError;
use crate::inventory;
use crate::model::{ReportDocument, RunSnapshot};
use crate::ranker;
use crate::redaction;

const CLASSES: &[&str] = &[
    "orphan_candidate",
    "runaway_suspect",
    "memory_hog",
    "duplicate_orphan",
];

pub fn build_report(run: &RunSnapshot, source_run: &Path) -> Result<ReportDocument, CleanOsError> {
    let self_pid = std::process::id();
    let classified = classifier::classify(run, self_pid);
    let findings = ranker::rank(classified);

    let mut findings_by_class: BTreeMap<String, u32> = BTreeMap::new();
    for c in CLASSES {
        findings_by_class.insert((*c).to_string(), 0);
    }
    for f in &findings {
        *findings_by_class.entry(f.finding.clone()).or_insert(0) += 1;
    }

    let mut zero_finding_notes = Vec::new();
    for c in CLASSES {
        if findings_by_class.get(*c).copied().unwrap_or(0) == 0 {
            zero_finding_notes.push(format!("Zero findings for class {c}."));
        }
    }

    let probe_count = [
        run.system.is_some(),
        run.memory.is_some(),
        !run.processes.is_empty(),
        run.launchd.is_some(),
        run.power.is_some(),
        run.thermal.is_some(),
        run.display.is_some(),
    ]
    .iter()
    .filter(|x| **x)
    .count() as u32;

    let error_count = run.probe_errors.len() as u32;
    let summary_line = format!(
        "probes={} errors={} findings: orphan_candidate={} runaway_suspect={} memory_hog={} duplicate_orphan={}",
        probe_count,
        error_count,
        findings_by_class.get("orphan_candidate").copied().unwrap_or(0),
        findings_by_class.get("runaway_suspect").copied().unwrap_or(0),
        findings_by_class.get("memory_hog").copied().unwrap_or(0),
        findings_by_class.get("duplicate_orphan").copied().unwrap_or(0),
    );

    let source = source_run
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("run.json")
        .to_string();

    Ok(ReportDocument {
        schema_version: "1".into(),
        generated_at: Local::now().to_rfc3339(),
        source_run: source,
        collected_at: run.collected_at.clone(),
        probe_count,
        error_count,
        findings_by_class,
        findings,
        launch_inventory: inventory::collect_launch_inventory(),
        zero_finding_notes,
        summary_line,
    })
}

pub fn format_report_table(report: &ReportDocument) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\n",
        redaction::redact_text(&report.summary_line)
    ));
    for note in &report.zero_finding_notes {
        out.push_str(&format!("{}\n", note));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<7} {:>6} {:>7} {:>10} {:<18} {}\n",
        "SCORE", "PID", "CPU%", "RSS_MB", "LABEL", "SUMMARY"
    ));
    out.push_str(&format!(
        "{:-<7} {:->6} {:->7} {:->10} {:-<18} {}\n",
        "", "", "", "", "", ""
    ));
    if report.findings.is_empty() {
        out.push_str("(no ranked findings)\n");
    } else {
        for f in &report.findings {
            let rss_mb = f.rss_bytes as f64 / (1024.0 * 1024.0);
            let summary = redaction::redact_text(&f.summary);
            out.push_str(&format!(
                "{:<7} {:>6} {:>7.1} {:>10.1} {:<18} {}\n",
                f.score, f.pid, f.cpu_pct, rss_mb, f.label, summary
            ));
        }
    }
    out.push('\n');
    out.push_str("Launch-item inventory:\n");
    for dir in &report.launch_inventory {
        let path = redaction::redact_text(&dir.path);
        out.push_str(&format!("  {} : {} items\n", path, dir.count));
        for label in &dir.top_labels {
            out.push_str(&format!("    - {}\n", redaction::redact_text(label)));
        }
    }
    out
}

pub fn write_report(
    report: &ReportDocument,
    path: &Path,
) -> Result<std::path::PathBuf, CleanOsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| CleanOsError::Io(format!("create report dir: {e}")))?;
    }
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| CleanOsError::Io(format!("serialize report: {e}")))?;
    let redacted = redaction::redact_text(&json);
    fs::write(path, redacted)
        .map_err(|e| CleanOsError::Io(format!("write {}: {e}", path.display())))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        DisplayInfo, LaunchdInfo, MemoryInfo, PowerInfo, ProcessInfo, SystemInfo, ThermalInfo,
    };

    fn empty_run() -> RunSnapshot {
        RunSnapshot {
            schema_version: "1".into(),
            collected_at: "2026-08-10T12:00:00+02:00".into(),
            duration_ms: 1,
            system: Some(SystemInfo {
                os_version: "26.4.1".into(),
                chip: "Apple M2 Pro".into(),
                cpu_count: 12,
                boot_time_epoch: 1,
                loadavg_1: 1.0,
                loadavg_5: 1.0,
                loadavg_15: 1.0,
            }),
            memory: Some(MemoryInfo {
                total_bytes: 16,
                used_bytes: 8,
                free_bytes: 8,
                swap_used_bytes: 0,
                swap_total_bytes: 0,
                compressor_bytes: 0,
                pressure_level: 0,
                page_size_bytes: 16384,
            }),
            processes: vec![ProcessInfo {
                pid: 99,
                ppid: 50,
                cpu_pct: 0.1,
                rss_bytes: 1000,
                elapsed_secs: 1,
                executable: "ok".into(),
                command: "ok".into(),
            }],
            launchd: Some(LaunchdInfo {
                managed: Default::default(),
            }),
            power: Some(PowerInfo {
                source: "AC".into(),
                percentage: Some(100),
            }),
            thermal: Some(ThermalInfo {
                thermal_pressure_level: None,
                raw_summary: "none".into(),
            }),
            display: Some(DisplayInfo {
                display_count: 1,
                primary_summary: "Built-in".into(),
            }),
            probe_errors: vec![],
        }
    }

    #[test]
    fn zero_finding_honesty() {
        let report = build_report(&empty_run(), Path::new("empty.json")).unwrap();
        let table = format_report_table(&report);
        assert!(table.contains("Zero findings for class orphan_candidate."));
        assert!(table.contains("Zero findings for class runaway_suspect."));
        assert!(table.contains("Zero findings for class memory_hog."));
        assert!(table.contains("Zero findings for class duplicate_orphan."));
        assert!(!table.contains("/Users/marcelspatz"));
    }
}
