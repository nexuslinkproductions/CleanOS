//! Benchmark suite: bounded, read-only probes with stored JSON results.
//!
//! Probes shell out to macOS tools and parse raw output: the system output
//! IS the evidence (same philosophy as the scan core). A probe that cannot
//! run (missing binary, no root) is recorded as skipped with a reason and
//! never aborts the run.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::error::CleanOsError;
use crate::parsers;
use crate::paths;
use crate::redaction;

const FFMPEG_BREW: &str = "/opt/homebrew/bin/ffmpeg";
const OPENSSL_BREW: &str = "/opt/homebrew/opt/openssl@3/bin/openssl";
const OPENSSL_SYSTEM: &str = "/usr/bin/openssl";
const TIME_BIN: &str = "/usr/bin/time";

/// OpenSSL 3.x speed block sizes in bytes (no 4096 column exists there).
const BLOCK_SIZES_CANONICAL: [u64; 6] = [16, 64, 256, 1024, 8192, 16384];

const MEMORY_ALLOC_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB touched buffer

// ---------------------------------------------------------------------------
// Result models (SPEC section 4)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub collected_at: String,
    pub duration_ms: u64,
    pub machine: MachineInfo,
    pub probes: Probes,
    pub skipped: Vec<SkippedProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MachineInfo {
    pub chip: String,
    pub cpu_count: u32,
    pub os_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Probes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_burst: Option<CpuBurstProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_sustained: Option<CpuTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto: Option<CryptoProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryProbe>,
    pub power: PowerProbe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuBurstProbe {
    pub runs: Vec<CpuTiming>,
    pub median: CpuTiming,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CpuTiming {
    pub real: f64,
    pub user: f64,
    pub sys: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoProbe {
    pub aes_128_gcm_4kb: f64,
    pub aes_128_gcm_peak: f64,
    pub sha256_4kb: f64,
    pub sha256_peak: f64,
    pub fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProbe {
    pub pages_free_delta: i64,
    pub swap_used_bytes_before: u64,
    pub swap_used_bytes_after: u64,
    pub compressor_bytes_before: u64,
    pub compressor_bytes_after: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowerProbe {
    pub captured: bool,
    pub reason: Option<String>,
    pub peak_p0_mhz: Option<u64>,
    pub peak_p1_mhz: Option<u64>,
    pub max_core_mhz: Option<u64>,
    pub e_cluster_mhz: Option<u64>,
    pub cpu_mw: Option<u64>,
    pub gpu_mw: Option<u64>,
    pub combined_mw: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedProbe {
    pub probe: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Parsers (fixture tested; raw system output is the evidence)
// ---------------------------------------------------------------------------

/// Parse `/usr/bin/time -p` output: `real X`, `user Y`, `sys Z`.
pub fn parse_time_p(output: &str) -> Result<(f64, f64, f64), String> {
    let mut real: Option<f64> = None;
    let mut user: Option<f64> = None;
    let mut sys: Option<f64> = None;
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        let value: f64 = match tokens[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        match tokens[0].to_ascii_lowercase().as_str() {
            "real" => real = Some(value),
            "user" => user = Some(value),
            "sys" => sys = Some(value),
            _ => {}
        }
    }
    match (real, user, sys) {
        (Some(r), Some(u), Some(s)) => Ok((r, u, s)),
        _ => Err(format!(
            "time -p output missing real/user/sys lines: {}",
            output.trim()
        )),
    }
}

/// One parsed openssl speed row: block sizes and throughput values in
/// 1000s of bytes per second (the k-suffixed numbers openssl prints).
#[derive(Debug, Clone)]
pub struct SpeedRow {
    pub sizes: Vec<u64>,
    pub values_k: Vec<f64>,
}

/// Parse an openssl speed data row for `algo` from either the multi-child
/// format (aggregate row at the end, sizes in `+H:` lines) or the single
/// format (`type ... bytes` header line).
pub fn parse_openssl_speed(output: &str, algo: &str) -> Result<SpeedRow, String> {
    let algo_lower = algo.to_ascii_lowercase();
    let data_line = output
        .lines()
        .rev()
        .find(|line| {
            let trimmed = line.trim();
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            tokens.len() > 1
                && tokens[0].to_ascii_lowercase() == algo_lower
                && !trimmed.to_ascii_lowercase().starts_with("got:")
        })
        .ok_or_else(|| format!("no speed row found for {algo}"))?;
    let tokens: Vec<&str> = data_line.trim().split_whitespace().collect();
    let values_k: Vec<f64> = tokens[1..]
        .iter()
        .map(|token| {
            let num = token.trim_end_matches(|c| c == 'k' || c == 'K');
            num.parse::<f64>()
                .map_err(|_| format!("invalid speed value: {token}"))
        })
        .collect::<Result<_, _>>()?;
    let mut sizes = find_block_sizes(output);
    if sizes.len() != values_k.len() {
        if values_k.len() == BLOCK_SIZES_CANONICAL.len() {
            sizes = BLOCK_SIZES_CANONICAL.to_vec();
        } else {
            return Err(format!(
                "speed row column count {} does not match block sizes {}",
                values_k.len(),
                sizes.len()
            ));
        }
    }
    Ok(SpeedRow { sizes, values_k })
}

fn find_block_sizes(output: &str) -> Vec<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("type") {
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            let mut sizes = Vec::new();
            let mut i = 1;
            while i < tokens.len() {
                if let Ok(n) = tokens[i].parse::<u64>() {
                    sizes.push(n);
                }
                i += 2;
            }
            if !sizes.is_empty() {
                return sizes;
            }
        }
        if let Some(idx) = trimmed.find("+H:") {
            let after = &trimmed[idx + 3..];
            let body: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ':')
                .collect();
            let sizes: Vec<u64> = body.split(':').filter_map(|p| p.parse().ok()).collect();
            if !sizes.is_empty() {
                return sizes;
            }
        }
    }
    BLOCK_SIZES_CANONICAL.to_vec()
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn gb_per_s(thousand_bytes_per_s: f64) -> f64 {
    // openssl prints 1000s of bytes per second; one GB/s is 1e9 bytes/s.
    round2(thousand_bytes_per_s * 1000.0 / 1_000_000_000.0)
}

/// Extract the 4 KB row and the peak row as GB/s.
///
/// The SPEC calls the first value the 4 KB block row. OpenSSL 3.x tables
/// list 16, 64, 256, 1024, 8192 and 16384 byte blocks with no 4096 column;
/// the 2026-08-11 baseline captured this value from the 8192 byte column
/// (the largest mid-size block), so that column is the fallback when 4096
/// is absent. The peak is the highest-throughput column, which the baseline
/// records as the 8 KB peak row.
pub fn crypto_values(row: &SpeedRow) -> Result<(f64, f64), String> {
    let four_kb_index = row
        .sizes
        .iter()
        .position(|s| *s == 4096)
        .or_else(|| row.sizes.iter().position(|s| *s == 8192))
        .ok_or_else(|| "speed row has no 4 KB or 8 KB block column".to_string())?;
    let peak_k = row
        .values_k
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    Ok((gb_per_s(row.values_k[four_kb_index]), gb_per_s(peak_k)))
}

/// One parsed powermetrics sample with the SPEC envelope fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerSample {
    pub p0_mhz: Option<u64>,
    pub p1_mhz: Option<u64>,
    pub max_core_mhz: Option<u64>,
    pub e_cluster_mhz: Option<u64>,
    pub cpu_mw: Option<u64>,
    pub gpu_mw: Option<u64>,
    pub combined_mw: Option<u64>,
}

fn parse_mhz_mw(line: &str, key: &str, unit: &str) -> Option<u64> {
    let rest = line.strip_prefix(key)?;
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let value: u64 = digits.parse().ok()?;
    if after[digits.len()..].trim_start().starts_with(unit) {
        Some(value)
    } else {
        None
    }
}

fn parse_core_frequency(line: &str) -> Option<u64> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 || !tokens[0].eq_ignore_ascii_case("cpu") {
        return None;
    }
    if tokens[2].trim_end_matches(':') != "frequency" {
        return None;
    }
    tokens[3].parse::<u64>().ok()
}

/// Parse powermetrics output into per-sample envelope structs. Sample
/// blocks are separated by `Sample:` headers or `***` lines.
pub fn parse_powermetrics(output: &str) -> Vec<PowerSample> {
    let mut samples: Vec<PowerSample> = Vec::new();
    let mut current = PowerSample::default();
    let mut current_has_data = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sample:") || trimmed.starts_with("***") {
            if current_has_data {
                samples.push(std::mem::take(&mut current));
                current_has_data = false;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(v) = parse_mhz_mw(trimmed, "P0-Cluster HW active frequency", "MHz") {
            current.p0_mhz = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_mhz_mw(trimmed, "P1-Cluster HW active frequency", "MHz") {
            current.p1_mhz = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_mhz_mw(trimmed, "E-Cluster HW active frequency", "MHz") {
            current.e_cluster_mhz = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_mhz_mw(trimmed, "CPU Power", "mW") {
            current.cpu_mw = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_mhz_mw(trimmed, "GPU Power", "mW") {
            current.gpu_mw = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_mhz_mw(trimmed, "Combined Power", "mW") {
            current.combined_mw = Some(v);
            current_has_data = true;
        } else if let Some(v) = parse_core_frequency(trimmed) {
            let max = current.max_core_mhz.unwrap_or(0).max(v);
            current.max_core_mhz = Some(max);
            current_has_data = true;
        }
    }
    if current_has_data {
        samples.push(current);
    }
    samples
}

/// The peak-cluster sample: the one with the highest P0-Cluster active
/// frequency (falling back to P1, E-Cluster, then CPU power when P0 is
/// absent in a sample).
pub fn pick_peak_sample(samples: &[PowerSample]) -> Option<&PowerSample> {
    fn rank(s: &PowerSample) -> u64 {
        s.p0_mhz
            .or(s.p1_mhz)
            .or(s.e_cluster_mhz)
            .or(s.cpu_mw)
            .unwrap_or(0)
    }
    samples.iter().max_by_key(|s| rank(s))
}

// ---------------------------------------------------------------------------
// Memory probe helpers
// ---------------------------------------------------------------------------

/// Memory snapshot used by the delta logic (fixture tested, no live alloc).
#[derive(Debug, Clone, Copy)]
pub struct MemSnap {
    pub pages_free: u64,
    pub page_size: u64,
    pub compressor_pages: u64,
    pub swap_used: u64,
}

/// Pure delta logic. The live 4 GiB allocation happens in the smoke, never
/// in unit tests; this function is the fixture-tested contract.
pub fn memory_deltas(before: &MemSnap, during: &MemSnap, after: &MemSnap) -> MemoryProbe {
    MemoryProbe {
        pages_free_delta: during.pages_free as i64 - before.pages_free as i64,
        swap_used_bytes_before: before.swap_used,
        swap_used_bytes_after: after.swap_used,
        compressor_bytes_before: before.compressor_pages.saturating_mul(before.page_size),
        compressor_bytes_after: after.compressor_pages.saturating_mul(after.page_size),
    }
}

fn allocate_touch(total_bytes: u64, page_size: u64) -> Vec<u8> {
    let total = total_bytes as usize;
    let stride = page_size.max(1) as usize;
    let mut buffer = vec![0u8; total];
    let mut i = 0usize;
    while i < total {
        buffer[i] = 1;
        i += stride;
    }
    buffer
}

fn vm_snapshot() -> Result<MemSnap, String> {
    let vm_out = run_capture("vm_stat", &[])?;
    let vm = parsers::parse_vm_stat(&vm_out)?;
    let swap_out = run_capture("sysctl", &["vm.swapusage"])?;
    let (swap_used, _swap_total) = parsers::parse_swapusage(&swap_out)?;
    Ok(MemSnap {
        pages_free: vm.free,
        page_size: vm.page_size,
        compressor_pages: vm.compressor,
        swap_used,
    })
}

// ---------------------------------------------------------------------------
// Probe runners
// ---------------------------------------------------------------------------

fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

fn run_ok(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn find_ffmpeg() -> Option<String> {
    if Path::new(FFMPEG_BREW).exists() {
        Some(FFMPEG_BREW.to_string())
    } else {
        Some("ffmpeg".to_string())
    }
}

/// Prefer Homebrew OpenSSL 3 with `-multi`; fall back to the system
/// openssl without `-multi` and flag the fallback in the result.
fn find_openssl() -> (String, bool) {
    if Path::new(OPENSSL_BREW).exists() {
        (OPENSSL_BREW.to_string(), false)
    } else if Path::new(OPENSSL_SYSTEM).exists() {
        (OPENSSL_SYSTEM.to_string(), true)
    } else {
        ("openssl".to_string(), true)
    }
}

fn ffmpeg_encode_args(duration_secs: &str) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        format!("testsrc2=size=1920x1080:rate=60:duration={duration_secs}"),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "medium".to_string(),
        "-threads".to_string(),
        "12".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]
}

fn probe_cpu_burst(runs: u32) -> Result<CpuBurstProbe, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
    let args = ffmpeg_encode_args("12");
    let mut timings = Vec::new();
    for _ in 0..runs {
        let mut cmd_args: Vec<&str> = vec!["-p", &ffmpeg];
        cmd_args.extend(args.iter().map(|s| s.as_str()));
        let out = run_capture(TIME_BIN, &cmd_args)?;
        let (real, user, sys) = parse_time_p(&out)?;
        timings.push(CpuTiming {
            real: round2(real),
            user: round2(user),
            sys: round2(sys),
        });
    }
    let median = median_timing(&timings);
    Ok(CpuBurstProbe {
        runs: timings,
        median,
    })
}

fn probe_cpu_sustained() -> Result<CpuTiming, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
    let args = ffmpeg_encode_args("60");
    let mut cmd_args: Vec<&str> = vec!["-p", &ffmpeg];
    cmd_args.extend(args.iter().map(|s| s.as_str()));
    let out = run_capture(TIME_BIN, &cmd_args)?;
    let (real, user, sys) = parse_time_p(&out)?;
    Ok(CpuTiming {
        real: round2(real),
        user: round2(user),
        sys: round2(sys),
    })
}

fn openssl_aes_args(fallback: bool) -> Vec<&'static str> {
    if fallback {
        vec!["speed", "-evp", "aes-128-gcm", "-seconds", "5"]
    } else {
        vec![
            "speed",
            "-evp",
            "aes-128-gcm",
            "-seconds",
            "5",
            "-multi",
            "12",
        ]
    }
}

fn openssl_sha_args(fallback: bool) -> Vec<&'static str> {
    if fallback {
        vec!["speed", "-seconds", "5", "sha256"]
    } else {
        vec!["speed", "-seconds", "5", "-multi", "12", "sha256"]
    }
}

fn probe_crypto() -> Result<CryptoProbe, String> {
    let (bin, fallback) = find_openssl();
    let aes_out = run_capture(&bin, &openssl_aes_args(fallback))?;
    let aes_row = parse_openssl_speed(&aes_out, "aes-128-gcm")?;
    let (aes_4kb, aes_peak) = crypto_values(&aes_row)?;
    let sha_out = run_capture(&bin, &openssl_sha_args(fallback))?;
    let sha_row = parse_openssl_speed(&sha_out, "sha256")?;
    let (sha_4kb, sha_peak) = crypto_values(&sha_row)?;
    Ok(CryptoProbe {
        aes_128_gcm_4kb: aes_4kb,
        aes_128_gcm_peak: aes_peak,
        sha256_4kb: sha_4kb,
        sha256_peak: sha_peak,
        fallback,
    })
}

fn probe_memory() -> Result<MemoryProbe, String> {
    let before = vm_snapshot()?;
    let buffer = allocate_touch(MEMORY_ALLOC_BYTES, before.page_size);
    let during = vm_snapshot()?;
    drop(buffer);
    let after = vm_snapshot()?;
    Ok(memory_deltas(&before, &during, &after))
}

/// The power probe runs only when --power is passed and root is available
/// via `sudo -n true`. A failed check skips the probe with a reason.
fn probe_power() -> Result<PowerProbe, String> {
    let root_ok = Command::new("sudo")
        .args(["-n", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !root_ok {
        return Err("powermetrics requires root; sudo -n true failed".to_string());
    }
    let ffmpeg = find_ffmpeg().ok_or_else(|| "ffmpeg not found".to_string())?;
    let pm = Command::new("sudo")
        .args([
            "-n",
            "powermetrics",
            "-n",
            "8",
            "-i",
            "5000",
            "--samplers",
            "cpu_power",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn powermetrics: {e}"))?;
    let _encode_status = Command::new(&ffmpeg)
        .args(ffmpeg_encode_args("20"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("failed to spawn encode: {e}"))?;
    let out = pm
        .wait_with_output()
        .map_err(|e| format!("powermetrics wait failed: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let samples = parse_powermetrics(&text);
    let peak = pick_peak_sample(&samples)
        .ok_or_else(|| "powermetrics produced no parseable samples".to_string())?;
    Ok(PowerProbe {
        captured: true,
        reason: None,
        peak_p0_mhz: peak.p0_mhz,
        peak_p1_mhz: peak.p1_mhz,
        max_core_mhz: peak.max_core_mhz,
        e_cluster_mhz: peak.e_cluster_mhz,
        cpu_mw: peak.cpu_mw,
        gpu_mw: peak.gpu_mw,
        combined_mw: peak.combined_mw,
    })
}

fn probe_machine() -> MachineInfo {
    MachineInfo {
        chip: run_ok("sysctl", &["-n", "machdep.cpu.brand_string"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        cpu_count: run_ok("sysctl", &["-n", "hw.ncpu"])
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0),
        os_version: run_ok("sw_vers", &["-productVersion"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

fn median_timing(timings: &[CpuTiming]) -> CpuTiming {
    let mut real: Vec<f64> = timings.iter().map(|t| t.real).collect();
    let mut user: Vec<f64> = timings.iter().map(|t| t.user).collect();
    let mut sys: Vec<f64> = timings.iter().map(|t| t.sys).collect();
    CpuTiming {
        real: round2(median(&mut real)),
        user: round2(median(&mut user)),
        sys: round2(median(&mut sys)),
    }
}

// ---------------------------------------------------------------------------
// Compare (SPEC section 5)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CompareRow {
    pub probe: String,
    pub unit: &'static str,
    pub decimals: usize,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub delta: Option<f64>,
    pub pct: Option<f64>,
}

fn row(
    probe: &str,
    unit: &'static str,
    decimals: usize,
    before: Option<f64>,
    after: Option<f64>,
) -> CompareRow {
    let delta = match (before, after) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };
    let pct = match (before, after) {
        (Some(b), Some(a)) if b != 0.0 => Some((a - b) / b * 100.0),
        _ => None,
    };
    CompareRow {
        probe: probe.to_string(),
        unit,
        decimals,
        before,
        after,
        delta,
        pct,
    }
}

fn power_combined_mw(result: &BenchResult) -> Option<f64> {
    let power = &result.probes.power;
    if power.captured {
        power.combined_mw.map(|v| v as f64)
    } else {
        None
    }
}

/// Canonical per-metric compare rows: cpu_burst median real first, then the
/// remaining metrics in SPEC order. Missing or skipped probes compare as
/// None, which formats as n/a.
pub fn compare_rows(before: &BenchResult, after: &BenchResult) -> Vec<CompareRow> {
    let burst_b = before.probes.cpu_burst.as_ref();
    let burst_a = after.probes.cpu_burst.as_ref();
    let sustained_b = before.probes.cpu_sustained.as_ref();
    let sustained_a = after.probes.cpu_sustained.as_ref();
    let crypto_b = before.probes.crypto.as_ref();
    let crypto_a = after.probes.crypto.as_ref();
    let memory_b = before.probes.memory.as_ref();
    let memory_a = after.probes.memory.as_ref();
    let mut rows = Vec::new();
    rows.push(row(
        "cpu_burst median real",
        "s",
        2,
        burst_b.map(|p| p.median.real),
        burst_a.map(|p| p.median.real),
    ));
    rows.push(row(
        "cpu_burst median user",
        "s",
        2,
        burst_b.map(|p| p.median.user),
        burst_a.map(|p| p.median.user),
    ));
    rows.push(row(
        "cpu_burst median sys",
        "s",
        2,
        burst_b.map(|p| p.median.sys),
        burst_a.map(|p| p.median.sys),
    ));
    rows.push(row(
        "cpu_sustained real",
        "s",
        2,
        sustained_b.map(|t| t.real),
        sustained_a.map(|t| t.real),
    ));
    rows.push(row(
        "cpu_sustained user",
        "s",
        2,
        sustained_b.map(|t| t.user),
        sustained_a.map(|t| t.user),
    ));
    rows.push(row(
        "cpu_sustained sys",
        "s",
        2,
        sustained_b.map(|t| t.sys),
        sustained_a.map(|t| t.sys),
    ));
    rows.push(row(
        "crypto aes_128_gcm_4kb",
        "GB/s",
        1,
        crypto_b.map(|c| c.aes_128_gcm_4kb),
        crypto_a.map(|c| c.aes_128_gcm_4kb),
    ));
    rows.push(row(
        "crypto aes_128_gcm_peak",
        "GB/s",
        1,
        crypto_b.map(|c| c.aes_128_gcm_peak),
        crypto_a.map(|c| c.aes_128_gcm_peak),
    ));
    rows.push(row(
        "crypto sha256_4kb",
        "GB/s",
        1,
        crypto_b.map(|c| c.sha256_4kb),
        crypto_a.map(|c| c.sha256_4kb),
    ));
    rows.push(row(
        "crypto sha256_peak",
        "GB/s",
        1,
        crypto_b.map(|c| c.sha256_peak),
        crypto_a.map(|c| c.sha256_peak),
    ));
    rows.push(row(
        "memory pages_free_delta",
        "pages",
        0,
        memory_b.map(|m| m.pages_free_delta as f64),
        memory_a.map(|m| m.pages_free_delta as f64),
    ));
    rows.push(row(
        "memory swap_used_bytes_after",
        "MiB",
        1,
        memory_b.map(|m| m.swap_used_bytes_after as f64 / 1048576.0),
        memory_a.map(|m| m.swap_used_bytes_after as f64 / 1048576.0),
    ));
    rows.push(row(
        "memory compressor_bytes_after",
        "MiB",
        1,
        memory_b.map(|m| m.compressor_bytes_after as f64 / 1048576.0),
        memory_a.map(|m| m.compressor_bytes_after as f64 / 1048576.0),
    ));
    rows.push(row(
        "power combined_mw",
        "mW",
        0,
        power_combined_mw(before),
        power_combined_mw(after),
    ));
    rows
}

fn fmt_value(value: Option<f64>, unit: &str, decimals: usize) -> String {
    match value {
        Some(v) => format!("{:.*} {}", decimals, v, unit),
        None => "n/a".to_string(),
    }
}

fn fmt_delta(delta: Option<f64>, unit: &str, decimals: usize) -> String {
    match delta {
        Some(d) => format!("{:+.*} {}", decimals, d, unit),
        None => "n/a".to_string(),
    }
}

fn fmt_pct(pct: Option<f64>) -> String {
    match pct {
        Some(p) => {
            let rounded = (p * 10.0).round() / 10.0;
            format!("{:+.1}%", rounded)
        }
        None => "n/a".to_string(),
    }
}

/// Render the compare table. The caller redacts it before printing.
pub fn format_compare_table(rows: &[CompareRow]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<32}{:>14}{:>14}{:>14}{:>10}\n",
        "probe", "before", "after", "delta", "pct"
    ));
    for r in rows {
        out.push_str(&format!(
            "{:<32}{:>14}{:>14}{:>14}{:>10}\n",
            r.probe,
            fmt_value(r.before, r.unit, r.decimals),
            fmt_value(r.after, r.unit, r.decimals),
            fmt_delta(r.delta, r.unit, r.decimals),
            fmt_pct(r.pct)
        ));
    }
    out
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("result")
        .to_string()
}

/// Compare document written under the benchmarks directory with --json.
pub fn build_compare_json(
    rows: &[CompareRow],
    before_path: &Path,
    after_path: &Path,
) -> serde_json::Value {
    serde_json::json!({
        "compared_at": Local::now().to_rfc3339(),
        "ref1": stem(before_path),
        "ref2": stem(after_path),
        "rows": rows.iter().map(|r| serde_json::json!({
            "probe": r.probe,
            "before": r.before,
            "after": r.after,
            "delta": r.delta,
            "pct": r.pct,
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn load_bench(path: &Path) -> Result<BenchResult, CleanOsError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| CleanOsError::Io(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|e| CleanOsError::Io(format!("parse {}: {e}", path.display())))
}

/// Diagnostic quick burst run: median real seconds (used by `cleanos doctor`).
pub fn quick_burst_median(runs: u32) -> Result<f64, CleanOsError> {
    let probe = probe_cpu_burst(runs).map_err(CleanOsError::ProbeFatal)?;
    Ok(probe.median.real)
}

/// `cleanos bench [--quick] [--power] [--runs N]`
pub fn cmd_bench(quick: bool, power: bool, runs: Option<u32>) -> Result<(), CleanOsError> {
    let runs_n = match (quick, runs) {
        (_, Some(0)) => return Err(CleanOsError::Usage("--runs must be at least 1".to_string())),
        (_, Some(n)) => n,
        (true, None) => 1,
        (false, None) => 3,
    };
    let started = Instant::now();
    let now = Local::now();
    let collected_at = now.to_rfc3339();
    let machine = probe_machine();

    let mut probes = Probes::default();
    let mut skipped: Vec<SkippedProbe> = Vec::new();

    if quick {
        match probe_cpu_burst(runs_n) {
            Ok(p) => probes.cpu_burst = Some(p),
            Err(e) => skipped.push(SkippedProbe {
                probe: "cpu_burst".to_string(),
                reason: e,
            }),
        }
        for name in ["cpu_sustained", "crypto", "memory"] {
            skipped.push(SkippedProbe {
                probe: name.to_string(),
                reason: "not included in --quick mode".to_string(),
            });
        }
    } else {
        match probe_cpu_burst(runs_n) {
            Ok(p) => probes.cpu_burst = Some(p),
            Err(e) => skipped.push(SkippedProbe {
                probe: "cpu_burst".to_string(),
                reason: e,
            }),
        }
        match probe_cpu_sustained() {
            Ok(t) => probes.cpu_sustained = Some(t),
            Err(e) => skipped.push(SkippedProbe {
                probe: "cpu_sustained".to_string(),
                reason: e,
            }),
        }
        match probe_crypto() {
            Ok(c) => probes.crypto = Some(c),
            Err(e) => skipped.push(SkippedProbe {
                probe: "crypto".to_string(),
                reason: e,
            }),
        }
        match probe_memory() {
            Ok(m) => probes.memory = Some(m),
            Err(e) => skipped.push(SkippedProbe {
                probe: "memory".to_string(),
                reason: e,
            }),
        }
    }

    if power && quick {
        probes.power = PowerProbe {
            captured: false,
            reason: Some("not included in --quick mode".to_string()),
            ..PowerProbe::default()
        };
    } else if power {
        match probe_power() {
            Ok(p) => probes.power = p,
            Err(e) => {
                probes.power = PowerProbe {
                    captured: false,
                    reason: Some(e.clone()),
                    ..PowerProbe::default()
                };
                skipped.push(SkippedProbe {
                    probe: "power".to_string(),
                    reason: e,
                });
            }
        }
    } else {
        probes.power = PowerProbe {
            captured: false,
            reason: Some("not requested (pass --power)".to_string()),
            ..PowerProbe::default()
        };
    }

    let result = BenchResult {
        collected_at,
        duration_ms: started.elapsed().as_millis() as u64,
        machine,
        probes,
        skipped,
    };
    let path = paths::bench_path_for(&now)?;
    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| CleanOsError::Io(format!("serialize bench result: {e}")))?;
    let redacted = redaction::redact_text(&json);
    fs::write(&path, redacted)
        .map_err(|e| CleanOsError::Io(format!("write {}: {e}", path.display())))?;
    println!("{}", redaction::redact_text(&path.display().to_string()));
    for s in &result.skipped {
        eprintln!(
            "bench: skipped {}: {}",
            s.probe,
            redaction::redact_text(&s.reason)
        );
    }
    Ok(())
}

/// `cleanos bench compare [REF] [--json]`
pub fn cmd_compare(reference: Option<&str>, json_out: bool) -> Result<(), CleanOsError> {
    let (before_path, after_path) = match reference {
        Some(r) => {
            let named = resolve_bench_arg(Some(r))?;
            let newest_other = paths::latest_two_benches(Some(&named))?;
            let other = newest_other
                .into_iter()
                .next()
                .ok_or_else(|| {
                    CleanOsError::Usage(format!(
                        "only one benchmark result found: {r}. Run `cleanos bench` once more to compare."
                    ))
                })?;
            // before is the older result, after is the newer one.
            if named < other {
                (named, other)
            } else {
                (other, named)
            }
        }
        None => {
            let mut newest_first = paths::latest_two_benches(None)?;
            if newest_first.len() < 2 {
                return Err(CleanOsError::Usage(
                    "need at least two stored benchmark results; run `cleanos bench` twice"
                        .to_string(),
                ));
            }
            let newer = newest_first.remove(0);
            let older = newest_first.remove(0);
            (older, newer)
        }
    };

    let before = load_bench(&before_path)?;
    let after = load_bench(&after_path)?;
    let rows = compare_rows(&before, &after);
    let table = format_compare_table(&rows);
    print!("{}", redaction::redact_text(&table));

    if json_out {
        let doc = build_compare_json(&rows, &before_path, &after_path);
        let json = serde_json::to_string_pretty(&doc)
            .map_err(|e| CleanOsError::Io(format!("serialize compare: {e}")))?;
        let path = paths::compare_path_for(&before_path, &after_path)?;
        fs::write(&path, redaction::redact_text(&json))
            .map_err(|e| CleanOsError::Io(format!("write {}: {e}", path.display())))?;
        eprintln!(
            "compare written: {}",
            redaction::redact_text(&path.display().to_string())
        );
    }
    Ok(())
}

fn resolve_bench_arg(arg: Option<&str>) -> Result<PathBuf, CleanOsError> {
    match arg {
        Some(s) => {
            let p = PathBuf::from(s);
            if p.exists() {
                return Ok(p);
            }
            let under = paths::benchmarks_dir()?.join(s);
            if under.exists() {
                return Ok(under);
            }
            let with_json = paths::benchmarks_dir()?.join(format!("{s}.json"));
            if with_json.exists() {
                return Ok(with_json);
            }
            Err(CleanOsError::Usage(format!(
                "benchmark result not found: {s}. Pass a path or basename under the benchmarks directory."
            )))
        }
        None => paths::latest_bench(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
