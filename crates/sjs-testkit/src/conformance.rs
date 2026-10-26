//! The conformance fixture format (STORIES.md §0.3): `// expect:` lines inside a fixture
//! say which diagnostics it must produce, at which position, with which replacement.
//!
//! This module knows nothing about the compiler; it parses text, discovers fixtures and
//! compares expectations with findings a runner hands it.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// One `// expect:` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    /// The fixture file (relative to the fixture) the line was found in.
    pub file: String,
    /// The 1-based line of the `// expect:` comment itself.
    pub declared_at: u32,
    /// The code, e.g. `"E103"`.
    pub code: String,
    /// The rule name, when the line gives one.
    pub name: Option<String>,
    /// The expected 1-based line of the finding.
    pub line: u32,
    /// The expected 1-based column of the finding.
    pub column: u32,
    /// `(current, replacement)` when the line gives them.
    pub fix: Option<(String, String)>,
}

/// A finding a runner observed, in the harness's own terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The fixture file (relative to the fixture).
    pub file: String,
    /// The code text.
    pub code: String,
    /// The rule name.
    pub name: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub column: u32,
    /// `(current, replacement)` for a style finding.
    pub fix: Option<(String, String)>,
}

/// A malformed `// expect:` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectError {
    /// The file.
    pub file: String,
    /// The 1-based line of the comment.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for ExpectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for ExpectError {}

const MARKER: &str = "// expect:";

/// Parses every `// expect:` line of `text`. A leading byte-order mark is skipped so a
/// fixture that *is* about the BOM can still declare its expectation.
///
/// # Errors
///
/// The first malformed line.
pub fn parse_expectations(file: &str, text: &str) -> Result<Vec<Expectation>, ExpectError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let declared_at = saturating_u32(index + 1);
        let Some(rest) = raw.trim_start().strip_prefix(MARKER) else {
            continue;
        };
        let error = |message: String| ExpectError {
            file: file.to_owned(),
            line: declared_at,
            message,
        };
        let (head, fix_text) = match rest.find('"') {
            Some(quote) => (&rest[..quote], Some(&rest[quote..])),
            None => (rest, None),
        };
        let mut words = head.split_whitespace();
        let code = words
            .next()
            .ok_or_else(|| error("missing code".to_owned()))?;
        if !is_code(code) {
            return Err(error(format!("{code:?} is not a code like E103 or S001")));
        }
        let mut name = None;
        let mut position = words.next();
        if let Some(word) = position {
            if !word.starts_with('@') {
                name = Some(word.to_owned());
                position = words.next();
            }
        }
        let position = position.ok_or_else(|| error("missing @line:column".to_owned()))?;
        let (line, column) = parse_position(position)
            .ok_or_else(|| error(format!("{position:?} is not @line:column")))?;
        if let Some(extra) = words.next() {
            return Err(error(format!("unexpected {extra:?} after the position")));
        }
        let fix =
            match fix_text {
                None => None,
                Some(text) => Some(parse_fix(text).ok_or_else(|| {
                    error(format!("{text:?} is not \"current\" -> \"replacement\""))
                })?),
            };
        out.push(Expectation {
            file: file.to_owned(),
            declared_at,
            code: code.to_owned(),
            name,
            line,
            column,
            fix,
        });
    }
    Ok(out)
}

fn is_code(word: &str) -> bool {
    word.len() == 4
        && (word.starts_with('E') || word.starts_with('S'))
        && word[1..].bytes().all(|b| b.is_ascii_digit())
}

fn parse_position(word: &str) -> Option<(u32, u32)> {
    let (line, column) = word.strip_prefix('@')?.split_once(':')?;
    let line: u32 = line.parse().ok()?;
    let column: u32 = column.parse().ok()?;
    (line >= 1 && column >= 1).then_some((line, column))
}

/// `"current" -> "replacement"` with `\"`, `\\`, `\n` and `\t` escapes.
fn parse_fix(text: &str) -> Option<(String, String)> {
    let (current, rest) = parse_string(text)?;
    let rest = rest.trim_start().strip_prefix("->")?;
    let (replacement, rest) = parse_string(rest.trim_start())?;
    rest.trim().is_empty().then_some((current, replacement))
}

/// Parses one double-quoted string at the start of `text`; returns it and what follows.
fn parse_string(text: &str) -> Option<(String, &str)> {
    let mut chars = text.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Some((out, &text[index + 2..])),
            '\\' => match chars.next()?.1 {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                _ => return None,
            },
            other => out.push(other),
        }
    }
    None
}

fn saturating_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Checks that every expectation points inside its file (`text` is that file).
#[must_use]
pub fn out_of_range(expectations: &[Expectation], text: &str) -> Vec<String> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let line_count = saturating_u32(text.lines().count().max(1));
    expectations
        .iter()
        .filter(|e| e.line > line_count)
        .map(|e| {
            format!(
                "{}:{}: expect {} @{}:{} is past the end of the file ({line_count} lines)",
                e.file, e.declared_at, e.code, e.line, e.column
            )
        })
        .collect()
}

/// Compares expectations with findings: every expectation must be produced exactly as
/// often as it is declared, and nothing else may be produced. Returns the mismatches.
#[must_use]
pub fn compare(expected: &[Expectation], actual: &[Finding]) -> Vec<String> {
    let mut unmatched: Vec<&Finding> = actual.iter().collect();
    let mut mismatches = Vec::new();
    for expectation in expected {
        match unmatched
            .iter()
            .position(|finding| matches(expectation, finding))
        {
            Some(index) => {
                unmatched.remove(index);
            }
            None => mismatches.push(format!(
                "{}:{}: expected {} not produced",
                expectation.file,
                expectation.declared_at,
                describe_expectation(expectation)
            )),
        }
    }
    for finding in unmatched {
        mismatches.push(format!(
            "{}: unexpected {}",
            finding.file,
            describe_finding(finding)
        ));
    }
    mismatches
}

fn matches(expectation: &Expectation, finding: &Finding) -> bool {
    expectation.file == finding.file
        && expectation.code == finding.code
        && expectation.line == finding.line
        && expectation.column == finding.column
        && expectation
            .name
            .as_ref()
            .is_none_or(|name| *name == finding.name)
        && expectation
            .fix
            .as_ref()
            .is_none_or(|fix| finding.fix.as_ref() == Some(fix))
}

fn describe_expectation(e: &Expectation) -> String {
    let name = e
        .name
        .as_ref()
        .map_or_else(String::new, |name| format!(" {name}"));
    format!(
        "{}{name} @{}:{}{}",
        e.code,
        e.line,
        e.column,
        describe_fix(e.fix.as_ref())
    )
}

fn describe_finding(f: &Finding) -> String {
    format!(
        "{} {} @{}:{}{}",
        f.code,
        f.name,
        f.line,
        f.column,
        describe_fix(f.fix.as_ref())
    )
}

fn describe_fix(fix: Option<&(String, String)>) -> String {
    fix.map_or_else(String::new, |(current, replacement)| {
        format!(" {current:?} -> {replacement:?}")
    })
}

/// One file of a fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureFile {
    /// Path relative to the fixture (`entry.js`, `lib/a.js`, `a.test.js`).
    pub relative: String,
    /// Raw bytes: a fixture may deliberately be invalid UTF-8.
    pub bytes: Vec<u8>,
    /// True for `*.test.js`.
    pub is_test: bool,
}

/// A fixture: one `.js` file, or a directory whose root is `entry.js`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    /// Path relative to the conformance root, for messages.
    pub name: String,
    /// The root file's relative path within the fixture.
    pub root: String,
    /// Every file, root first, then the rest sorted.
    pub files: Vec<FixtureFile>,
}

impl Fixture {
    /// The decoded text of a file (lossily, so expectations can be read from a fixture
    /// that is deliberately not UTF-8), or `None` for a file the fixture lacks.
    #[must_use]
    pub fn text(&self, relative: &str) -> Option<String> {
        self.files
            .iter()
            .find(|file| file.relative == relative)
            .map(|file| String::from_utf8_lossy(&file.bytes).into_owned())
    }
}

/// Finds every fixture under `root`, sorted by name. A directory containing `entry.js`
/// is one fixture and is not descended into; any other `.js` file is a fixture on its
/// own; a directory whose name starts with `_` is skipped (the harness's own tests live
/// there).
///
/// # Errors
///
/// Any I/O error while reading.
pub fn discover(root: &Path) -> io::Result<Vec<Fixture>> {
    let mut fixtures = Vec::new();
    walk(root, root, &mut fixtures)?;
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(fixtures)
}

fn walk(root: &Path, dir: &Path, fixtures: &mut Vec<Fixture>) -> io::Result<()> {
    if dir.join("entry.js").is_file() {
        fixtures.push(load_directory(root, dir)?);
        return Ok(());
    }
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('_') {
                walk(root, &path, fixtures)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "js") {
            fixtures.push(Fixture {
                name: relative(root, &path),
                root: name.clone(),
                files: vec![FixtureFile {
                    bytes: fs::read(&path)?,
                    is_test: name.ends_with(".test.js"),
                    relative: name,
                }],
            });
        }
    }
    Ok(())
}

fn load_directory(root: &Path, dir: &Path) -> io::Result<Fixture> {
    let mut files = Vec::new();
    collect_js(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    let entry = files
        .iter()
        .position(|f| f.relative == "entry.js")
        .unwrap_or_default();
    let root_file = files.remove(entry);
    files.insert(0, root_file);
    Ok(Fixture {
        name: relative(root, dir),
        root: "entry.js".to_owned(),
        files,
    })
}

fn collect_js(base: &Path, dir: &Path, files: &mut Vec<FixtureFile>) -> io::Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_js(base, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "js") {
            let relative = relative(base, &path);
            files.push(FixtureFile {
                is_test: relative.ends_with(".test.js"),
                bytes: fs::read(&path)?,
                relative,
            });
        }
    }
    Ok(())
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Convenience for a runner: parse the expectations of every file of a fixture.
///
/// # Errors
///
/// The first malformed `// expect:` line.
pub fn expectations_of(fixture: &Fixture) -> Result<Vec<Expectation>, ExpectError> {
    let mut all = Vec::new();
    for file in &fixture.files {
        all.extend(parse_expectations(
            &file.relative,
            &String::from_utf8_lossy(&file.bytes),
        )?);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::{
        compare, discover, expectations_of, out_of_range, parse_expectations, Expectation, Finding,
    };
    use crate::fs::TempDir;

    fn expectation(code: &str, line: u32, column: u32) -> Expectation {
        Expectation {
            file: "a.js".to_owned(),
            declared_at: 1,
            code: code.to_owned(),
            name: None,
            line,
            column,
            fix: None,
        }
    }

    fn finding(code: &str, name: &str, line: u32, column: u32) -> Finding {
        Finding {
            file: "a.js".to_owned(),
            code: code.to_owned(),
            name: name.to_owned(),
            line,
            column,
            fix: None,
        }
    }

    #[test]
    fn parses_the_two_documented_forms() {
        let text = "const a = 1\n// expect: E103 non-exhaustive @7:3\nlet b = 'x'\n  // expect: S001 @9:14  \"'a'\" -> \"\\\"a\\\"\"\n";
        let parsed = parse_expectations("a.js", text).expect("well formed");
        assert_eq!(
            parsed,
            vec![
                Expectation {
                    file: "a.js".to_owned(),
                    declared_at: 2,
                    code: "E103".to_owned(),
                    name: Some("non-exhaustive".to_owned()),
                    line: 7,
                    column: 3,
                    fix: None,
                },
                Expectation {
                    file: "a.js".to_owned(),
                    declared_at: 4,
                    code: "S001".to_owned(),
                    name: None,
                    line: 9,
                    column: 14,
                    fix: Some(("'a'".to_owned(), "\"a\"".to_owned())),
                },
            ]
        );
    }

    #[test]
    fn lines_that_are_not_expectations_are_ignored_and_a_bom_is_skipped() {
        assert_eq!(parse_expectations("a.js", "").expect("ok"), vec![]);
        assert_eq!(
            parse_expectations("a.js", "// expected: E000 @1:1\n// expect E000 @1:1\n")
                .expect("ok"),
            vec![]
        );
        let with_bom = "\u{FEFF}// expect: E000 syntax @1:1\n";
        assert_eq!(
            parse_expectations("a.js", with_bom).expect("ok")[0].declared_at,
            1
        );
    }

    #[test]
    fn escapes_in_fix_strings() {
        let parsed = parse_expectations("a.js", "// expect: S002 @1:1 \"a\\\\b\\n\\t\" -> \"\"")
            .expect("ok");
        assert_eq!(parsed[0].fix, Some(("a\\b\n\t".to_owned(), String::new())));
    }

    #[test]
    fn malformed_lines_are_errors_naming_the_line() {
        for (text, message) in [
            ("// expect:", "missing code"),
            (
                "// expect: e103 @1:1",
                "\"e103\" is not a code like E103 or S001",
            ),
            (
                "// expect: E1034 @1:1",
                "\"E1034\" is not a code like E103 or S001",
            ),
            ("// expect: E103", "missing @line:column"),
            ("// expect: E103 name", "missing @line:column"),
            ("// expect: E103 1:1", "missing @line:column"),
            ("// expect: E103 @1", "\"@1\" is not @line:column"),
            ("// expect: E103 @0:1", "\"@0:1\" is not @line:column"),
            ("// expect: E103 @1:0", "\"@1:0\" is not @line:column"),
            ("// expect: E103 @x:1", "\"@x:1\" is not @line:column"),
            ("// expect: E103 @1:x", "\"@1:x\" is not @line:column"),
            ("// expect: E103 name 1:1", "\"1:1\" is not @line:column"),
            (
                "// expect: E103 @1:1 extra",
                "unexpected \"extra\" after the position",
            ),
            (
                "// expect: S001 @1:1 \"a\"",
                "\"\\\"a\\\"\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"a\" -> b",
                "\"\\\"a\\\" -> b\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"a\" -> \"b\" c",
                "\"\\\"a\\\" -> \\\"b\\\" c\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"a\\q\" -> \"b\"",
                "\"\\\"a\\\\q\\\" -> \\\"b\\\"\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"unterminated -> \"b\"",
                "\"\\\"unterminated -> \\\"b\\\"\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"a\" -> \"b",
                "\"\\\"a\\\" -> \\\"b\" is not \"current\" -> \"replacement\"",
            ),
            (
                "// expect: S001 @1:1 \"a\" -> \"b\\",
                "\"\\\"a\\\" -> \\\"b\\\\\" is not \"current\" -> \"replacement\"",
            ),
        ] {
            let error = parse_expectations("a.js", &format!("x\n{text}\n")).expect_err(text);
            assert_eq!(error.line, 2, "{text}");
            assert_eq!(error.message, message, "{text}");
            assert_eq!(error.to_string(), format!("a.js:2: {message}"));
        }
    }

    #[test]
    fn out_of_range_names_expectations_past_the_last_line() {
        let text = "a\nb\nc";
        assert_eq!(
            out_of_range(&[expectation("E000", 3, 1)], text),
            Vec::<String>::new()
        );
        assert_eq!(
            out_of_range(&[expectation("E000", 4, 1)], text),
            ["a.js:1: expect E000 @4:1 is past the end of the file (3 lines)"]
        );
        assert_eq!(
            out_of_range(&[expectation("E000", 1, 1)], ""),
            Vec::<String>::new(),
            "an empty file has line 1"
        );
        assert_eq!(
            out_of_range(&[expectation("E000", 2, 1)], "\u{FEFF}x").len(),
            1
        );
    }

    #[test]
    fn compare_matches_multisets_exactly() {
        let e = [
            expectation("E000", 1, 1),
            expectation("E000", 1, 1),
            expectation("S001", 2, 5),
        ];
        let ok = [
            finding("E000", "syntax", 1, 1),
            finding("S001", "single-quoted-string", 2, 5),
            finding("E000", "syntax", 1, 1),
        ];
        assert_eq!(compare(&e, &ok), Vec::<String>::new());
        let once = [
            finding("E000", "syntax", 1, 1),
            finding("S001", "single-quoted-string", 2, 5),
        ];
        assert_eq!(
            compare(&e, &once),
            ["a.js:1: expected E000 @1:1 not produced"]
        );
        let wrong_line = [finding("E000", "syntax", 2, 1)];
        assert_eq!(
            compare(&[expectation("E000", 1, 1)], &wrong_line),
            [
                "a.js:1: expected E000 @1:1 not produced",
                "a.js: unexpected E000 syntax @2:1"
            ]
        );
        assert_eq!(
            compare(&[], &wrong_line),
            ["a.js: unexpected E000 syntax @2:1"]
        );
        assert_eq!(compare(&[], &[]), Vec::<String>::new());
    }

    #[test]
    fn names_fixes_and_files_take_part_in_matching() {
        let mut named = expectation("E000", 1, 1);
        named.name = Some("syntax".to_owned());
        assert_eq!(
            compare(&[named.clone()], &[finding("E000", "syntax", 1, 1)]),
            Vec::<String>::new()
        );
        assert_eq!(
            compare(&[named], &[finding("E000", "other", 1, 1)]),
            [
                "a.js:1: expected E000 syntax @1:1 not produced",
                "a.js: unexpected E000 other @1:1"
            ]
        );
        let mut fixed = expectation("S001", 1, 1);
        fixed.fix = Some(("'a'".to_owned(), "\"a\"".to_owned()));
        let mut found = finding("S001", "single-quoted-string", 1, 1);
        assert_eq!(
            compare(&[fixed.clone()], &[found.clone()]),
            [
                "a.js:1: expected S001 @1:1 \"'a'\" -> \"\\\"a\\\"\" not produced",
                "a.js: unexpected S001 single-quoted-string @1:1"
            ]
        );
        found.fix = Some(("'a'".to_owned(), "\"a\"".to_owned()));
        assert_eq!(
            compare(&[fixed.clone()], &[found.clone()]),
            Vec::<String>::new()
        );
        assert_eq!(
            compare(&[], &[found]),
            ["a.js: unexpected S001 single-quoted-string @1:1 \"'a'\" -> \"\\\"a\\\"\""]
        );
        let mut other_file = finding("S001", "single-quoted-string", 1, 1);
        other_file.file = "b.js".to_owned();
        other_file.fix = fixed.fix.clone();
        assert_eq!(compare(&[fixed], &[other_file]).len(), 2);
    }

    #[test]
    fn discovery_finds_single_files_and_entry_directories_and_skips_underscore_dirs() {
        let dir = TempDir::new("conformance").expect("temp dir");
        dir.write("02-lexical/E000/bom.js", "// expect: E000 @1:1\n")
            .expect("write");
        dir.write("02-lexical/E000/notes.txt", "ignored")
            .expect("write");
        dir.write(
            "05-modules/cycle/entry.js",
            "import { a } from \"./a.js\"\n",
        )
        .expect("write");
        dir.write("05-modules/cycle/a.js", "export const a = 1\n")
            .expect("write");
        dir.write("05-modules/cycle/README.txt", "not a module")
            .expect("write");
        dir.write("05-modules/cycle/lib/b.test.js", "// expect: E301 @1:1\n")
            .expect("write");
        dir.write(
            "05-modules/cycle/nested/entry.js",
            "not a fixture of its own",
        )
        .expect("write");
        dir.write("_harness/wrong.js", "// expect: E000 @9:9\n")
            .expect("write");
        dir.write("loose.test.js", "").expect("write");
        let fixtures = discover(dir.path()).expect("discover");
        let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "02-lexical/E000/bom.js",
                "05-modules/cycle",
                "loose.test.js"
            ]
        );
        let cycle = &fixtures[1];
        assert_eq!(cycle.root, "entry.js");
        let files: Vec<(&str, bool)> = cycle
            .files
            .iter()
            .map(|f| (f.relative.as_str(), f.is_test))
            .collect();
        assert_eq!(
            files,
            [
                ("entry.js", false),
                ("a.js", false),
                ("lib/b.test.js", true),
                ("nested/entry.js", false)
            ]
        );
        assert_eq!(cycle.text("a.js").as_deref(), Some("export const a = 1\n"));
        assert_eq!(cycle.text("missing.js"), None);
        assert!(fixtures[2].files[0].is_test);
        let all = expectations_of(cycle).expect("well formed");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].file, "lib/b.test.js");
        assert_eq!(
            discover(&dir.path().join("empty"))
                .expect_err("missing root")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    /// Every I/O failure while reading fixtures is an error, never a panic.
    #[cfg(unix)]
    #[test]
    fn unreadable_files_and_directories_are_io_errors() {
        use std::os::unix::fs::PermissionsExt;
        let lock = |path: &std::path::Path| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        };
        let unlock = |path: &std::path::Path| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod back");
        };
        // (a) a single-file fixture that cannot be read
        let dir = TempDir::new("conformance").expect("temp dir");
        let file = dir.write("locked.js", "").expect("write");
        lock(&file);
        assert!(discover(dir.path()).is_err(), "unreadable single file");
        unlock(&file);
        // (b) a plain directory that cannot be listed
        let dir = TempDir::new("conformance").expect("temp dir");
        let sub = dir.mkdir("locked").expect("mkdir");
        lock(&sub);
        assert!(discover(dir.path()).is_err(), "unlistable directory");
        unlock(&sub);
        // (c) a directory fixture with an unlistable subdirectory
        let dir = TempDir::new("conformance").expect("temp dir");
        dir.write("f/entry.js", "").expect("write");
        let sub = dir.mkdir("f/sub").expect("mkdir");
        lock(&sub);
        assert!(
            discover(dir.path()).is_err(),
            "unlistable subdirectory of a fixture"
        );
        unlock(&sub);
        // (d) a directory fixture with an unreadable module
        let dir = TempDir::new("conformance").expect("temp dir");
        dir.write("g/entry.js", "").expect("write");
        let module = dir.write("g/a.js", "").expect("write");
        lock(&module);
        assert!(
            discover(dir.path()).is_err(),
            "unreadable module of a fixture"
        );
        unlock(&module);
    }

    #[test]
    fn a_malformed_expectation_anywhere_in_a_directory_fixture_is_reported() {
        let dir = TempDir::new("conformance").expect("temp dir");
        dir.write("f/entry.js", "").expect("write");
        dir.write("f/a.js", "// expect: nope\n").expect("write");
        let fixtures = discover(dir.path()).expect("discover");
        let error = expectations_of(&fixtures[0]).expect_err("malformed");
        assert_eq!((error.file.as_str(), error.line), ("a.js", 1));
    }
}
