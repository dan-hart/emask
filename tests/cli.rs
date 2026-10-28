// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exercise command dispatch as a user or script sees it, without credentials.

use std::process::{Command, Stdio};

fn emask(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_emask"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn global_config_flag_works_before_housekeeping_commands() {
    let directory = std::env::temp_dir().join(format!("emask-cli-{}", std::process::id()));
    let config = directory.join("config.toml");
    let path = config.to_str().unwrap();
    for args in [
        vec!["--config", path, "providers"],
        vec!["--config", path, "config", "path"],
    ] {
        let output = emask(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!config.exists());
}

#[test]
fn setup_without_terminal_exits_with_actionable_guidance() {
    let output = emask(&["setup", "ddg"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("requires an interactive terminal"),
        "{error}"
    );
    assert!(error.contains("--token-file"));
    assert!(output.stdout.is_empty());
}

#[test]
fn mint_arguments_cannot_be_silently_ignored_by_subcommands() {
    let output = emask(&["--no-copy", "setup"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mint options cannot precede"));
}
