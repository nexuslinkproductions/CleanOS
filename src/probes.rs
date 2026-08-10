//! Read-only probes. Each probe is isolated: failures become `probe_errors`.

use std::process::Command;
use std::time::Instant;

use chrono::Local;

use crate::model::{
    DisplayInfo, LaunchdInfo, MemoryInfo, PowerInfo, ProbeError, ProcessInfo, RunSnapshot,
    SystemInfo, ThermalInfo,
};
use crate::parsers;

fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{program} {} exited {}: {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("non-utf8 output from {program}: {e}"))
}

fn record_err(errors: &mut Vec<ProbeError>, probe: &str, err: String) {
    errors.push(ProbeError {
        probe: probe.to_string(),
        message: err,
    });
}

fn probe_system(errors: &mut Vec<ProbeError>) -> Option<SystemInfo> {
    let mut ok = true;
    let os_version = match run_cmd("sw_vers", &["-productVersion"]) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            record_err(errors, "system", e);
            ok = false;
            String::new()
        }
    };
    let chip = match run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            record_err(errors, "system", e);
            ok = false;
            String::new()
        }
    };
    let cpu_count = match run_cmd("sysctl", &["-n", "hw.ncpu"]) {
        Ok(s) => s.trim().parse().unwrap_or(0),
        Err(e) => {
            record_err(errors, "system", e);
            ok = false;
            0
        }
    };
    let boot_time_epoch = match run_cmd("sysctl", &["-n", "kern.boottime"]) {
        Ok(s) => match parsers::parse_boottime(&s) {
            Ok(v) => v,
            Err(e) => {
                record_err(errors, "system", e);
                ok = false;
                0
            }
        },
        Err(e) => {
            record_err(errors, "system", e);
            ok = false;
            0
        }
    };
    let (loadavg_1, loadavg_5, loadavg_15) = match run_cmd("sysctl", &["-n", "vm.loadavg"]) {
        Ok(s) => match parsers::parse_loadavg(&s) {
            Ok(v) => v,
            Err(e) => {
                record_err(errors, "system", e);
                ok = false;
                (0.0, 0.0, 0.0)
            }
        },
        Err(e) => {
            record_err(errors, "system", e);
            ok = false;
            (0.0, 0.0, 0.0)
        }
    };
    if !ok && os_version.is_empty() && chip.is_empty() {
        return None;
    }
    Some(SystemInfo {
        os_version,
        chip,
        cpu_count,
        boot_time_epoch,
        loadavg_1,
        loadavg_5,
        loadavg_15,
    })
}

fn probe_memory(errors: &mut Vec<ProbeError>) -> Option<MemoryInfo> {
    let vm_out = match run_cmd("vm_stat", &[]) {
        Ok(s) => s,
        Err(e) => {
            record_err(errors, "memory", e);
            return None;
        }
    };
    let total_bytes = match run_cmd("sysctl", &["-n", "hw.memsize"]) {
        Ok(s) => s.trim().parse().unwrap_or(0),
        Err(e) => {
            record_err(errors, "memory", e);
            0
        }
    };
    let vm = match parsers::parse_vm_stat(&vm_out) {
        Ok(v) => v,
        Err(e) => {
            record_err(errors, "memory", e);
            return None;
        }
    };
    let mut mem = parsers::memory_from_vm_stat(&vm, total_bytes);
    match run_cmd("sysctl", &["vm.swapusage"]) {
        Ok(s) => match parsers::parse_swapusage(&s) {
            Ok((used, total)) => {
                mem.swap_used_bytes = used;
                mem.swap_total_bytes = total;
            }
            Err(e) => record_err(errors, "memory", e),
        },
        Err(e) => record_err(errors, "memory", e),
    }
    match run_cmd("sysctl", &["-n", "kern.memorystatus_vm_pressure_level"]) {
        Ok(s) => match s.trim().parse::<u32>() {
            Ok(v) => mem.pressure_level = v,
            Err(e) => record_err(errors, "memory", format!("pressure parse: {e}")),
        },
        Err(e) => record_err(errors, "memory", e),
    }
    Some(mem)
}

fn probe_processes(errors: &mut Vec<ProbeError>) -> Vec<ProcessInfo> {
    match run_cmd("ps", &["-axo", "pid=,ppid=,pcpu=,rss=,etime=,comm=,args="]) {
        Ok(s) => match parsers::parse_ps(&s) {
            Ok(v) => v,
            Err(e) => {
                record_err(errors, "processes", e);
                Vec::new()
            }
        },
        Err(e) => {
            record_err(errors, "processes", e);
            Vec::new()
        }
    }
}

fn probe_launchd(errors: &mut Vec<ProbeError>) -> Option<LaunchdInfo> {
    // Layer 1 managed set: the user-domain `launchctl list` output. The
    // system domain (`launchctl print system`, readable without sudo) was
    // investigated: it reports no per-service pid fields (only `pid/NNNNN`
    // XPC subdomain handles), so it cannot contribute pids cleanly. System
    // daemons are excluded by the classifier's Layer 2 system-path rule
    // instead (see classifier::is_system_domain_path).
    match run_cmd("launchctl", &["list"]) {
        Ok(s) => match parsers::parse_launchctl_list(&s) {
            Ok(managed) => Some(LaunchdInfo { managed }),
            Err(e) => {
                record_err(errors, "launchd", e);
                Some(LaunchdInfo {
                    managed: Default::default(),
                })
            }
        },
        Err(e) => {
            record_err(errors, "launchd", e);
            None
        }
    }
}

fn probe_power(errors: &mut Vec<ProbeError>) -> Option<PowerInfo> {
    match run_cmd("pmset", &["-g", "batt"]) {
        Ok(s) => match parsers::parse_pmset_batt(&s) {
            Ok(v) => Some(v),
            Err(e) => {
                record_err(errors, "power", e);
                None
            }
        },
        Err(e) => {
            record_err(errors, "power", e);
            None
        }
    }
}

fn probe_thermal(errors: &mut Vec<ProbeError>) -> Option<ThermalInfo> {
    match run_cmd("pmset", &["-g", "therm"]) {
        Ok(s) => match parsers::parse_pmset_therm(&s) {
            Ok(v) => Some(v),
            Err(e) => {
                record_err(errors, "thermal", e);
                None
            }
        },
        Err(e) => {
            record_err(errors, "thermal", e);
            None
        }
    }
}

fn probe_display(errors: &mut Vec<ProbeError>) -> Option<DisplayInfo> {
    match run_cmd("system_profiler", &["SPDisplaysDataType"]) {
        Ok(s) => match parsers::parse_displays(&s) {
            Ok(v) => Some(v),
            Err(e) => {
                record_err(errors, "display", e);
                None
            }
        },
        Err(e) => {
            record_err(errors, "display", e);
            None
        }
    }
}

/// Collect a full run snapshot. Probe failures never abort the run.
pub fn collect_run() -> RunSnapshot {
    let started = Instant::now();
    let collected_at = Local::now().to_rfc3339();
    let mut probe_errors = Vec::new();

    let system = probe_system(&mut probe_errors);
    let memory = probe_memory(&mut probe_errors);
    let processes = probe_processes(&mut probe_errors);
    let launchd = probe_launchd(&mut probe_errors);
    let power = probe_power(&mut probe_errors);
    let thermal = probe_thermal(&mut probe_errors);
    let display = probe_display(&mut probe_errors);

    RunSnapshot {
        schema_version: "1".into(),
        collected_at,
        duration_ms: started.elapsed().as_millis() as u64,
        system,
        memory,
        processes,
        launchd,
        power,
        thermal,
        display,
        probe_errors,
    }
}
