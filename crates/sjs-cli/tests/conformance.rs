//! The conformance harness (STORIES.md §0.3, US-007): every fixture under
//! `tests/conformance/` is run through the pipeline and must produce exactly the
//! diagnostics its `// expect:` lines declare. `codes.txt` maps every code to the story
//! that adds its fixtures; a code without fixtures is "pending" until that story is
//! marked complete in STORIES.md, and a failure afterwards.

use sjs_cli::pipeline::{check_files, InputFile};
use sjs_diag::{Code, Diagnostics};
use sjs_testkit::conformance::{
    compare, discover, expectations_of, out_of_range, Finding, Fixture,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn conformance_root() -> PathBuf {
    workspace_root().join("tests/conformance")
}

fn findings(diagnostics: &Diagnostics) -> Vec<Finding> {
    diagnostics
        .iter()
        .map(|d| Finding {
            file: d.path.clone(),
            code: d.code.as_str().to_owned(),
            name: d.code.name().to_owned(),
            line: d.span.start.line,
            column: d.span.start.column,
            fix: d.current.clone().zip(d.replacement.clone()),
        })
        .collect()
}

/// Runs one fixture and returns its mismatches (empty when it conforms).
fn run_fixture(fixture: &Fixture) -> Vec<String> {
    let expected = match expectations_of(fixture) {
        Ok(expected) => expected,
        Err(error) => return vec![format!("{}: {error}", fixture.name)],
    };
    let mut problems = Vec::new();
    for file in &fixture.files {
        let text = String::from_utf8_lossy(&file.bytes);
        let own: Vec<_> = expected
            .iter()
            .filter(|e| e.file == file.relative)
            .cloned()
            .collect();
        problems.extend(out_of_range(&own, &text));
    }
    if !problems.is_empty() {
        return problems;
    }
    let inputs: Vec<InputFile> = fixture
        .files
        .iter()
        .map(|f| InputFile {
            path: f.relative.clone(),
            bytes: f.bytes.clone(),
        })
        .collect();
    compare(&expected, &findings(&check_files(&inputs)))
}

#[test]
fn every_fixture_produces_exactly_its_declared_diagnostics() {
    let fixtures = discover(&conformance_root()).expect("tests/conformance is readable");
    assert!(!fixtures.is_empty(), "no fixtures found");
    let mut failures = Vec::new();
    for fixture in &fixtures {
        for problem in run_fixture(fixture) {
            failures.push(format!("{}: {problem}", fixture.name));
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

/// The grammar sweep (US-033): every `02-grammar/pass/` fixture (one per §2
/// production) declares no diagnostic, and every `02-grammar/fail/` fixture (one per
/// removed production or syntactic §12 row) declares exactly one. The main test holds
/// each to what it declares.
#[test]
fn the_grammar_sweep_declares_none_for_pass_and_one_for_fail() {
    let fixtures = discover(&conformance_root().join("02-grammar")).expect("readable");
    let mut pass = 0;
    let mut fail = 0;
    for fixture in &fixtures {
        let expected = expectations_of(fixture).expect("well-formed expectations");
        let is_pass = fixture.name.starts_with("pass/");
        let is_fail = fixture.name.starts_with("fail/");
        assert!(
            is_pass || is_fail,
            "{}: not under pass/ or fail/",
            fixture.name
        );
        assert_eq!(
            expected.len(),
            usize::from(is_fail),
            "{}: {expected:?}",
            fixture.name
        );
        pass += usize::from(is_pass);
        fail += usize::from(is_fail);
    }
    assert!(pass >= 38, "{pass} pass fixtures: one per §2 production");
    assert!(
        fail >= 39,
        "{fail} fail fixtures: one per removed production and §12 row"
    );
}

/// The harness's own fixtures under `_harness/` are deliberately wrong; each must fail
/// with the message a reviewer would need.
#[test]
fn the_harness_reports_each_kind_of_mismatch() {
    let root = conformance_root().join("_harness");
    let fixtures = discover(&root).expect("_harness is readable");
    let by_name = |name: &str| {
        fixtures
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };

    assert_eq!(
        run_fixture(by_name("wrong-line.js")),
        [
            "wrong-line.js:2: expected E000 syntax @2:1 not produced",
            "wrong-line.js: unexpected E000 syntax @1:1"
        ]
    );
    assert_eq!(
        run_fixture(by_name("missing.js")),
        ["missing.js:1: expected E000 @1:1 not produced"]
    );
    assert_eq!(
        run_fixture(by_name("unexpected.js")),
        ["unexpected.js: unexpected E000 syntax @1:1"]
    );
    assert_eq!(
        run_fixture(by_name("past-end.js")),
        ["past-end.js:1: expect E000 @99:1 is past the end of the file (2 lines)"]
    );
    assert_eq!(
        run_fixture(by_name("declared-twice.js")),
        ["declared-twice.js:2: expected E000 @1:1 not produced"]
    );
    assert_eq!(
        run_fixture(by_name("malformed.js")),
        ["malformed.js: malformed.js:1: missing @line:column"]
    );
    assert_eq!(run_fixture(by_name("conforming.js")), Vec::<String>::new());
    assert_eq!(run_fixture(by_name("directory")), Vec::<String>::new());
    let directory = by_name("directory");
    assert_eq!(directory.root, "entry.js");
    let roots: Vec<&str> = directory
        .files
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.relative.as_str())
        .collect();
    let tests: Vec<&str> = directory
        .files
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.relative.as_str())
        .collect();
    assert_eq!(roots, ["entry.js", "lib/helper.js"]);
    assert_eq!(tests, ["helper.test.js"]);
}

/// `codes.txt`: one `CODE US-nnn` per line.
fn code_table() -> Vec<(String, String)> {
    let text =
        std::fs::read_to_string(conformance_root().join("codes.txt")).expect("codes.txt exists");
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split_whitespace();
            let code = words.next().expect("code").to_owned();
            let story = words
                .next()
                .unwrap_or_else(|| panic!("codes.txt: {code} has no story"))
                .to_owned();
            assert_eq!(words.next(), None, "codes.txt: extra text after {code}");
            (code, story)
        })
        .collect()
}

/// Story IDs marked complete in STORIES.md (`#### US-nnn … **Status: complete**`).
fn completed_stories(stories: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = stories.lines().collect();
    lines
        .windows(2)
        .filter(|pair| pair[0].starts_with("#### US-") && pair[1].contains("**Status: complete**"))
        .map(|pair| pair[0][5..11].to_owned())
        .collect()
}

fn all_stories(stories: &str) -> BTreeSet<String> {
    stories
        .lines()
        .filter(|line| line.starts_with("#### US-"))
        .map(|line| line[5..11].to_owned())
        .collect()
}

#[test]
fn every_code_has_fixtures_once_its_story_is_complete() {
    let table = code_table();
    let listed: BTreeSet<&str> = table.iter().map(|(code, _)| code.as_str()).collect();
    assert_eq!(listed.len(), table.len(), "codes.txt lists a code twice");
    let known: BTreeSet<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(
        listed, known,
        "codes.txt must list every code of SPEC.md exactly once"
    );

    let stories = std::fs::read_to_string(workspace_root().join("STORIES.md")).expect("STORIES.md");
    let complete = completed_stories(&stories);
    let existing = all_stories(&stories);
    assert!(
        complete.contains("US-007"),
        "this story must be marked complete for the audit to be live"
    );

    let fixtures = discover(&conformance_root()).expect("readable");
    let mut covered = BTreeSet::new();
    for fixture in &fixtures {
        for expectation in expectations_of(fixture).unwrap_or_default() {
            covered.insert(expectation.code);
        }
    }
    let mut failures = Vec::new();
    for (code, story) in &table {
        assert!(
            existing.contains(story),
            "codes.txt: {code} names {story}, which STORIES.md does not define"
        );
        if covered.contains(code) {
            continue;
        }
        if complete.contains(story) {
            failures.push(format!(
                "{code}: {story} is complete but no fixture expects {code}"
            ));
        } else {
            eprintln!("pending: {code} ({story})");
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}
