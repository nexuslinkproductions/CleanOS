//! Deterministic ranking (SPEC section 6).

use crate::classifier::FindingClass;
use crate::model::{ProcessInfo, RankedFinding};

pub const BASE_ORPHAN: f64 = 40.0;
pub const BASE_RUNAWAY: f64 = 25.0;
pub const BASE_MEMORY_HOG: f64 = 15.0;
pub const BASE_DUPLICATE: f64 = 10.0;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FactOrInference;
    use std::collections::BTreeMap;

    fn shell(
        pid: u32,
        class: FindingClass,
        cpu: f64,
        rss: u64,
        managed: bool,
    ) -> (FindingClass, ProcessInfo, RankedFinding) {
        let mut evidence = BTreeMap::new();
        evidence.insert("launchd_managed".into(), serde_json::json!(managed));
        (
            class,
            ProcessInfo {
                pid,
                ppid: 1,
                cpu_pct: cpu,
                rss_bytes: rss,
                elapsed_secs: 1,
                executable: "x".into(),
                command: "x".into(),
            },
            RankedFinding {
                id: format!("processes.orphans.{pid}"),
                category: "processes".into(),
                subcategory: "orphans".into(),
                summary: "s".into(),
                evidence,
                expected_gain: "high".into(),
                risk: "med".into(),
                reversible: "yes".into(),
                requires_user_action: true,
                mode: "cleanup".into(),
                auto_ok: false,
                fact_or_inference: FactOrInference::Inference,
                finding: class.as_str().into(),
                score: 0,
                pid,
                cpu_pct: cpu,
                rss_bytes: rss,
                label: "INFERENCE".into(),
            },
        )
    }

    #[test]
    fn determinism_same_input_same_order() {
        let input = vec![
            shell(30, FindingClass::MemoryHog, 1.0, memory_bytes(), false),
            shell(10, FindingClass::OrphanCandidate, 10.0, 0, false),
            shell(20, FindingClass::RunawaySuspect, 50.0, 0, false),
        ];
        let a = rank(input.clone());
        let b = rank(input);
        let pids_a: Vec<_> = a.iter().map(|f| f.pid).collect();
        let pids_b: Vec<_> = b.iter().map(|f| f.pid).collect();
        assert_eq!(pids_a, pids_b);
        assert_eq!(pids_a, vec![10, 20, 30]);
    }

    fn memory_bytes() -> u64 {
        1_073_741_824
    }

    #[test]
    fn tie_break_by_pid_ascending() {
        let input = vec![
            shell(40, FindingClass::OrphanCandidate, 0.0, 0, false),
            shell(20, FindingClass::OrphanCandidate, 0.0, 0, false),
            shell(30, FindingClass::OrphanCandidate, 0.0, 0, false),
        ];
        let ranked = rank(input);
        let pids: Vec<_> = ranked.iter().map(|f| f.pid).collect();
        assert_eq!(pids, vec![20, 30, 40]);
    }

    #[test]
    fn formula_matches_spec_constants() {
        let s = score_candidate(FindingClass::OrphanCandidate, 10.0, 1_073_741_824, false);
        assert!((s - 43.0).abs() < 1e-9);
        let penalized = score_candidate(FindingClass::OrphanCandidate, 10.0, 1_073_741_824, true);
        assert!((penalized - 18.0).abs() < 1e-9);
    }
}
