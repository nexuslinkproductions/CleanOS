use std::fs;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cleanos"))
}

#[test]
fn bench_help_shows_quick_power_compare_and_no_mutation() {
    let output = bin()
        .args(["bench", "--help"])
        .output()
        .expect("spawn cleanos");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    for want in ["quick", "power", "compare"] {
        assert!(text.contains(want), "bench help missing {want}: {text}");
    }
    for banned in ["kill", "mutate", "execute", "apply", "remediate"] {
        assert!(
            !text.contains(banned),
            "bench help mentions mutation surface '{banned}': {text}"
        );
    }
    let full = String::from_utf8_lossy(&output.stdout);
    assert!(!full.contains('\u{2014}'), "bench help contains an em dash");
}

#[test]
fn bench_compare_help_shows_reference_and_json() {
    let output = bin()
        .args(["bench", "compare", "--help"])
        .output()
        .expect("spawn cleanos");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    assert!(text.contains("json"), "compare help missing json: {text}");
    assert!(
        text.contains("reference"),
        "compare help missing reference: {text}"
    );
}

#[test]
fn unknown_bench_subcommand_exits_2() {
    let output = bin()
        .args(["bench", "bogus"])
        .output()
        .expect("spawn cleanos");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bench_runs_zero_is_usage_error_exit_2() {
    let output = bin()
        .args(["bench", "--runs", "0"])
        .output()
        .expect("spawn cleanos");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bench_compare_without_results_exits_2() {
    let tmp = std::env::temp_dir().join(format!(
        "cleanos-bench-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&tmp);
    let output = bin()
        .env("CLEANOS_DATA_ROOT", &tmp)
        .args(["bench", "compare"])
        .output()
        .expect("spawn cleanos");
    assert_eq!(output.status.code(), Some(2));
    let _ = fs::remove_dir_all(&tmp);
}
