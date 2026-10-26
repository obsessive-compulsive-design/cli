//! The validated options of one invocation (SPEC.md §5.3, §13.1): the root, the common
//! flags, and each command's own values checked against the spec's enumerations.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::args::{Invocation, UsageError};
use crate::commands::Command;

/// `--reporter` for `sjs test` (§10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reporter {
    /// TAP 14, the default.
    Tap,
    /// One character per test.
    Dots,
    /// `JUnit` XML.
    Junit,
    /// JSON.
    Json,
}

/// `--format` for `sjs map` (§13.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapFormat {
    /// A self-contained page, the default.
    Html,
    /// The same data as JSON.
    Json,
    /// Graphviz.
    Dot,
    /// One Markdown file per module.
    Md,
}

/// `--format` for `sjs docs` (§13.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsFormat {
    /// A self-contained static site, the default.
    Html,
    /// One Markdown file per module plus an index.
    Md,
}

/// The values specific to one command, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOptions {
    /// `sjs build`.
    Build,
    /// `sjs lint`.
    Lint {
        /// `--signatures`: append the inferred signature of every export.
        signatures: bool,
    },
    /// `sjs format`; `--check` is the default spelled out.
    Format {
        /// `--write`: apply the canonical layout.
        write: bool,
    },
    /// `sjs test`.
    Test {
        /// False under `--no-coverage`.
        coverage: bool,
        /// `--ci`.
        ci: bool,
        /// `--reporter`, default TAP.
        reporter: Reporter,
        /// `--coverage-lines`, a percentage.
        coverage_lines: Option<u8>,
        /// `--coverage-branches`, a percentage.
        coverage_branches: Option<u8>,
    },
    /// `sjs map`.
    Map {
        /// `--format`, default html.
        format: MapFormat,
    },
    /// `sjs docs`.
    Docs {
        /// `--format`, default html.
        format: DocsFormat,
    },
    /// `sjs help`: the command named, if any (validated by dispatch).
    Help,
    /// `sjs lsp`.
    Lsp,
}

/// Everything a command needs to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The command.
    pub command: Command,
    /// Canonical paths of the root(s): the entries given, deduplicated, in the order
    /// first given; or `<cwd>/src/index.js` when none was given.
    pub entries: Vec<PathBuf>,
    /// True when no entry was given and the default root was used.
    pub default_root: bool,
    /// `*.test.js` roots for `sjs test` with the default root: every one beneath
    /// `<cwd>/src`, sorted. With explicit entries this is empty here; US-060 adds the test
    /// files beside each module of the graph once the graph exists.
    pub test_files: Vec<PathBuf>,
    /// `--out`, or the command's default, for the commands that write; `None` otherwise.
    pub out: Option<PathBuf>,
    /// `--json`.
    pub json: bool,
    /// `--quiet`.
    pub quiet: bool,
    /// The command's own values.
    pub specific: CommandOptions,
}

/// Why options could not be built: the programmer's mistake, or the file system's.
#[derive(Debug)]
pub enum OptionsError {
    /// A usage error (exit 2).
    Usage(UsageError),
    /// An I/O failure while resolving a path or walking `src/` (exit 70).
    Io {
        /// The path being examined.
        path: PathBuf,
        /// What went wrong.
        error: io::Error,
    },
}

impl fmt::Display for OptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionsError::Usage(error) => error.fmt(f),
            OptionsError::Io { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for OptionsError {}

impl OptionsError {
    /// The usage error, if that is what this is.
    #[must_use]
    pub fn usage(self) -> Option<UsageError> {
        match self {
            OptionsError::Usage(error) => Some(error),
            OptionsError::Io { .. } => None,
        }
    }
}

impl From<UsageError> for OptionsError {
    fn from(error: UsageError) -> Self {
        OptionsError::Usage(error)
    }
}

impl Options {
    /// Validates an invocation against the file system rooted at `cwd`.
    ///
    /// # Errors
    ///
    /// A [`UsageError`] for a missing root or entry, a value outside its enumeration or
    /// range, or conflicting flags; an I/O error for anything the file system refuses.
    pub fn from_invocation(invocation: &Invocation, cwd: &Path) -> Result<Self, OptionsError> {
        let command = invocation.command;
        let specific = specific(invocation)?;
        let default_root = invocation.positionals.is_empty();
        let entries = if matches!(command, Command::Help | Command::Lsp) {
            Vec::new()
        } else if default_root {
            vec![resolve(&cwd.join("src").join("index.js"), true)?]
        } else {
            let mut seen = BTreeSet::new();
            let mut entries = Vec::new();
            for given in &invocation.positionals {
                let path = resolve(&cwd.join(given), false)?;
                if seen.insert(path.clone()) {
                    entries.push(path);
                }
            }
            entries
        };
        let test_files = if command == Command::Test && default_root {
            discover_tests(&cwd.join("src"))?
        } else {
            Vec::new()
        };
        let out = default_out(command).map(|default| {
            invocation
                .flags
                .get("--out")
                .and_then(Clone::clone)
                .map_or_else(|| cwd.join(default), |given| cwd.join(given))
        });
        Ok(Self {
            command,
            entries,
            default_root,
            test_files,
            out,
            json: invocation.flags.contains_key("--json"),
            quiet: invocation.flags.contains_key("--quiet"),
            specific,
        })
    }
}

/// The default `--out` of a command that writes (§13.1, §10.5), or `None`.
#[must_use]
pub fn default_out(command: Command) -> Option<&'static str> {
    match command {
        Command::Build => Some("dist"),
        Command::Test => Some("coverage"),
        Command::Map => Some("map"),
        Command::Docs => Some("docs"),
        Command::Lint | Command::Format | Command::Help | Command::Lsp => None,
    }
}

/// Canonicalises a root or entry that must be an existing file. A file that exists but
/// cannot be canonicalised keeps its given path (deduplication is then best effort).
fn resolve(path: &Path, is_default_root: bool) -> Result<PathBuf, UsageError> {
    if !path.is_file() {
        return Err(if is_default_root {
            UsageError::MissingRoot(path.to_path_buf())
        } else {
            UsageError::MissingEntry(path.to_path_buf())
        });
    }
    Ok(path.canonicalize().unwrap_or(path.to_path_buf()))
}

/// Every `*.test.js` beneath `dir`, canonical and sorted; nothing is ignored (§5.3 has
/// no ignore rule). Symlinked directories are followed once each. `dir` exists whenever
/// this is called: it is the parent of the default root, which was just resolved.
fn discover_tests(dir: &Path) -> Result<Vec<PathBuf>, OptionsError> {
    let mut found = BTreeSet::new();
    let mut visited = BTreeSet::new();
    walk(dir, &mut found, &mut visited)?;
    Ok(found.into_iter().collect())
}

fn walk(
    dir: &Path,
    found: &mut BTreeSet<PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), OptionsError> {
    let io = |error| OptionsError::Io {
        path: dir.to_path_buf(),
        error,
    };
    let canonical = dir.canonicalize().unwrap_or(dir.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(());
    }
    // An entry that fails to read mid-iteration is skipped: `flatten` drops the error.
    for entry in std::fs::read_dir(dir).map_err(io)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found, visited)?;
        } else if path.is_file() && path.to_string_lossy().ends_with(".test.js") {
            found.insert(path.canonicalize().unwrap_or(path));
        }
    }
    Ok(())
}

fn specific(invocation: &Invocation) -> Result<CommandOptions, UsageError> {
    let command = invocation.command;
    let has = |flag: &str| invocation.flags.contains_key(flag);
    let value = |flag: &str| invocation.flags.get(flag).and_then(|v| v.as_deref());
    Ok(match command {
        Command::Build => CommandOptions::Build,
        Command::Lint => CommandOptions::Lint {
            signatures: has("--signatures"),
        },
        Command::Format => {
            if has("--write") && has("--check") {
                return Err(UsageError::ConflictingFlags(command, "--write", "--check"));
            }
            CommandOptions::Format {
                write: has("--write"),
            }
        }
        Command::Test => CommandOptions::Test {
            coverage: !has("--no-coverage"),
            ci: has("--ci"),
            reporter: match value("--reporter") {
                None | Some("tap") => Reporter::Tap,
                Some("dots") => Reporter::Dots,
                Some("junit") => Reporter::Junit,
                Some("json") => Reporter::Json,
                Some(other) => {
                    return Err(invalid(
                        command,
                        "--reporter",
                        other,
                        "tap, dots, junit or json",
                    ))
                }
            },
            coverage_lines: percentage(command, "--coverage-lines", value("--coverage-lines"))?,
            coverage_branches: percentage(
                command,
                "--coverage-branches",
                value("--coverage-branches"),
            )?,
        },
        Command::Map => CommandOptions::Map {
            format: match value("--format") {
                None | Some("html") => MapFormat::Html,
                Some("json") => MapFormat::Json,
                Some("dot") => MapFormat::Dot,
                Some("md") => MapFormat::Md,
                Some(other) => {
                    return Err(invalid(command, "--format", other, "html, json, dot or md"))
                }
            },
        },
        Command::Docs => CommandOptions::Docs {
            format: match value("--format") {
                None | Some("html") => DocsFormat::Html,
                Some("md") => DocsFormat::Md,
                Some(other) => return Err(invalid(command, "--format", other, "html or md")),
            },
        },
        Command::Help => CommandOptions::Help,
        Command::Lsp => CommandOptions::Lsp,
    })
}

/// A whole number from 0 to 100, as `--coverage-lines=90` (§10.5).
fn percentage(
    command: Command,
    flag: &'static str,
    value: Option<&str>,
) -> Result<Option<u8>, UsageError> {
    let Some(text) = value else {
        return Ok(None);
    };
    match text.parse::<u8>() {
        Ok(n) if n <= 100 => Ok(Some(n)),
        _ => Err(invalid(command, flag, text, "a whole number from 0 to 100")),
    }
}

fn invalid(
    command: Command,
    flag: &'static str,
    given: &str,
    expected: &'static str,
) -> UsageError {
    UsageError::InvalidValue {
        command,
        flag,
        given: given.to_owned(),
        expected,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_out, CommandOptions, DocsFormat, MapFormat, Options, OptionsError, Reporter,
    };
    use crate::args::{parse, UsageError};
    use crate::commands::Command;
    use sjs_testkit::fs::TempDir;
    use std::path::{Path, PathBuf};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn options(list: &[&str], cwd: &Path) -> Result<Options, OptionsError> {
        Options::from_invocation(&parse(&args(list)).expect("parses"), cwd)
    }

    fn ok(list: &[&str], cwd: &Path) -> Options {
        options(list, cwd).expect("the options build")
    }

    fn usage(list: &[&str], cwd: &Path) -> UsageError {
        options(list, cwd)
            .expect_err("a usage error was expected")
            .usage()
            .expect("a usage error, not an I/O error")
    }

    fn project() -> TempDir {
        let dir = TempDir::new("options").expect("temp dir");
        dir.write("src/index.js", "export const A = 1\n")
            .expect("write");
        dir
    }

    fn canonical(path: &Path) -> PathBuf {
        path.canonicalize().expect("exists")
    }

    #[test]
    fn the_default_root_is_src_index_js_under_the_working_directory() {
        let dir = project();
        let opts = ok(&["lint"], dir.path());
        assert_eq!(opts.command, Command::Lint);
        assert!(opts.default_root);
        assert_eq!(opts.entries, [canonical(&dir.path().join("src/index.js"))]);
        assert_eq!(
            opts.test_files,
            Vec::<PathBuf>::new(),
            "lint never adds test roots"
        );
        assert_eq!(opts.out, None, "lint writes nothing");
        assert!(!opts.json && !opts.quiet);
        assert_eq!(opts.specific, CommandOptions::Lint { signatures: false });
    }

    #[test]
    fn a_missing_default_root_is_a_usage_error_naming_the_path() {
        let dir = TempDir::new("options").expect("temp dir");
        let expected = dir.path().join("src").join("index.js");
        assert_eq!(
            usage(&["lint"], dir.path()),
            UsageError::MissingRoot(expected.clone())
        );
        assert_eq!(
            UsageError::MissingRoot(expected.clone()).to_string(),
            format!(
                "no entry given and the default root {} does not exist",
                expected.display()
            )
        );
        dir.mkdir("src/index.js").expect("a directory, not a file");
        assert_eq!(
            usage(&["build"], dir.path()),
            UsageError::MissingRoot(expected)
        );
    }

    #[test]
    fn explicit_entries_are_resolved_deduplicated_and_kept_in_order() {
        let dir = project();
        let b = dir.write("src/b.js", "").expect("write");
        let outside = dir.write("elsewhere/c.js", "").expect("write");
        let opts = ok(
            &[
                "lint",
                "src/b.js",
                "src/index.js",
                "src/b.js",
                "./src/../src/b.js",
                "elsewhere/c.js",
            ],
            dir.path(),
        );
        assert!(!opts.default_root);
        assert_eq!(
            opts.entries,
            [
                canonical(&b),
                canonical(&dir.path().join("src/index.js")),
                canonical(&outside)
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_entry_given_through_a_symlink_counts_once() {
        let dir = project();
        let target = dir.path().join("src/index.js");
        let link = dir.path().join("link.js");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let opts = ok(&["lint", "link.js", "src/index.js"], dir.path());
        assert_eq!(opts.entries, [canonical(&target)]);
    }

    #[test]
    fn a_missing_entry_is_a_usage_error_naming_the_path() {
        let dir = project();
        let expected = dir.path().join("nope.js");
        assert_eq!(
            usage(&["lint", "src/index.js", "nope.js"], dir.path()),
            UsageError::MissingEntry(expected.clone())
        );
        assert_eq!(
            UsageError::MissingEntry(expected.clone()).to_string(),
            format!("entry {} does not exist", expected.display())
        );
        assert_eq!(
            usage(&["lint", "src"], dir.path()),
            UsageError::MissingEntry(dir.path().join("src")),
            "a directory"
        );
        assert_eq!(
            UsageError::MissingEntry(expected).command(),
            Some(Command::Lint).and(None),
            "no command's help fits"
        );
    }

    #[test]
    fn options_errors_display_and_unwrap_to_their_cause() {
        let usage = OptionsError::Usage(UsageError::MissingCommand);
        assert_eq!(usage.to_string(), "missing command");
        assert_eq!(usage.usage(), Some(UsageError::MissingCommand));
        let io = OptionsError::Io {
            path: PathBuf::from("/p/src"),
            error: std::io::Error::other("boom"),
        };
        assert_eq!(io.to_string(), "/p/src: boom");
        assert_eq!(io.usage(), None);
    }

    #[test]
    fn help_and_lsp_resolve_no_root() {
        let dir = TempDir::new("options").expect("temp dir");
        for command in ["help", "lsp"] {
            let opts = ok(&[command], dir.path());
            assert_eq!(opts.entries, Vec::<PathBuf>::new(), "{command}");
            assert_eq!(opts.out, None, "{command}");
        }
        assert_eq!(ok(&["help"], dir.path()).specific, CommandOptions::Help);
        assert_eq!(ok(&["lsp"], dir.path()).specific, CommandOptions::Lsp);
    }

    #[test]
    fn out_defaults_per_command_and_is_resolved_against_the_working_directory() {
        let dir = project();
        assert_eq!(default_out(Command::Build), Some("dist"));
        assert_eq!(default_out(Command::Test), Some("coverage"));
        assert_eq!(default_out(Command::Map), Some("map"));
        assert_eq!(default_out(Command::Docs), Some("docs"));
        for command in [Command::Lint, Command::Format, Command::Help, Command::Lsp] {
            assert_eq!(default_out(command), None, "{command}");
        }
        assert_eq!(
            ok(&["build"], dir.path()).out,
            Some(dir.path().join("dist"))
        );
        assert_eq!(
            ok(&["test"], dir.path()).out,
            Some(dir.path().join("coverage"))
        );
        assert_eq!(ok(&["map"], dir.path()).out, Some(dir.path().join("map")));
        assert_eq!(ok(&["docs"], dir.path()).out, Some(dir.path().join("docs")));
        assert_eq!(
            ok(&["build", "--out", "build/out"], dir.path()).out,
            Some(dir.path().join("build/out"))
        );
        assert_eq!(
            ok(&["build", "--out=/abs/out"], dir.path()).out,
            Some(PathBuf::from("/abs/out"))
        );
    }

    #[test]
    fn json_and_quiet_are_read_where_accepted() {
        let dir = project();
        let opts = ok(&["lint", "--json", "--quiet", "--signatures"], dir.path());
        assert!(opts.json && opts.quiet);
        assert_eq!(opts.specific, CommandOptions::Lint { signatures: true });
        assert_eq!(ok(&["build"], dir.path()).specific, CommandOptions::Build);
    }

    #[test]
    fn format_write_and_check_conflict() {
        let dir = project();
        assert_eq!(
            ok(&["format"], dir.path()).specific,
            CommandOptions::Format { write: false }
        );
        assert_eq!(
            ok(&["format", "--check"], dir.path()).specific,
            CommandOptions::Format { write: false }
        );
        assert_eq!(
            ok(&["format", "--write"], dir.path()).specific,
            CommandOptions::Format { write: true }
        );
        let error = usage(&["format", "--write", "--check"], dir.path());
        assert_eq!(
            error,
            UsageError::ConflictingFlags(Command::Format, "--write", "--check")
        );
        assert_eq!(
            error.to_string(),
            "sjs format: --write and --check cannot be combined"
        );
        assert_eq!(error.command(), Some(Command::Format));
    }

    #[test]
    fn test_values_default_and_validate() {
        let dir = project();
        assert_eq!(
            ok(&["test"], dir.path()).specific,
            CommandOptions::Test {
                coverage: true,
                ci: false,
                reporter: Reporter::Tap,
                coverage_lines: None,
                coverage_branches: None,
            }
        );
        assert_eq!(
            ok(
                &[
                    "test",
                    "--no-coverage",
                    "--ci",
                    "--reporter=junit",
                    "--coverage-lines",
                    "90",
                    "--coverage-branches=0"
                ],
                dir.path()
            )
            .specific,
            CommandOptions::Test {
                coverage: false,
                ci: true,
                reporter: Reporter::Junit,
                coverage_lines: Some(90),
                coverage_branches: Some(0),
            }
        );
        for (name, reporter) in [
            ("tap", Reporter::Tap),
            ("dots", Reporter::Dots),
            ("junit", Reporter::Junit),
            ("json", Reporter::Json),
        ] {
            assert_eq!(
                ok(&["test", "--reporter", name], dir.path()).specific,
                CommandOptions::Test {
                    coverage: true,
                    ci: false,
                    reporter,
                    coverage_lines: None,
                    coverage_branches: None,
                },
                "{name}"
            );
        }
        let error = usage(&["test", "--reporter", "TAP"], dir.path());
        assert_eq!(
            error,
            UsageError::InvalidValue {
                command: Command::Test,
                flag: "--reporter",
                given: "TAP".to_owned(),
                expected: "tap, dots, junit or json",
            }
        );
        assert_eq!(
            error.to_string(),
            "sjs test: --reporter got \"TAP\"; expected tap, dots, junit or json"
        );
        assert_eq!(error.command(), Some(Command::Test));
        assert_eq!(
            ok(&["test", "--coverage-lines", "100"], dir.path()).specific,
            CommandOptions::Test {
                coverage: true,
                ci: false,
                reporter: Reporter::Tap,
                coverage_lines: Some(100),
                coverage_branches: None,
            }
        );
        for flag in ["--coverage-lines", "--coverage-branches"] {
            for bad in ["101", "-1", "1.5", "", "90%", "ninety", "256", "00x"] {
                let error = usage(&["test", flag, bad], dir.path());
                assert!(
                    matches!(&error, UsageError::InvalidValue { flag: got, given, .. } if *got == flag && given == bad),
                    "{flag} {bad:?}: {error}"
                );
            }
        }
    }

    #[test]
    fn map_and_docs_formats_default_and_validate() {
        let dir = project();
        assert_eq!(
            ok(&["map"], dir.path()).specific,
            CommandOptions::Map {
                format: MapFormat::Html
            }
        );
        for (name, format) in [
            ("html", MapFormat::Html),
            ("json", MapFormat::Json),
            ("dot", MapFormat::Dot),
            ("md", MapFormat::Md),
        ] {
            assert_eq!(
                ok(&["map", "--format", name], dir.path()).specific,
                CommandOptions::Map { format },
                "{name}"
            );
        }
        assert_eq!(
            ok(&["docs"], dir.path()).specific,
            CommandOptions::Docs {
                format: DocsFormat::Html
            }
        );
        for (name, format) in [("html", DocsFormat::Html), ("md", DocsFormat::Md)] {
            assert_eq!(
                ok(&["docs", "--format", name], dir.path()).specific,
                CommandOptions::Docs { format },
                "{name}"
            );
        }
        assert!(matches!(
            usage(&["map", "--format", "svg"], dir.path()),
            UsageError::InvalidValue {
                flag: "--format",
                ..
            }
        ));
        assert!(matches!(
            usage(&["docs", "--format", "dot"], dir.path()),
            UsageError::InvalidValue {
                flag: "--format",
                ..
            }
        ));
        assert!(matches!(
            usage(&["docs", "--format", "HTML"], dir.path()),
            UsageError::InvalidValue {
                flag: "--format",
                ..
            }
        ));
    }

    #[test]
    fn test_discovery_finds_every_test_file_beneath_src_sorted_ignoring_nothing() {
        let dir = project();
        let a = dir.write("src/z/a.test.js", "").expect("write");
        let b = dir.write("src/b.test.js", "").expect("write");
        let nm = dir
            .write("src/node_modules/dep/dep.test.js", "")
            .expect("write");
        let hidden = dir.write("src/.hidden/h.test.js", "").expect("write");
        let inner = dir
            .write("src/weird.test.js/inner.test.js", "")
            .expect("write");
        dir.write("src/not-a-test.js", "").expect("write");
        dir.write("src/also.test.jsx", "").expect("write");
        dir.write("src/x.test.js.bak", "").expect("write");
        dir.write("elsewhere/outside.test.js", "").expect("write");
        let opts = ok(&["test"], dir.path());
        let mut expected: Vec<PathBuf> = [a, b, nm, hidden, inner]
            .iter()
            .map(|p| canonical(p))
            .collect();
        expected.sort();
        assert_eq!(opts.test_files, expected);
    }

    #[test]
    fn test_discovery_is_empty_with_explicit_entries_or_without_src() {
        let dir = project();
        dir.write("src/b.test.js", "").expect("write");
        assert_eq!(
            ok(&["test", "src/index.js"], dir.path()).test_files,
            Vec::<PathBuf>::new()
        );
        let bare = TempDir::new("options").expect("temp dir");
        let entry = bare.write("app.js", "").expect("write");
        assert_eq!(
            ok(&["test", entry.to_str().expect("utf-8")], bare.path()).test_files,
            Vec::<PathBuf>::new()
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_discovery_follows_a_symlinked_directory_once() {
        let dir = project();
        let t = dir.write("src/sub/t.test.js", "").expect("write");
        std::os::unix::fs::symlink(dir.path().join("src/sub"), dir.path().join("src/alias"))
            .expect("symlink");
        std::os::unix::fs::symlink(dir.path().join("src"), dir.path().join("src/sub/loop"))
            .expect("symlink");
        let opts = ok(&["test"], dir.path());
        assert_eq!(
            opts.test_files,
            [canonical(&t)],
            "the alias and the loop add nothing new"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_is_an_io_error_not_a_panic() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        let locked = dir.mkdir("src/locked").expect("mkdir");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let result = options(&["test"], dir.path());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
            .expect("chmod back");
        let error = result.expect_err("an I/O error was expected");
        assert!(
            matches!(&error, OptionsError::Io { path, .. } if *path == locked),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(
            text.contains("locked") && text.contains("ermission"),
            "{text}"
        );
    }
}
