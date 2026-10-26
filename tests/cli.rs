//! Runs the real binary so `main` is covered by the coverage gate (STORIES.md §0.2 item 3)
//! and so every command's observable output is asserted (mutation gate, §0.2 item 4).
//! Uses only `std` (§0.2 item 11): cargo exposes the binary's path to integration tests.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ocd-cli"))
        .args(args)
        .output()
        .expect("the binary built by cargo test can be spawned")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("the binary prints UTF-8")
}

#[test]
fn no_arguments_defaults_to_help() {
    let out = run(&[]);
    assert!(out.status.success());
    assert!(text(&out.stderr).contains("Defaulting to help"));
    assert!(text(&out.stdout).contains("help was matched"));
}

#[test]
fn every_known_command_is_dispatched_and_echoes_argv() {
    for command in ["build", "format", "help", "lint", "test"] {
        let out = run(&[command]);
        let stdout = text(&out.stdout);
        assert!(out.status.success(), "{command} exited non-zero");
        assert!(
            stdout.contains(&format!("{command} was matched")),
            "{stdout}"
        );
        // `main` echoes argv before dispatch and every command echoes it again.
        let echoes = stdout.matches(&format!("\"{command}\"]")).count();
        assert_eq!(echoes, 2, "{command}: expected two argv echoes in {stdout}");
    }
}

#[test]
fn unknown_command_is_reported_on_stderr() {
    let out = run(&["bogus"]);
    assert!(out.status.success());
    assert!(text(&out.stderr).contains("bogus is not a valid command"));
    assert_eq!(text(&out.stdout).matches("[\"").count(), 1);
}
