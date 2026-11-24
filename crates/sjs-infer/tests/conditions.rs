//! STORIES.md US-112: truthiness, boolean conditions and constant conditions.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Printer, Types};

/// The names every test may use: a flag, values that may be absent, and values of
/// each kind that has a value that counts as false.
const HEAD: &str = "const flag = Math.random() < 0.5\nconst other = Math.random() < 0.5\nconst num = Math.random()\nconst str = `${num}`\nconst big = flag ? 1n : 2n\nconst xs = [num]\nconst shape = { size: num }\nconst maybeShape = flag ? shape : undefined\nconst maybeStr = flag ? str : undefined\nconst maybeA = flag ? \"a\" : undefined\nconst found = /a/v.exec(str)\nconst raw = JSON.parse(str)\n";

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

/// The types of the names given, of a body that has no finding.
fn types(body: &str, names: &[&str]) -> Vec<String> {
    let c = checked(body);
    assert_eq!(c.found, [""; 0], "{body}");
    let of = |name: &&str| c.types.iter().find(|(declared, _)| declared == name);
    names
        .iter()
        .map(|name| of(name).map_or("?".to_owned(), |(_, ty)| ty.clone()))
        .collect()
}

const NONE: [&str; 0] = [];

#[test]
fn a_value_that_may_be_false_is_not_asked_for_its_truth() {
    assert_eq!(
        found("if (xs.length) {\n  xs.at(0)\n}\n"),
        ["E102 @1:5 -> xs.length > 0"]
    );
    assert_eq!(
        found("if (maybeStr) {\n  xs.at(0)\n}\n"),
        ["E102 @1:5 -> maybeStr !== undefined"],
        "a string may be empty"
    );
    assert_eq!(
        found("const a = num ? 1 : 2\nconst b = str ? 1 : 2\nconst c = big ? 1 : 2\nconst d = found?.index ? 1 : 2\nconst e = xs.at(0) ? 1 : 2\nconst g = (flag ? found : 0) ? 1 : 2\n"),
        [
            "E102 @1:11 -> num !== 0",
            "E102 @2:11 -> str !== \"\"",
            "E102 @3:11 -> big !== 0n",
            "E102 @4:11",
            "E102 @5:11 -> xs.at(0) !== undefined",
            "E102 @6:11"
        ],
        "what is no name, member or call is not written anew"
    );
    assert_eq!(
        found("while (!xs.length) {\n  xs.at(0)\n  break\n}\nconst a = !num\nconst b = !maybeStr\nconst c = !str\n"),
        [
            "E102 @1:8 -> xs.length === 0",
            "E102 @5:11 -> num === 0",
            "E102 @6:11 -> maybeStr === undefined",
            "E102 @7:11 -> str === \"\""
        ]
    );
    // What is never false is not asked either, nor what is never anything.
    let c = checked("const a = shape ? 1 : 2\nconst b = raw ? 1 : 2\nconst c = undefined ? 1 : 2\nif (num) {\n  xs.at(0)\n}\n");
    assert_eq!(
        c.found,
        [
            "E102 @1:11",
            "E102 @2:11",
            "E102 @3:11",
            "E102 @4:5 -> num !== 0"
        ]
    );
    assert_eq!(
        [&c.messages[0], &c.messages[3]],
        [
            "a `{ size: number }` is asked for its truth: a condition is a boolean, or a value that is `undefined` or else never false; compare it with the value it is to be told from (SPEC.md §3.5)",
            "a `number` is asked for its truth: a condition is a boolean, or a value that is `undefined` or else never false; write `num !== 0` (SPEC.md §3.5)"
        ]
    );
    // A literal that counts as false is a value that may be false; one that does
    // not is never false.
    assert_eq!(
        found("const a = flag ? 0n : undefined\nconst b = flag ? \"\" : undefined\nconst c = flag ? false : undefined\nconst d = flag ? 0 : undefined\nconst e = flag ? 1n : undefined\nconst g = flag ? \"a\" : undefined\nconst h = flag ? true : undefined\nconst i = flag ? -1 : undefined\nconst all = [a ? 1 : 2, b ? 1 : 2, c ? 1 : 2, d ? 1 : 2, e ? 1 : 2, g ? 1 : 2, h ? 1 : 2, i ? 1 : 2]\n"),
        [
            "E102 @9:14 -> a !== undefined",
            "E102 @9:25 -> b !== undefined",
            "E102 @9:36 -> c !== undefined",
            "E102 @9:47 -> d !== undefined"
        ]
    );
    let text = "const m = flag ? found : 0\nconst a = m ? 1 : 2\n";
    assert_eq!(found(text), ["E102 @2:11 -> m !== null"]);
}

#[test]
fn a_value_that_is_absent_or_else_never_false_is_asked_and_narrowed() {
    assert_eq!(
        types(
            "function f() {\n  const box = { v: maybeShape }\n  if (box.v) {\n    const m = box.v\n  }\n  const n = (maybeShape) ? maybeShape : shape\n  const e = maybeShape && maybeShape.size > 0 ? 1 : 2\n  if (maybeShape) {\n    const a = maybeShape\n    return a.size\n  }\n  const b = maybeShape\n  if (!maybeA) {\n    return e\n  }\n  const c = maybeA\n  const d = found ? found.index : 0\n  return d\n}\nexport const r = f()\n",
            &["a", "b", "c", "d", "e", "m", "n"]
        ),
        [
            "{ size: number }",
            "undefined",
            "\"a\"",
            "int32",
            "1 | 2",
            "{ size: number }",
            "{ size: number }"
        ]
    );
    // A boolean is narrowed to what was asked of it; a value that may be false
    // keeps what is not false of it.
    assert_eq!(
        types(
            "function f(n) {\n  const seen = typeof n\n  if (flag) {\n    const a = flag\n  } else {\n    const b = flag\n  }\n  return n\n}\nf(0)\nf(\"\")\nf(undefined)\n",
            &["a", "b"]
        ),
        ["true", "false"]
    );
    let c = checked("const v = flag ? 0 : other ? \"a\" : other ? shape : undefined\nconst a = v ? v : 1\nconst b = v ? 1 : v\n");
    let of = |name: &str| c.types.iter().find(|(declared, _)| declared == name);
    assert_eq!(
        ["a", "b"].map(|name| of(name).map(|(_, ty)| ty.as_str())),
        [
            Some("1 | \"a\" | { size: number }"),
            Some("0 | 1 | undefined")
        ]
    );
}

#[test]
fn in_a_condition_each_side_of_a_logical_operator_is_asked() {
    assert_eq!(
        found("if (flag && (maybeShape || other)) {\n  xs.at(0)\n}\nif (flag && maybeShape) {\n  xs.at(0)\n}\nif (!(maybeShape || flag)) {\n  xs.at(0)\n}\nconst a = (maybeShape && flag) ? 1 : 2\nwhile ((flag) || (maybeShape)) {\n  break\n}\n"),
        NONE
    );
    assert_eq!(
        found("if (num || flag) {\n  xs.at(0)\n}\nif (flag && str) {\n  xs.at(0)\n}\n"),
        ["E102 @1:5 -> num !== 0", "E102 @4:13 -> str !== \"\""]
    );
    // As a value it is one of its sides: both are booleans.
    assert_eq!(
        found("const a = maybeShape && flag\nconst b = flag ? flag || maybeShape : flag\nconst c = typeof (maybeShape && flag)\nconst d = [maybeShape || flag]\nif (flag) {\n  const e = [maybeShape || other]\n}\n"),
        ["E138 @1:11", "E138 @2:26", "E138 @3:19", "E138 @4:12", "E138 @6:14"]
    );
}

#[test]
fn a_condition_that_is_always_the_same_is_refused() {
    assert_eq!(
        found("if (true) {\n  xs.at(0)\n}\nconst off = false\nif (off) {\n  xs.at(0)\n}\nconst a = !off ? 1 : 2\nconst b = (off) ? 1 : 2\nwhile (false) {\n  xs.at(0)\n}\n"),
        ["E121 @1:5", "E121 @5:5", "E121 @8:11", "E121 @9:11", "E121 @10:8"]
    );
    let c = checked("if (true) {\n  xs.at(0)\n}\nwhile (true) {\n  xs.at(0)\n}\n");
    assert_eq!(
        c.messages,
        [
            "this condition is always true: what it guards runs always or never (SPEC.md §8.1)",
            "this loop never ends: its body has no `break`, `return` or `throw` (SPEC.md §4.3)"
        ]
    );
    // What a narrowing has settled.
    assert_eq!(
        found("function f() {\n  if (flag) {\n    return flag ? 1 : 2\n  }\n  if (!flag) {\n    return 3\n  }\n  if (typeof maybeStr === \"string\") {\n    return typeof maybeStr === \"string\" ? 4 : 5\n  }\n  return maybeStr !== undefined ? 6 : 7\n}\nexport const r = f()\n"),
        ["E121 @3:12", "E121 @5:7", "E121 @9:12", "E121 @11:10"]
    );
    assert_eq!(
        found("const a = typeof num === \"number\" ? 1 : 2\nconst b = flag === other ? 1 : 2\nconst c = num === 1 ? 1 : 2\n"),
        ["E121 @1:11"]
    );
    // In parentheses too; `typeof` and `&&` settle nothing, and two values that
    // differ are E105's.
    assert_eq!(
        found("const off = false\nconst a = (typeof num === \"number\") ? 1 : 2\nconst b = typeof off ? 1 : 2\nconst c = off && off ? 1 : 2\nconst d = off === true ? 1 : 2\nconst e = off || off ? 1 : 2\n"),
        ["E121 @2:11", "E102 @3:11", "E105 @5:15"]
    );
    let c = checked("const a = typeof num === \"number\" ? 1 : 2\nconst b = typeof num !== \"number\" ? 1 : 2\n");
    assert_eq!(
        c.messages.iter().map(|m| &m[..30]).collect::<Vec<_>>(),
        [
            "this condition is always true:",
            "this condition is always false"
        ]
    );
    let c =
        checked("const off = false\nwhile (!off) {\n  xs.at(0)\n}\nwhile (!off) {\n  break\n}\n");
    assert_eq!(
        (c.found, c.messages.concat().contains("never ends")),
        (vec!["E121 @2:8".to_owned()], true)
    );
    assert_eq!(
        found("const a = num ?? 1\nconst b = maybeStr ?? \"\"\nconst c = shape?.size\n"),
        ["E121 @1:11", "E121 @3:16"]
    );
}

#[test]
fn a_loop_without_end_has_a_way_out() {
    assert_eq!(
        found("function f() {\n  while (true) {\n    if (flag) {\n      break\n    }\n  }\n  while (true) {\n    for (const x of xs) {\n      if (x > 0) {\n        throw new Error(\"some\")\n      }\n    }\n  }\n}\nfunction g() {\n  while (true) {\n    switch (num) {\n      case 1: {\n        throw new Error(\"one\")\n      }\n      default: {\n        xs.at(0)\n      }\n    }\n  }\n}\nf()\ng()\n"),
        NONE
    );
    assert_eq!(
        found("while (true) {\n  switch (num) {\n    case 1: {\n      break\n    }\n    default: {\n      xs.at(0)\n    }\n  }\n}\n"),
        ["E121 @1:8"],
        "the `break` is the `switch`'s"
    );
    assert_eq!(
        found("while (true) {\n  for (const x of xs) {\n    break\n  }\n  while (flag) {\n    break\n  }\n  xs.map((x) => {\n    return x\n  })\n}\n"),
        ["E121 @1:8"],
        "the `break` of a loop inside, and the `return` of an arrow, leave something else"
    );
    assert_eq!(
        found("const on = true\nwhile (on) {\n  if (flag) {\n    break\n  }\n}\n"),
        NONE,
        "a `const` that is `true` is the same loop"
    );
    // A function that ends in such a loop does not run off its end, unless the loop
    // is left by a `break`.
    assert_eq!(
        found("function f() {\n  while (true) {\n    if (flag) {\n      return 1\n    }\n  }\n}\nfunction g() {\n  while (true) {\n    if (flag) {\n      return 1\n    }\n    if (other) {\n      break\n    }\n  }\n}\nfunction h() {\n  while (true && flag) {\n    return 1\n  }\n}\nexport const r = [f(), g(), h()]\n"),
        ["E111 @11:7", "E111 @20:5"]
    );
}
