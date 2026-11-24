//! STORIES.md US-114: exhaustiveness (SPEC.md §3.5, §4.3).

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Printer, Types};

/// The names every test may use: two flags, a value that is one of three words, one
/// that is one of two shapes or a third that shares a `kind`, and values no list
/// holds.
const HEAD: &str = "const flag = Math.random() < 0.5\nconst other = Math.random() < 0.5\nconst word = flag ? \"a\" : other ? \"b\" : \"c\"\nconst shape = flag ? { kind: \"circle\", r: 1 } : other ? { kind: \"square\", s: 2 } : { kind: \"circle\", d: 3 }\nconst num = Math.random()\nconst str = `${num}`\nconst sign = flag ? -1 : 1\nconst mixed = flag ? str : other ? num : undefined\n";

struct Checked {
    types: Vec<(String, String)>,
    /// `CODE @line:column`, the lines counted from the first after the head, with
    /// ` -> replacement` when the finding has one.
    found: Vec<String>,
    messages: Vec<String>,
}

fn checked(body: &str) -> Checked {
    let text = format!("{HEAD}{body}");
    let lines = u32::try_from(HEAD.lines().count()).expect("few lines");
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let name = |class: ClassId| facts.class_names.get(&class).cloned().unwrap_or_default();
    let printer = Printer::new(&types, &name);
    let declared = facts.declarations.iter();
    let types = declared
        .filter_map(|(&at, &ty)| {
            Some((
                bindings.declarations.get(at)?.name.clone(),
                printer.print(ty),
            ))
        })
        .collect();
    let mut sorted: Vec<_> = diagnostics.iter().collect();
    sorted.sort_by_key(|d| d.span.byte_start);
    let found = sorted
        .iter()
        .map(|d| {
            let fix = d.replacement.as_ref().map(|fix| format!(" -> {fix}"));
            format!(
                "{:?} @{}:{}{}",
                d.code,
                d.span.start.line - lines,
                d.span.start.column,
                fix.unwrap_or_default()
            )
        })
        .collect();
    let messages = sorted.iter().map(|d| d.message.clone()).collect();
    Checked {
        types,
        found,
        messages,
    }
}

fn found(body: &str) -> Vec<String> {
    checked(body).found
}

const NONE: [&str; 0] = [];

/// A `switch` on `subject` with one clause for each group of stacked labels; each
/// clause calls `use` and breaks.
fn switch(subject: &str, clauses: &[&[&str]]) -> String {
    let mut clauses_text = Vec::new();
    for labels in clauses {
        let stacked: Vec<String> = labels
            .iter()
            .map(|label| match *label {
                "default" => "  default:".to_owned(),
                value => format!("  case {value}:"),
            })
            .collect();
        clauses_text.push(format!(
            "{} {{\n    num.toFixed(0)\n    break\n  }}\n",
            stacked.join("\n")
        ));
    }
    format!("switch ({subject}) {{\n{}}}\n", clauses_text.concat())
}

#[test]
fn a_switch_names_every_member_or_has_a_default() {
    let c = checked(&switch("word", &[&["\"a\""], &["\"b\""]]));
    assert_eq!(c.found, ["E103 @1:9"]);
    assert_eq!(
        c.messages,
        ["this `switch` names no `case` for `\"c\"`: name each, or add a `default` (SPEC.md §3.5)"]
    );
    assert_eq!(
        found(&switch("word", &[&["\"a\""], &["\"b\""], &["\"c\""]])),
        NONE
    );
    assert_eq!(found(&switch("word", &[&["\"a\""], &["default"]])), NONE);
    // Stacked labels name what each is.
    assert_eq!(
        found(&switch("word", &[&["\"a\"", "\"b\"", "\"c\""]])),
        NONE
    );
    assert_eq!(
        found(&switch("word", &[&["\"a\"", "\"b\""]])),
        ["E103 @1:9"]
    );
    // A boolean is one of two.
    assert_eq!(found(&switch("flag", &[&["true"]])), ["E103 @1:9"]);
    assert_eq!(found(&switch("flag", &[&["true"], &["false"]])), NONE);
    // Signed numbers.
    assert_eq!(found(&switch("sign", &[&["-1"], &["1"]])), NONE);
    assert_eq!(found(&switch("sign", &[&["1"]])), ["E103 @1:9"]);
    // What `typeof` gives is one of a list too.
    assert_eq!(
        found(&switch("typeof mixed", &[&["\"string\""], &["\"number\""]])),
        ["E103 @1:9"]
    );
    assert_eq!(
        found(&switch(
            "typeof mixed",
            &[&["\"string\"", "\"number\"", "\"undefined\""]]
        )),
        NONE
    );
}

#[test]
fn a_discriminant_is_one_of_a_list() {
    let c = checked(&switch("shape.kind", &[&["\"circle\""]]));
    assert_eq!(c.found, ["E103 @1:9"]);
    assert!(
        c.messages[0].contains("for `\"square\"`"),
        "{:?}",
        c.messages
    );
    assert_eq!(
        found(&switch("shape.kind", &[&["\"circle\""], &["\"square\""]])),
        NONE,
        "two shapes share a `kind`: one `case` names both"
    );
}

#[test]
fn a_default_nothing_is_left_for_is_never_reached() {
    let c = checked(&switch(
        "word",
        &[&["\"a\"", "\"b\""], &["\"c\""], &["default"]],
    ));
    assert_eq!(c.found, ["E117 @11:3"]);
    assert_eq!(
        c.messages,
        ["this `default` is never reached: every value is named by a `case` (SPEC.md §3.5)"]
    );
    assert_eq!(
        found(&switch("flag", &[&["default"], &["true"], &["false"]])),
        ["E117 @2:3"]
    );
    assert_eq!(found(&switch("str", &[&["\"a\""], &["default"]])), NONE);
}

#[test]
fn a_case_the_subject_is_never_is_refused() {
    let c = checked(&switch(
        "word",
        &[&["\"a\"", "\"z\""], &["1"], &["default"]],
    ));
    assert_eq!(c.found, ["E105 @3:8", "E105 @7:8"]);
    assert_eq!(
        c.messages[0],
        "a `\"a\" | \"b\" | \"c\"` is never `\"z\"`: this `case` is never taken (SPEC.md §4.1)"
    );
    assert_eq!(
        found(&switch("num", &[&["\"a\""], &["default"]])),
        ["E105 @2:8", "E117 @3:5"],
        "and what it guards is never reached"
    );
    assert_eq!(
        found(&switch("sign", &[&["-1"], &["1"], &["-2"]])),
        ["E105 @10:8"]
    );
}

#[test]
fn a_value_no_list_holds_needs_a_default() {
    let c = checked(&switch("str", &[&["\"a\""]]));
    assert_eq!(c.found, ["E103 @1:9"]);
    assert_eq!(
        c.messages,
        ["a `string` has more values than its `case`s name: add a `default` (SPEC.md §4.3)"]
    );
    assert_eq!(found(&switch("num", &[&["1"], &["2"]])), ["E103 @1:9"]);
    assert_eq!(found(&switch("num", &[&["1"], &["default"]])), NONE);
    assert_eq!(found(&switch("mixed", &[&["undefined"]])), ["E103 @1:9"]);
    // What has no type yet is not judged.
    let untyped = format!(
        "import {{ later }} from \"./later.js\"\n{}",
        switch("later()", &[&["1"]])
    );
    assert_eq!(found(&untyped), NONE);
    assert_eq!(found("switch () {\n}\n"), NONE);
}

#[test]
fn nothing_is_left_after_a_switch_that_names_every_member_and_leaves() {
    let text = "function f() {\n  switch (word) {\n    case \"a\": {\n      return 1\n    }\n    case \"b\":\n    case \"c\": {\n      return 2\n    }\n  }\n}\nfunction g() {\n  switch (word) {\n    case \"a\": {\n      return 1\n    }\n    case \"b\": {\n      return 2\n    }\n    case \"c\": {\n      throw new Error(word)\n    }\n  }\n  num.toFixed(0)\n  if (flag) {\n    str.at(word.length)\n    str.at(word.length)\n  }\n  switch (word) {\n    case \"a\": {\n      return 3\n    }\n  }\n  return word\n}\nexport const r = [f(), g()]\n";
    let c = checked(text);
    assert_eq!(
        c.found,
        ["E117 @26:12", "E117 @29:11"],
        "a function that ends in such a `switch` returns on every path; a use of what is spent is never reached, once for each block"
    );
    assert_eq!(
        c.messages[0],
        "`word` is never reached here: every value it may be has been dealt with (SPEC.md §3.5)"
    );
    let of = |name: &str| c.types.iter().find(|(declared, _)| declared == name);
    assert_eq!(of("f").map(|(_, ty)| ty.as_str()), Some("(): int32"));
}

#[test]
fn a_chain_whose_branches_all_leave_is_a_switch_written_out() {
    let c = checked("function f() {\n  if (word === \"a\") {\n    return 1\n  } else if (word === \"b\") {\n    throw new Error(word)\n  }\n}\nexport const r = f()\n");
    assert_eq!(c.found, ["E103 @2:3", "E111 @3:5"]);
    assert_eq!(
        c.messages[0],
        "this chain has no branch for `\"c\"`: add one, or an `else` (SPEC.md §3.5)"
    );
    // Every member tested for: nothing runs past it.
    let text = "function f() {\n  if (word === \"a\") {\n    return 1\n  } else if (word === \"b\") {\n    throw new Error(word)\n  } else if (word === \"c\") {\n    return 3\n  }\n}\nfunction g() {\n  if (shape.kind === \"circle\") {\n    return 1\n  } else if (shape.kind === \"square\") {\n    return 2\n  }\n  return shape.kind\n}\nexport const r = [f(), g()]\n";
    let c = checked(text);
    assert_eq!(c.found, ["E117 @16:10"]);
    let of = |name: &str| c.types.iter().find(|(declared, _)| declared == name);
    assert_eq!(of("f").map(|(_, ty)| ty.as_str()), Some("(): int32"));
    // A boolean asked for its truth, then for the other answer.
    let c = checked("function f() {\n  if (flag) {\n    return 1\n  } else if (!flag) {\n    return 2\n  }\n}\nexport const r = f()\n");
    assert_eq!(c.found, NONE);
    let of = |name: &str| c.types.iter().find(|(declared, _)| declared == name);
    assert_eq!(of("f").map(|(_, ty)| ty.as_str()), Some("(): int32"));
    // A test that closes a chain is no constant condition; with an `else` after
    // it, it is. What is tested through a `const` is written nowhere in the test.
    assert_eq!(
        found("function f() {\n  if (flag) {\n    return 1\n  } else if (!flag) {\n    return 2\n  } else {\n    return 3\n  }\n}\nfunction g() {\n  const kind = typeof mixed\n  if (kind === \"string\") {\n    return 1\n  } else if (kind === \"number\") {\n    return 2\n  }\n}\nexport const r = [f(), g()]\n"),
        ["E121 @4:14", "E111 @13:5"]
    );
    // A value no list holds, and tests that are no comparison.
    assert_eq!(
        found("function f() {\n  if (typeof mixed === \"string\") {\n    return 1\n  } else if (typeof mixed === \"number\" || mixed === undefined) {\n    return 2\n  }\n}\nfunction g() {\n  if (str === \"a\") {\n    return 1\n  } else if (str === \"b\") {\n    return 2\n  }\n}\nfunction h() {\n  if (typeof mixed === \"string\") {\n    return 1\n  } else if (typeof mixed === \"number\") {\n    return 2\n  }\n}\nexport const r = [f(), g(), h()]\n"),
        ["E103 @9:3", "E111 @10:5", "E103 @16:3", "E111 @17:5"]
    );
}

#[test]
fn a_chain_that_is_no_switch_is_not_judged() {
    // What follows it is what the rest is for; an `else` is; one branch is no
    // chain; a branch that goes on leaves the rest to what follows.
    assert_eq!(
        found("function f() {\n  if (word === \"a\") {\n    return 1\n  } else if (word === \"b\") {\n    return 2\n  }\n  return 3\n}\nfunction g() {\n  if (word === \"a\") {\n    return 1\n  } else if (word === \"b\") {\n    return 2\n  } else {\n    return 3\n  }\n}\nfunction h() {\n  if (word === \"a\") {\n    return 1\n  }\n  return 2\n}\nfunction i() {\n  if (word === \"a\") {\n    num.toFixed(0)\n  } else if (word === \"b\") {\n    return\n  }\n}\nfunction j() {\n  if (word === \"a\") {\n    return\n  } else if (flag) {\n    return\n  }\n}\nfunction k() {\n  if (word === \"a\") {\n    return\n  } else if (later()) {\n    return\n  }\n  function later() {\n    return other\n  }\n}\nexport const r = [f(), g(), h(), i(), j(), k()]\n"),
        NONE
    );
    assert_eq!(
        found("if (word === \"a\") {\n  num.toFixed(0)\n} else if (word === \"b\") {\n  num.toFixed(1)\n}\n"),
        NONE
    );
}
