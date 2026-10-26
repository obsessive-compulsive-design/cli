//! Runs the real `sjs` binary so `main` is covered (STORIES.md §0.2 item 3).

use sjs_testkit::process::run;

const SJS: &str = env!("CARGO_BIN_EXE_sjs");

#[test]
fn no_arguments_prints_the_command_list_and_exits_zero() {
    let out = run(SJS, &[], None).expect("sjs can be spawned");
    assert_eq!(out.status, Some(0));
    assert_eq!(out.stdout, format!("{}\n", sjs_cli::COMMANDS));
    assert_eq!(out.stderr, "");
}

#[test]
fn an_argument_is_a_usage_error_with_the_list_on_stderr() {
    let out = run(SJS, &["lint"], None).expect("sjs can be spawned");
    assert_eq!(out.status, Some(i32::from(sjs_cli::EXIT_USAGE)));
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains(sjs_cli::COMMANDS), "{}", out.stderr);
}

#[cfg(unix)]
#[test]
fn a_non_utf8_argument_exits_two_not_seventy() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    let output = Command::new(SJS)
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .expect("sjs can be spawned");
    assert_eq!(output.status.code(), Some(i32::from(sjs_cli::EXIT_USAGE)));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not valid UTF-8"));
}
