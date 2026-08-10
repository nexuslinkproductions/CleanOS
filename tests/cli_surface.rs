use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cleanos"))
}

#[test]
fn unknown_subcommand_exits_2() {
    let output = bin().arg("mutate").output().expect("spawn cleanos");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}").to_lowercase();
    assert!(
        combined.contains("usage")
            || combined.contains("unrecognized")
            || combined.contains("error"),
        "expected usage-style output, got: {combined}"
    );
}

#[test]
fn help_exposes_collect_report_version_only() {
    let output = bin().arg("--help").output().expect("spawn cleanos");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    assert!(text.contains("collect"), "help missing collect: {text}");
    assert!(text.contains("report"), "help missing report: {text}");
    assert!(
        text.contains("version") || text.contains("-v"),
        "help missing version: {text}"
    );
    for banned in ["kill", "mutate", "execute", "apply", "remediate"] {
        assert!(
            !text.contains(banned),
            "help unexpectedly mentions mutation surface '{banned}': {text}"
        );
    }
}

#[test]
fn collect_help_and_report_help() {
    let collect = bin().args(["collect", "--help"]).output().expect("spawn");
    assert!(collect.status.success());
    let collect_text = String::from_utf8_lossy(&collect.stdout).to_lowercase();
    assert!(collect_text.contains("collect"));

    let report = bin().args(["report", "--help"]).output().expect("spawn");
    assert!(report.status.success());
    let report_text = String::from_utf8_lossy(&report.stdout).to_lowercase();
    assert!(report_text.contains("report"));
}
