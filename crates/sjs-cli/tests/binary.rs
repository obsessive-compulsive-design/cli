//! Runs the real `sjs` binary so `main` is covered (STORIES.md §0.2 item 3) and the exit
//! codes of SPEC.md §13.1 are observed from outside the process.

use sjs_testkit::fs::TempDir;
use sjs_testkit::process::run;

const SJS: &str = env!("CARGO_BIN_EXE_sjs");

#[test]
fn no_arguments_prints_the_command_list_and_exits_zero() {
    let out = run(SJS, &[], None).expect("sjs can be spawned");
    assert_eq!(out.status, Some(0));
    assert_eq!(out.stdout, sjs_cli::command_list());
    assert_eq!(out.stderr, "");
}

#[test]
fn help_for_a_command_is_the_same_in_all_three_spellings() {
    let expected = sjs_cli::help_text(sjs_cli::Command::Lint);
    for args in [
        &["lint", "--help"][..],
        &["help", "lint"][..],
        &["--help", "lint"][..],
    ] {
        let out = run(SJS, args, None).expect("sjs can be spawned");
        assert_eq!(out.status, Some(0), "{args:?}");
        assert_eq!(out.stdout, expected, "{args:?}");
        assert_eq!(out.stderr, "", "{args:?}");
    }
}

#[test]
fn a_usage_error_exits_two_with_help_on_stderr() {
    let out = run(SJS, &["lint", "--target", "node"], None).expect("sjs can be spawned");
    assert_eq!(out.status, Some(i32::from(sjs_cli::EXIT_USAGE)));
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("does not accept \"--target\""),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .ends_with(&sjs_cli::help_text(sjs_cli::Command::Lint)),
        "{}",
        out.stderr
    );
}

#[test]
fn an_unimplemented_command_exits_seventy_once_its_root_exists() {
    let dir = TempDir::new("binary").expect("temp dir");
    dir.write(
        "src/index.js",
        "export const A = 1
",
    )
    .expect("write");
    let out = run(SJS, &["lint"], Some(dir.path())).expect("sjs can be spawned");
    assert_eq!(out.status, Some(i32::from(sjs_cli::EXIT_INTERNAL)));
    assert_eq!(out.stdout, "");
    assert!(out.stderr.contains("not implemented yet"), "{}", out.stderr);
}

#[test]
fn a_missing_default_root_exits_two_naming_the_path() {
    let dir = TempDir::new("binary").expect("temp dir");
    let out = run(SJS, &["lint"], Some(dir.path())).expect("sjs can be spawned");
    assert_eq!(out.status, Some(i32::from(sjs_cli::EXIT_USAGE)));
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("index.js does not exist"),
        "{}",
        out.stderr
    );
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

#[test]
fn the_diff_format_prints_applies_with_patch_to_what_write_writes() {
    let text = "import { b } from './b.js'\nimport { a } from './a.js'\nconst x = a+b;\nclass A {\n  m() {}\n  f = x\n}\n";
    let patched = TempDir::new("binary").expect("temp dir");
    let written = TempDir::new("binary").expect("temp dir");
    for dir in [&patched, &written] {
        dir.write("src/index.js", text).expect("write");
    }
    let diff = run(SJS, &["format"], Some(patched.path())).expect("sjs can be spawned");
    assert_eq!(diff.status, Some(1), "{}", diff.stderr);
    assert_eq!(diff.stderr, "sjs format: 1 file would change\n");
    let patch = sjs_testkit::process::run_with_stdin(
        "patch",
        &["-p0"],
        Some(patched.path()),
        Some(diff.stdout.as_bytes()),
    )
    .expect("patch can be spawned");
    assert!(patch.success(), "{}{}", patch.stdout, patch.stderr);
    let write = run(SJS, &["format", "--write"], Some(written.path())).expect("sjs can be spawned");
    assert_eq!(write.status, Some(0), "{}", write.stderr);
    let read =
        |dir: &TempDir| std::fs::read_to_string(dir.path().join("src/index.js")).expect("read");
    assert_eq!(read(&patched), read(&written));
    assert_ne!(read(&written), text);
    let again = run(SJS, &["format"], Some(written.path())).expect("sjs can be spawned");
    assert_eq!((again.status, again.stdout.as_str()), (Some(0), ""));
}
