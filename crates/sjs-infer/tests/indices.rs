//! STORIES.md US-115: array index proofs (SPEC.md §4.6).

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Printer, Types};

struct Checked {
    /// `name: type` of each declaration that has a type.
    types: Vec<(String, String)>,
    /// `CODE @line:column` of each finding.
    found: Vec<String>,
    messages: Vec<String>,
}

impl Checked {
    /// The type of the declaration `name`: `?` for one that has none.
    fn of(&self, name: &str) -> &str {
        let found = self.types.iter().find(|(declared, _)| declared == name);
        found.map_or("?", |(_, ty)| ty.as_str())
    }
}

fn checked(text: &str) -> Checked {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let name = |class: ClassId| facts.class_names.get(&class).cloned().unwrap_or_default();
    let printer = Printer::new(&types, &name);
    let declared = facts.declarations.iter();
    // What the ambient library declares is no declaration of the file.
    let types = declared
        .filter_map(|(&at, &ty)| {
            Some((
                bindings.declarations.get(at)?.name.clone(),
                printer.print(ty),
            ))
        })
        .collect();
    let found = diagnostics
        .iter()
        .map(|d| {
            format!(
                "{:?} @{}:{}",
                d.code, d.span.start.line, d.span.start.column
            )
        })
        .collect();
    let messages = diagnostics.iter().map(|d| d.message.clone()).collect();
    Checked {
        types,
        found,
        messages,
    }
}

const NONE: [&str; 0] = [];

/// A function of an array of numbers `xs`, another `ys` and a number `i`, with
/// this body, called once.
fn of_array(body: &str) -> String {
    format!("function f(xs, ys, i) {{\n  let total = 0\n{body}  return total\n}}\nexport const r = f([1.5, 2.5], [3.5], 1)\n")
}

fn found(body: &str) -> Vec<String> {
    checked(&of_array(body)).found
}

#[test]
fn the_variable_of_a_loop_over_the_array_is_in_it() {
    let c = checked(&of_array("  for (let j = 0; j < xs.length; j += 1) {\n    const a = xs[j]\n    total += xs[j]\n  }\n  let k = 0\n  while (k < xs.length) {\n    total += xs[k] * 2\n    k += 2\n  }\n  for (let m = 0; m < xs.length; m++) {\n    total += xs[m]\n  }\n"));
    assert_eq!(c.found, NONE);
    assert_eq!(c.of("a"), "number", "an element, and not `undefined`");
    // Not `<=`, not another array, not a variable that may be negative.
    assert_eq!(
        found("  for (let j = 0; j <= xs.length; j += 1) {\n    total += xs[j]\n  }\n  for (let j = 0; j < ys.length; j += 1) {\n    total += xs[j]\n  }\n  for (let j = -1; j < xs.length; j += 1) {\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; j -= 1) {\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; j += i) {\n    total += xs[j]\n  }\n"),
        ["E124 @4:14", "E124 @7:14", "E124 @10:14", "E124 @13:14", "E124 @16:14"]
    );
}

#[test]
fn a_test_that_comes_before_every_read_proves_the_place() {
    assert_eq!(
        found("  if (i >= 0 && i < xs.length) {\n    total += xs[i]\n  }\n  if (0 <= i && xs.length > i) {\n    total += xs[i]\n  }\n  if (i < 0 || i >= xs.length) {\n    return 0\n  }\n  total += xs[i]\n  if (i > 0 && !(xs.length <= i)) {\n    total += xs[i]\n  }\n"),
        NONE
    );
    assert_eq!(
        found("  if (i < xs.length) {\n    total += xs[i]\n  }\n  if (i >= 0) {\n    total += xs[i]\n  }\n  if (i >= 0 || i < xs.length) {\n    total += xs[i]\n  }\n  if (flagged(i) && i < xs.length) {\n    total += xs[i]\n  }\n  function flagged(n) {\n    return n >= 0\n  }\n"),
        ["E124 @4:14", "E124 @7:14", "E124 @10:14", "E124 @13:14"],
        "one half is no proof, nor one way of two, nor what a function asked"
    );
    // A place written as a number, and a `const` that is one.
    assert_eq!(
        found("  const second = 1\n  if (xs.length > 0) {\n    total += xs[0]\n  }\n  if (xs.length >= 2 && 3 <= ys.length) {\n    total += xs[1] + xs[second] + ys[2]\n  }\n  if (xs.length === 0 || ys.length !== 1) {\n    return 0\n  }\n  total += xs[0] + ys[0]\n  if (0 === xs.length) {\n    return 1\n  }\n  if (2 < xs.length) {\n    total += xs[2]\n  }\n"),
        NONE
    );
    assert_eq!(
        found("  if (xs.length > 0) {\n    total += xs[1]\n  }\n  if (xs.length >= 1) {\n    total += xs[1]\n  }\n  if (xs.length !== 2) {\n    total += xs[0]\n  }\n  if (xs.length === 1) {\n    total += xs[1] + ys[0]\n  }\n  if (xs.length !== 0) {\n    total += xs[1]\n  }\n  if (1.5 < xs.length && xs.length > i) {\n    total += xs[1]\n  }\n  if (xs.length > total) {\n    total += xs[0]\n  }\n"),
        [
            "E124 @4:14",
            "E124 @7:14",
            "E124 @10:14",
            "E124 @13:14",
            "E124 @13:22",
            "E124 @16:14",
            "E124 @19:14",
            "E124 @22:14"
        ]
    );
}

#[test]
fn what_changes_the_index_or_the_array_cancels_the_proof() {
    assert_eq!(
        found("  const far = 9.5\n  for (let j = 0; j < xs.length; j += 1) {\n    xs.push(1)\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; j += 1) {\n    xs.pop()\n    total += xs[j]\n  }\n  let zs = xs\n  for (let j = 0; j < zs.length; j += 1) {\n    zs = ys\n    total += zs[j]\n  }\n  for (let j = 0; j < xs.length; j += 1) {\n    j = far\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; j += 1) {\n    j += 1\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; j += 1) {\n    j--\n    total += xs[j]\n  }\n  for (let j = 0; j < xs.length; ++j) {\n    ++j\n    total += xs[j]\n  }\n"),
        [
            "E124 @6:14",
            "E124 @10:14",
            "E124 @15:14",
            "E124 @19:14",
            "E124 @23:14",
            "E124 @27:14",
            "E124 @31:14"
        ]
    );
    // An element written, another array mutated and a call that mutates nothing
    // cancel nothing; nor does an index set to a number again.
    assert_eq!(
        found("  for (let j = 0; j < xs.length; j += 1) {\n    xs[j] = total\n    ys.push(1)\n    Math.random()\n    total += xs[j]\n  }\n  let k = -1\n  k = 0\n  if (k < xs.length) {\n    total += xs[k]\n  }\n"),
        NONE
    );
}

#[test]
fn an_index_set_to_what_is_no_number_written_there_is_lost() {
    let c = checked(&of_array(
        "  let k = 0\n  k = -1\n  if (k < xs.length) {\n    total += xs[k]\n  }\n",
    ));
    assert_eq!(
        (c.found.clone(), c.of("k")),
        (vec!["E124 @6:14".to_owned()], "int32")
    );
    let c = checked(&of_array(
        "  let k = -1\n  k = 0\n  if (k < xs.length) {\n    total += xs[k]\n  }\n",
    ));
    assert_eq!((c.found.clone(), c.of("k")), (Vec::new(), "int32"));
}

#[test]
fn a_call_cancels_what_other_code_may_undo() {
    // An array that is mutated, and that a closure can reach or that is passed on.
    let text = "function drop(list) {\n  list.pop()\n}\nfunction f(xs, ys, zs, run) {\n  let total = 0\n  let j = 0\n  const bump = () => {\n    j += 1\n    ys.pop()\n  }\n  if (xs.length > 0 && ys.length > 0 && zs.length > 0 && j < zs.length) {\n    total += xs[0] + ys[0] + zs[0] + zs[j]\n    run(bump)\n    total += xs[0]\n    total += ys[0]\n    total += zs[j]\n    drop(zs)\n    total += zs[0]\n  }\n  xs.push(1)\n  return total\n}\nexport const r = f([1.5], [2.5], [3.5], (g) => g())\n";
    assert_eq!(
        checked(text).found,
        ["E124 @15:14", "E124 @16:14", "E124 @18:14"],
        "a mutated array no other code reaches keeps its proof"
    );
}

#[test]
fn a_tuple_read_by_a_number_in_range_is_that_element() {
    let c = checked("const pair = [1.5, \"a\"]\nconst a = pair[0]\nconst [b, d] = pair\nconst e = a + b\nconst g = d.length\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        [c.of("a"), c.of("b"), c.of("d")],
        ["1.5", "number", "string"]
    );
}

#[test]
fn an_unproven_read_is_the_element_or_nothing() {
    let c = checked(&of_array("  const a = xs[i]\n  if (a !== undefined) {\n    total += a\n  }\n  total += xs[i] ?? 0\n  const b = xs[i]?.toFixed(0)\n  const d = [xs[i], (xs[i])]\n  const e = { x: xs[i] }\n  const g = typeof xs[i] === \"number\" || !xs[i]\n  const h = xs[i] === undefined ? 0 : 1\n  ys.push(xs[i])\n  xs[i] = 1\n  xs[i]\n  if (xs[i]) {\n    total += 1\n  }\n  while (xs[i]) {\n    break\n  }\n  const k = () => xs[i]\n  const m = xs?.[i]\n  const n = new Set([xs[i]])\n  const o = flag(xs[i]) ? xs[i] : 2\n  function flag(v) {\n    return v !== undefined\n  }\n"));
    assert_eq!(
        c.found,
        ["E102 @11:42", "E102 @16:7", "E102 @19:10", "E121 @23:15"],
        "kept, passed on, compared and asked: `undefined` goes where it may"
    );
    assert_eq!(
        [c.of("a"), c.of("b")],
        ["number | undefined", "string | undefined"]
    );
}

#[test]
fn an_unproven_read_used_as_the_element_is_refused() {
    let c = checked(&of_array("  total += xs[i]\n  const a = xs[i].toFixed(0)\n  const b = xs[i] * 2\n  const d = -xs[i]\n  const e = ys[xs[i]] ?? 0\n  const g = `${xs[i]}`\n  const h = xs[xs.length - 1] + xs[3]\n  xs[i] += 1\n  xs[i]++\n  ++ys[i]\n  const k = [...ys, xs[i]].length + [[1]][i][0]\n  const m = xs[i] < 1 ? (xs[i]).toFixed(1) : \"\"\n"));
    assert_eq!(
        c.found,
        [
            "E124 @3:12",
            "E124 @4:13",
            "E124 @5:13",
            "E124 @6:14",
            "E124 @7:16",
            "E124 @8:16",
            "E124 @9:13",
            "E124 @9:33",
            "E124 @10:3",
            "E124 @11:3",
            "E124 @12:5",
            "E124 @13:37",
            "E124 @13:37",
            "E124 @14:13",
            "E124 @14:26"
        ]
    );
    assert_eq!(
        [c.of("a"), c.of("b")],
        ["string", "number"],
        "once refused, it is read as the element: one finding, not two"
    );
}

#[test]
fn the_finding_names_the_test_to_add() {
    let c = checked(&of_array(
        "  total += xs[i]\n  total += xs[xs.length - 1]\n  total += xs[3]\n",
    ));
    assert_eq!(
        c.messages,
        [
            "`xs[i]` may be `undefined`: nothing shows that the place is in the array; test `i >= 0 && i < xs.length` before it, or compare what is read with `undefined` (SPEC.md §4.6)",
            "`xs[xs.length-1]` may be `undefined`: nothing shows that the place is in the array; read the last element with `xs.at(-1)` and compare it with `undefined` (SPEC.md §4.6)",
            "`xs[3]` may be `undefined`: nothing shows that the place is in the array; test `xs.length > 3` before it, or compare what is read with `undefined` (SPEC.md §4.6)"
        ]
    );
}
