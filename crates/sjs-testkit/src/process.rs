//! Runs a program and captures what it did, with nothing but `std`.

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::Command;

/// What a finished process left behind. Output is decoded lossily so a test can always
/// print it; a test that cares about exact bytes asserts on them before this layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `None` when the process was killed by a signal.
    pub status: Option<i32>,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

impl Output {
    /// True when the process exited with code 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

/// Runs `program` with `args` in `current_dir` (or the inherited directory) and waits for
/// it to finish.
///
/// # Errors
///
/// Returns the I/O error when the program cannot be spawned or its output cannot be read.
pub fn run(
    program: impl AsRef<OsStr>,
    args: &[&str],
    current_dir: Option<&Path>,
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    let output = command.output()?;
    Ok(Output {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{run, Output};
    use std::path::Path;

    /// Cargo sets `CARGO` while compiling, so the path is baked in at build time.
    const CARGO: &str = env!("CARGO");

    #[test]
    fn captures_stdout_and_exit_code_of_a_real_program() {
        let out = run(CARGO, &["--version"], None).expect("cargo is runnable under cargo test");
        assert!(out.success(), "{out:?}");
        assert!(out.stdout.starts_with("cargo "), "{out:?}");
        assert_eq!(out.stderr, "");
    }

    #[test]
    fn honours_the_working_directory_and_captures_stderr_on_failure() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = run(
            CARGO,
            &["tree", "--offline", "-p", "no-such-crate"],
            Some(dir),
        )
        .expect("cargo is runnable under cargo test");
        assert!(!out.success(), "{out:?}");
        assert!(out.stderr.contains("no-such-crate"), "{out:?}");
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_panic() {
        let err = run("sjs-testkit-no-such-program", &[], None).expect_err("spawn must fail");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn success_is_exactly_exit_code_zero() {
        let mut out = Output {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(out.success());
        out.status = Some(1);
        assert!(!out.success());
        out.status = None;
        assert!(!out.success());
    }
}
