//! The eight commands of SPEC.md §13.1, their flags, and the help text for each
//! (SPEC.md §13.8). US-156 adds the defining spec section to the help text.

use std::fmt;
use std::str::FromStr;

/// A command of SPEC.md §13.1. Names are case-sensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Command {
    /// `sjs build`: check and copy byte for byte (§13.2).
    Build,
    /// `sjs lint`: the full analysis without copying (§13.3).
    Lint,
    /// `sjs format`: report or apply the canonical layout (§13.4).
    Format,
    /// `sjs test`: check, then run the tests (§13.5).
    Test,
    /// `sjs map`: the architectural map (§13.6).
    Map,
    /// `sjs docs`: reference documentation (§13.7).
    Docs,
    /// `sjs help`: this text (§13.8).
    Help,
    /// `sjs lsp`: the language server (§13.9).
    Lsp,
}

/// The name is not one of the eight commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCommand {
    /// What was typed.
    pub text: String,
}

impl fmt::Display for UnknownCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown command {:?}", self.text)
    }
}

impl std::error::Error for UnknownCommand {}

/// What a flag takes after its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    /// Nothing: a switch.
    Nothing,
    /// One value, as `--flag value` or `--flag=value`; the word is the placeholder.
    Value(&'static str),
}

/// One flag a command accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagSpec {
    /// The flag with its dashes, e.g. `"--out"`.
    pub name: &'static str,
    /// Whether it takes a value.
    pub takes: Takes,
    /// One line for the help text.
    pub help: &'static str,
}

/// A flag that takes nothing. A macro rather than a `const fn` because a `const fn` is
/// only ever evaluated at compile time here and would be an uncovered function.
macro_rules! switch {
    ($name:literal, $help:literal $(,)?) => {
        FlagSpec {
            name: $name,
            takes: Takes::Nothing,
            help: $help,
        }
    };
}

/// A flag that takes one value, shown with `$placeholder` in the help text.
macro_rules! valued {
    ($name:literal, $placeholder:literal, $help:literal $(,)?) => {
        FlagSpec {
            name: $name,
            takes: Takes::Value($placeholder),
            help: $help,
        }
    };
}

const JSON: FlagSpec = switch!(
    "--json",
    "print machine-readable diagnostics (the report.json document) on stdout",
);
const QUIET: FlagSpec = switch!("--quiet", "print only the summary line");

const BUILD_FLAGS: &[FlagSpec] = &[
    valued!("--out", "<dir>", "output directory (default dist/)"),
    JSON,
    QUIET,
];
const LINT_FLAGS: &[FlagSpec] = &[
    switch!(
        "--signatures",
        "append the inferred signature of every export, one per line",
    ),
    JSON,
    QUIET,
];
const FORMAT_FLAGS: &[FlagSpec] = &[
    switch!("--write", "apply the canonical layout (never S020/S021)"),
    switch!(
        "--check",
        "print the diff and exit with the count of files that would change (the default)",
    ),
    JSON,
    QUIET,
];
const TEST_FLAGS: &[FlagSpec] = &[
    switch!(
        "--no-coverage",
        "run the exact source bytes without instrumentation",
    ),
    switch!(
        "--ci",
        "treat it.only, describe.only and unexplained skips as errors; enforce thresholds",
    ),
    valued!("--reporter", "<name>", "tap (default), dots, junit or json"),
    valued!(
        "--coverage-lines",
        "<N>",
        "per-file line coverage threshold, enforced under --ci",
    ),
    valued!(
        "--coverage-branches",
        "<N>",
        "per-file branch coverage threshold, enforced under --ci",
    ),
    valued!(
        "--out",
        "<dir>",
        "coverage report directory (default coverage/)",
    ),
    JSON,
    QUIET,
];
const MAP_FLAGS: &[FlagSpec] = &[
    valued!("--format", "<fmt>", "html (default), json, dot or md"),
    valued!("--out", "<dir>", "output directory (default map/)"),
    JSON,
    QUIET,
];
const DOCS_FLAGS: &[FlagSpec] = &[
    valued!("--format", "<fmt>", "html (default) or md"),
    valued!("--out", "<dir>", "output directory (default docs/)"),
    JSON,
    QUIET,
];
const NO_FLAGS: &[FlagSpec] = &[];

impl Command {
    /// Every command in the order SPEC.md §13.1 lists them.
    pub const ALL: [Command; 8] = [
        Command::Build,
        Command::Lint,
        Command::Format,
        Command::Test,
        Command::Map,
        Command::Docs,
        Command::Help,
        Command::Lsp,
    ];

    /// The command name as typed.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Command::Build => "build",
            Command::Lint => "lint",
            Command::Format => "format",
            Command::Test => "test",
            Command::Map => "map",
            Command::Docs => "docs",
            Command::Help => "help",
            Command::Lsp => "lsp",
        }
    }

    /// One line for the command list (§13.8).
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Command::Build => "check the program and copy it byte for byte to dist/",
            Command::Lint => "check the program and print every finding",
            Command::Format => "print the canonical-layout diff, or apply it with --write",
            Command::Test => "check the program, then run its tests with coverage",
            Command::Map => "write the architectural map (modules, dependencies, types, calls)",
            Command::Docs => "write reference documentation from doc comments and inferred types",
            Command::Help => "print this list, or a command's flags, defaults and exit codes",
            Command::Lsp => "serve the Language Server Protocol on stdin/stdout",
        }
    }

    /// The SPEC.md section that defines the command.
    #[must_use]
    pub const fn section(self) -> &'static str {
        match self {
            Command::Build => "§13.2",
            Command::Lint => "§13.3",
            Command::Format => "§13.4",
            Command::Test => "§13.5",
            Command::Map => "§13.6",
            Command::Docs => "§13.7",
            Command::Help => "§13.8",
            Command::Lsp => "§13.9",
        }
    }

    /// The flags the command accepts, besides `--help`.
    #[must_use]
    pub const fn flags(self) -> &'static [FlagSpec] {
        match self {
            Command::Build => BUILD_FLAGS,
            Command::Lint => LINT_FLAGS,
            Command::Format => FORMAT_FLAGS,
            Command::Test => TEST_FLAGS,
            Command::Map => MAP_FLAGS,
            Command::Docs => DOCS_FLAGS,
            Command::Help | Command::Lsp => NO_FLAGS,
        }
    }

    /// The flag with this exact name, if the command accepts it.
    #[must_use]
    pub fn flag(self, name: &str) -> Option<&'static FlagSpec> {
        self.flags().iter().find(|flag| flag.name == name)
    }

    /// What the command accepts after its name, for the usage line.
    #[must_use]
    pub const fn positional(self) -> &'static str {
        match self {
            Command::Help => "[command]",
            Command::Lsp => "",
            _ => "[entries…]",
        }
    }

    /// The exit codes the command produces, as `(code, meaning)` rows (§13.1).
    #[must_use]
    pub const fn exit_codes(self) -> &'static [(u8, &'static str)] {
        match self {
            Command::Build | Command::Lint | Command::Map | Command::Docs => &[
                (0, "no E-code found"),
                (1, "at least one E-code"),
                (2, "usage error"),
                (70, "internal failure"),
            ],
            Command::Format => &[
                (0, "no file would change"),
                (1, "1–255: the number of files that would change, capped"),
                (2, "usage error"),
                (70, "internal failure"),
            ],
            Command::Test => &[
                (0, "every test file passed"),
                (1, "1–255: the number of failed files, capped"),
                (2, "usage error"),
                (70, "internal failure"),
            ],
            Command::Help => &[(0, "always"), (2, "unknown command")],
            Command::Lsp => &[
                (0, "clean shutdown"),
                (2, "usage error"),
                (70, "internal failure"),
            ],
        }
    }
}

impl FromStr for Command {
    type Err = UnknownCommand;

    /// Exact, case-sensitive match on the command name.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Command::ALL
            .into_iter()
            .find(|command| command.name() == text)
            .ok_or_else(|| UnknownCommand {
                text: text.to_owned(),
            })
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The list `sjs help` prints: one line per command (§13.8).
#[must_use]
pub fn command_list() -> String {
    let mut lines = vec!["sjs <command> [entries…] [flags]".to_owned(), String::new()];
    lines.extend(Command::ALL.map(|command| {
        format!(
            "  {:<7} {}  ({})",
            command.name(),
            command.summary(),
            command.section()
        )
    }));
    lines.push(String::new());
    lines.push(
        "Run `sjs help <command>` for its flags, defaults and exit codes; every command also accepts --help."
            .to_owned(),
    );
    join_lines(&lines)
}

/// The help text for one command: usage line, flags, exit codes (§13.8).
#[must_use]
pub fn help_text(command: Command) -> String {
    let width = command
        .flags()
        .iter()
        .map(|flag| flag_syntax(flag).len())
        .chain(std::iter::once("--help".len()))
        .max()
        .unwrap_or_default();
    let mut lines = vec![
        usage_line(command),
        String::new(),
        format!("{}  (SPEC.md {})", command.summary(), command.section()),
        String::new(),
        "Flags:".to_owned(),
    ];
    lines.extend(
        command
            .flags()
            .iter()
            .map(|flag| format!("  {:<width$}  {}", flag_syntax(flag), flag.help)),
    );
    lines.push(format!("  {:<width$}  print this text", "--help"));
    lines.push(String::new());
    lines.push("Exit codes:".to_owned());
    lines.extend(
        command
            .exit_codes()
            .iter()
            .map(|(code, meaning)| format!("  {code:<3} {meaning}")),
    );
    join_lines(&lines)
}

/// Lines joined with `\n` and a final `\n`.
fn join_lines(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn usage_line(command: Command) -> String {
    let mut parts = vec![format!("sjs {}", command.name())];
    if !command.positional().is_empty() {
        parts.push(command.positional().to_owned());
    }
    parts.extend(
        command
            .flags()
            .iter()
            .map(|flag| format!("[{}]", flag_syntax(flag))),
    );
    parts.join(" ")
}

fn flag_syntax(flag: &FlagSpec) -> String {
    match flag.takes {
        Takes::Nothing => flag.name.to_owned(),
        Takes::Value(placeholder) => format!("{} {placeholder}", flag.name),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_list, help_text, Command, Takes, UnknownCommand};

    #[test]
    fn the_eight_commands_parse_by_exact_name_only() {
        assert_eq!(Command::ALL.len(), 8);
        for command in Command::ALL {
            assert_eq!(command.name().parse::<Command>(), Ok(command));
            assert_eq!(command.to_string(), command.name());
        }
        for bad in [
            "",
            "Lint",
            "LINT",
            " lint",
            "lint ",
            "lints",
            "--lint",
            "build-all",
        ] {
            assert_eq!(
                bad.parse::<Command>(),
                Err(UnknownCommand {
                    text: bad.to_owned()
                }),
                "{bad:?} must not parse"
            );
        }
        assert_eq!(
            "Lint"
                .parse::<Command>()
                .expect_err("case-sensitive")
                .to_string(),
            "unknown command \"Lint\""
        );
    }

    #[test]
    fn every_command_has_a_distinct_name_summary_and_section() {
        let mut names = std::collections::BTreeSet::new();
        let mut summaries = std::collections::BTreeSet::new();
        for command in Command::ALL {
            assert!(names.insert(command.name()));
            assert!(summaries.insert(command.summary()));
            assert!(command.section().starts_with("§13."), "{command}");
            assert_ne!(command.summary(), "");
        }
        assert_eq!(Command::Build.section(), "§13.2");
        assert_eq!(Command::Lsp.section(), "§13.9");
    }

    #[test]
    fn flag_tables_match_the_spec_command_lines() {
        let names =
            |command: Command| -> Vec<&str> { command.flags().iter().map(|f| f.name).collect() };
        assert_eq!(names(Command::Build), ["--out", "--json", "--quiet"]);
        assert_eq!(names(Command::Lint), ["--signatures", "--json", "--quiet"]);
        assert_eq!(
            names(Command::Format),
            ["--write", "--check", "--json", "--quiet"]
        );
        assert_eq!(
            names(Command::Test),
            [
                "--no-coverage",
                "--ci",
                "--reporter",
                "--coverage-lines",
                "--coverage-branches",
                "--out",
                "--json",
                "--quiet"
            ]
        );
        assert_eq!(
            names(Command::Map),
            ["--format", "--out", "--json", "--quiet"]
        );
        assert_eq!(
            names(Command::Docs),
            ["--format", "--out", "--json", "--quiet"]
        );
        assert_eq!(names(Command::Help), [] as [&str; 0]);
        assert_eq!(names(Command::Lsp), [] as [&str; 0]);
    }

    #[test]
    fn valued_flags_are_exactly_out_reporter_thresholds_and_format() {
        for command in Command::ALL {
            for flag in command.flags() {
                let valued = matches!(flag.takes, Takes::Value(_));
                let expected = matches!(
                    flag.name,
                    "--out"
                        | "--reporter"
                        | "--coverage-lines"
                        | "--coverage-branches"
                        | "--format"
                );
                assert_eq!(valued, expected, "{command} {}", flag.name);
                assert!(!flag.help.is_empty(), "{command} {}", flag.name);
                assert!(flag.name.starts_with("--"), "{command} {}", flag.name);
            }
        }
    }

    #[test]
    fn flag_lookup_is_exact_and_per_command() {
        assert_eq!(Command::Build.flag("--out").map(|f| f.name), Some("--out"));
        assert_eq!(Command::Lint.flag("--out"), None, "lint writes nothing");
        assert_eq!(Command::Build.flag("--OUT"), None);
        assert_eq!(Command::Build.flag("out"), None);
        assert_eq!(Command::Help.flag("--json"), None);
        assert_eq!(Command::Lsp.flag("--json"), None);
        assert_eq!(
            Command::Test.flag("--target"),
            None,
            "removed by SPEC.md §13.1"
        );
    }

    #[test]
    fn positionals_and_exit_codes_follow_the_spec_table() {
        assert_eq!(Command::Help.positional(), "[command]");
        assert_eq!(Command::Lsp.positional(), "");
        for command in [
            Command::Build,
            Command::Lint,
            Command::Format,
            Command::Test,
            Command::Map,
            Command::Docs,
        ] {
            assert_eq!(command.positional(), "[entries…]", "{command}");
        }
        let codes = |command: Command| -> Vec<u8> {
            command.exit_codes().iter().map(|(code, _)| *code).collect()
        };
        for command in [
            Command::Build,
            Command::Lint,
            Command::Map,
            Command::Docs,
            Command::Format,
            Command::Test,
        ] {
            assert_eq!(codes(command), [0, 1, 2, 70], "{command}");
        }
        assert_eq!(codes(Command::Help), [0, 2]);
        assert_eq!(codes(Command::Lsp), [0, 2, 70]);
        assert!(Command::Format.exit_codes()[1].1.starts_with("1–255"));
        assert!(Command::Test.exit_codes()[1].1.starts_with("1–255"));
    }

    #[test]
    fn the_command_list_names_every_command_once_with_its_section() {
        let list = command_list();
        assert_eq!(list, include_str!("../tests/snapshots/help.txt"));
        for command in Command::ALL {
            assert_eq!(
                list.matches(&format!("  {:<7} ", command.name())).count(),
                1,
                "{command}"
            );
            assert!(list.contains(command.section()), "{command}");
        }
    }

    #[test]
    fn help_text_matches_the_snapshot_for_every_command() {
        for (command, snapshot) in [
            (
                Command::Build,
                include_str!("../tests/snapshots/help-build.txt"),
            ),
            (
                Command::Lint,
                include_str!("../tests/snapshots/help-lint.txt"),
            ),
            (
                Command::Format,
                include_str!("../tests/snapshots/help-format.txt"),
            ),
            (
                Command::Test,
                include_str!("../tests/snapshots/help-test.txt"),
            ),
            (
                Command::Map,
                include_str!("../tests/snapshots/help-map.txt"),
            ),
            (
                Command::Docs,
                include_str!("../tests/snapshots/help-docs.txt"),
            ),
            (
                Command::Help,
                include_str!("../tests/snapshots/help-help.txt"),
            ),
            (
                Command::Lsp,
                include_str!("../tests/snapshots/help-lsp.txt"),
            ),
        ] {
            assert_eq!(help_text(command), snapshot, "{command}");
        }
    }

    #[test]
    fn help_text_lists_every_flag_with_its_value_placeholder_and_every_exit_code() {
        for command in Command::ALL {
            let text = help_text(command);
            assert!(
                text.starts_with(&format!("sjs {}", command.name())),
                "{command}"
            );
            for flag in command.flags() {
                let syntax = match flag.takes {
                    Takes::Nothing => flag.name.to_owned(),
                    Takes::Value(placeholder) => format!("{} {placeholder}", flag.name),
                };
                assert!(
                    text.contains(&format!("[{syntax}]")),
                    "{command}: usage lacks {syntax}"
                );
                assert!(
                    text.contains(flag.help),
                    "{command}: flags lack {}",
                    flag.name
                );
            }
            assert!(text.contains("--help"), "{command}");
            for (code, meaning) in command.exit_codes() {
                assert!(
                    text.contains(&format!("  {code:<3} {meaning}\n")),
                    "{command}: {code}"
                );
            }
            assert!(text.contains(command.section()), "{command}");
        }
    }
}
