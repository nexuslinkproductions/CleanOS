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

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(real: f64, user: f64, sys: f64) -> CpuTiming {
        CpuTiming { real, user, sys }
    }

    fn snap(pages_free: u64, page_size: u64, compressor_pages: u64, swap_used: u64) -> MemSnap {
        MemSnap {
            pages_free,
            page_size,
            compressor_pages,
            swap_used,
        }
    }

    fn fixture_result_full() -> BenchResult {
        BenchResult {
            collected_at: "2026-08-11T08:00:00+02:00".to_string(),
            duration_ms: 90_000,
            machine: MachineInfo {
                chip: "Apple M2 Pro".to_string(),
                cpu_count: 12,
                os_version: "26.4.1".to_string(),
            },
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

    fn fixture_result_quick() -> BenchResult {
        BenchResult {
            collected_at: "2026-08-11T09:00:00+02:00".to_string(),
            duration_ms: 5_000,
            machine: MachineInfo {
                chip: "Apple M2 Pro".to_string(),
                cpu_count: 12,
                os_version: "26.4.1".to_string(),
            },
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

    // ---- fixture parsers -------------------------------------------------

    #[test]
    fn parse_time_p_parses_real_user_sys() {
        let out = "real 2.93\nuser 22.81\nsys 0.77\n";
        let (real, user, sys) = parse_time_p(out).unwrap();
        assert!((real - 2.93).abs() < 1e-9);
        assert!((user - 22.81).abs() < 1e-9);
        assert!((sys - 0.77).abs() < 1e-9);
    }

    #[test]
    fn parse_time_p_second_fixture_and_leading_noise() {
        let out = "some noise line\nreal 13.97\nuser 114.03\nsys 3.50\n";
        let (real, user, sys) = parse_time_p(out).unwrap();
        assert!((real - 13.97).abs() < 1e-9);
        assert!((user - 114.03).abs() < 1e-9);
        assert!((sys - 3.50).abs() < 1e-9);
    }

    #[test]
    fn parse_time_p_missing_field_errors() {
        assert!(parse_time_p("real 1.00\nuser 2.00\n").is_err());
        assert!(parse_time_p("nothing here").is_err());
    }

    #[test]
    fn openssl_multi_format_row_parses_4kb_and_peak() {
        // Real capture from this machine (openssl 3.6.3, -multi 12, 5 s).
        let out = "\
Got: +H:16:64:256:1024:8192:16384 from 0
Got: +F:25:AES-128-GCM:109670672.00:390395456.00:1159867648.00:3198570496.00:5917581312.00:5967527936.00 from 0
version: 3.6.3
built on: Tue Jun  9 11:46:57 2026 UTC
CPUINFO: OPENSSL_armcap=0x987d
AES-128-GCM    1351685.18k  5130487.30k 15646069.10k 42437400.66k 76078858.38k 80628904.76k
";
        let row = parse_openssl_speed(out, "aes-128-gcm").unwrap();
        assert_eq!(row.sizes, vec![16, 64, 256, 1024, 8192, 16384]);
        assert_eq!(row.values_k.len(), 6);
        let (four_kb, peak) = crypto_values(&row).unwrap();
        assert!((four_kb - 76.08).abs() < 1e-9, "four_kb was {four_kb}");
        assert!((peak - 80.63).abs() < 1e-9, "peak was {peak}");
    }

    #[test]
    fn openssl_multi_format_sha256_row() {
        let out = "\
Got: +H:16:64:256:1024:8192:16384 from 11
Got: +F:6:sha256:96420105.60:280084364.80:844757862.40:1438773452.80:1751180902.40:1798779699.20 from 11
version: 3.6.3
CPUINFO: OPENSSL_armcap=0x987d
sha256         1140053.82k  3369305.32k  9897521.46k 17025106.53k 20755701.76k 21723542.32k
";
        let row = parse_openssl_speed(out, "sha256").unwrap();
        let (four_kb, peak) = crypto_values(&row).unwrap();
        assert!((four_kb - 20.76).abs() < 1e-9, "four_kb was {four_kb}");
        assert!((peak - 21.72).abs() < 1e-9, "peak was {peak}");
    }

    #[test]
    fn openssl_single_format_row_with_type_header() {
        let out = "\
version: 3.6.3
built on: Tue Jun  9 11:46:57 2026 UTC
The 'numbers' are in 1000s of bytes per second processed.
type             16 bytes     64 bytes    256 bytes   1024 bytes   8192 bytes  16384 bytes
AES-128-GCM     148375.31k   554651.35k  1705805.59k  4648599.01k  8385581.98k  8887968.91k
";
        let row = parse_openssl_speed(out, "aes-128-gcm").unwrap();
        assert_eq!(row.sizes, vec![16, 64, 256, 1024, 8192, 16384]);
        let (four_kb, peak) = crypto_values(&row).unwrap();
        assert!((four_kb - 8.39).abs() < 1e-9, "four_kb was {four_kb}");
        assert!((peak - 8.89).abs() < 1e-9, "peak was {peak}");
    }

    #[test]
    fn openssl_libressl_style_row_uses_8192_for_4kb() {
        // LibreSSL-style table without 16384 or 4096 columns: the 4 KB
        // value falls back to the 8192 byte column and the peak is the
        // same column (it is the largest one present).
        let out = "\
type             16 bytes     64 bytes    256 bytes   1024 bytes   8192 bytes
aes-128-gcm     342589.61k  1359539.33k  5440423.22k 21774788.61k 173613874.36k
";
        let row = parse_openssl_speed(out, "aes-128-gcm").unwrap();
        assert_eq!(row.sizes, vec![16, 64, 256, 1024, 8192]);
        let (four_kb, peak) = crypto_values(&row).unwrap();
        assert!((four_kb - 173.61).abs() < 1e-9, "four_kb was {four_kb}");
        assert!((peak - 173.61).abs() < 1e-9, "peak was {peak}");
    }

    #[test]
    fn openssl_row_missing_algo_errors() {
        assert!(parse_openssl_speed("no rows here\n", "sha256").is_err());
    }

    #[test]
    fn powermetrics_spec_block_parses() {
        // Format reference block from the SPEC (real capture 2026-08-11).
        let out = "\
P0-Cluster HW active frequency: 2839 MHz
CPU 4 frequency: 3404 MHz
CPU 7 frequency: 3467 MHz
CPU 2 frequency: 3109 MHz
CPU 0 frequency: 3329 MHz
CPU 1 frequency: 3182 MHz
CPU 3 frequency: 3221 MHz
CPU 5 frequency: 3361 MHz
CPU 6 frequency: 3254 MHz
P1-Cluster HW active frequency: 2846 MHz
E-Cluster HW active frequency: 2186 MHz
CPU Power: 11024 mW
GPU Power: 2024 mW
Combined Power (CPU + GPU + ANE): 13047 mW
";
        let samples = parse_powermetrics(out);
        assert_eq!(samples.len(), 1);
        let s = &samples[0];
        assert_eq!(s.p0_mhz, Some(2839));
        assert_eq!(s.p1_mhz, Some(2846));
        assert_eq!(s.max_core_mhz, Some(3467));
        assert_eq!(s.e_cluster_mhz, Some(2186));
        assert_eq!(s.cpu_mw, Some(11024));
        assert_eq!(s.gpu_mw, Some(2024));
        assert_eq!(s.combined_mw, Some(13047));
        let peak = pick_peak_sample(&samples).unwrap();
        assert_eq!(peak.p0_mhz, Some(2839));
        assert_eq!(peak.combined_mw, Some(13047));
    }

    #[test]
    fn powermetrics_second_block_with_sample_headers() {
        // A second distinct block: two samples with different values,
        // separated by Sample headers and *** lines.
        let out = "\
Sample: 0, Time: 2026/08/11 08:21:33, Duration: 5.000s
P0-Cluster HW active frequency: 2329 MHz
CPU 0 frequency: 3329 MHz
CPU 7 frequency: 3111 MHz
P1-Cluster HW active frequency: 2392 MHz
E-Cluster HW active frequency: 1945 MHz
CPU Power: 9976 mW
GPU Power: 1994 mW
Combined Power (CPU + GPU + ANE): 11970 mW
***
Sample: 1, Time: 2026/08/11 08:21:38, Duration: 5.000s
P0-Cluster HW active frequency: 3091 MHz
CPU 3 frequency: 3555 MHz
CPU 0 frequency: 3422 MHz
CPU 5 frequency: 3501 MHz
P1-Cluster HW active frequency: 3018 MHz
E-Cluster HW active frequency: 2244 MHz
CPU Power: 14220 mW
GPU Power: 2311 mW
Combined Power (CPU + GPU + ANE): 16531 mW
";
        let samples = parse_powermetrics(out);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].p0_mhz, Some(2329));
        assert_eq!(samples[0].max_core_mhz, Some(3329));
        assert_eq!(samples[0].combined_mw, Some(11970));
        assert_eq!(samples[1].p0_mhz, Some(3091));
        assert_eq!(samples[1].max_core_mhz, Some(3555));
        assert_eq!(samples[1].combined_mw, Some(16531));
        let peak = pick_peak_sample(&samples).unwrap();
        assert_eq!(peak.p0_mhz, Some(3091));
        assert_eq!(peak.p1_mhz, Some(3018));
        assert_eq!(peak.max_core_mhz, Some(3555));
        assert_eq!(peak.e_cluster_mhz, Some(2244));
        assert_eq!(peak.cpu_mw, Some(14220));
        assert_eq!(peak.gpu_mw, Some(2311));
        assert_eq!(peak.combined_mw, Some(16531));
    }

    #[test]
    fn powermetrics_empty_output_yields_no_samples() {
        assert!(parse_powermetrics("").is_empty());
        assert!(pick_peak_sample(&[]).is_none());
    }

    #[test]
    fn vm_stat_header_with_16384_page_size_parses() {
        // Real capture from this machine.
        let out = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                              232174.
Pages active:                            229585.
Pages inactive:                          262130.
Pages speculative:                         8786.
Pages wired down:                        129110.
Pages purgeable:                           6562.
Pages occupied by compressor:             23379.
";
        let vm = parsers::parse_vm_stat(out).unwrap();
        assert_eq!(vm.page_size, 16384);
        assert_eq!(vm.free, 232174);
        assert_eq!(vm.compressor, 23379);
    }

    #[test]
    fn swapusage_zero_line_parses() {
        let out = "vm.swapusage: total = 0.00M  used = 0.00M  free = 0.00M  (encrypted)\n";
        let (used, total) = parsers::parse_swapusage(out).unwrap();
        assert_eq!(used, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn swapusage_nonzero_line_parses() {
        let out = "vm.swapusage: total = 2048.00M  used = 512.50M  free = 1535.50M  (encrypted)\n";
        let (used, total) = parsers::parse_swapusage(out).unwrap();
        assert_eq!(used, 537_395_200); // 512.5 MiB
        assert_eq!(total, 2_147_483_648); // 2048 MiB
    }

    // ---- memory delta logic ----------------------------------------------

    #[test]
    fn memory_deltas_clean_machine_leaves_no_swap() {
        // Fixture mirrors the 2026-08-11 baseline: pages free collapse
        // during the 4 GB allocation and swap stays at zero bytes.
        let before = snap(209_612, 16384, 80_000, 0);
        let during = snap(4_621, 16384, 260_000, 0);
        let after = snap(267_678, 16384, 85_000, 0);
        let m = memory_deltas(&before, &during, &after);
        assert_eq!(m.pages_free_delta, -204_991);
        assert_eq!(m.swap_used_bytes_before, 0);
        assert_eq!(m.swap_used_bytes_after, 0);
        assert_eq!(m.compressor_bytes_before, 80_000 * 16384);
        assert_eq!(m.compressor_bytes_after, 85_000 * 16384);
    }

    #[test]
    fn memory_deltas_swap_growth_is_recorded() {
        let before = snap(200_000, 16384, 90_000, 104_857_600); // 100 MiB
        let during = snap(5_000, 16384, 400_000, 104_857_600);
        let after = snap(260_000, 16384, 120_000, 2_684_354_560); // 2.5 GiB
        let m = memory_deltas(&before, &during, &after);
        assert_eq!(m.pages_free_delta, -195_000);
        assert_eq!(m.swap_used_bytes_before, 104_857_600);
        assert_eq!(m.swap_used_bytes_after, 2_684_354_560);
    }

    // ---- median ----------------------------------------------------------

    #[test]
    fn median_odd_and_even() {
        let mut odd = vec![2.93, 2.90, 2.91];
        assert!((median(&mut odd) - 2.91).abs() < 1e-9);
        let mut even = vec![1.0, 2.0, 3.0, 4.0];
        assert!((median(&mut even) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn median_timing_uses_three_runs() {
        let timings = vec![
            timing(2.93, 22.81, 0.77),
            timing(2.90, 22.72, 0.75),
            timing(2.91, 22.71, 0.74),
        ];
        let m = median_timing(&timings);
        assert!((m.real - 2.91).abs() < 1e-9);
        assert!((m.user - 22.72).abs() < 1e-9);
        assert!((m.sys - 0.75).abs() < 1e-9);
    }

    // ---- compare ---------------------------------------------------------

    #[test]
    fn compare_math_delta_and_percent() {
        let mut before = fixture_result_full();
        let mut after = fixture_result_full();
        before.probes.cpu_burst.as_mut().unwrap().median = timing(2.90, 22.0, 0.7);
        after.probes.cpu_burst.as_mut().unwrap().median = timing(3.00, 23.0, 0.8);
        before.probes.crypto.as_mut().unwrap().aes_128_gcm_4kb = 70.0;
        after.probes.crypto.as_mut().unwrap().aes_128_gcm_4kb = 76.08;
        let rows = compare_rows(&before, &after);
        let first = &rows[0];
        assert_eq!(first.probe, "cpu_burst median real");
        assert_eq!(first.before, Some(2.90));
        assert_eq!(first.after, Some(3.00));
        assert!((first.delta.unwrap() - 0.10).abs() < 1e-9);
        assert!((first.pct.unwrap() - 0.1 / 2.9 * 100.0).abs() < 1e-9);
        let crypto = rows
            .iter()
            .find(|r| r.probe == "crypto aes_128_gcm_4kb")
            .unwrap();
        assert!((crypto.delta.unwrap() - 6.08).abs() < 1e-9);
        assert!((crypto.pct.unwrap() - 6.08 / 70.0 * 100.0).abs() < 1e-9);
    }

    #[test]
    fn compare_percent_rounding_and_negative_delta() {
        let mut before = fixture_result_full();
        let mut after = fixture_result_full();
        before.probes.cpu_burst.as_mut().unwrap().median = timing(3.0, 10.0, 1.0);
        after.probes.cpu_burst.as_mut().unwrap().median = timing(4.0, 8.0, 1.0);
        let rows = compare_rows(&before, &after);
        let real = &rows[0];
        assert!((real.pct.unwrap() - 33.3333).abs() < 0.01);
        assert_eq!(fmt_pct(real.pct), "+33.3%");
        let user = &rows[1];
        assert!((user.delta.unwrap() + 2.0).abs() < 1e-9);
        assert!((user.pct.unwrap() + 20.0).abs() < 1e-9);
        assert_eq!(fmt_delta(user.delta, "s", 2), "-2.00 s");
        assert_eq!(fmt_pct(user.pct), "-20.0%");
    }

    #[test]
    fn compare_zero_before_gives_no_percent() {
        let mut before = fixture_result_full();
        let mut after = fixture_result_full();
        before.probes.cpu_burst.as_mut().unwrap().median = timing(0.0, 0.0, 0.0);
        let rows = compare_rows(&before, &after);
        assert!(rows[0].pct.is_none());
        assert_eq!(fmt_pct(rows[0].pct), "n/a");
    }

    #[test]
    fn compare_canonical_ordering_starts_with_cpu_burst_median_real() {
        let rows = compare_rows(&fixture_result_full(), &fixture_result_quick());
        let labels: Vec<&str> = rows.iter().map(|r| r.probe.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "cpu_burst median real",
                "cpu_burst median user",
                "cpu_burst median sys",
                "cpu_sustained real",
                "cpu_sustained user",
                "cpu_sustained sys",
                "crypto aes_128_gcm_4kb",
                "crypto aes_128_gcm_peak",
                "crypto sha256_4kb",
                "crypto sha256_peak",
                "memory pages_free_delta",
                "memory swap_used_bytes_after",
                "memory compressor_bytes_after",
                "power combined_mw",
            ]
        );
    }

    #[test]
    fn compare_missing_and_skipped_probes_are_n_a() {
        let rows = compare_rows(&fixture_result_full(), &fixture_result_quick());
        let crypto = rows
            .iter()
            .find(|r| r.probe == "crypto aes_128_gcm_4kb")
            .unwrap();
        assert_eq!(crypto.before, Some(70.0));
        assert_eq!(crypto.after, None);
        assert_eq!(crypto.delta, None);
        assert_eq!(crypto.pct, None);
        assert_eq!(fmt_value(crypto.after, "GB/s", 1), "n/a");
        assert_eq!(fmt_delta(crypto.delta, "GB/s", 1), "n/a");
        assert_eq!(fmt_pct(crypto.pct), "n/a");
        let power = rows
            .iter()
            .find(|r| r.probe == "power combined_mw")
            .unwrap();
        assert_eq!(power.before, Some(13047.0));
        assert_eq!(power.after, None);
        assert_eq!(fmt_value(power.after, "mW", 0), "n/a");
    }

    #[test]
    fn compare_table_formats_values_and_n_a() {
        let rows = compare_rows(&fixture_result_full(), &fixture_result_quick());
        let table = format_compare_table(&rows);
        assert!(table.contains("cpu_burst median real"));
        assert!(table.contains("2.90 s"));
        assert!(table.contains("n/a"));
        let header_line = table.lines().next().unwrap();
        assert!(header_line.contains("probe"));
        assert!(header_line.contains("pct"));
    }

    #[test]
    fn compare_json_document_shape() {
        let rows = compare_rows(&fixture_result_full(), &fixture_result_quick());
        let doc = build_compare_json(
            &rows,
            Path::new("20260811-080000.json"),
            Path::new("20260811-090000.json"),
        );
        assert_eq!(doc["ref1"], "20260811-080000");
        assert_eq!(doc["ref2"], "20260811-090000");
        assert_eq!(doc["rows"][0]["probe"], "cpu_burst median real");
        assert!(doc["rows"][0]["before"].is_number());
        assert!(doc["rows"][0]["pct"].is_number());
        let crypto = doc["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["probe"] == "crypto aes_128_gcm_4kb")
            .unwrap();
        assert!(crypto["after"].is_null());
    }

    // ---- compare command writes files ------------------------------------

    #[test]
    fn compare_command_writes_json_file() {
        let tmp = std::env::temp_dir().join(format!(
            "cleanos-bench-cmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&tmp);
        std::env::set_var("CLEANOS_DATA_ROOT", &tmp);
        let bdir = paths::benchmarks_dir().expect("benchmarks dir");
        fs::write(
            bdir.join("20260811-080000.json"),
            serde_json::to_string(&fixture_result_full()).unwrap(),
        )
        .unwrap();
        fs::write(
            bdir.join("20260811-090000.json"),
            serde_json::to_string(&fixture_result_quick()).unwrap(),
        )
        .unwrap();
        cmd_compare(None, true).expect("compare runs");
        let cmp_path = bdir.join("compare-20260811-080000-20260811-090000.json");
        assert!(cmp_path.exists(), "compare file missing");
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&cmp_path).unwrap()).unwrap();
        assert_eq!(doc["rows"][0]["probe"], "cpu_burst median real");
        assert!(doc["rows"][0]["before"].is_number());
        assert!(doc["rows"][0]["after"].is_number());
        let _ = fs::remove_dir_all(&tmp);
    }
}
