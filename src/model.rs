//! Shared data models for run snapshots and ranked reports.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub schema_version: String,
    pub collected_at: String,
    pub duration_ms: u64,
    pub system: Option<SystemInfo>,
    pub memory: Option<MemoryInfo>,
    pub processes: Vec<ProcessInfo>,
    pub launchd: Option<LaunchdInfo>,
    pub power: Option<PowerInfo>,
    pub thermal: Option<ThermalInfo>,
    pub display: Option<DisplayInfo>,
    /// pid -> LISTEN socket entries from `lsof -nP -iTCP -sTCP:LISTEN`.
    #[serde(default)]
    pub sockets: BTreeMap<String, Vec<SocketEntry>>,
    pub probe_errors: Vec<ProbeError>,
}

/// One LISTEN socket entry: host and port as reported by lsof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SocketEntry {
    pub port: u16,
    pub host: String,
}

/// Harness process state: orphaned when reparented to launchd with no job
/// behind it, attached otherwise (SPEC section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessState {
    Orphaned,
    Attached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeError {
    pub probe: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os_version: String,
    pub chip: String,
    pub cpu_count: u32,
    pub boot_time_epoch: i64,
    pub loadavg_1: f64,
    pub loadavg_5: f64,
    pub loadavg_15: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub compressor_bytes: u64,
    pub pressure_level: u32,
    pub page_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub cpu_pct: f64,
    pub rss_bytes: u64,
    pub elapsed_secs: u64,
    pub executable: String,
    pub command: String,
    /// Harness markers matched on command/args (SPEC section 2); absent when
    /// no marker matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_markers: Option<Vec<String>>,
}

/// Launchd managed set: pid string -> label (schema object map).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LaunchdInfo {
    pub managed: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerInfo {
    pub source: String,
    pub percentage: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalInfo {
    pub thermal_pressure_level: Option<String>,
    pub raw_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub display_count: u32,
    pub primary_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactOrInference {
    Fact,
    Inference,
    None,
}

/// Ranked finding shell used by classifier, ranker, and report JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedFinding {
    pub id: String,
    pub category: String,
    pub subcategory: String,
    pub summary: String,
    pub evidence: BTreeMap<String, serde_json::Value>,
    pub expected_gain: String,
    pub risk: String,
    pub reversible: String,
    pub requires_user_action: bool,
    pub mode: String,
    pub auto_ok: bool,
    pub fact_or_inference: FactOrInference,
    pub finding: String,
    pub score: i32,
    pub pid: u32,
    pub cpu_pct: f64,
    pub rss_bytes: u64,
    pub label: String,
}

pub type Finding = RankedFinding;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchDirInventory {
    pub path: String,
    pub count: u32,
    pub top_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDocument {
    pub schema_version: String,
    pub generated_at: String,
    pub source_run: String,
    pub collected_at: String,
    pub probe_count: u32,
    pub error_count: u32,
    pub findings_by_class: BTreeMap<String, u32>,
    pub findings: Vec<RankedFinding>,
    pub launch_inventory: Vec<LaunchDirInventory>,
    pub zero_finding_notes: Vec<String>,
    pub summary_line: String,
}
