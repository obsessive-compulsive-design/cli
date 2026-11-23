//! STORIES.md US-111: narrowing (SPEC.md §3.5).

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
    Checked { types, found }
}

/// The types of the names given, in order.
fn types(text: &str, names: &[&str]) -> Vec<String> {
    let c = checked(text);
    assert_eq!(c.found, [""; 0], "{text}");
    names.iter().map(|name| c.of(name).to_owned()).collect()
}

/// A function of one parameter `x` that is each of `values` in turn.
fn taking(values: &[&str], body: &str) -> String {
    let calls: Vec<String> = values.iter().map(|value| format!("f({value})\n")).collect();
    // What is asked its `typeof` is no type parameter.
    format!(
        "function f(x) {{\n  const seen = typeof x\n{body}}}\n{}",
        calls.concat()
    )
}

#[test]
fn typeof_narrows_to_the_kind_and_removes_it() {
    let text = taking(
        &["1", "\"a\"", "1n", "true", "undefined", "() => 1", "/a/v.exec(\"a\")"],
        "  if (typeof x === \"string\") {\n    const a = x\n  } else {\n    const b = x\n  }\n  if (typeof x !== \"number\") {\n    const c = x\n  } else {\n    const d = x\n  }\n  if (typeof x === \"object\") {\n    const e = x\n  }\n  if (typeof x === \"function\") {\n    const g = x\n  }\n  if (typeof x === \"undefined\" || typeof x === \"boolean\" || typeof x === \"bigint\") {\n    const h = x\n  }\n",
    );
    let shape = "{ \"0\": string, groups: undefined, index: int32, input: string }";
    assert_eq!(
        types(&text, &["a", "b", "c", "d", "e", "g", "h"]),
        [
            "string".to_owned(),
            format!("int32 | bigint | boolean | {shape} | ((): int32) | null | undefined"),
            format!("bigint | string | boolean | {shape} | ((): int32) | null | undefined"),
            "int32".to_owned(),
            format!("{shape} | null"),
            "(): int32".to_owned(),
            "bigint | boolean | undefined".to_owned(),
        ],
        "`typeof x === \"object\"` leaves `null`"
    );
    // Of what may be anything, the test says what it is when a type says that.
    let text = "const x = JSON.parse(\"1\")\nconst a = typeof x === \"string\" ? x : \"\"\nconst b = typeof x === \"number\" ? x : 0\nconst c = typeof x === \"object\" ? x : 0\nconst d = typeof x === \"string\" ? 0 : x\nconst e = typeof x === \"bigint\" ? x : 0n\nconst g = typeof x === \"boolean\" ? x : false\nconst h = typeof x === \"undefined\" ? x : 0\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d", "e", "g", "h"]),
        [
            "string",
            "number",
            "unknown",
            "unknown",
            "bigint",
            "boolean",
            "0 | undefined"
        ]
    );
    // Only `typeof` asks the kind: `!flag` is no `typeof flag`.
    let c = checked(
        "const flag = Math.random() < 0.5\nconst z = !flag === \"string\" ? flag : false\n",
    );
    assert_eq!((c.of("z"), c.found.len()), ("boolean", 1));
}

#[test]
fn equality_narrows_to_the_value_and_removes_it() {
    let text = taking(
        &["\"a\"", "undefined"],
        "  if (x === undefined) {\n    const a = x\n  } else {\n    const b = x\n  }\n  if (x !== undefined) {\n    const c = x\n  }\n  if (undefined === x) {\n    return 0\n  }\n  const d = x\n  return 1\n",
    );
    assert_eq!(
        types(&text, &["a", "b", "c", "d"]),
        ["undefined", "string", "string", "string"]
    );
    let text = "const m = /a/v.exec(\"a\")\nconst a = m === null ? 0 : m.index\nconst b = m !== null ? m : 0\nconst c = m === null ? m : 0\n";
    assert_eq!(types(text, &["a", "c"]), ["int32", "0 | null"]);
    // A literal: the value itself, and what is left without it.
    let text = "const flag = Math.random() < 0.5\nconst v = flag ? \"a\" : flag ? 2 : false\nconst a = v === \"a\" ? v : \"\"\nconst b = v === \"a\" ? 0 : v\nconst c = v !== 2 ? v : 0\nconst d = flag === true ? 1 : flag\nconst e = flag !== false ? flag : 1\nconst g = v !== false ? v : 0\nconst h = (v) === 2 ? 0 : v\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d", "e", "g", "h"]),
        [
            "\"\" | \"a\"",
            "0 | 2 | false",
            "0 | \"a\" | false",
            "1 | false",
            "1 | true",
            "0 | 2 | \"a\"",
            "0 | \"a\" | false"
        ]
    );
    // A test that is no branch tells nothing, and `?.` is no path of keys.
    let text = "const flag = Math.random() < 0.5\nconst p = flag ? \"a\" : undefined\nconst pair = [p === undefined, p ?? \"b\"]\nconst z = p\nconst o = flag ? { v: p } : undefined\nconst q = o?.v === undefined ? 0 : o?.v\n";
    assert_eq!(
        types(text, &["z", "q"]),
        ["\"a\" | undefined", "0 | \"a\" | undefined"]
    );
    // A primitive is the literal it is compared with, when it is that.
    let text = taking(
        &["\"a\"", "\"b\"", "1"],
        "  if (x === \"a\") {\n    const a = x\n  } else {\n    const b = x\n  }\n",
    );
    assert_eq!(types(&text, &["a", "b"]), ["\"a\"", "int32 | string"]);
}

#[test]
fn two_names_compared_are_both_what_they_share() {
    let text = "function f(x, y) {\n  const seen = [typeof x, typeof y]\n  if (x === y) {\n    const a = x\n    const b = y\n  } else {\n    const c = x\n  }\n  return x\n}\nf(1, \"s\")\nf(\"t\", undefined)\nf(true, \"u\")\n";
    assert_eq!(
        types(text, &["a", "b", "c"]),
        ["string", "string", "int32 | string | boolean"],
        "what is not equal says nothing"
    );
    // What may be anything is what the other is.
    let text = "const raw = JSON.parse(\"1\")\nfunction f(x) {\n  const seen = typeof x\n  if (raw === x) {\n    const a = raw\n    const b = x\n  }\n  return x\n}\nf(1)\nf(\"s\")\n";
    assert_eq!(
        types(text, &["a", "b"]),
        ["int32 | string", "int32 | string"]
    );
}

#[test]
fn a_discriminant_tells_the_members_of_a_union_apart() {
    let head = "function r(n) {\n  if (n === 0) {\n    return { kind: \"none\" }\n  }\n  if (n === 1) {\n    return { kind: \"circle\", radius: 1 }\n  }\n  return { kind: \"square\", side: 2 }\n}\nconst s = r(0)\n";
    let text = format!("{head}const a = s.kind === \"circle\" ? s.radius : 0\nconst b = s.kind !== \"circle\" ? s : 0\nconst c = s.kind === \"circle\" ? s.kind : \"\"\nconst d = \"square\" === s.kind ? s.side : 0\n");
    assert_eq!(
        types(&text, &["a", "b", "c", "d"]),
        [
            "0 | 1",
            "0 | { kind: \"none\" } | { kind: \"square\", side: 2 }",
            "\"\" | \"circle\"",
            "0 | 2"
        ]
    );
    let text = format!("{head}function f(x) {{\n  switch (x.kind) {{\n    case \"none\": {{\n      return 0\n    }}\n    case \"circle\": {{\n      return x.radius\n    }}\n    default: {{\n      return x.side\n    }}\n  }}\n}}\nf(s)\n");
    assert_eq!(
        checked(&text).found,
        ["E137 @17:16", "E137 @20:16"],
        "an argument is no literal any more"
    );
    let text = format!("{head}const t = {{ o: s }}\nconst a = t.o.kind === \"circle\" ? t.o.radius : 0\nconst b = t.o.kind === s.kind ? t.o : 0\n");
    assert_eq!(
        types(&text, &["a", "b"])[0],
        "0 | 1",
        "a path of keys is narrowed as a name is"
    );
    // One member's `kind` is any string: no discriminant, and the read is refused.
    let text = "function r(n, k) {\n  if (n === 0) {\n    return { kind: k, side: 1 }\n  }\n  return { kind: \"circle\", radius: 1 }\n}\nconst s = r(0, \"a\")\nconst a = s.kind === \"circle\" ? s.radius : 0\nconst n = 1\nconst b = n.kind === 1 ? n : 0\n";
    assert_eq!(checked(text).found, ["E137 @8:35", "E137 @10:13"]);
}

#[test]
fn instanceof_narrows_to_the_class_and_removes_it() {
    let head = "class A {\n  a = 1\n}\nclass B extends A {\n  b = 2\n}\nclass C {\n  c = 3\n}\n";
    let text = format!("{head}function f(x, u) {{\n  if (x instanceof A) {{\n    const a = x\n  }} else {{\n    const b = x\n  }}\n  if (x instanceof B) {{\n    const c = x\n  }} else {{\n    const d = x\n  }}\n  if (u instanceof C) {{\n    const e = u\n  }} else {{\n    const g = u\n  }}\n  if (u instanceof Error) {{\n    const h = u.message\n  }}\n  if (x instanceof ns.A) {{\n    const i = x\n  }}\n  if (x instanceof f) {{\n    const j = x\n  }}\n}}\nf(new A(), JSON.parse(\"1\"))\nf(new B(), 1)\nf(new C(), 1)\nf(1, 1)\n");
    let c = checked(&text);
    let of = |name: &str| c.of(name).to_owned();
    assert_eq!(
        ["a", "b", "c", "d", "e", "g", "h", "i", "j"].map(of),
        [
            "A | B",
            "int32 | C",
            "B",
            "int32 | A | C",
            "C",
            "unknown",
            "string",
            "int32 | A | B | C",
            "int32 | A | B | C"
        ],
        "an `A` that is a `B` is a `B`; what names no class says nothing"
    );
}

#[test]
fn the_statics_that_tell_what_a_value_is_narrow() {
    let text = taking(
        &["[1]", "\"a\"", "1.5", "1"],
        "  if (Array.isArray(x)) {\n    const a = x\n  } else {\n    const b = x\n  }\n  if (Number.isNaN(x)) {\n    const c = x\n  } else {\n    const d = x\n  }\n  if (typeof x === \"string\" && Number.isNaN(x)) {\n    const e = x\n  }\n",
    );
    assert_eq!(
        types(&text, &["a", "b", "c", "d", "e"]),
        [
            "readonly int32[]",
            "number | string",
            "number",
            "number | string | readonly int32[]",
            "never"
        ]
    );
    let text = "const raw = JSON.parse(\"1\")\nconst a = Array.isArray(raw) ? raw : [1]\nconst b = Number.isNaN(raw) ? raw : 0\nconst c = Array.isArray(raw) ? 0 : raw\nfunction g(Number) {\n  const d = Number.isNaN(raw) ? raw : 0\n  return d\n}\ng({ isNaN: (v) => v === 1 })\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d"]),
        [
            "readonly int32[] | readonly unknown[]",
            "number",
            "unknown",
            "unknown"
        ]
    );
}

#[test]
fn the_boundary_idiom_narrows_what_may_be_anything() {
    // SPEC.md §6.3.
    let text = "function name(data) {\n  if (typeof data === \"object\" && data !== null && Object.hasOwn(data, \"name\") && typeof data.name === \"string\") {\n    const a = data\n    const b = data.name\n    return data.name.length\n  }\n  return 0\n}\nexport const n = name(JSON.parse(\"1\"))\n";
    assert_eq!(
        types(text, &["a", "b", "n"]),
        ["{ name: unknown }", "string", "int32"]
    );
    let text = "const raw = JSON.parse(\"1\")\nconst a = Object.hasOwn(raw, \"x\") && Object.hasOwn(raw, \"y\") ? raw : 0\nconst b = Object.hasOwn(raw, \"x\") ? 0 : raw\nconst key = \"k\"\nconst c = Object.hasOwn(raw, key) ? raw : 0\nconst flag = Math.random() < 0.5\nconst s = flag ? { x: 1 } : flag ? { y: 2 } : \"s\"\nconst d = Object.hasOwn(s, \"x\") ? s : 0\nconst e = Object.hasOwn(s, \"x\") ? 0 : s\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d", "e"]),
        [
            "0 | { x: unknown, y: unknown }",
            "unknown",
            "unknown",
            "0 | \"s\" | { x: 1 } | { y: 2, x: unknown }",
            "0 | \"s\" | { y: 2 }"
        ],
        "a key that is no string written there says nothing"
    );
}

#[test]
fn conditions_compose() {
    let text = taking(
        &["1", "\"a\"", "undefined"],
        "  if (!(typeof x === \"string\")) {\n    const a = x\n  } else {\n    const b = x\n  }\n  if (x !== undefined && typeof x !== \"string\") {\n    const c = x\n  } else {\n    const d = x\n  }\n  if (x === undefined || typeof x === \"string\") {\n    const e = x\n  } else {\n    const g = x\n  }\n  const h = x === undefined ? 0 : x\n  const i = typeof x === \"number\" && x > 0\n  const j = (typeof x === \"string\") ? x : \"\"\n  return i\n",
    );
    assert_eq!(
        types(&text, &["a", "b", "c", "d", "e", "g", "h", "j"]),
        [
            "int32 | undefined",
            "string",
            "int32",
            "string | undefined",
            "string | undefined",
            "int32",
            "int32 | string",
            "string"
        ]
    );
}

#[test]
fn an_early_exit_narrows_the_rest_of_the_block() {
    let text = taking(
        &["1", "\"a\"", "undefined"],
        "  if (x === undefined) {\n    return 0\n  }\n  const a = x\n  if (typeof x === \"string\") {\n    throw new Error(x)\n  }\n  const b = x\n  return b\n",
    );
    assert_eq!(types(&text, &["a", "b"]), ["int32 | string", "int32"]);
    let text = "function f(xs) {\n  let n = 0\n  for (const x of xs) {\n    if (x === undefined) {\n      continue\n    }\n    const a = x\n    if (typeof x === \"string\") {\n      break\n    }\n    const b = x\n    n += b\n  }\n  return n\n}\nf([1, \"a\", undefined])\n";
    assert_eq!(types(text, &["a", "b"]), ["int32 | string", "int32"]);
}

#[test]
fn a_switch_narrows_case_by_case() {
    let text = taking(
        &["1", "\"a\"", "1n", "undefined"],
        "  switch (typeof x) {\n    case \"string\": {\n      const a = x\n      break\n    }\n    case \"number\":\n    case \"bigint\": {\n      const b = x\n      break\n    }\n    default: {\n      const c = x\n    }\n  }\n  const d = x\n",
    );
    assert_eq!(
        types(&text, &["a", "b", "c", "d"]),
        [
            "string",
            "int32 | bigint",
            "undefined",
            "int32 | bigint | string | undefined"
        ]
    );
    let text = "const flag = Math.random() < 0.5\nconst v = flag ? \"a\" : flag ? \"b\" : \"c\"\nfunction f() {\n  switch (v) {\n    case \"a\": {\n      return v\n    }\n    default: {\n      const rest = v\n      return rest\n    }\n  }\n}\nexport const r = f()\n";
    assert_eq!(types(text, &["rest"]), ["\"b\" | \"c\""]);
    // A value that is an expression: `-1`.
    let text = "const flag = Math.random() < 0.5\nconst n = flag ? -1 : flag ? 2 : 3\nfunction f() {\n  switch (n) {\n    case -1: {\n      return n\n    }\n    default: {\n      const others = n\n      return others\n    }\n  }\n}\nexport const r = f()\n";
    assert_eq!(types(text, &["others"]), ["2 | 3"]);
}

#[test]
fn an_assignment_undoes_what_was_told_of_a_let() {
    let text = "const p = Math.random() < 0.5 ? \"a\" : undefined\nfunction f() {\n  let x = p\n  if (x === undefined) {\n    return 0\n  }\n  const a = x\n  x = p\n  const b = x\n  const o = { v: p }\n  if (o.v === undefined) {\n    return 1\n  }\n  const c = o.v\n  o.v = p\n  const d = o.v\n  let n = 0\n  if (x !== undefined) {\n    n += 1\n    const e = x\n  }\n  return n\n}\nexport const r = f()\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d", "e"]),
        [
            "string",
            "string | undefined",
            "string",
            "string | undefined",
            "string"
        ],
        "an assignment to another name undoes nothing"
    );
}

#[test]
fn a_loop_takes_back_what_its_body_assigns() {
    let text = "const p = Math.random() < 0.5 ? 1 : undefined\nfunction f() {\n  let node = p\n  let n = 0\n  while (node !== undefined) {\n    const a = node\n    n += a\n    node = n < 9 ? p : undefined\n  }\n  const b = node\n  let cur = p\n  if (cur === undefined) {\n    return n\n  }\n  const c = cur\n  while (n < 9) {\n    const d = cur\n    n += 1\n    cur = undefined\n  }\n  return n\n}\nexport const r = f()\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d"]),
        ["int32", "undefined", "int32", "int32 | undefined"],
        "narrowed by the loop's condition; at the back edge, what the body assigned comes round"
    );
    // A declaration that runs again holds a new value.
    let text = "function f(xs) {\n  let n = 0\n  for (const x of xs) {\n    const a = x\n    if (x === undefined) {\n      continue\n    }\n    n += x\n  }\n  while (n < 9) {\n    const y = xs[0]\n    const b = y\n    if (y === undefined) {\n      break\n    }\n    n += y\n  }\n  return n\n}\nf([1, undefined])\n";
    assert_eq!(
        types(text, &["a", "b"]),
        ["int32 | undefined", "int32 | undefined"]
    );
}

#[test]
fn a_call_undoes_what_other_code_may_change() {
    let text = "const p = Math.random() < 0.5 ? \"a\" : undefined\nfunction f(cb) {\n  let x = p\n  let y = p\n  const set = () => {\n    y = undefined\n  }\n  const kept = { v: p }\n  const changed = { v: p }\n  changed.v = undefined\n  if (x !== undefined && y !== undefined && kept.v !== undefined && changed.v !== undefined && p !== undefined) {\n    const a = [x, y, kept.v, changed.v, p]\n    cb(set)\n    const b = x\n    const c = y\n    const d = kept.v\n    const e = changed.v\n    const g = p\n    return 1\n  }\n  return 0\n}\nasync function h(wait) {\n  const size = 1.5\n  let y = p\n  const set = () => {\n    y = undefined\n  }\n  if (y !== undefined) {\n    await wait\n    const i = y\n  }\n  if (y !== undefined && !Number.isNaN(size)) {\n    const j = y\n  }\n  return set\n}\nf(() => 1)\nexport const done = h(Promise.resolve(1))\n";
    assert_eq!(
        types(text, &["a", "b", "c", "d", "e", "g", "i", "j"]),
        [
            "[string, string, \"a\", string, \"a\"]",
            "string",
            "string | undefined",
            "\"a\"",
            "string | undefined",
            "\"a\"",
            "string | undefined",
            "string"
        ],
        "a `let` a closure assigns, and a key of an object that is mutated"
    );
}

#[test]
fn an_alias_of_typeof_narrows_what_it_was_taken_of() {
    let text = taking(
        &["1", "\"a\""],
        "  const kind = typeof x\n  if (kind === \"string\") {\n    const a = x\n  }\n  let y = x\n  const of = typeof y\n  y = 1\n  if (of === \"string\") {\n    const b = y\n  }\n  const o = { v: x }\n  const deep = typeof o.v\n  if (deep === \"string\") {\n    const c = o.v\n  }\n  const other = \"string\"\n  if (other === \"string\" && kind === other) {\n    const d = x\n  }\n",
    );
    assert_eq!(
        types(&text, &["a", "b", "c", "d"]),
        [
            "string",
            "int32 | string",
            "int32 | string",
            "int32 | string"
        ],
        "not of a name assigned to since, nor of a path of keys"
    );
}

#[test]
fn an_arrow_keeps_what_was_told_of_a_name_never_assigned() {
    let text = "const p = Math.random() < 0.5 ? \"a\" : undefined\nfunction f() {\n  let x = p\n  const o = { v: p }\n  if (p !== undefined && x !== undefined && o.v !== undefined) {\n    const a = [1].map(() => p)\n    const b = [1].map(() => x)\n    const c = [1].map(() => o.v)\n    const d = [1].map((q) => (typeof q === \"number\" ? q : p))\n    x = p\n    function later() {\n      return p\n    }\n    return [a, b, c, d, later()]\n  }\n  return 0\n}\nexport const r = f()\nclass K {\n  field = p === undefined ? 0 : p\n  static tag = p === undefined ? 0 : p\n  m(v = p) {\n    return v\n  }\n}\nexport const k = p === undefined ? 0 : new K()\n";
    let c = checked(text);
    assert_eq!(c.found, [""; 0]);
    assert_eq!(
        ["a", "b", "c", "d", "later"].map(|name| c.of(name).to_owned()),
        [
            "readonly string[]",
            "readonly (string | undefined)[]",
            "readonly (string | undefined)[]",
            "readonly (int32 | string)[]",
            "(): string | undefined"
        ],
        "a declared function may be called from anywhere"
    );
}

#[test]
fn code_that_is_not_reached_and_a_finally_are_not_narrowed_wrongly() {
    let text = "const p = Math.random() < 0.5 ? \"a\" : undefined\nfunction f(g) {\n  try {\n    if (p === undefined) {\n      return 0\n    }\n    g()\n  } finally {\n    const a = p\n  }\n  while (true) {\n    const b = p\n    break\n    const c = p\n  }\n  return 1\n}\nf(() => 1)\n";
    assert_eq!(
        types(text, &["a", "b", "c"]),
        ["\"a\" | undefined", "\"a\"", "\"a\" | undefined"],
        "a `finally` is reached each way its `try` is left; what is not reached is told nothing"
    );
}
