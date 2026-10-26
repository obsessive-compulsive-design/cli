//! The one place exit codes are decided (SPEC.md §13.1).

/// No E-code found (`build`, `lint`, `map`, `docs`), or `help`, or a clean `lsp` shutdown.
pub const EXIT_OK: u8 = 0;
/// At least one E-code (`build`, `lint`, `map`, `docs`).
pub const EXIT_ERRORS: u8 = 1;
/// Usage error: unknown command or flag, missing entry file, out-of-range value.
pub const EXIT_USAGE: u8 = 2;
/// Internal failure such as an I/O error; never produced by diagnostics alone.
pub const EXIT_INTERNAL: u8 = 70;
/// Largest count `format` and `test` report; larger counts are capped here.
pub const EXIT_COUNT_CAP: u8 = 255;

/// How a command ended, before it becomes a process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `build`, `lint`, `map`, `docs`: whether any E-code was reported.
    Checked {
        /// True when at least one E-code was found.
        errors: bool,
    },
    /// `format`: the number of files that would change.
    FilesWouldChange(usize),
    /// `test`: the number of failed files.
    FailedFiles(usize),
    /// `help`, and `lsp` after a clean shutdown.
    Clean,
    /// A usage error (§13.1); the help text was printed.
    Usage,
    /// An internal failure; the error was printed.
    Internal,
}

/// Maps an outcome to the process exit code of SPEC.md §13.1.
#[must_use]
pub fn exit_code(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Checked { errors: false } | Outcome::Clean => EXIT_OK,
        Outcome::Checked { errors: true } => EXIT_ERRORS,
        Outcome::FilesWouldChange(count) | Outcome::FailedFiles(count) => {
            u8::try_from(count).unwrap_or(EXIT_COUNT_CAP)
        }
        Outcome::Usage => EXIT_USAGE,
        Outcome::Internal => EXIT_INTERNAL,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        exit_code, Outcome, EXIT_COUNT_CAP, EXIT_ERRORS, EXIT_INTERNAL, EXIT_OK, EXIT_USAGE,
    };

    #[test]
    fn checked_commands_exit_zero_or_one() {
        assert_eq!(exit_code(Outcome::Checked { errors: false }), 0);
        assert_eq!(exit_code(Outcome::Checked { errors: true }), 1);
        assert_eq!((EXIT_OK, EXIT_ERRORS), (0, 1));
    }

    #[test]
    fn counting_commands_exit_with_the_count_capped_at_255() {
        for (count, expected) in [
            (0, 0),
            (1, 1),
            (7, 7),
            (254, 254),
            (255, 255),
            (256, 255),
            (10_000, 255),
        ] {
            assert_eq!(
                exit_code(Outcome::FilesWouldChange(count)),
                expected,
                "format {count}"
            );
            assert_eq!(
                exit_code(Outcome::FailedFiles(count)),
                expected,
                "test {count}"
            );
        }
        assert_eq!(EXIT_COUNT_CAP, 255);
    }

    #[test]
    fn help_and_clean_lsp_shutdown_exit_zero() {
        assert_eq!(exit_code(Outcome::Clean), 0);
    }

    #[test]
    fn usage_errors_and_internal_failures_have_their_own_codes() {
        assert_eq!(exit_code(Outcome::Usage), 2);
        assert_eq!(exit_code(Outcome::Internal), 70);
        assert_eq!((EXIT_USAGE, EXIT_INTERNAL), (2, 70));
    }
}
