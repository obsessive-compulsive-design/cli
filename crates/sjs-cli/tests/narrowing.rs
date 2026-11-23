//! STORIES.md US-111: the fixtures under `tests/conformance/03-narrowing` are
//! generated: every form of test, in every position a test narrows from, written
//! plainly and negated. A `pass` fixture uses the value as only the narrowed type
//! allows, so that it has a finding unless the narrowing is made; a `fail` fixture
//! uses it as only the other branch allows, so that it has none unless the narrowing
//! is made the right way round. This test fails when the fixtures differ from what
//! is generated, and `SJS_UPDATE_SNAPSHOTS=1` writes them anew. The conformance
//! harness then holds the compiler to each of them.

use sjs_testkit::fs::TempDir;
use sjs_testkit::snapshot::Mode;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/03-narrowing")
}

/// A use of the subject that only one branch of a test allows.
struct Use {
    /// The expression.
    text: &'static str,
    /// A value of its type, for the branch that does not use the subject.
    default: &'static str,
    /// What the other branch says of it: the code with its slug, and where in the
    /// expression.
    refused: (&'static str, usize),
}

/// A form of test.
struct Form {
    name: &'static str,
    /// What the file declares before its functions.
    setup: &'static [&'static str],
    /// What `subject` is, by a boolean `flag`.
    subject: &'static str,
    plain: &'static str,
    negated: &'static str,
    /// What the subject allows when the test holds.
    when: Use,
    /// What it allows when the test does not hold, when that tells anything.
    unless: Option<Use>,
}

const MISSING: &str = "E137 missing-property";
const MISMATCH: &str = "E138 type-mismatch";

#[allow(clippy::too_many_lines)]
fn forms() -> Vec<Form> {
    let key = |text: &'static str, default: &'static str| Use {
        text,
        default,
        refused: (MISSING, "subject.".len()),
    };
    vec![
        Form {
            name: "typeof",
            setup: &[],
            subject: "flag ? \"a\" : 1",
            plain: "typeof subject === \"string\"",
            negated: "typeof subject !== \"string\"",
            when: key("subject.length", "0"),
            unless: Some(Use {
                text: "subject + 1",
                default: "0",
                refused: (MISMATCH, "subject ".len()),
            }),
        },
        Form {
            name: "undefined",
            setup: &[],
            subject: "flag ? \"a\" : undefined",
            plain: "subject === undefined",
            negated: "subject !== undefined",
            when: Use {
                text: "subject ?? \"b\"",
                default: "\"b\"",
                refused: (MISMATCH, 0),
            },
            unless: Some(key("subject.length", "0")),
        },
        Form {
            name: "null",
            setup: &[],
            subject: "/a/v.exec(flag ? \"a\" : \"b\")",
            plain: "subject === null",
            negated: "subject !== null",
            when: Use {
                text: "subject ?? 0",
                default: "0",
                refused: (MISMATCH, 0),
            },
            unless: Some(key("subject.index", "0")),
        },
        Form {
            name: "literal",
            setup: &["const onlyA = { a: 1 }", "const onlyB = { b: 2 }"],
            subject: "flag ? \"a\" : \"b\"",
            plain: "subject === \"a\"",
            negated: "subject !== \"a\"",
            when: Use {
                text: "onlyA[subject]",
                default: "1",
                refused: (MISSING, "onlyA[".len()),
            },
            unless: Some(Use {
                text: "onlyB[subject]",
                default: "2",
                refused: (MISSING, "onlyB[".len()),
            }),
        },
        Form {
            name: "discriminant",
            setup: &[],
            subject: "flag ? { kind: \"circle\", radius: 1 } : { kind: \"square\", side: 2 }",
            plain: "subject.kind === \"circle\"",
            negated: "subject.kind !== \"circle\"",
            when: key("subject.radius", "1"),
            unless: Some(key("subject.side", "2")),
        },
        Form {
            name: "instanceof",
            setup: &[
                "class Circle {\n  radius = 1\n}",
                "class Square {\n  side = 2\n}",
            ],
            subject: "flag ? new Circle() : new Square()",
            plain: "subject instanceof Circle",
            negated: "!(subject instanceof Circle)",
            when: key("subject.radius", "1"),
            unless: Some(key("subject.side", "2")),
        },
        Form {
            name: "is-array",
            setup: &[],
            subject: "flag ? [1] : \"a\"",
            plain: "Array.isArray(subject)",
            negated: "!Array.isArray(subject)",
            when: key("subject.join(\"\")", "\"\""),
            unless: Some(key("subject.toUpperCase()", "\"\"")),
        },
        Form {
            name: "is-nan",
            setup: &[],
            subject: "flag ? Math.random() : \"a\"",
            plain: "Number.isNaN(subject)",
            negated: "!Number.isNaN(subject)",
            when: Use {
                text: "subject + 1",
                default: "0",
                refused: (MISMATCH, "subject ".len()),
            },
            // A number that is not `NaN` is a number still.
            unless: None,
        },
    ]
}

/// A position a test narrows from: its name, whether the test holds where the use
/// is, and the body of the function, with `{C}` for the test, `{U}` for the use and
/// `{D}` for the default.
const POSITIONS: [(&str, bool, &str); 8] = [
    (
        "inIf",
        true,
        "  if ({C}) {\n    return {U}\n  }\n  return {D}\n",
    ),
    (
        "inElse",
        false,
        "  if ({C}) {\n    return {D}\n  } else {\n    return {U}\n  }\n",
    ),
    (
        "afterReturn",
        false,
        "  if ({C}) {\n    return {D}\n  }\n  return {U}\n",
    ),
    ("inThen", true, "  return {C} ? {U} : {D}\n"),
    ("inOtherwise", false, "  return {C} ? {D} : {U}\n"),
    ("afterAnd", true, "  return {C} && isIt({U}, {D})\n"),
    ("afterOr", false, "  return {C} || isIt({U}, {D})\n"),
    (
        "inWhile",
        true,
        "  while ({C}) {\n    return {U}\n  }\n  return {D}\n",
    ),
];

/// One fixture of a form: every position, plain and negated, with the use the
/// narrowed type allows (`right`) or the one it refuses.
fn fixture(form: &Form, right: bool) -> String {
    let mut body: Vec<String> = vec!["const isIt = (value, wanted) => value === wanted".to_owned()];
    body.extend(form.setup.iter().map(|&line| line.to_owned()));
    // Each finding expected: the line within the body, the column, the code.
    let mut expected: Vec<(usize, usize, &str)> = Vec::new();
    let mut lines = body
        .iter()
        .map(|part| part.lines().count() + 1)
        .sum::<usize>();
    for (position, holds_plain, shape) in POSITIONS {
        for negated in [false, true] {
            let holds = holds_plain != negated;
            let allowed = if holds {
                Some(&form.when)
            } else {
                form.unless.as_ref()
            };
            let refused = if holds {
                form.unless.as_ref()
            } else {
                Some(&form.when)
            };
            let Some(used) = (if right { allowed } else { refused }) else {
                continue;
            };
            let test = if negated { form.negated } else { form.plain };
            let name = format!("{position}{}", if negated { "Negated" } else { "" });
            let inside = shape
                .replace("{C}", test)
                .replace("{U}", used.text)
                .replace("{D}", used.default);
            let written = format!(
                "export function {name}(flag) {{\n  const subject = {}\n{inside}}}",
                form.subject
            );
            let at = written.lines().enumerate().find_map(|(line, written)| {
                let column = written.find(used.text).filter(|_| line > 1)?;
                Some((
                    lines + line + 1,
                    column + used.refused.1 + 1,
                    used.refused.0,
                ))
            });
            expected.extend(at.filter(|_| !right));
            lines += written.lines().count() + 1;
            body.push(written);
        }
    }
    let mut head: Vec<String> = Vec::new();
    let offset = expected.len() + 2;
    for (line, column, code) in expected {
        head.push(format!("// expect: {code} @{}:{column}", line + offset));
    }
    head.push("// Generated, not written by hand:".to_owned());
    head.push("// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test narrowing".to_owned());
    format!("{}\n{}\n", head.join("\n"), body.join("\n\n"))
}

/// The fixtures: path beneath the directory, and text.
fn generated() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for form in forms() {
        files.insert(format!("pass/{}.js", form.name), fixture(&form, true));
        files.insert(format!("fail/{}.js", form.name), fixture(&form, false));
    }
    files
}

/// The files beneath `directory`, one level down: path and text.
fn on_disk(directory: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let entries = |path: &Path| fs::read_dir(path).into_iter().flatten().flatten();
    for group in entries(directory) {
        for file in entries(&group.path()) {
            let path = format!(
                "{}/{}",
                group.file_name().to_string_lossy(),
                file.file_name().to_string_lossy()
            );
            files.insert(path, fs::read_to_string(file.path()).expect("a fixture"));
        }
    }
    files
}

/// Makes the files beneath `directory` exactly `files`.
fn write(directory: &Path, files: &BTreeMap<String, String>) {
    let stored = on_disk(directory);
    for stale in stored.keys().filter(|path| !files.contains_key(*path)) {
        fs::remove_file(directory.join(stale)).expect("a stale fixture is removed");
    }
    for (path, text) in files {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        fs::write(path, text).expect("the fixture");
    }
}

#[test]
fn the_fixtures_of_the_matrix_are_the_ones_on_disk() {
    let files = generated();
    let scratch = TempDir::new("us111").expect("a temporary directory");
    scratch.write("pass/stale.js", "").expect("a stale file");
    let update = Mode::from_env() == Mode::Update;
    let targets = [scratch.path().to_path_buf(), fixtures()];
    for target in targets.iter().take(1 + usize::from(update)) {
        write(target, &files);
    }
    assert!(on_disk(scratch.path()) == files, "writing is exact");
    let stored = on_disk(&fixtures());
    let differing: Vec<&String> = files
        .iter()
        .filter(|(path, text)| stored.get(*path) != Some(*text))
        .map(|(path, _)| path)
        .chain(stored.keys().filter(|path| !files.contains_key(*path)))
        .collect();
    assert!(
        differing.is_empty(),
        "the fixtures differ from the matrix; regenerate them with SJS_UPDATE_SNAPSHOTS=1: {differing:?}"
    );
}

#[test]
fn every_form_is_in_every_position_plain_and_negated() {
    let files = generated();
    let text = |path: &str| files.get(path).cloned().unwrap_or_default();
    let functions = |path: &str| text(path).matches("export function ").count();
    let expected = |path: &str| text(path).matches("// expect: ").count();
    for form in forms() {
        let (pass, fail) = (
            format!("pass/{}.js", form.name),
            format!("fail/{}.js", form.name),
        );
        // A test that tells nothing when it does not hold has half the cells.
        let cells = if form.unless.is_some() { 16 } else { 8 };
        assert_eq!((functions(&pass), expected(&pass)), (cells, 0), "{pass}");
        assert_eq!(
            (functions(&fail), expected(&fail)),
            (cells, cells),
            "{fail}"
        );
    }
    assert!(text("fail/undefined.js").starts_with("// expect: E137 missing-property @24:20\n"));
    assert!(text("pass/typeof.js").contains(
        "export function afterOrNegated(flag) {\n  const subject = flag ? \"a\" : 1\n  return typeof subject !== \"string\" || isIt(subject.length, 0)\n}\n"
    ));
    assert!(text("fail/instanceof.js").contains(
        "export function inWhile(flag) {\n  const subject = flag ? new Circle() : new Square()\n  while (subject instanceof Circle) {\n    return subject.side\n  }\n  return 2\n}\n"
    ));
}
