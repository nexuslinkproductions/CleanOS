//! cleanos doctor: diagnostics against the live machine.
//! This is the verification mechanic: it runs the probes and report on the
//! machine and cross-checks the output against ground truth. Fixture-based
//! unit tests are deliberately absent; they encode the same assumptions as
//! the code and verify nothing.

use std::fs;

use crate::error::CleanOsError;
use crate::model::RunSnapshot;
use crate::paths::run_path_for;
use crate::probes;
use crate::reporter;

pub struct Diagnostic {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

const SYSTEM_PATHS: [&str; 8] = [
    "/System/",
    "/usr/libexec",
    "/usr/sbin",
    "/usr/lib/",
    "/sbin/",
    "/bin/",
    "/usr/bin/",
    "/Library/SystemExtensions",
];

pub fn run(include_bench: bool) -> Result<Vec<Diagnostic>, CleanOsError> {
    let mut diags = Vec::new();

    let snapshot = probes::collect_run();
    let run_json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CleanOsError::Io(format!("serialize run: {e}")))?;

    diags.push(Diagnostic {
        name: "probe errors".into(),
        ok: snapshot.probe_errors.is_empty(),
        detail: if snapshot.probe_errors.is_empty() {
            "all probes ran clean".into()
        } else {
            format!(
                "{}: {}",
                snapshot.probe_errors[0].probe, snapshot.probe_errors[0].message
            )
        },
    });

    diags.push(Diagnostic {
        name: "sockets probe".into(),
        ok: !snapshot.sockets.is_empty(),
        detail: format!("{} listening sockets", snapshot.sockets.len()),
    });

    let run_path = run_path_for(&chrono::Local::now())?;
    fs::write(&run_path, &run_json)
        .map_err(|e| CleanOsError::Io(format!("write {}: {e}", run_path.display())))?;
    let report = reporter::build_report(&snapshot, &run_path)?;

    let leaks: Vec<String> = report
        .findings
        .iter()
        .filter(|f| {
            let cmd = f
                .evidence
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            SYSTEM_PATHS.iter().any(|p| cmd.starts_with(p)) || cmd.contains("Core Audio Driver")
        })
        .map(|f| f.id.clone())
        .collect();
    diags.push(Diagnostic {
        name: "system-path leak".into(),
        ok: leaks.is_empty(),
        detail: if leaks.is_empty() {
            format!(
                "{} findings, none under system paths",
                report.findings.len()
            )
        } else {
            format!(
                "{} leaked findings: {}",
                leaks.len(),
                leaks[..leaks.len().min(5)].join(", ")
            )
        },
    });

    let violations: Vec<String> = report
        .findings
        .iter()
        .filter(|f| {
            let ev = &f.evidence;
            ev.get("reap_safe").and_then(|v| v.as_bool()) == Some(true)
                && (ev.get("launchd_managed").and_then(|v| v.as_bool()) == Some(true)
                    || ev.get("state").and_then(|v| v.as_str()) == Some("attached"))
        })
        .map(|f| f.id.clone())
        .collect();
    diags.push(Diagnostic {
        name: "reap_safe discipline".into(),
        ok: violations.is_empty(),
        detail: if violations.is_empty() {
            "no managed or attached finding is marked reap_safe".into()
        } else {
            format!(
                "violations: {}",
                violations[..violations.len().min(5)].join(", ")
            )
        },
    });

    let username = std::env::var("USER").unwrap_or_default();
    let report_path = crate::paths::report_path_for_run(&run_path)?;
    let written_path = reporter::write_report(&report, &report_path)?;
    let written_text = fs::read_to_string(&written_path).unwrap_or_default();
    let home_leak = !username.is_empty() && written_text.contains(&format!("/Users/{username}"));
    diags.push(Diagnostic {
        name: "redaction".into(),
        ok: !home_leak,
        detail: if home_leak {
            "written report JSON contains the home-absolute username".into()
        } else {
            "written report JSON is redacted".into()
        },
    });

    let roundtrip = serde_json::from_str::<RunSnapshot>(&run_json).is_ok();
    diags.push(Diagnostic {
        name: "run JSON roundtrip".into(),
        ok: roundtrip,
        detail: if roundtrip {
            "run snapshot serializes and parses back".into()
        } else {
            "roundtrip failed".into()
        },
    });

    if include_bench {
        const BASELINE_BURST_SEC: f64 = 2.90;
        const TOLERANCE_PCT: f64 = 25.0;
        match crate::bench::quick_burst_median(1) {
            Ok(median) => {
                let pct = (median - BASELINE_BURST_SEC).abs() / BASELINE_BURST_SEC * 100.0;
                diags.push(Diagnostic {
                    name: "bench tolerance".into(),
                    ok: pct <= TOLERANCE_PCT,
                    detail: format!(
                        "cpu burst median {median:.2}s vs baseline {BASELINE_BURST_SEC}s ({pct:.1}%)"
                    ),
                });
            }
            Err(e) => diags.push(Diagnostic {
                name: "bench tolerance".into(),
                ok: false,
                detail: format!("quick bench failed: {e}"),
            }),
        }
    }

    Ok(diags)
}

pub fn format_diagnostics(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<26} {:<6} {}\n",
        "DIAGNOSTIC", "STATE", "DETAIL"
    ));
    out.push_str(&format!("{:-<26} {:-<6} {}\n", "", "", ""));
    for d in diags {
        out.push_str(&format!(
            "{:<26} {:<6} {}\n",
            d.name,
            if d.ok { "PASS" } else { "FAIL" },
            crate::redaction::redact_text(&d.detail)
        ));
    }
    out
}
