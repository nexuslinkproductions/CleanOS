//! Deterministic ranking (SPEC section 6).

use crate::classifier::FindingClass;
use crate::model::{ProcessInfo, RankedFinding};

pub const BASE_ORPHAN: f64 = 40.0;
pub const BASE_RUNAWAY: f64 = 25.0;
pub const BASE_MEMORY_HOG: f64 = 15.0;
pub const BASE_DUPLICATE: f64 = 10.0;
pub const BASE_HARNESS_MCP: f64 = 35.0;
pub const BASE_HARNESS_AGENT: f64 = 30.0;
pub const BASE_HARNESS_LSP: f64 = 20.0;
pub const BASE_STALE_DEV: f64 = 15.0;
pub const GIB: f64 = 1_073_741_824.0;

pub fn score_candidate(
    class: FindingClass,
    cpu_pct: f64,
    rss_bytes: u64,
    launchd_managed: bool,
) -> f64 {
    let base = match class {
        FindingClass::OrphanCandidate => BASE_ORPHAN,
        FindingClass::RunawaySuspect => BASE_RUNAWAY,
        FindingClass::MemoryHog => BASE_MEMORY_HOG,
        FindingClass::DuplicateOrphan => BASE_DUPLICATE,
        FindingClass::HarnessMcpServer => BASE_HARNESS_MCP,
        FindingClass::HarnessLsp => BASE_HARNESS_LSP,
        FindingClass::HarnessAgentDaemon => BASE_HARNESS_AGENT,
        FindingClass::StaleDevServer => BASE_STALE_DEV,
        FindingClass::Observed => 0.0,
    };
    let cpu_part = (cpu_pct / 5.0).min(20.0);
    let rss_part = ((rss_bytes as f64) / GIB).min(10.0);
    let mut score = base + cpu_part + rss_part;
    if launchd_managed {
        score -= 25.0;
    }
    score
}

/// Rank classified findings. Ties break by pid ascending.
pub fn rank(items: Vec<(FindingClass, ProcessInfo, RankedFinding)>) -> Vec<RankedFinding> {
    let mut ranked: Vec<RankedFinding> = items
        .into_iter()
        .map(|(class, proc, mut finding)| {
            let managed = finding
                .evidence
                .get("launchd_managed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let s = score_candidate(class, proc.cpu_pct, proc.rss_bytes, managed);
            finding.score = s.round() as i32;
            finding
        })
        .collect();
    ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.pid.cmp(&b.pid)));
    ranked
}
