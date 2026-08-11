//! Classification rules and thresholds (SPEC section 5, harness-fleet section 2).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::model::{
    FactOrInference, HarnessState, ProcessInfo, RankedFinding, RunSnapshot, SocketEntry,
};

/// CPU percent threshold for orphan_candidate.
pub const ORPHAN_CPU_PCT: f64 = 5.0;
/// RSS bytes threshold for orphan_candidate (512 MiB).
pub const ORPHAN_RSS_BYTES: u64 = 536_870_912;
/// CPU percent threshold for runaway_suspect.
pub const RUNAWAY_CPU_PCT: f64 = 50.0;
/// RSS bytes threshold for memory_hog (1 GiB).
pub const MEMORY_HOG_RSS_BYTES: u64 = 1_073_741_824;
/// Elapsed seconds threshold for stale_dev_server (6 hours).
pub const STALE_DEV_ELAPSED_SECS: u64 = 6 * 60 * 60;
/// Executables considered dev servers for stale_dev_server.
pub const DEV_SERVER_EXECUTABLES: &[&str] = &["node", "python", "python3", "deno"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingClass {
    OrphanCandidate,
    RunawaySuspect,
    MemoryHog,
    DuplicateOrphan,
    Observed,
    HarnessMcpServer,
    HarnessLsp,
    HarnessAgentDaemon,
    StaleDevServer,
}

impl FindingClass {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingClass::OrphanCandidate => "orphan_candidate",
            FindingClass::RunawaySuspect => "runaway_suspect",
            FindingClass::MemoryHog => "memory_hog",
            FindingClass::DuplicateOrphan => "duplicate_orphan",
            FindingClass::Observed => "observed",
            FindingClass::HarnessMcpServer => "harness_mcp_server",
            FindingClass::HarnessLsp => "harness_lsp",
            FindingClass::HarnessAgentDaemon => "harness_agent_daemon",
            FindingClass::StaleDevServer => "stale_dev_server",
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
    if proc
        .command
        .to_ascii_lowercase()
        .contains("core audio driver")
    {
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

/// Marker rules for the four harness classes (SPEC section 2), matched
/// against the lowercase command/args tokens.
const MCP_MARKERS: &[&str] = &["mcp", "--mcp", "mcp.json", "stdio+mcp", "mcp-socket"];
const LSP_MARKERS: &[&str] = &[
    "typescript-language-server",
    "pyright-langserver",
    "rust-analyzer",
    "vscode-langservers",
    "lsp",
];
const AGENT_DAEMON_MARKERS: &[&str] = &["codex", "opencode", "cursor-agent", "claude", "hermes"];

/// Harness markers found in a process command line, in the SPEC class order.
/// Returns the concrete marker strings; empty means no harness marker.
pub fn harness_markers(proc: &ProcessInfo) -> Vec<String> {
    let lower = proc.command.to_ascii_lowercase();
    let mut markers = Vec::new();

    // harness_mcp_server: token "mcp", "--mcp", "mcp.json", "stdio"+"mcp",
    // socket path under /tmp/ or ~/Library/.../mcp.
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let has_mcp_token = tokens.iter().any(|t| *t == "mcp");
    if has_mcp_token {
        markers.push("mcp".to_string());
    }
    if lower.contains("--mcp") {
        markers.push("--mcp".to_string());
    }
    if lower.contains("mcp.json") {
        markers.push("mcp.json".to_string());
    }
    let has_stdio = tokens.iter().any(|t| *t == "stdio");
    if has_stdio && has_mcp_token {
        markers.push("stdio+mcp".to_string());
    }
    let mcp_socket = tokens.iter().any(|t| {
        (t.starts_with("/tmp/") || t.starts_with("~/library/") || t.contains("/mcp"))
            && t.contains("mcp")
    });
    if mcp_socket {
        markers.push("mcp-socket".to_string());
    }

    // harness_lsp: "--stdio" plus one of the LSP binary names.
    if tokens.iter().any(|t| *t == "--stdio") {
        for m in LSP_MARKERS {
            if tokens.iter().any(|t| t.contains(m)) {
                markers.push((*m).to_string());
                break;
            }
        }
    }

    // harness_agent_daemon: known agent binaries or daemon args. The cleanos
    // process itself is excluded by pid in classify(), never by name.
    for m in AGENT_DAEMON_MARKERS {
        if tokens.iter().any(|t| t.contains(m)) {
            markers.push((*m).to_string());
            break;
        }
    }

    // stale_dev_server: node/python/deno executable (socket and elapsed
    // checks happen in classify against the sockets map).
    let exe = proc.executable.to_ascii_lowercase();
    if DEV_SERVER_EXECUTABLES.iter().any(|e| *e == exe) {
        markers.push("dev-server".to_string());
    }

    markers
}

/// True when the process listens on a localhost address.
fn listens_on_localhost(proc: &ProcessInfo, run: &RunSnapshot) -> bool {
    run.sockets
        .get(&proc.pid.to_string())
        .map(|entries| {
            entries.iter().any(|s| {
                matches!(
                    s.host.as_str(),
                    "127.0.0.1" | "::1" | "localhost" | "[::1]" | "*"
                )
            })
        })
        .unwrap_or(false)
}

/// Socket probe availability: false when lsof was unavailable and the probe
/// was skipped with a note, so stale_dev_server degrades to elapsed-only.
pub fn sockets_available(run: &RunSnapshot) -> bool {
    !run.probe_errors.iter().any(|e| e.probe == "sockets")
}

/// The harness class for a marker set, in SPEC table order. The dev-server
/// marker alone is not enough: stale_dev_server needs LISTEN evidence and
/// elapsed time, so it is decided separately in classify().
fn harness_class_for(markers: &[String]) -> Option<FindingClass> {
    if markers.iter().any(|m| MCP_MARKERS.iter().any(|k| k == m)) {
        return Some(FindingClass::HarnessMcpServer);
    }
    if markers.iter().any(|m| LSP_MARKERS.iter().any(|k| k == m)) {
        return Some(FindingClass::HarnessLsp);
    }
    if markers
        .iter()
        .any(|m| AGENT_DAEMON_MARKERS.iter().any(|k| k == m))
    {
        return Some(FindingClass::HarnessAgentDaemon);
    }
    None
}

/// Harness state: orphaned only when PPID=1 with no launchd job behind it
/// (the GitNexus incident shape); anything else is attached.
fn harness_state(proc: &ProcessInfo, managed: &HashSet<u32>, system_managed: bool) -> HarnessState {
    let is_managed = managed.contains(&proc.pid) || system_managed;
    if proc.ppid == 1 && !is_managed {
        HarnessState::Orphaned
    } else {
        HarnessState::Attached
    }
}

/// reap_safe per SPEC section 2: true only when user-land AND not
/// launchd-managed AND not self AND not system-path AND state == orphaned.
/// Computed for the future reaper, never acted on.
pub fn reap_safe(
    proc: &ProcessInfo,
    managed: &HashSet<u32>,
    system_managed: bool,
    self_pid: u32,
) -> bool {
    let user_land = !is_system_domain_path(proc);
    let is_managed = managed.contains(&proc.pid) || system_managed;
    user_land
        && !is_managed
        && proc.pid != self_pid
        && !is_system_domain_path(proc)
        && harness_state(proc, managed, system_managed) == HarnessState::Orphaned
}

/// True when a harness marker process must be excluded (SPEC section 2):
/// the cleanos process itself, launchd-managed, system-path, kernel.
fn harness_excluded(proc: &ProcessInfo, managed: &HashSet<u32>, self_pid: u32) -> bool {
    if proc.pid == self_pid {
        return true;
    }
    if proc.pid == 1 || proc.pid == 0 || proc.executable == "kernel_task" {
        return true;
    }
    if managed.contains(&proc.pid) {
        return true;
    }
    if is_system_domain_path(proc) {
        return true;
    }
    false
}

fn harness_finding(
    class: FindingClass,
    proc: &ProcessInfo,
    markers: &[String],
    managed: &HashSet<u32>,
    system_managed: bool,
    self_pid: u32,
    sockets: Option<Vec<SocketEntry>>,
) -> RankedFinding {
    let state = harness_state(proc, managed, system_managed);
    let rs = reap_safe(proc, managed, system_managed, self_pid);
    let is_managed = managed.contains(&proc.pid) || system_managed;
    let mut evidence = BTreeMap::new();
    evidence.insert("pid".into(), serde_json::json!(proc.pid));
    evidence.insert("ppid".into(), serde_json::json!(proc.ppid));
    evidence.insert("executable".into(), serde_json::json!(proc.executable));
    evidence.insert("command".into(), serde_json::json!(proc.command));
    evidence.insert("cpu_pct".into(), serde_json::json!(proc.cpu_pct));
    evidence.insert("rss_bytes".into(), serde_json::json!(proc.rss_bytes));
    evidence.insert("elapsed_secs".into(), serde_json::json!(proc.elapsed_secs));
    evidence.insert("launchd_managed".into(), serde_json::json!(is_managed));
    evidence.insert(
        "state".into(),
        serde_json::json!(match state {
            HarnessState::Orphaned => "orphaned",
            HarnessState::Attached => "attached",
        }),
    );
    evidence.insert("reap_safe".into(), serde_json::json!(rs));
    evidence.insert("harnessreap_compatible".into(), serde_json::json!(true));
    evidence.insert("markers".into(), serde_json::json!(markers));
    if let Some(sock) = sockets {
        evidence.insert("sockets".into(), serde_json::json!(sock));
    }

    let (category, subcategory, mode, summary) = taxonomy_for(class, proc);
    let summary = format!(
        "{} state={} reap_safe={}",
        summary,
        match state {
            HarnessState::Orphaned => "orphaned",
            HarnessState::Attached => "attached",
        },
        rs
    );

    RankedFinding {
        id: format!("{category}.{subcategory}.{}", proc.pid),
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
    }
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
        if p.ppid == 1 && !managed.contains(&p.pid) && !is_system_domain_path(p) && p.pid != 1 {
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
        FindingClass::HarnessMcpServer => (
            "harness-fleet".into(),
            "harness_mcp_server".into(),
            "cleanup".into(),
            format!("MCP server pid={} executable={}", proc.pid, proc.executable),
        ),
        FindingClass::HarnessLsp => (
            "harness-fleet".into(),
            "harness_lsp".into(),
            "cleanup".into(),
            format!(
                "LSP instance pid={} executable={}",
                proc.pid, proc.executable
            ),
        ),
        FindingClass::HarnessAgentDaemon => (
            "harness-fleet".into(),
            "harness_agent_daemon".into(),
            "cleanup".into(),
            format!(
                "Agent daemon pid={} executable={}",
                proc.pid, proc.executable
            ),
        ),
        FindingClass::StaleDevServer => (
            "harness-fleet".into(),
            "stale_dev_server".into(),
            "cleanup".into(),
            format!(
                "Dev server pid={} executable={} elapsed={}s",
                proc.pid, proc.executable, proc.elapsed_secs
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
        FindingClass::HarnessMcpServer
        | FindingClass::HarnessLsp
        | FindingClass::HarnessAgentDaemon
        | FindingClass::StaleDevServer => "med",
        FindingClass::Observed => "low",
    }
}

fn risk(class: FindingClass) -> &'static str {
    match class {
        FindingClass::OrphanCandidate | FindingClass::DuplicateOrphan => "med",
        FindingClass::RunawaySuspect | FindingClass::MemoryHog => "low",
        FindingClass::HarnessMcpServer
        | FindingClass::HarnessLsp
        | FindingClass::HarnessAgentDaemon
        | FindingClass::StaleDevServer => "low",
        FindingClass::Observed => "low",
    }
}

/// Classify processes into ranked finding shells (score filled by ranker).
pub fn classify(
    run: &RunSnapshot,
    self_pid: u32,
) -> Vec<(FindingClass, ProcessInfo, RankedFinding)> {
    let managed = launchd_managed_pids(run);
    let dup_keys = duplicate_orphan_keys(&run.processes, &managed);
    let mut out = Vec::new();

    for proc in &run.processes {
        // Layer 2: a PPID=1 process on a system path (or a Core Audio
        // Driver context) is managed by the system launchd domain.
        let system_managed = proc.ppid == 1 && is_system_domain_path(proc);

        // Harness-fleet lane (SPEC section 2): marker processes are reported
        // with state attached or orphaned, subject to the harness exclusions.
        let markers = harness_markers(proc);
        if !markers.is_empty() && !harness_excluded(proc, &managed, self_pid) {
            let sockets_for = if listens_on_localhost(proc, run) {
                run.sockets.get(&proc.pid.to_string()).cloned()
            } else {
                None
            };
            let class = harness_class_for(&markers).or_else(|| {
                // dev-server marker alone: stale_dev_server needs LISTEN
                // evidence and elapsed >= 6h; degrades to elapsed-only when
                // the socket probe was skipped (SPEC section 3).
                let elapsed_ok = proc.elapsed_secs >= STALE_DEV_ELAPSED_SECS;
                let listen_ok = if sockets_available(run) {
                    listens_on_localhost(proc, run)
                } else {
                    true
                };
                if markers.iter().any(|m| m == "dev-server") && elapsed_ok && listen_ok {
                    Some(FindingClass::StaleDevServer)
                } else {
                    None
                }
            });
            if let Some(class) = class {
                let sockets_for = if class == FindingClass::StaleDevServer {
                    run.sockets.get(&proc.pid.to_string()).cloned()
                } else {
                    sockets_for
                };
                let finding = harness_finding(
                    class,
                    proc,
                    &markers,
                    &managed,
                    system_managed,
                    self_pid,
                    sockets_for,
                );
                out.push((class, proc.clone(), finding));
                continue;
            }
        }

        if is_excluded(proc, self_pid) {
            continue;
        }
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
