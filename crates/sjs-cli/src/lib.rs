//! The `sjs` command line (SPEC.md §13).
//!
//! US-002 ships the skeleton: the eight commands are listed, a non-UTF-8 argument is a
//! usage error, and every other invocation is a usage error until US-005 adds dispatch.

use std::ffi::OsString;
use std::io::{self, Write};

/// The eight commands of SPEC.md §13.1, one line, as `sjs` with no arguments prints it.
/// US-156 replaces this with the spec-derived help text.
pub const COMMANDS: &str = "sjs <command>: build  lint  format  test  map  docs  help  lsp";

/// Exit code: no E-code found (SPEC.md §13.1).
pub const EXIT_OK: u8 = 0;
/// Exit code: usage error (unknown command or flag, bad argument), SPEC.md §13.1.
pub const EXIT_USAGE: u8 = 2;
/// Exit code: internal failure such as an I/O error, SPEC.md §13.1.
pub const EXIT_INTERNAL: u8 = 70;

/// Runs `sjs` with the arguments after the program name and returns the exit code.
///
/// Every failure is turned into an exit code here, so `main` has nothing to decide: a
/// write error on `stdout` or `stderr` is an internal failure (70).
///
/// The signature is deliberately monomorphic (`Vec`, `dyn Write`): a generic entry point
/// gets one instantiation per test and the coverage gate then demands every branch in
/// every instantiation.
pub fn run(args: Vec<OsString>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    match try_run(args, stdout, stderr) {
        Ok(code) => code,
        Err(error) => report_internal(stderr, &error),
    }
}

fn try_run(args: Vec<OsString>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> io::Result<u8> {
    let mut arguments = Vec::new();
    for arg in args {
        match arg.into_string() {
            Ok(text) => arguments.push(text),
            Err(bad) => {
                writeln!(
                    stderr,
                    "sjs: argument is not valid UTF-8: {}",
                    bad.to_string_lossy()
                )?;
                return Ok(EXIT_USAGE);
            }
        }
    }
    if arguments.is_empty() {
        writeln!(stdout, "{COMMANDS}")?;
        return Ok(EXIT_OK);
    }
    writeln!(
        stderr,
        "sjs: command dispatch is not implemented yet (STORIES.md US-005)\n{COMMANDS}"
    )?;
    Ok(EXIT_USAGE)
}

fn report_internal(stderr: &mut dyn Write, error: &io::Error) -> u8 {
    // The report itself may fail (that is how we got here); the exit code still says 70.
    let _ = writeln!(stderr, "sjs: internal failure: {error}");
    EXIT_INTERNAL
}

#[cfg(test)]
mod tests {
    use super::{run, COMMANDS, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE};
    use std::ffi::OsString;
    use std::io::{self, Write};

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("sjs prints UTF-8")
    }

    #[test]
    fn no_arguments_lists_the_commands_on_stdout_and_exits_zero() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(args(&[]), &mut out, &mut err), EXIT_OK);
        assert_eq!(text(&out), format!("{COMMANDS}\n"));
        assert_eq!(text(&err), "");
    }

    #[test]
    fn any_argument_is_a_usage_error_until_dispatch_exists() {
        for list in [
            vec!["lint"],
            vec!["help"],
            vec!["--help"],
            vec!["lint", "src/index.js"],
        ] {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            assert_eq!(run(args(&list), &mut out, &mut err), EXIT_USAGE, "{list:?}");
            assert_eq!(text(&out), "", "{list:?}");
            let err = text(&err);
            assert!(err.contains("US-005"), "{err}");
            assert!(err.ends_with(&format!("{COMMANDS}\n")), "{err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_argument_is_a_usage_error_not_an_internal_failure() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![b'l', 0xff, b'x']);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(vec![bad], &mut out, &mut err), EXIT_USAGE);
        assert_eq!(text(&out), "");
        assert!(text(&err).contains("not valid UTF-8"));
    }

    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stdout is closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_failure_is_an_internal_failure() {
        let mut err = Vec::new();
        assert_eq!(run(args(&[]), &mut Broken, &mut err), EXIT_INTERNAL);
        assert!(text(&err).contains("stdout is closed"));
        assert!(
            Broken.flush().is_ok(),
            "flushing a broken writer is harmless"
        );
    }

    #[test]
    fn a_write_failure_on_stderr_itself_still_exits_seventy() {
        let mut out = Vec::new();
        assert_eq!(run(args(&["lint"]), &mut out, &mut Broken), EXIT_INTERNAL);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_failure_while_reporting_a_bad_argument_still_exits_seventy() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0xff]);
        let mut out = Vec::new();
        assert_eq!(run(vec![bad], &mut out, &mut Broken), EXIT_INTERNAL);
        assert!(out.is_empty(), "nothing reaches stdout on a usage error");
    }
}
