//! Parsers for macOS command output. Fixture-tested; raw system output is the evidence.

use crate::model::{DisplayInfo, PowerInfo, ProcessInfo, ThermalInfo};

#[derive(Debug, Clone)]
pub struct VmStat {
    pub page_size: u64,
    pub free: u64,
    pub active: u64,
    pub inactive: u64,
    pub speculative: u64,
    pub wired: u64,
    pub compressor: u64,
    pub purgeable: u64,
}

/// Parse `ps -axo pid=,ppid=,pcpu=,rss=,etime=,comm=,args=` output.
pub fn parse_ps(output: &str) -> Result<Vec<ProcessInfo>, String> {
    let mut procs = Vec::new();
    for (idx, line) in output.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().starts_with("pid") {
            continue;
        }
        match parse_ps_line(line) {
            Ok(p) => procs.push(p),
            Err(e) => {
                if e == "missing rss" {
                    continue;
                }
                return Err(format!("ps line {}: {}", idx + 1, e));
            }
        }
    }
    Ok(procs)
}

fn parse_ps_line(line: &str) -> Result<ProcessInfo, String> {
    let mut parts = line.split_whitespace();
    let pid: u32 = parts
        .next()
        .ok_or_else(|| "missing pid".to_string())?
        .parse()
        .map_err(|_| "invalid pid".to_string())?;
    let ppid: u32 = parts
        .next()
        .ok_or_else(|| "missing ppid".to_string())?
        .parse()
        .map_err(|_| "invalid ppid".to_string())?;
    let cpu_pct: f64 = parts
        .next()
        .ok_or_else(|| "missing pcpu".to_string())?
        .parse()
        .map_err(|_| "invalid pcpu".to_string())?;
    let rss_token = parts.next().ok_or_else(|| "missing rss".to_string())?;
    if rss_token.is_empty() || !rss_token.chars().all(|c| c.is_ascii_digit()) {
        return Err("missing rss".to_string());
    }
    let rss_kb: u64 = rss_token.parse().map_err(|_| "invalid rss".to_string())?;
    let etime = parts.next().ok_or_else(|| "missing etime".to_string())?;
    let remaining: Vec<&str> = parts.collect();
    if remaining.is_empty() {
        return Err("missing comm/args".to_string());
    }
    // macOS truncates the comm column to 16 chars, so comm is unreliable
    // as an executable name. The args first token carries the full path:
    // executable is the basename of args[0], command is the full args join.
    // The comm token is only a fallback when args is empty.
    let (comm, args) = remaining
        .split_first()
        .expect("remaining is not empty by the check above");
    let (executable, command) = if args.is_empty() {
        (basename(comm), comm.to_string())
    } else {
        (basename(args[0]), args.join(" "))
    };
    Ok(ProcessInfo {
        pid,
        ppid,
        cpu_pct,
        rss_bytes: rss_kb.saturating_mul(1024),
        elapsed_secs: parse_etime(etime),
        executable,
        command,
    })
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn parse_etime(s: &str) -> u64 {
    let mut days = 0u64;
    let rest = if let Some((d, r)) = s.split_once('-') {
        days = d.parse().unwrap_or(0);
        r
    } else {
        s
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [mm, ss] => (0u64, mm.parse().unwrap_or(0), ss.parse().unwrap_or(0)),
        [hh, mm, ss] => (
            hh.parse().unwrap_or(0),
            mm.parse().unwrap_or(0),
            ss.parse().unwrap_or(0),
        ),
        _ => (0, 0, 0),
    };
    days * 86400 + h * 3600 + m * 60 + sec
}

pub fn parse_launchctl_list(
    output: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut map = std::collections::BTreeMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.to_ascii_lowercase().starts_with("pid") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let pid_tok = parts
            .next()
            .ok_or_else(|| "missing pid column".to_string())?;
        let _status = parts
            .next()
            .ok_or_else(|| "missing status column".to_string())?;
        let label = parts.collect::<Vec<_>>().join(" ");
        if label.is_empty() {
            continue;
        }
        if pid_tok != "-" {
            if let Ok(pid) = pid_tok.parse::<u32>() {
                map.insert(pid.to_string(), label);
            }
        }
    }
    Ok(map)
}

pub fn parse_vm_stat(output: &str) -> Result<VmStat, String> {
    let mut page_size = 4096u64;
    let mut free = 0u64;
    let mut active = 0u64;
    let mut inactive = 0u64;
    let mut speculative = 0u64;
    let mut wired = 0u64;
    let mut compressor = 0u64;
    let mut purgeable = 0u64;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Mach Virtual Memory Statistics:") {
            if let Some(idx) = rest.find("page size of ") {
                let after = &rest[idx + "page size of ".len()..];
                let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num.parse() {
                    page_size = n;
                }
            }
            continue;
        }
        let (key, val) = match line.split_once(':') {
            Some(p) => p,
            None => continue,
        };
        let digits: String = val.chars().filter(|c| c.is_ascii_digit()).collect();
        let n: u64 = digits.parse().unwrap_or(0);
        match key.trim() {
            "Pages free" => free = n,
            "Pages active" => active = n,
            "Pages inactive" => inactive = n,
            "Pages speculative" => speculative = n,
            "Pages wired down" => wired = n,
            "Pages purgeable" => purgeable = n,
            "Pages occupied by compressor" => compressor = n,
            _ => {}
        }
    }
    Ok(VmStat {
        page_size,
        free,
        active,
        inactive,
        speculative,
        wired,
        compressor,
        purgeable,
    })
}

pub fn memory_from_vm_stat(vm: &VmStat, total_bytes: u64) -> crate::model::MemoryInfo {
    let used_pages = vm.active + vm.inactive + vm.speculative + vm.wired + vm.compressor;
    let used_bytes = used_pages.saturating_mul(vm.page_size);
    let free_bytes = vm.free.saturating_mul(vm.page_size);
    crate::model::MemoryInfo {
        total_bytes,
        used_bytes: if total_bytes > 0 {
            used_bytes.min(total_bytes)
        } else {
            used_bytes
        },
        free_bytes,
        swap_used_bytes: 0,
        swap_total_bytes: 0,
        compressor_bytes: vm.compressor.saturating_mul(vm.page_size),
        pressure_level: 0,
        page_size_bytes: vm.page_size,
    }
}

pub fn parse_swapusage(output: &str) -> Result<(u64, u64), String> {
    let mut total = 0u64;
    let mut used = 0u64;
    let lower = output.to_ascii_lowercase();
    if let Some(idx) = lower.find("total =") {
        total = parse_size_token(&output[idx + 7..])?;
    }
    if let Some(idx) = lower.find("used =") {
        used = parse_size_token(&output[idx + 6..])?;
    }
    Ok((used, total))
}

fn parse_size_token(s: &str) -> Result<u64, String> {
    let token = s
        .trim()
        .split_whitespace()
        .next()
        .ok_or_else(|| "missing size token".to_string())?;
    if token.is_empty() {
        return Ok(0);
    }
    let (num, mult) = if let Some(n) = token.strip_suffix('M').or_else(|| token.strip_suffix('m')) {
        (n, 1024u64 * 1024)
    } else if let Some(n) = token.strip_suffix('G').or_else(|| token.strip_suffix('g')) {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = token.strip_suffix('K').or_else(|| token.strip_suffix('k')) {
        (n, 1024u64)
    } else {
        (token, 1u64)
    };
    let f: f64 = num.parse().map_err(|_| format!("invalid size: {token}"))?;
    Ok((f * mult as f64) as u64)
}

pub fn parse_loadavg(output: &str) -> Result<(f64, f64, f64), String> {
    let cleaned = output.replace(['{', '}'], " ");
    let nums: Vec<f64> = cleaned
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    if nums.len() < 3 {
        return Err(format!("loadavg expected 3 numbers, got: {output}"));
    }
    Ok((nums[0], nums[1], nums[2]))
}

pub fn parse_boottime(output: &str) -> Result<i64, String> {
    if let Some(idx) = output.find("sec =") {
        let after = &output[idx + 5..];
        let num: String = after
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return num
            .parse()
            .map_err(|_| format!("invalid boottime: {output}"));
    }
    Err(format!("boottime missing sec=: {output}"))
}

pub fn parse_pmset_batt(output: &str) -> Result<PowerInfo, String> {
    let mut source = "unknown".to_string();
    let mut percentage = None;
    for line in output.lines() {
        let l = line.trim();
        if l.contains("AC Power") {
            source = "AC".to_string();
        } else if l.to_ascii_lowercase().contains("battery power") {
            source = "Battery".to_string();
        }
        if let Some(pct_idx) = l.find('%') {
            let before = &l[..pct_idx];
            let digits: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if let Ok(pct) = digits.parse::<u32>() {
                percentage = Some(pct);
            }
        }
    }
    Ok(PowerInfo { source, percentage })
}

pub fn parse_pmset_therm(output: &str) -> Result<ThermalInfo, String> {
    let raw = output.trim().to_string();
    let lower = raw.to_ascii_lowercase();
    let level = if lower.contains("no thermal warning") {
        "none".to_string()
    } else if let Some(idx) = lower.find("thermal warning level") {
        let after = &raw[idx..];
        after
            .split_whitespace()
            .last()
            .unwrap_or("unknown")
            .to_string()
    } else if raw.is_empty() {
        "unavailable".to_string()
    } else {
        "recorded".to_string()
    };
    Ok(ThermalInfo {
        thermal_pressure_level: Some(level),
        raw_summary: raw,
    })
}

pub fn parse_displays(output: &str) -> Result<DisplayInfo, String> {
    let mut display_count = 0u32;
    let mut primary_summary = String::new();
    let mut current_name = String::new();
    let mut current_res = String::new();
    let mut current_hz = String::new();
    let mut is_main = false;

    for line in output.lines() {
        let trimmed = line.trim_end();
        let indent = line.len() - line.trim_start().len();
        let content = trimmed.trim();
        if content.is_empty() {
            continue;
        }
        if content.ends_with(':') {
            let name = content.trim_end_matches(':').trim();
            let known = [
                "Graphics/Displays",
                "Displays",
                "Chipset Model",
                "Type",
                "Bus",
                "Total Number of Cores",
                "Vendor",
                "Metal Support",
                "Resolution",
                "UI Looks like",
                "Main Display",
                "Mirror",
                "Online",
                "Rotation",
                "Display Type",
                "Automatically Adjust Brightness",
                "Connection Type",
            ];
            if !known.iter().any(|k| name.eq_ignore_ascii_case(k)) && indent >= 4 {
                if !current_name.is_empty() && (!current_res.is_empty() || is_main) {
                    display_count += 1;
                    if is_main || primary_summary.is_empty() {
                        primary_summary = format_display(&current_name, &current_res, &current_hz);
                    }
                }
                current_name = name.to_string();
                current_res.clear();
                current_hz.clear();
                is_main = false;
            }
            continue;
        }
        if let Some(rest) = content.strip_prefix("Resolution:") {
            current_res = rest.trim().to_string();
        } else if let Some(rest) = content.strip_prefix("UI Looks like:") {
            if let Some(at) = rest.find('@') {
                current_hz = rest[at + 1..].trim().to_string();
            }
            if current_res.is_empty() {
                current_res = rest.trim().to_string();
            }
        } else if let Some(rest) = content.strip_prefix("Main Display:") {
            is_main = rest.trim().eq_ignore_ascii_case("Yes");
        }
    }
    if !current_name.is_empty() && (!current_res.is_empty() || is_main) {
        display_count += 1;
        if is_main || primary_summary.is_empty() {
            primary_summary = format_display(&current_name, &current_res, &current_hz);
        }
    }
    if display_count == 0 {
        display_count = output
            .lines()
            .filter(|l| l.trim().starts_with("Resolution:"))
            .count() as u32;
        if primary_summary.is_empty() {
            primary_summary = format!("{display_count} display(s)");
        }
    }
    Ok(DisplayInfo {
        display_count,
        primary_summary,
    })
}

fn format_display(name: &str, res: &str, hz: &str) -> String {
    if hz.is_empty() {
        format!("{name}: {res}")
    } else {
        format!("{name}: {res} @ {hz}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_args_with_spaces() {
        let out = "  100     1   2.5   4096 01:02:03 /usr/bin/python3 /usr/bin/python3 script.py --name hello world\n";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 100);
        assert_eq!(procs[0].executable, "python3");
        assert_eq!(
            procs[0].command,
            "/usr/bin/python3 script.py --name hello world"
        );
        assert_eq!(procs[0].rss_bytes, 4096 * 1024);
    }

    #[test]
    fn ps_truncated_comm_derives_executable_from_args() {
        // Ground truth ps lines: the comm column is truncated to 16 chars
        // by ps itself (distno, coreau, a bare "/System/Library/" token).
        // The executable must come from the args first token instead.
        let out = "\
418     1  72.4 116304 20-15:28:36 /System/Library/ /System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer -daemon
400     1   0.0   2160 20-15:28:37 /usr/sbin/distno /usr/sbin/distnoted daemon
432     1  43.8 106848 20-15:28:36 /usr/sbin/coreau /usr/sbin/coreaudiod
";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 3);
        let ws = procs.iter().find(|p| p.pid == 418).unwrap();
        assert_eq!(ws.executable, "WindowServer");
        assert!(!ws.executable.is_empty());
        assert_ne!(ws.executable, "/System/Library/");
        assert_eq!(
            ws.command,
            "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer -daemon"
        );
        let dn = procs.iter().find(|p| p.pid == 400).unwrap();
        assert_eq!(dn.executable, "distnoted");
        assert_eq!(dn.command, "/usr/sbin/distnoted daemon");
        let ca = procs.iter().find(|p| p.pid == 432).unwrap();
        assert_eq!(ca.executable, "coreaudiod");
        assert_eq!(ca.command, "/usr/sbin/coreaudiod");
    }

    #[test]
    fn ps_args_with_quoted_spaces_keeps_full_command() {
        let out = "812     1   0.0   5000 1-02:03:04 /Applications/Microsoft Teams.app/Contents/PlugIns/TeamsWidgetExtension.appex/Contents/MacOS/TeamsWidgetExtension -AppleLanguages (\"en_us\", \"de-CH\")\n";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 812);
        assert_eq!(procs[0].executable, "TeamsWidgetExtension");
        assert_eq!(
            procs[0].command,
            "Teams.app/Contents/PlugIns/TeamsWidgetExtension.appex/Contents/MacOS/TeamsWidgetExtension -AppleLanguages (\"en_us\", \"de-CH\")"
        );
    }

    #[test]
    fn ps_comm_only_falls_back_to_comm_token() {
        let out = "900     1   0.0    100 00:00:01 somecomm\n";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].executable, "somecomm");
        assert_eq!(procs[0].command, "somecomm");
    }

    #[test]
    fn ps_missing_rss_skipped() {
        let out = "  100     1   2.5   ?? 01:02:03 /bin/zsh /bin/zsh\n  101     1   1.0   1024 00:00:01 /bin/ls /bin/ls\n";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 101);
    }

    #[test]
    fn ps_header_edge_case() {
        let out = "  PID  PPID  %CPU   RSS     ELAPSED COMM ARGS\n  50     1   0.1   2048 00:00:05 /bin/cat /bin/cat\n";
        let procs = parse_ps(out).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].pid, 50);
    }

    #[test]
    fn launchctl_list_parses_managed() {
        let out = "PID\tStatus\tLabel\n-\t0\tcom.apple.SafariHistoryServiceAgent\n95652\t-9\tcom.apple.cloudphotod\n";
        let map = parse_launchctl_list(out).unwrap();
        assert_eq!(map.get("95652").unwrap(), "com.apple.cloudphotod");
    }

    #[test]
    fn vm_stat_page_conversion() {
        let out = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                                10.\nPages active:                            20.\nPages inactive:                          30.\nPages speculative:                         1.\nPages wired down:                        40.\nPages purgeable:                           2.\nPages occupied by compressor:            5.\n";
        let vm = parse_vm_stat(out).unwrap();
        assert_eq!(vm.page_size, 16384);
        let mem = memory_from_vm_stat(&vm, 16_000_000_000);
        assert_eq!(mem.free_bytes, 10 * 16384);
        assert_eq!(mem.compressor_bytes, 5 * 16384);
    }

    #[test]
    fn pmset_batt_and_therm() {
        let batt = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=23527523)\t100%; charged; 0:00 remaining present: true\n";
        let p = parse_pmset_batt(batt).unwrap();
        assert_eq!(p.source, "AC");
        assert_eq!(p.percentage, Some(100));
        let therm = "Note: No thermal warning level has been recorded\n";
        let t = parse_pmset_therm(therm).unwrap();
        assert_eq!(t.thermal_pressure_level.as_deref(), Some("none"));
    }

    #[test]
    fn system_profiler_display_summary() {
        let out = "Graphics/Displays:\n\n    Apple M2 Pro:\n\n      Displays:\n        LC27G7xT:\n          Resolution: 2560 x 1440 (QHD/WQHD - Wide Quad High Definition)\n          UI Looks like: 2560 x 1440 @ 240.00Hz\n          Main Display: Yes\n          Online: Yes\n        Color LCD:\n          Resolution: 3456 x 2234 Retina\n          Online: Yes\n";
        let d = parse_displays(out).unwrap();
        assert!(d.display_count >= 2);
        assert!(d.primary_summary.contains("LC27G7xT") || d.primary_summary.contains("2560"));
    }

    #[test]
    fn swap_and_loadavg_boottime() {
        let (used, total) =
            parse_swapusage("vm.swapusage: total = 9216.00M  used = 8755.69M  free = 460.31M")
                .unwrap();
        assert!(total > used);
        let (a, b, c) = parse_loadavg("{ 7.87 6.78 6.68 }").unwrap();
        assert!((a - 7.87).abs() < 0.01);
        assert!((b - 6.78).abs() < 0.01);
        assert!((c - 6.68).abs() < 0.01);
        let boot = parse_boottime("{ sec = 1784614781, usec = 492467 } Tue Jul 21").unwrap();
        assert_eq!(boot, 1784614781);
    }
}
