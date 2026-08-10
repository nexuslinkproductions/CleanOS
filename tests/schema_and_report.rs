use cleanos::model::{
    DisplayInfo, LaunchdInfo, MemoryInfo, PowerInfo, ProcessInfo, ReportDocument, RunSnapshot,
    SystemInfo, ThermalInfo,
};
use cleanos::redaction;
use cleanos::reporter::{build_report, format_report_table, write_report};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn fixture_run() -> RunSnapshot {
    RunSnapshot {
        schema_version: "1".into(),
        collected_at: "2026-08-10T12:00:00+02:00".into(),
        duration_ms: 12,
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
            total_bytes: 16_000_000_000,
            used_bytes: 8_000_000_000,
            free_bytes: 8_000_000_000,
            compressor_bytes: 1000,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            pressure_level: 1,
            page_size_bytes: 16384,
        }),
        processes: vec![
            ProcessInfo {
                pid: 42,
                ppid: 1,
                cpu_pct: 12.0,
                rss_bytes: 600_000_000,
                elapsed_secs: 10,
                executable: "orphan".into(),
                command: "/Users/marcelspatz/bin/orphan --token sk-abc123XYZ --uuid 550e8400-e29b-41d4-a716-446655440000".into(),
            },
            ProcessInfo {
                pid: 7,
                ppid: 1,
                cpu_pct: 0.1,
                rss_bytes: 1000,
                elapsed_secs: 1,
                executable: "quiet".into(),
                command: "quiet".into(),
            },
        ],
        launchd: Some(LaunchdInfo {
            managed: Default::default(),
        }),
        power: Some(PowerInfo {
            source: "AC Power".into(),
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

fn validate_run_structure(v: &Value) {
    let obj = v.as_object().expect("run object");
    for key in [
        "schema_version",
        "collected_at",
        "duration_ms",
        "system",
        "memory",
        "processes",
        "launchd",
        "power",
        "thermal",
        "display",
        "probe_errors",
    ] {
        assert!(obj.contains_key(key), "missing run field {key}");
    }
    assert_eq!(obj["schema_version"], "1");
    assert!(obj["processes"].is_array());
    assert!(obj["probe_errors"].is_array());
}

fn validate_report_structure(v: &Value) {
    let obj = v.as_object().expect("report object");
    for key in [
        "schema_version",
        "generated_at",
        "source_run",
        "collected_at",
        "probe_count",
        "error_count",
        "findings_by_class",
        "findings",
        "launch_inventory",
        "zero_finding_notes",
        "summary_line",
    ] {
        assert!(obj.contains_key(key), "missing report field {key}");
    }
    assert_eq!(obj["schema_version"], "1");
    assert!(obj["findings"].is_array());
    if let Some(first) = obj["findings"].as_array().unwrap().first() {
        for key in [
            "id",
            "category",
            "subcategory",
            "summary",
            "evidence",
            "expected_gain",
            "risk",
            "reversible",
            "requires_user_action",
            "mode",
            "auto_ok",
            "fact_or_inference",
            "finding",
            "score",
            "pid",
            "cpu_pct",
            "rss_bytes",
            "label",
        ] {
            assert!(first.get(key).is_some(), "missing finding field {key}");
        }
        assert_eq!(first["auto_ok"], false);
    }
}

#[test]
fn schemas_exist_and_match_emitted_shape() {
    let tmp = std::env::temp_dir().join(format!("cleanos-schema-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("reports")).unwrap();
    std::env::set_var("CLEANOS_DATA_ROOT", &tmp);

    let run = fixture_run();
    let run_v = serde_json::to_value(&run).unwrap();
    validate_run_structure(&run_v);

    let report = build_report(&run, &PathBuf::from("20260810-120000.json")).unwrap();
    let report_path = write_report(&report, &tmp.join("reports").join("20260810-120000.report.json")).unwrap();
    let report_text = fs::read_to_string(&report_path).unwrap();
    assert!(!report_text.contains("/Users/marcelspatz"));
    assert!(report_text.contains("/Users/<user>") || report_text.contains("<redacted>") || report_text.contains("<uuid>"));
    let report_v: Value = serde_json::from_str(&report_text).unwrap();
    validate_report_structure(&report_v);

    let schema_run: Value = serde_json::from_str(
        &fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/run.schema.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        schema_run["$schema"],
        "http://json-schema.org/draft-07/schema#"
    );
    assert_eq!(schema_run["title"], "CleanOS Run Snapshot");

    let schema_report: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/report.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(schema_report["title"], "CleanOS Report");

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn report_output_is_redacted_and_honest() {
    let run = fixture_run();
    let report = build_report(&run, &PathBuf::from("fixture.json")).unwrap();
    let table = format_report_table(&report);
    assert!(!table.contains("/Users/marcelspatz"));
    assert!(table.contains("orphan_candidate") || table.contains("FINDINGS"));
    let redacted_cmd = redaction::redact_text(
        "/Users/marcelspatz/bin/orphan --token sk-abc123XYZ Bearer tok Serial C02ABCDEFG12",
    );
    assert!(!redacted_cmd.contains("marcelspatz"));
    assert!(redacted_cmd.contains("<user>"));
    assert!(redacted_cmd.contains("<redacted>"));
    assert!(redacted_cmd.contains("<serial>"));

    let empty = RunSnapshot {
        processes: vec![],
        ..fixture_run()
    };
    let empty_report = build_report(&empty, &PathBuf::from("empty.json")).unwrap();
    for note in &empty_report.zero_finding_notes {
        assert!(note.starts_with("Zero findings for class "));
    }
}

#[test]
fn report_document_roundtrip() {
    let run = fixture_run();
    let report = build_report(&run, &PathBuf::from("x.json")).unwrap();
    let json = serde_json::to_string(&report).unwrap();
    let back: ReportDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(back.schema_version, "1");
    assert!(!back.findings.is_empty());
}
