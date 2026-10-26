//! The `sjs` command line (SPEC.md §13).
//!
//! US-005 ships dispatch: the eight commands are recognised case-sensitively, every
//! command honours `--help`, a malformed invocation is a usage error (exit 2) that prints
//! the command's help, and an I/O failure is an internal failure (exit 70). Commands other
//! than `help` are stubs that exit 70 until their own story lands.

mod args;
mod commands;
mod exit;

use std::ffi::OsString;
use std::io::{self, Write};

pub use args::{parse, Invocation, UsageError};
pub use commands::{command_list, help_text, Command, FlagSpec, Takes, UnknownCommand};
pub use exit::{
    exit_code, Outcome, EXIT_COUNT_CAP, EXIT_ERRORS, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE,
};

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
        Ok(outcome) => exit_code(outcome),
        Err(error) => report_internal(stderr, &error),
    }
}

fn try_run(
    args: Vec<OsString>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<Outcome> {
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
                return Ok(Outcome::Usage);
            }
        }
    }
    if arguments.is_empty() {
        stdout.write_all(command_list().as_bytes())?;
        return Ok(Outcome::Clean);
    }
    let invocation = match parse(&arguments) {
        Ok(invocation) => invocation,
        Err(error) => return report_usage(stderr, &error),
    };
    if invocation.help {
        stdout.write_all(help_text(invocation.command).as_bytes())?;
        return Ok(Outcome::Clean);
    }
    dispatch(&invocation, stdout, stderr)
}

fn dispatch(
    invocation: &Invocation,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<Outcome> {
    match invocation.command {
        Command::Help => run_help(invocation, stdout, stderr),
        Command::Build => not_implemented(stderr, Command::Build, "US-138"),
        Command::Lint => not_implemented(stderr, Command::Lint, "US-137"),
        Command::Format => not_implemented(stderr, Command::Format, "US-049"),
        Command::Test => not_implemented(stderr, Command::Test, "US-148"),
        Command::Map => not_implemented(stderr, Command::Map, "US-150"),
        Command::Docs => not_implemented(stderr, Command::Docs, "US-154"),
        Command::Lsp => not_implemented(stderr, Command::Lsp, "US-162"),
    }
}

/// `sjs help` lists the commands; `sjs help <command>` prints that command's help (§13.8).
fn run_help(
    invocation: &Invocation,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<Outcome> {
    match invocation.positionals.as_slice() {
        [] => {
            stdout.write_all(command_list().as_bytes())?;
            Ok(Outcome::Clean)
        }
        [name] => match name.parse::<Command>() {
            Ok(command) => {
                stdout.write_all(help_text(command).as_bytes())?;
                Ok(Outcome::Clean)
            }
            Err(error) => report_usage(stderr, &UsageError::UnknownCommand(error.text)),
        },
        [_, extra, ..] => report_usage(
            stderr,
            &UsageError::UnknownFlag(Command::Help, extra.clone()),
        ),
    }
}

/// The only stub this plan permits: a command whose story has not landed exits 70.
fn not_implemented(stderr: &mut dyn Write, command: Command, story: &str) -> io::Result<Outcome> {
    writeln!(
        stderr,
        "sjs {command}: not implemented yet (STORIES.md {story})"
    )?;
    Ok(Outcome::Internal)
}

fn report_usage(stderr: &mut dyn Write, error: &UsageError) -> io::Result<Outcome> {
    // One write: a second write after a failed first one is a branch no test can reach.
    let help = error.command().map_or_else(command_list, help_text);
    let text = format!("sjs: {error}\n\n{help}");
    stderr.write_all(text.as_bytes())?;
    Ok(Outcome::Usage)
}

fn report_internal(stderr: &mut dyn Write, error: &io::Error) -> u8 {
    // The report itself may fail (that is how we got here); the exit code still says 70.
    let _ = writeln!(stderr, "sjs: internal failure: {error}");
    EXIT_INTERNAL
}

#[cfg(test)]
mod tests {
    use super::{command_list, help_text, run, Command, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE};
    use std::ffi::OsString;
    use std::io::{self, Write};

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("sjs prints UTF-8")
    }

    /// Runs and returns `(exit code, stdout, stderr)`.
    fn sjs(list: &[&str]) -> (u8, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(args(list), &mut out, &mut err);
        (code, text(&out), text(&err))
    }

    #[test]
    fn no_arguments_lists_the_commands_on_stdout_and_exits_zero() {
        assert_eq!(sjs(&[]), (EXIT_OK, command_list(), String::new()));
    }

    #[test]
    fn help_in_every_spelling_prints_the_same_text_and_exits_zero() {
        assert_eq!(sjs(&["help"]), (EXIT_OK, command_list(), String::new()));
        assert_eq!(sjs(&["--help"]), (EXIT_OK, command_list(), String::new()));
        for command in Command::ALL {
            let expected = (EXIT_OK, help_text(command), String::new());
            assert_eq!(
                sjs(&[command.name(), "--help"]),
                expected,
                "sjs {command} --help"
            );
            assert_eq!(
                sjs(&["help", command.name()]),
                expected,
                "sjs help {command}"
            );
            assert_eq!(
                sjs(&["--help", command.name()]),
                expected,
                "sjs --help {command}"
            );
            assert_eq!(
                sjs(&[command.name(), "--help", "--help"]),
                expected,
                "repeated --help"
            );
        }
        assert_eq!(
            sjs(&["build", "src/index.js", "--out", "x", "--help"]),
            (EXIT_OK, help_text(Command::Build), String::new()),
            "--help wins over everything else that parses"
        );
    }

    #[test]
    fn usage_errors_exit_two_and_print_the_relevant_help_on_stderr() {
        let (code, out, err) = sjs(&["Lint"]);
        assert_eq!((code, out), (EXIT_USAGE, String::new()));
        assert!(
            err.starts_with("sjs: unknown command \"Lint\"\n\n"),
            "{err}"
        );
        assert!(err.ends_with(&command_list()), "{err}");

        let (code, out, err) = sjs(&["lint", "--target", "node"]);
        assert_eq!((code, out), (EXIT_USAGE, String::new()));
        assert!(
            err.starts_with("sjs: sjs lint does not accept \"--target\"\n\n"),
            "{err}"
        );
        assert!(err.ends_with(&help_text(Command::Lint)), "{err}");

        for (list, first_line) in [
            (
                &["build", "--out="][..],
                "sjs: sjs build: --out= has an empty value",
            ),
            (
                &["build", "--out"][..],
                "sjs: sjs build: --out needs a value",
            ),
            (
                &["build", "--json", "--json"][..],
                "sjs: sjs build: --json given twice",
            ),
            (
                &["build", "--json=1"][..],
                "sjs: sjs build: --json takes no value",
            ),
            (&["help", "bogus"][..], "sjs: unknown command \"bogus\""),
            (
                &["help", "lint", "extra"][..],
                "sjs: sjs help does not accept \"extra\"",
            ),
            (
                &["help", "--json"][..],
                "sjs: sjs help does not accept \"--json\"",
            ),
        ] {
            let (code, out, err) = sjs(list);
            assert_eq!((code, out), (EXIT_USAGE, String::new()), "{list:?}");
            assert_eq!(err.lines().next(), Some(first_line), "{list:?}: {err}");
        }
    }

    #[test]
    fn unimplemented_commands_exit_seventy_naming_their_story() {
        for (command, story) in [
            (Command::Build, "US-138"),
            (Command::Lint, "US-137"),
            (Command::Format, "US-049"),
            (Command::Test, "US-148"),
            (Command::Map, "US-150"),
            (Command::Docs, "US-154"),
            (Command::Lsp, "US-162"),
        ] {
            let (code, out, err) = sjs(&[command.name()]);
            assert_eq!(code, EXIT_INTERNAL, "{command}");
            assert_eq!(out, "", "{command}");
            assert_eq!(
                err,
                format!("sjs {command}: not implemented yet (STORIES.md {story})\n")
            );
        }
        let (code, _, _) = sjs(&["lint", "--", "--weird.js"]);
        assert_eq!(
            code, EXIT_INTERNAL,
            "an entry named --weird.js parses; the stub then exits 70"
        );
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
            Err(io::Error::other("stream is closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_failures_on_stdout_are_internal_failures() {
        for list in [
            &[][..],
            &["help"][..],
            &["help", "lint"][..],
            &["lint", "--help"][..],
        ] {
            let mut err = Vec::new();
            assert_eq!(
                run(args(list), &mut Broken, &mut err),
                EXIT_INTERNAL,
                "{list:?}"
            );
            assert!(text(&err).contains("stream is closed"), "{list:?}");
        }
        assert!(
            Broken.flush().is_ok(),
            "flushing a broken writer is harmless"
        );
    }

    #[test]
    fn write_failures_on_stderr_still_exit_seventy() {
        for list in [
            &["lint"][..],
            &["Lint"][..],
            &["lint", "--bogus"][..],
            &["help", "bogus"][..],
            &["help", "a", "b"][..],
        ] {
            let mut out = Vec::new();
            assert_eq!(
                run(args(list), &mut out, &mut Broken),
                EXIT_INTERNAL,
                "{list:?}"
            );
        }
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
