//! STORIES.md US-088: the operator × operand-kind matrix as generated fixtures. The
//! table below is SPEC.md §4.1 written out as data; the fixtures under
//! `tests/conformance/04-operators/table` are generated from it, this test fails when
//! they differ and `SJS_UPDATE_SNAPSHOTS=1` writes them anew. The conformance harness
//! then holds the compiler to each of them.

use sjs_testkit::fs::TempDir;
use sjs_testkit::snapshot::Mode;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn table_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/04-operators/table")
}

/// The kinds of operand, each as a literal of it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Number,
    Int32,
    BigInt,
    String,
    Boolean,
    Undefined,
}

const KINDS: [(Kind, &str); 6] = [
    (Kind::Number, "1.5"),
    (Kind::Int32, "1"),
    (Kind::BigInt, "2n"),
    (Kind::String, "\"a\""),
    (Kind::Boolean, "true"),
    (Kind::Undefined, "undefined"),
];

/// The rows of SPEC.md §4.1.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Plus,
    Arithmetic,
    Remainder,
    Bitwise,
    UnsignedShift,
    Ordering,
    Equality,
    Logical,
    Nullish,
}

const BINARY: [(&str, &str, Row); 21] = [
    ("plus", "+", Row::Plus),
    ("minus", "-", Row::Arithmetic),
    ("times", "*", Row::Arithmetic),
    ("divide", "/", Row::Arithmetic),
    ("power", "**", Row::Arithmetic),
    ("remainder", "%", Row::Remainder),
    ("bit-and", "&", Row::Bitwise),
    ("bit-or", "|", Row::Bitwise),
    ("bit-xor", "^", Row::Bitwise),
    ("shift-left", "<<", Row::Bitwise),
    ("shift-right", ">>", Row::Bitwise),
    ("unsigned-shift", ">>>", Row::UnsignedShift),
    ("less", "<", Row::Ordering),
    ("greater", ">", Row::Ordering),
    ("less-or-equal", "<=", Row::Ordering),
    ("greater-or-equal", ">=", Row::Ordering),
    ("equal", "===", Row::Equality),
    ("not-equal", "!==", Row::Equality),
    ("and", "&&", Row::Logical),
    ("or", "||", Row::Logical),
    ("nullish", "??", Row::Nullish),
];

const E102: &str = "E102 ambiguous-truthiness";
const E104: &str = "E104 bitwise-on-double";
const E105: &str = "E105 no-overlap";
const E120: &str = "E120 self-assign-or-compare";
const E121: &str = "E121 constant-condition";
const E138: &str = "E138 type-mismatch";

/// Where a finding is, in `left operator right`.
#[derive(Clone, Copy)]
enum At {
    Left,
    Operator,
    Right,
}

fn is_number(kind: Kind) -> bool {
    matches!(kind, Kind::Number | Kind::Int32)
}

/// What §4.1 says of `left operator right`: nothing, or the code and where.
fn judged(row: Row, left: Kind, right: Kind) -> Option<(&'static str, At)> {
    let both = |kind: Kind| left == kind && right == kind;
    let numbers = is_number(left) && is_number(right);
    let mismatch = Some((E138, At::Operator));
    // `undefined` is a name: compared with itself it is a place compared with itself.
    let compares = matches!(row, Row::Ordering | Row::Equality);
    if compares && both(Kind::Undefined) {
        return Some((E120, At::Left));
    }
    // `+` and the orderings take two strings too.
    let text_too = matches!(row, Row::Plus | Row::Ordering);
    let accepted = match row {
        Row::Plus | Row::Arithmetic | Row::Remainder | Row::Ordering => {
            numbers || both(Kind::BigInt) || (text_too && both(Kind::String))
        }
        Row::Bitwise if both(Kind::Int32) || both(Kind::BigInt) => true,
        Row::UnsignedShift if both(Kind::Int32) => true,
        // A number that may be no int32 is E104; any other kind a mismatch.
        Row::Bitwise | Row::UnsignedShift if numbers => return Some((E104, At::Operator)),
        Row::Bitwise | Row::UnsignedShift => false,
        // Each kind here is one literal: two literals share a value when they are one.
        Row::Equality if left != right => return Some((E105, At::Operator)),
        Row::Logical if left != Kind::Boolean => return Some((E138, At::Left)),
        Row::Logical if right != Kind::Boolean => return Some((E138, At::Right)),
        // A left that is never absent makes `??` always its left.
        Row::Nullish if left != Kind::Undefined => return Some((E121, At::Left)),
        Row::Equality | Row::Logical | Row::Nullish => true,
    };
    if accepted {
        None
    } else {
        mismatch
    }
}

/// What §4.1 says of a unary operator on an operand, or of `operand ? 1 : 2`.
fn judged_one(operator: &str, operand: Kind) -> Option<&'static str> {
    let numeric = is_number(operand) || operand == Kind::BigInt;
    match operator {
        "-" | "+" if numeric => None,
        "~" if matches!(operand, Kind::Int32 | Kind::BigInt) => None,
        "~" if operand == Kind::Number => Some(E104),
        "!" if operand == Kind::Boolean => None,
        // A condition written `true` is always the same.
        "?" if operand == Kind::Boolean => Some(E121),
        // What is asked for its truth and is no boolean (US-112).
        "!" | "?" => Some(E102),
        "typeof " => None,
        _ => Some(E138),
    }
}

/// One line of a fixture: the expression, and the finding it is to produce, as the
/// code with its name and the column within the expression.
struct Line {
    expression: String,
    expected: Option<(&'static str, usize)>,
}

/// A fixture: its `// expect:` lines, a note, and one exported constant per line.
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
        "{head}// Generated from the table of SPEC.md §4.1, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators\n{body}"
    )
}

/// The fixtures of the table: path beneath the directory, and text. For each operator
/// one file of what it accepts and one per code it refuses with.
fn generated() -> BTreeMap<String, String> {
    let mut groups: BTreeMap<String, Vec<Line>> = BTreeMap::new();
    let mut add = |name: &str, line: Line| {
        let directory = line.expected.map_or("pass", |(code, _)| &code[..4]);
        groups
            .entry(format!("{directory}/{name}.js"))
            .or_default()
            .push(line);
    };
    for (name, operator, row) in BINARY {
        for (left, left_text) in KINDS {
            for (right, right_text) in KINDS {
                let column = |at: At| match at {
                    At::Left => 0,
                    At::Operator => left_text.len() + 1,
                    At::Right => left_text.len() + operator.len() + 2,
                };
                let line = Line {
                    expression: format!("{left_text} {operator} {right_text}"),
                    expected: judged(row, left, right).map(|(code, at)| (code, column(at))),
                };
                add(name, line);
            }
        }
    }
    let unary = [
        ("negate", "-"),
        ("unary-plus", "+"),
        ("bit-not", "~"),
        ("not", "!"),
        ("typeof", "typeof "),
    ];
    for (name, operator) in unary {
        for (operand, text) in KINDS {
            let line = Line {
                expression: format!("{operator}{text}"),
                // What `!` refuses is the whole `!x`: it is what is written anew.
                expected: judged_one(operator, operand).map(|code| {
                    (
                        code,
                        if [E102, E104].contains(&code) {
                            0
                        } else {
                            operator.len()
                        },
                    )
                }),
            };
            add(name, line);
        }
    }
    for (operand, text) in KINDS {
        let line = Line {
            expression: format!("{text} ? 1 : 2"),
            expected: judged_one("?", operand).map(|code| (code, 0)),
        };
        add("conditional", line);
    }
    groups
        .into_iter()
        .map(|(path, lines)| (path, fixture(&lines)))
        .collect()
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
    let files = generated();
    let scratch = TempDir::new("us088").expect("a temporary directory");
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
        "the fixtures differ from the table; regenerate them with SJS_UPDATE_SNAPSHOTS=1: {differing:?}"
    );
}

#[test]
fn every_operator_has_what_it_accepts_and_what_it_refuses() {
    let files = generated();
    let text = |path: &str| files.get(path).cloned().unwrap_or_default();
    for (name, _, _) in BINARY {
        assert!(
            files.contains_key(&format!("pass/{name}.js")),
            "{name} accepts nothing"
        );
        let refused = ["E138", "E104", "E105", "E121"]
            .iter()
            .any(|code| files.contains_key(&format!("{code}/{name}.js")));
        assert!(refused, "{name} refuses nothing");
    }
    assert!(
        text("pass/plus.js").contains("= \"a\" + \"a\"\n")
            && !text("pass/minus.js").contains("\"a\"")
    );
    assert!(
        text("E138/plus.js").contains("= \"a\" + 1\n")
            && text("E138/plus.js").contains("= 1 + 2n\n")
    );
    assert!(
        text("pass/bit-and.js").contains("= 1 & 1\n")
            && text("E104/bit-and.js").contains("= 1.5 & 1\n")
    );
    assert!(text("E138/unsigned-shift.js").contains("= 2n >>> 2n\n"));
    assert!(
        text("E105/equal.js").contains("= 1.5 === undefined\n")
            && text("pass/equal.js").contains("= 1 === 1\n")
    );
    assert!(
        text("E138/negate.js").contains("= -\"a\"\n")
            && text("pass/typeof.js").contains("= typeof undefined\n")
    );
    assert!(!files.contains_key("E138/typeof.js") && files.contains_key("E102/conditional.js"));
    assert!(text("E121/conditional.js").contains("= true ? 1 : 2\n"));
    assert!(text("E102/not.js").starts_with("// expect: E102 ambiguous-truthiness @8:21\n"));
    assert!(text("E121/nullish.js").contains("= 1 ?? undefined\n"));
    assert!(text("E120/equal.js").contains("= undefined === undefined\n"));
    assert!(text("E120/less.js").starts_with("// expect: E120 self-assign-or-compare @4:21\n"));
    assert_eq!(
        text("E104/bit-not.js"),
        "// expect: E104 bitwise-on-double @4:21\n// Generated from the table of SPEC.md §4.1, not written by hand:\n// SJS_UPDATE_SNAPSHOTS=1 cargo test -p sjs-cli --test operators\nexport const use1 = ~1.5\n"
    );
    let first = text("E138/and.js");
    assert!(
        first.starts_with("// expect: E138 type-mismatch @"),
        "{first}"
    );
}
