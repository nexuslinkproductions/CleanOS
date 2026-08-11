use cleanos::bench::{
    BenchResult, CpuBurstProbe, CpuTiming, CryptoProbe, MachineInfo, MemoryProbe, PowerProbe,
    Probes, SkippedProbe,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn timing(real: f64, user: f64, sys: f64) -> CpuTiming {
    CpuTiming { real, user, sys }
}

fn machine() -> MachineInfo {
    MachineInfo {
        chip: "Apple M2 Pro".to_string(),
        cpu_count: 12,
        os_version: "26.4.1".to_string(),
    }
}

fn fixture_full() -> BenchResult {
    BenchResult {
        collected_at: "2026-08-11T08:00:00+02:00".to_string(),
        duration_ms: 90_000,
        machine: machine(),
        probes: Probes {
            cpu_burst: Some(CpuBurstProbe {
                runs: vec![
                    timing(2.93, 22.81, 0.77),
                    timing(2.90, 22.72, 0.75),
                    timing(2.91, 22.71, 0.74),
                ],
                median: timing(2.91, 22.72, 0.75),
            }),
            cpu_sustained: Some(timing(13.97, 114.03, 3.50)),
            crypto: Some(CryptoProbe {
                aes_128_gcm_4kb: 70.0,
                aes_128_gcm_peak: 74.0,
                sha256_4kb: 23.1,
                sha256_peak: 23.5,
                fallback: false,
            }),
            memory: Some(MemoryProbe {
                pages_free_delta: -204_991,
                swap_used_bytes_before: 0,
                swap_used_bytes_after: 0,
                compressor_bytes_before: 1_310_720_000,
                compressor_bytes_after: 1_392_640_000,
            }),
            power: PowerProbe {
                captured: true,
                reason: None,
                peak_p0_mhz: Some(2839),
                peak_p1_mhz: Some(2846),
                max_core_mhz: Some(3467),
                e_cluster_mhz: Some(2186),
                cpu_mw: Some(11024),
                gpu_mw: Some(2024),
                combined_mw: Some(13047),
            },
        },
        skipped: vec![],
    }
}

fn fixture_sparse() -> BenchResult {
    BenchResult {
        collected_at: "2026-08-11T09:00:00+02:00".to_string(),
        duration_ms: 5_000,
        machine: machine(),
        probes: Probes {
            cpu_burst: Some(CpuBurstProbe {
                runs: vec![timing(2.95, 22.90, 0.78)],
                median: timing(2.95, 22.90, 0.78),
            }),
            cpu_sustained: None,
            crypto: None,
            memory: None,
            power: PowerProbe {
                captured: false,
                reason: Some("not requested (pass --power)".to_string()),
                ..PowerProbe::default()
            },
        },
        skipped: vec![
            SkippedProbe {
                probe: "cpu_sustained".to_string(),
                reason: "not included in --quick mode".to_string(),
            },
            SkippedProbe {
                probe: "crypto".to_string(),
                reason: "not included in --quick mode".to_string(),
            },
            SkippedProbe {
                probe: "memory".to_string(),
                reason: "not included in --quick mode".to_string(),
            },
        ],
    }
}

fn type_matches(value: &Value, ty: &str) -> bool {
    match ty {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn value_type_name(value: &Value) -> &'static str {
    if value.is_string() {
        "string"
    } else if value.is_i64() || value.is_u64() {
        "integer"
    } else if value.is_number() {
        "number"
    } else if value.is_boolean() {
        "boolean"
    } else if value.is_array() {
        "array"
    } else if value.is_object() {
        "object"
    } else {
        "null"
    }
}

/// Minimal draft-07 validator: required fields, property types (including
/// type unions with null), recursive objects and arrays.
fn check(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for key in required {
            let name = key.as_str().unwrap_or("");
            if value.get(name).is_none() {
                errors.push(format!("{path}: missing required field {name}"));
            }
        }
    }
    if let Some(ty) = schema.get("type") {
        let types: Vec<&str> = match ty {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(|t| t.as_str()).collect(),
            _ => vec![],
        };
        if !types.is_empty() && !types.iter().any(|t| type_matches(value, t)) {
            errors.push(format!(
                "{path}: type mismatch, expected {types:?}, got {}",
                value_type_name(value)
            ));
        }
    }
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        if value.is_object() {
            for (key, sub) in props {
                if let Some(v) = value.get(key) {
                    check(v, sub, &format!("{path}.{key}"), errors);
                }
            }
        }
    }
    if value.is_array() {
        if let Some(items) = schema.get("items") {
            for (i, v) in value.as_array().unwrap().iter().enumerate() {
                check(v, items, &format!("{path}[{i}]"), errors);
            }
        }
    }
}

#[test]
fn bench_schema_is_draft07_with_matching_title() {
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/bench.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    assert_eq!(schema["title"], "CleanOS Benchmark Result");
}

#[test]
fn full_bench_result_validates_against_schema() {
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/bench.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let value = serde_json::to_value(fixture_full()).unwrap();
    let mut errors = Vec::new();
    check(&value, &schema, "root", &mut errors);
    assert!(errors.is_empty(), "schema violations: {errors:#?}");
}

#[test]
fn sparse_quick_bench_result_validates_against_schema() {
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/bench.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let value = serde_json::to_value(fixture_sparse()).unwrap();
    let mut errors = Vec::new();
    check(&value, &schema, "root", &mut errors);
    assert!(errors.is_empty(), "schema violations: {errors:#?}");
}

#[test]
fn emitted_shape_has_exact_spec_fields() {
    let value = serde_json::to_value(fixture_full()).unwrap();
    let obj = value.as_object().unwrap();
    for key in [
        "collected_at",
        "duration_ms",
        "machine",
        "probes",
        "skipped",
    ] {
        assert!(obj.contains_key(key), "missing top-level field {key}");
    }
    let probes = obj["probes"].as_object().unwrap();
    for key in ["cpu_burst", "cpu_sustained", "crypto", "memory", "power"] {
        assert!(probes.contains_key(key), "missing probe {key}");
    }
    let cpu = &probes["cpu_burst"]["median"];
    for key in ["real", "user", "sys"] {
        assert!(cpu.get(key).is_some(), "missing median field {key}");
    }
    let crypto = &probes["crypto"];
    for key in [
        "aes_128_gcm_4kb",
        "aes_128_gcm_peak",
        "sha256_4kb",
        "sha256_peak",
        "fallback",
    ] {
        assert!(crypto.get(key).is_some(), "missing crypto field {key}");
    }
    let memory = &probes["memory"];
    for key in [
        "pages_free_delta",
        "swap_used_bytes_before",
        "swap_used_bytes_after",
        "compressor_bytes_before",
        "compressor_bytes_after",
    ] {
        assert!(memory.get(key).is_some(), "missing memory field {key}");
    }
    let power = &probes["power"];
    for key in [
        "captured",
        "reason",
        "peak_p0_mhz",
        "peak_p1_mhz",
        "max_core_mhz",
        "e_cluster_mhz",
        "cpu_mw",
        "gpu_mw",
        "combined_mw",
    ] {
        assert!(power.get(key).is_some(), "missing power field {key}");
    }
    assert_eq!(power["captured"], true);
    assert!(power["reason"].is_null());
    assert!(power["peak_p0_mhz"].is_number());
}
