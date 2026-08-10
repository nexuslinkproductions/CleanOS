//! Classification rules and thresholds (SPEC section 5).

use std::collections::{HashMap, HashSet};

use crate::model::{FactOrInference, ProcessInfo, RankedFinding, RunSnapshot};

/// CPU percent threshold for orphan_candidate.
pub const ORPHAN_CPU_PCT: f64 = 5.0;
/// RSS bytes threshold for orphan_candidate (512 MiB).
pub const ORPHAN_RSS_BYTES: u64 = 536_870_912;
/// CPU percent threshold for runaway_suspect.
pub const RUNAWAY_CPU_PCT: f64 = 50.0;
/// RSS bytes threshold for memory_hog (1 GiB).
pub const MEMORY_HOG_RSS_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingClass {
    OrphanCandidate,
    RunawaySuspect,
    MemoryHog,
    DuplicateOrphan,
    Observed,
}

impl FindingClass {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingClass::OrphanCandidate => "orphan_candidate",
            FindingClass::RunawaySuspect => "runaway_suspect",
            FindingClass::MemoryHog => "memory_hog",
            FindingClass::DuplicateOrphan => "duplicate_orphan",
            FindingClass::Observed => "observed",
        }
    }
}

fn is_excluded(proc: &ProcessInfo, self_pid: u32) -> bool {
    if proc.pid == 1 {
        return true;
    }
    if proc.pid == 0 || proc.executable == "kernel_task" || proc.command.contains("kernel_task") {
        return true;
    }
    if proc.pid == self_pid {
        return true;
    }
    false
}

/// System-path prefixes that mark a PPID=1 process as system-managed.
/// Layer 2 of the orphan discriminator: system-domain daemons never appear
/// in `launchctl list` (user domain), so a path-based exclusion is required.
pub const SYSTEM_PATH_PREFIXES: &[&str] = &[
    "/System/",
    "/usr/libexec",
    "/usr/sbin",
    "/usr/lib",
    "/sbin/",
    "/bin/",
    "/usr/bin/",
    "/Library/SystemExtensions",
];

/// True when the process runs from a system-managed path or driver context.
/// Such processes are launchd-managed by the system domain and are never
/// orphan candidates, even though they have PPID=1.
pub fn is_system_domain_path(proc: &ProcessInfo) -> bool {
    if proc.command.to_ascii_lowercase().contains("core audio driver") {
        return true;
    }
    SYSTEM_PATH_PREFIXES
        .iter()
        .any(|p| proc.command.starts_with(p))
}

/// Build the launchd-managed pid set from the pid->label lookup.
fn launchd_managed_pids(run: &RunSnapshot) -> HashSet<u32> {
    run.launchd
        .as_ref()
        .map(|l| {
            l.managed
                .keys()
                .filter_map(|k| k.parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn classify_process(
    proc: &ProcessInfo,
    managed: &HashSet<u32>,
    system_managed: bool,
    duplicate_keys: &HashSet<(String, String)>,
) -> FindingClass {
    let is_managed = managed.contains(&proc.pid) || system_managed;
    let orphan_shaped = proc.ppid == 1 && !is_managed;

    if orphan_shaped && (proc.cpu_pct >= ORPHAN_CPU_PCT || proc.rss_bytes >= ORPHAN_RSS_BYTES) {
        return FindingClass::OrphanCandidate;
    }
    if orphan_shaped {
        let key = (proc.executable.clone(), proc.command.clone());
        if duplicate_keys.contains(&key) {
            return FindingClass::DuplicateOrphan;
        }
    }
    if proc.cpu_pct >= RUNAWAY_CPU_PCT && proc.ppid != 1 {
        return FindingClass::RunawaySuspect;
    }
    if proc.rss_bytes >= MEMORY_HOG_RSS_BYTES {
        return FindingClass::MemoryHog;
    }
    FindingClass::Observed
}

fn duplicate_orphan_keys(
    processes: &[ProcessInfo],
    managed: &HashSet<u32>,
) -> HashSet<(String, String)> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for p in processes {
        if p.ppid == 1
            && !managed.contains(&p.pid)
            && !is_system_domain_path(p)
            && p.pid != 1
        {
            let key = (p.executable.clone(), p.command.clone());
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|(k, _)| k)
        .collect()
}

fn taxonomy_for(class: FindingClass, proc: &ProcessInfo) -> (String, String, String, String) {
    match class {
        FindingClass::OrphanCandidate | FindingClass::DuplicateOrphan => (
            "processes".into(),
            "orphans".into(),
            "cleanup".into(),
            format!(
                "PPID=1 process pid={} executable={} cpu={:.1}% rss={}",
                proc.pid, proc.executable, proc.cpu_pct, proc.rss_bytes
            ),
        ),
        FindingClass::RunawaySuspect => (
            "cpu".into(),
            "busy-loop-suspects".into(),
            "optimization".into(),
            format!(
                "High CPU process cpu={:.1}% with ppid={}",
                proc.cpu_pct, proc.ppid
            ),
        ),
        FindingClass::MemoryHog => (
            "memory".into(),
            "rss-hogs".into(),
            "optimization".into(),
            format!(
                "RSS measurement {} bytes (>= 1 GiB); impact on pressure is inferred",
                proc.rss_bytes
            ),
        ),
        FindingClass::Observed => (
            "processes".into(),
            "observed".into(),
            "cleanup".into(),
            format!("Observed process pid={}", proc.pid),
        ),
    }
}

fn label_for(class: FindingClass) -> FactOrInference {
    match class {
        FindingClass::MemoryHog => FactOrInference::Fact,
        FindingClass::Observed => FactOrInference::None,
        _ => FactOrInference::Inference,
    }
}

fn expected_gain(class: FindingClass) -> &'static str {
    match class {
        FindingClass::OrphanCandidate | FindingClass::RunawaySuspect => "high",
        FindingClass::MemoryHog | FindingClass::DuplicateOrphan => "med",
        FindingClass::Observed => "low",
    }
}

fn risk(class: FindingClass) -> &'static str {
    match class {
        FindingClass::OrphanCandidate | FindingClass::DuplicateOrphan => "med",
        FindingClass::RunawaySuspect | FindingClass::MemoryHog => "low",
        FindingClass::Observed => "low",
    }
}

/// Classify processes into ranked finding shells (score filled by ranker).
pub fn classify(run: &RunSnapshot, self_pid: u32) -> Vec<(FindingClass, ProcessInfo, RankedFinding)> {
    let managed = launchd_managed_pids(run);
    let dup_keys = duplicate_orphan_keys(&run.processes, &managed);
    let mut out = Vec::new();

    for proc in &run.processes {
        if is_excluded(proc, self_pid) {
            continue;
        }
        // Layer 2: a PPID=1 process on a system path (or a Core Audio
        // Driver context) is managed by the system launchd domain.
        let system_managed = proc.ppid == 1 && is_system_domain_path(proc);
        let class = classify_process(proc, &managed, system_managed, &dup_keys);
        if class == FindingClass::Observed {
            continue;
        }
        let (category, subcategory, mode, summary) = taxonomy_for(class, proc);
        let mut evidence = std::collections::BTreeMap::new();
        evidence.insert("pid".into(), serde_json::json!(proc.pid));
        evidence.insert("ppid".into(), serde_json::json!(proc.ppid));
        evidence.insert("cpu_pct".into(), serde_json::json!(proc.cpu_pct));
        evidence.insert("rss_bytes".into(), serde_json::json!(proc.rss_bytes));
        evidence.insert("executable".into(), serde_json::json!(proc.executable));
        evidence.insert("command".into(), serde_json::json!(proc.command));
        evidence.insert(
            "launchd_managed".into(),
            serde_json::json!(managed.contains(&proc.pid) || system_managed),
        );
        if system_managed {
            evidence.insert("managed_by".into(), serde_json::json!("system domain path"));
        }

        let finding = RankedFinding {
            id: format!("{}.{}.{}", category, subcategory, proc.pid),
            category,
            subcategory,
            summary,
            evidence,
            expected_gain: expected_gain(class).into(),
            risk: risk(class).into(),
            reversible: "yes: SIGTERM after identity validation (PID+PPID+command)".into(),
            requires_user_action: true,
            mode,
            auto_ok: false,
            fact_or_inference: label_for(class),
            finding: class.as_str().into(),
            score: 0,
            pid: proc.pid,
            cpu_pct: proc.cpu_pct,
            rss_bytes: proc.rss_bytes,
            label: match label_for(class) {
                FactOrInference::Fact => "FACT".into(),
                FactOrInference::Inference => "INFERENCE".into(),
                FactOrInference::None => "none".into(),
            },
        };
        out.push((class, proc.clone(), finding));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        DisplayInfo, LaunchdInfo, MemoryInfo, PowerInfo, SystemInfo, ThermalInfo,
    };

    fn proc(pid: u32, ppid: u32, cpu: f64, rss: u64, exe: &str, cmd: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            cpu_pct: cpu,
            rss_bytes: rss,
            elapsed_secs: 10,
            executable: exe.into(),
            command: cmd.into(),
        }
    }

    fn run_with(processes: Vec<ProcessInfo>, managed: Vec<(u32, &str)>) -> RunSnapshot {
        let mut map = std::collections::BTreeMap::new();
        for (pid, label) in managed {
            map.insert(pid.to_string(), label.to_string());
        }
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
            processes,
            launchd: Some(LaunchdInfo { managed: map }),
            power: Some(PowerInfo {
                source: "AC".into(),
                percentage: Some(100),
            }),
            thermal: Some(ThermalInfo {
                thermal_pressure_level: None,
                raw_summary: "".into(),
            }),
            display: Some(DisplayInfo {
                display_count: 1,
                primary_summary: "ok".into(),
            }),
            probe_errors: vec![],
        }
    }

    #[test]
    fn orphan_vs_launchd_managed_vs_normal() {
        let snap = run_with(
            vec![
                proc(10, 1, 10.0, 100, "orphan", "orphan"),
                proc(11, 1, 10.0, 100, "agent", "agent"),
                proc(12, 50, 1.0, 100, "normal", "normal"),
            ],
            vec![(11, "com.example.agent")],
        );
        let findings = classify(&snap, 999);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, FindingClass::OrphanCandidate);
        assert_eq!(findings[0].1.pid, 10);
    }

    #[test]
    fn thresholds_and_classes() {
        let by_cpu = run_with(vec![proc(21, 1, 5.0, 100, "b", "b")], vec![]);
        assert_eq!(classify(&by_cpu, 1)[0].0, FindingClass::OrphanCandidate);
        let by_rss = run_with(
            vec![proc(22, 1, 0.0, ORPHAN_RSS_BYTES, "c", "c")],
            vec![],
        );
        assert_eq!(classify(&by_rss, 1)[0].0, FindingClass::OrphanCandidate);
        let low = run_with(
            vec![proc(20, 1, 4.9, ORPHAN_RSS_BYTES - 1, "a", "a")],
            vec![],
        );
        assert!(classify(&low, 1).is_empty());

        let runaway = run_with(vec![proc(30, 5, 50.0, 100, "r", "r")], vec![]);
        assert_eq!(classify(&runaway, 1)[0].0, FindingClass::RunawaySuspect);

        let hog = run_with(
            vec![proc(40, 5, 1.0, MEMORY_HOG_RSS_BYTES, "h", "h")],
            vec![],
        );
        assert_eq!(classify(&hog, 1)[0].0, FindingClass::MemoryHog);
        assert_eq!(
            classify(&hog, 1)[0].2.fact_or_inference,
            FactOrInference::Fact
        );
    }

    #[test]
    fn exclusions_pid1_kernel_self() {
        let snap = run_with(
            vec![
                proc(1, 0, 90.0, MEMORY_HOG_RSS_BYTES, "launchd", "/sbin/launchd"),
                proc(
                    0,
                    0,
                    90.0,
                    MEMORY_HOG_RSS_BYTES,
                    "kernel_task",
                    "kernel_task",
                ),
                proc(999, 1, 90.0, MEMORY_HOG_RSS_BYTES, "cleanos", "cleanos"),
            ],
            vec![],
        );
        assert!(classify(&snap, 999).is_empty());
    }

    #[test]
    fn duplicate_orphan_rule() {
        let snap = run_with(
            vec![
                proc(50, 1, 0.1, 100, "dup", "dup --x"),
                proc(51, 1, 0.1, 100, "dup", "dup --x"),
            ],
            vec![],
        );
        let findings = classify(&snap, 1);
        assert_eq!(findings.len(), 2);
        assert!(findings
            .iter()
            .all(|(c, _, _)| *c == FindingClass::DuplicateOrphan));
    }

    #[test]
    fn system_daemon_paths_are_not_orphans() {
        // Ground-truth system daemons: PPID=1 with high CPU, but launched by
        // the system launchd domain. Layer 2 must keep them out of findings.
        let snap = run_with(
            vec![
                proc(
                    418,
                    1,
                    72.4,
                    116_304 * 1024,
                    "WindowServer",
                    "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer -daemon",
                ),
                proc(432, 1, 43.8, 106_848 * 1024, "coreaudiod", "/usr/sbin/coreaudiod"),
                proc(
                    400,
                    1,
                    0.0,
                    2_160 * 1024,
                    "distnoted",
                    "/usr/sbin/distnoted daemon",
                ),
                proc(
                    402,
                    1,
                    0.0,
                    2_160 * 1024,
                    "tccd",
                    "/usr/libexec/tccd",
                ),
            ],
            vec![],
        );
        assert!(classify(&snap, 999).is_empty());
    }

    #[test]
    fn duplicate_orphan_excludes_system_daemons() {
        // distnoted xN run from /usr/sbin and must not count as duplicate
        // orphans even though they are not in the user launchctl list.
        let snap = run_with(
            vec![
                proc(410, 1, 0.1, 100, "distnoted", "/usr/sbin/distnoted daemon"),
                proc(411, 1, 0.1, 100, "distnoted", "/usr/sbin/distnoted daemon"),
            ],
            vec![],
        );
        assert!(classify(&snap, 1).is_empty());
    }

    #[test]
    fn userland_ppid1_not_managed_is_orphan() {
        // GitNexus-MCP pattern: user-land PPID=1 process under /Applications.
        let snap = run_with(
            vec![proc(
                812,
                1,
                6.0,
                100,
                "TeamsWidgetExtension",
                "/Applications/Microsoft Teams.app/Contents/PlugIns/TeamsWidgetExtension.appex/Contents/MacOS/TeamsWidgetExtension -AppleLanguages (\"en_us\", \"de-CH\")",
            )],
            vec![],
        );
        let findings = classify(&snap, 999);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, FindingClass::OrphanCandidate);
        assert_eq!(findings[0].1.pid, 812);
    }

    #[test]
    fn system_domain_path_evidence_note() {
        // A system-managed process can still surface as memory_hog; the
        // evidence must record launchd_managed=true with the path note.
        let snap = run_with(
            vec![proc(
                700,
                1,
                1.0,
                MEMORY_HOG_RSS_BYTES,
                "bigsys",
                "/System/Library/CoreServices/bigsys",
            )],
            vec![],
        );
        let findings = classify(&snap, 999);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, FindingClass::MemoryHog);
        assert_eq!(
            findings[0].2.evidence["launchd_managed"],
            serde_json::json!(true)
        );
        assert_eq!(
            findings[0].2.evidence["managed_by"],
            serde_json::json!("system domain path")
        );
    }
}
