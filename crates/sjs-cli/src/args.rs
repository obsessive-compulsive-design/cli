//! Argument parsing for one invocation: `sjs <command> [positionals] [flags]`.
//!
//! This story parses *syntax* (which flags exist, whether they take a value, `--`);
//! US-006 interprets the values (paths, numbers, enumerations) and their defaults.

use std::collections::BTreeMap;
use std::fmt;

use crate::commands::{Command, Takes};

/// A parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The command.
    pub command: Command,
    /// True when `--help` was given anywhere (or the command was spelled `--help`).
    pub help: bool,
    /// Each flag given, with its value if it takes one.
    pub flags: BTreeMap<&'static str, Option<String>>,
    /// Everything that is not a flag, in order; after `--`, everything.
    pub positionals: Vec<String>,
}

/// Why the arguments could not be parsed (SPEC.md §13.1: exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageError {
    /// No command was given (and `--help` was not).
    MissingCommand,
    /// The first argument is not a command.
    UnknownCommand(String),
    /// A flag the command does not accept (`--target`, a misspelling, a `-x` short flag).
    UnknownFlag(Command, String),
    /// The same flag twice.
    RepeatedFlag(Command, &'static str),
    /// A valued flag at the end of the arguments, or followed by another flag.
    MissingValue(Command, &'static str),
    /// `--flag=` with nothing after the `=`.
    EmptyValue(Command, &'static str),
    /// `--switch=value` for a flag that takes nothing.
    UnexpectedValue(Command, &'static str),
}

impl UsageError {
    /// The command the error belongs to, when one was recognised; its help is printed.
    #[must_use]
    pub fn command(&self) -> Option<Command> {
        match self {
            UsageError::MissingCommand | UsageError::UnknownCommand(_) => None,
            UsageError::UnknownFlag(command, _)
            | UsageError::RepeatedFlag(command, _)
            | UsageError::MissingValue(command, _)
            | UsageError::EmptyValue(command, _)
            | UsageError::UnexpectedValue(command, _) => Some(*command),
        }
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::MissingCommand => f.write_str("missing command"),
            UsageError::UnknownCommand(text) => write!(f, "unknown command {text:?}"),
            UsageError::UnknownFlag(command, flag) => {
                write!(f, "sjs {command} does not accept {flag:?}")
            }
            UsageError::RepeatedFlag(command, flag) => {
                write!(f, "sjs {command}: {flag} given twice")
            }
            UsageError::MissingValue(command, flag) => {
                write!(f, "sjs {command}: {flag} needs a value")
            }
            UsageError::EmptyValue(command, flag) => {
                write!(f, "sjs {command}: {flag}= has an empty value")
            }
            UsageError::UnexpectedValue(command, flag) => {
                write!(f, "sjs {command}: {flag} takes no value")
            }
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments after the program name.
///
/// `sjs --help` is `sjs help` (the command list); `sjs --help <command>` and
/// `sjs <command> --help` set `help`, which prints that command's help.
/// `--` ends flag parsing. A value is `--flag value` or `--flag=value`; a bare value may
/// not start with `--`, so `--out --json` is a missing value, not a directory named
/// `--json`.
///
/// # Errors
///
/// Every way the arguments can be malformed is a [`UsageError`].
pub fn parse(args: &[String]) -> Result<Invocation, UsageError> {
    let mut rest = args.iter();
    let first = rest.next().ok_or(UsageError::MissingCommand)?;
    let (command, help) = if first == "--help" {
        match rest.next() {
            Some(name) => (parse_command(name)?, true),
            None => (Command::Help, false),
        }
    } else {
        (parse_command(first)?, false)
    };
    let mut invocation = Invocation {
        command,
        help,
        flags: BTreeMap::new(),
        positionals: Vec::new(),
    };
    let mut after_dashes = false;
    while let Some(arg) = rest.next() {
        if after_dashes || !arg.starts_with('-') {
            invocation.positionals.push(arg.clone());
        } else if arg == "--" {
            after_dashes = true;
        } else if arg == "--help" {
            invocation.help = true;
        } else {
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (arg.as_str(), None),
            };
            let flag = command
                .flag(name)
                .ok_or_else(|| UsageError::UnknownFlag(command, name.to_owned()))?;
            let value = match (flag.takes, inline) {
                (Takes::Nothing, None) => None,
                (Takes::Nothing, Some(_)) => {
                    return Err(UsageError::UnexpectedValue(command, flag.name))
                }
                (Takes::Value(_), Some("")) => {
                    return Err(UsageError::EmptyValue(command, flag.name))
                }
                (Takes::Value(_), Some(value)) => Some(value.to_owned()),
                (Takes::Value(_), None) => match rest.next() {
                    Some(value) if !value.starts_with("--") => Some(value.clone()),
                    _ => return Err(UsageError::MissingValue(command, flag.name)),
                },
            };
            if invocation.flags.insert(flag.name, value).is_some() {
                return Err(UsageError::RepeatedFlag(command, flag.name));
            }
        }
    }
    Ok(invocation)
}

fn parse_command(text: &str) -> Result<Command, UsageError> {
    text.parse()
        .map_err(|_| UsageError::UnknownCommand(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{parse, Invocation, UsageError};
    use crate::commands::Command;
    use std::collections::BTreeMap;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn ok(list: &[&str]) -> Invocation {
        parse(&args(list)).expect("the arguments parse")
    }

    fn err(list: &[&str]) -> UsageError {
        parse(&args(list)).expect_err(&format!("{list:?} must not parse"))
    }

    fn flags(pairs: &[(&'static str, Option<&str>)]) -> BTreeMap<&'static str, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (*k, v.map(str::to_owned)))
            .collect()
    }

    #[test]
    fn a_bare_command_has_no_flags_and_no_positionals() {
        for command in Command::ALL {
            assert_eq!(
                ok(&[command.name()]),
                Invocation {
                    command,
                    help: false,
                    flags: BTreeMap::new(),
                    positionals: Vec::new(),
                }
            );
        }
    }

    #[test]
    fn missing_and_unknown_commands() {
        assert_eq!(err(&[]), UsageError::MissingCommand);
        assert_eq!(
            err(&["Lint"]),
            UsageError::UnknownCommand("Lint".to_owned())
        );
        assert_eq!(
            err(&["bogus", "--json"]),
            UsageError::UnknownCommand("bogus".to_owned())
        );
        assert_eq!(
            err(&["--json"]),
            UsageError::UnknownCommand("--json".to_owned())
        );
        assert_eq!(
            err(&["--help", "bogus"]),
            UsageError::UnknownCommand("bogus".to_owned())
        );
        assert_eq!(UsageError::MissingCommand.command(), None);
        assert_eq!(UsageError::UnknownCommand("x".to_owned()).command(), None);
    }

    #[test]
    fn help_is_a_synonym_in_all_three_spellings() {
        assert_eq!(
            ok(&["--help"]),
            ok(&["help"]),
            "sjs --help is sjs help: the command list"
        );
        assert_eq!(ok(&["--help"]).command, Command::Help);
        assert!(!ok(&["--help"]).help);
        assert!(
            ok(&["help", "--help"]).help,
            "help for the help command itself"
        );
        let direct = ok(&["lint", "--help"]);
        let leading = ok(&["--help", "lint"]);
        assert_eq!(direct, leading);
        assert_eq!(direct.command, Command::Lint);
        assert!(direct.help);
        assert!(ok(&["lint", "src/index.js", "--json", "--help"]).help);
        assert!(
            !ok(&["help", "lint"]).help,
            "`sjs help lint` is the help command with a positional"
        );
        assert_eq!(ok(&["help", "lint"]).positionals, ["lint"]);
    }

    #[test]
    fn switches_and_valued_flags_in_both_spellings() {
        let invocation = ok(&["build", "src/index.js", "--out", "dist", "--json"]);
        assert_eq!(invocation.command, Command::Build);
        assert_eq!(invocation.positionals, ["src/index.js"]);
        assert_eq!(
            invocation.flags,
            flags(&[("--out", Some("dist")), ("--json", None)])
        );
        assert_eq!(
            ok(&["build", "--out=dist"]).flags,
            flags(&[("--out", Some("dist"))])
        );
        assert_eq!(
            ok(&["build", "--out=a=b"]).flags,
            flags(&[("--out", Some("a=b"))]),
            "only the first = splits"
        );
        assert_eq!(
            ok(&["test", "--coverage-lines", "90", "--reporter=dots"]).flags,
            flags(&[
                ("--coverage-lines", Some("90")),
                ("--reporter", Some("dots"))
            ])
        );
        assert_eq!(
            ok(&["build", "--out", "-dash"]).flags,
            flags(&[("--out", Some("-dash"))]),
            "a value may start with one dash"
        );
    }

    #[test]
    fn flags_and_positionals_may_interleave() {
        let invocation = ok(&["lint", "a.js", "--json", "b.js", "--quiet", "c.js"]);
        assert_eq!(invocation.positionals, ["a.js", "b.js", "c.js"]);
        assert_eq!(
            invocation.flags,
            flags(&[("--json", None), ("--quiet", None)])
        );
    }

    #[test]
    fn double_dash_ends_flag_parsing() {
        let invocation = ok(&["lint", "--json", "--", "--weird.js", "--help", "-", "--"]);
        assert_eq!(invocation.positionals, ["--weird.js", "--help", "-", "--"]);
        assert_eq!(invocation.flags, flags(&[("--json", None)]));
        assert!(!invocation.help);
    }

    #[test]
    fn unknown_flags_including_removed_target_and_short_flags() {
        assert_eq!(
            err(&["lint", "--target", "node"]),
            UsageError::UnknownFlag(Command::Lint, "--target".to_owned())
        );
        assert_eq!(
            err(&["test", "--target=node"]),
            UsageError::UnknownFlag(Command::Test, "--target".to_owned())
        );
        assert_eq!(
            err(&["lint", "--out", "x"]),
            UsageError::UnknownFlag(Command::Lint, "--out".to_owned())
        );
        assert_eq!(
            err(&["lint", "-j"]),
            UsageError::UnknownFlag(Command::Lint, "-j".to_owned())
        );
        assert_eq!(
            err(&["lint", "-"]),
            UsageError::UnknownFlag(Command::Lint, "-".to_owned())
        );
        assert_eq!(
            err(&["lint", "--JSON"]),
            UsageError::UnknownFlag(Command::Lint, "--JSON".to_owned())
        );
        assert_eq!(
            err(&["help", "--json"]),
            UsageError::UnknownFlag(Command::Help, "--json".to_owned())
        );
        assert_eq!(
            err(&["lsp", "--quiet"]),
            UsageError::UnknownFlag(Command::Lsp, "--quiet".to_owned())
        );
        assert_eq!(err(&["lint", "--target"]).command(), Some(Command::Lint));
    }

    #[test]
    fn a_flag_given_twice_is_an_error() {
        assert_eq!(
            err(&["lint", "--json", "--json"]),
            UsageError::RepeatedFlag(Command::Lint, "--json")
        );
        assert_eq!(
            err(&["build", "--out", "a", "--out=b"]),
            UsageError::RepeatedFlag(Command::Build, "--out")
        );
        assert_eq!(
            err(&["build", "--out=a", "--out=a"]),
            UsageError::RepeatedFlag(Command::Build, "--out")
        );
        assert!(ok(&["lint", "--help", "--help"]).help, "--help may repeat");
    }

    #[test]
    fn values_that_are_missing_empty_or_unexpected() {
        assert_eq!(
            err(&["build", "--out"]),
            UsageError::MissingValue(Command::Build, "--out")
        );
        assert_eq!(
            err(&["build", "--out", "--json"]),
            UsageError::MissingValue(Command::Build, "--out")
        );
        assert_eq!(
            err(&["build", "--out", "--"]),
            UsageError::MissingValue(Command::Build, "--out")
        );
        assert_eq!(
            err(&["build", "--out="]),
            UsageError::EmptyValue(Command::Build, "--out")
        );
        assert_eq!(
            err(&["build", "--json=1"]),
            UsageError::UnexpectedValue(Command::Build, "--json")
        );
        assert_eq!(
            err(&["build", "--json="]),
            UsageError::UnexpectedValue(Command::Build, "--json")
        );
        assert_eq!(err(&["build", "--out="]).command(), Some(Command::Build));
    }

    #[test]
    fn every_flag_of_every_command_parses_once_and_errors_twice() {
        for command in Command::ALL {
            for flag in command.flags() {
                let once: Vec<&str> = match flag.takes {
                    crate::commands::Takes::Nothing => vec![command.name(), flag.name],
                    crate::commands::Takes::Value(_) => vec![command.name(), flag.name, "value"],
                };
                let parsed = ok(&once);
                assert!(
                    parsed.flags.contains_key(flag.name),
                    "{command} {}",
                    flag.name
                );
                let mut twice = once.clone();
                twice.extend_from_slice(&once[1..]);
                assert_eq!(err(&twice), UsageError::RepeatedFlag(command, flag.name));
            }
        }
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(UsageError::MissingCommand.to_string(), "missing command");
        assert_eq!(
            UsageError::UnknownCommand("Lint".to_owned()).to_string(),
            "unknown command \"Lint\""
        );
        assert_eq!(
            UsageError::UnknownFlag(Command::Lint, "--target".to_owned()).to_string(),
            "sjs lint does not accept \"--target\""
        );
        assert_eq!(
            UsageError::RepeatedFlag(Command::Lint, "--json").to_string(),
            "sjs lint: --json given twice"
        );
        assert_eq!(
            UsageError::MissingValue(Command::Build, "--out").to_string(),
            "sjs build: --out needs a value"
        );
        assert_eq!(
            UsageError::EmptyValue(Command::Build, "--out").to_string(),
            "sjs build: --out= has an empty value"
        );
        assert_eq!(
            UsageError::UnexpectedValue(Command::Build, "--json").to_string(),
            "sjs build: --json takes no value"
        );
    }
}
