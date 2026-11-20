//! STORIES.md US-068: the fixtures under `tests/conformance/06-ambient/table` are
//! generated from `ambient/globals.toml`, one set per global, so table and fixtures
//! cannot drift: this test fails when they differ, and `SJS_UPDATE_SNAPSHOTS=1` writes
//! them anew. The conformance harness then holds the compiler to each of them.

use sjs_bind::ambient::{Global, Table};
use sjs_testkit::fs::TempDir;
use sjs_testkit::snapshot::Mode;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn table_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/06-ambient/table")
}

/// A member no global has: what is not listed is not kept.
const UNLISTED: &str = "notKept";

/// One use of a global in a fixture: the expression, and the finding it is to
/// produce, as the code with its name and the column within the expression.
struct Line {
    expression: String,
    expected: Option<(&'static str, usize)>,
}

fn bare(global: &Global, expected: Option<&'static str>) -> Line {
    Line {
        expression: global.name.clone(),
        expected: expected.map(|code| (code, 0)),
    }
}

fn member(global: &Global, member: &str, expected: Option<&'static str>) -> Line {
    Line {
        expression: format!("{}.{member}", global.name),
        expected: expected.map(|code| (code, global.name.len() + 1)),
    }
}

/// A fixture: its `// expect:` lines, a note, and one exported constant per use.
fn fixture(lines: &[Line]) -> String {
    let expected = lines.iter().filter(|line| line.expected.is_some()).count();
    let mut head = String::new();
    let mut body = String::new();
    for (at, line) in lines.iter().enumerate() {
        let declared = format!("export const use{} = ", at + 1);
        let row = expected + 3 + 2 * at;
        if let Some((code, column)) = line.expected {
            let column = declared.len() + column + 1;
            writeln!(head, "// expect: {code} @{row}:{column}").expect("a string");
        }
        let gap = if at == 0 { "" } else { "\n" };
        writeln!(body, "{gap}{declared}{}", line.expression).expect("a string");
    }
    format!(
        "{head}// Generated from ambient/globals.toml, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient\n{body}"
    )
}

const E133: &str = "E133 removed-global-or-member";
const E204: &str = "E204 unknown-global";
const E000: &str = "E000 syntax";

/// The fixtures of the table: path beneath the directory, and text.
fn generated(table: &Table) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for global in table.globals() {
        let name = &global.name;
        let mut add = |directory: &str, suffix: &str, lines: &[Line]| {
            if !lines.is_empty() {
                files.insert(format!("{directory}/{name}{suffix}"), fixture(lines));
            }
        };
        if !global.missing().is_empty() {
            add("E204", ".js", &[bare(global, Some(E204))]);
            continue;
        }
        if global.removed.is_some() {
            let code = [E133, E000][usize::from(global.syntax)];
            add("E133", ".js", &[bare(global, Some(code))]);
            continue;
        }
        let suffix = if global.test { ".test.js" } else { ".js" };
        let outside: Vec<Line> = global
            .test
            .then(|| bare(global, Some(E204)))
            .into_iter()
            .collect();
        add("E204", ".js", &outside);
        let everywhere = |member: &&String| global.member_missing(member).is_empty();
        let mut kept: Vec<Line> = Vec::new();
        kept.extend(global.members_only.is_none().then(|| bare(global, None)));
        let members = global.members.iter().filter(everywhere);
        kept.extend(members.map(|kept| member(global, kept, None)));
        add("pass", suffix, &kept);
        let lacking = global.members.iter().filter(|member| !everywhere(member));
        let lacking: Vec<Line> = lacking.map(|m| member(global, m, Some(E204))).collect();
        add("E204", suffix, &lacking);
        let mut removed: Vec<Line> = Vec::new();
        removed.extend(
            global
                .members_only
                .is_some()
                .then(|| bare(global, Some(E133))),
        );
        let gone = global.removed_members.keys();
        removed.extend(gone.map(|gone| member(global, gone, Some(E133))));
        removed.extend((!global.open).then(|| member(global, UNLISTED, Some(E133))));
        add("E133", suffix, &removed);
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
    for stale in on_disk(directory)
        .keys()
        .filter(|path| !files.contains_key(*path))
    {
        fs::remove_file(directory.join(stale)).expect("a stale fixture is removed");
    }
    for (path, text) in files {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
        fs::write(path, text).expect("the fixture");
    }
}

#[test]
fn the_fixtures_of_the_table_are_the_ones_on_disk() {
    let files = generated(Table::shipped());
    let scratch = TempDir::new("us068").expect("a temporary directory");
    scratch.write("pass/stale.js", "").expect("a stale file");
    let update = Mode::from_env() == Mode::Update;
    let targets = [scratch.path().to_path_buf(), table_fixtures()];
    for target in targets.iter().take(1 + usize::from(update)) {
        write(target, &files);
    }
    assert!(on_disk(scratch.path()) == files, "writing is exact");
    let stored = on_disk(&table_fixtures());
    let differing: Vec<&String> = files
        .iter()
        .filter(|(path, text)| stored.get(*path) != Some(*text))
        .map(|(path, _)| path)
        .chain(stored.keys().filter(|path| !files.contains_key(*path)))
        .collect();
    assert!(
        differing.is_empty(),
        "the fixtures differ from ambient/globals.toml; regenerate them with SJS_UPDATE_SNAPSHOTS=1: {differing:?}"
    );
}

#[test]
fn every_global_of_the_table_has_its_fixtures() {
    let table = Table::shipped();
    let files = generated(table);
    let lower: Vec<String> = files.keys().map(|path| path.to_lowercase()).collect();
    for (at, path) in lower.iter().enumerate() {
        assert!(
            !lower[..at].contains(path),
            "{path}: two fixtures differ by case only"
        );
    }
    for global in table.globals() {
        let name = &global.name;
        let own = |path: &&String| {
            let file = path.split('/').nth(1).unwrap_or_default();
            file == format!("{name}.js") || file == format!("{name}.test.js")
        };
        assert!(files.keys().any(|path| own(&path)), "{name} has no fixture");
    }
    let text = |path: &str| files.get(path).cloned().unwrap_or_default();
    assert_eq!(
        text("E204/Temporal.js"),
        "// expect: E204 unknown-global @4:21\n// Generated from ambient/globals.toml, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test ambient\nexport const use1 = Temporal\n"
    );
    assert!(text("E204/Uint8Array.js").contains("use1 = Uint8Array.fromBase64\n"));
    assert!(!text("pass/Uint8Array.js").contains("fromBase64"));
    assert!(text("E133/navigator.js").contains("navigator.notKept\n"));
    assert!(text("pass/navigator.js").contains("navigator.hardwareConcurrency\n"));
    assert!(text("E133/Symbol.js").starts_with("// expect: E133 removed-global-or-member @"));
    assert!(text("E133/Symbol.js").contains("use1 = Symbol\n"));
    assert!(!text("pass/Symbol.js").contains("= Symbol\n"));
    assert!(text("pass/describe.test.js").contains("describe.only\n"));
    assert!(text("E133/eval.js").starts_with("// expect: E000 syntax @4:21\n"));
    assert!(files.contains_key("E204/describe.js") && files.contains_key("E133/encodeURI.js"));
    assert!(!files.contains_key("E133/Intl.js") && !files.contains_key("pass/window.js"));
}
