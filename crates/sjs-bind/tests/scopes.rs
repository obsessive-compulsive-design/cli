//! STORIES.md US-063: scopes, declarations, references and the temporal dead zone.

use sjs_bind::{bind, Bindings};
use sjs_diag::SourceFile;
use sjs_syntax::parse;
use std::fmt::Write as _;

fn bound(text: &str) -> (Bindings, Vec<String>) {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, diagnostics) = bind(&source, &tree);
    let found = diagnostics
        .iter()
        .map(|d| {
            let at = d.span.start;
            assert_eq!(d.path, "a.js");
            format!(
                "{} @{}:{} {}",
                d.code.as_str(),
                at.line,
                at.column,
                d.message
            )
        })
        .collect();
    (bindings, found)
}

/// The scopes (`n Kind <parent`), declarations (`Kind name@line:column in scope`) and
/// references (`name@line:column in scope -> line:column of the declaration`, or `?`).
fn dump(text: &str) -> String {
    let (bindings, _) = bound(text);
    let mut out = String::new();
    for (at, scope) in bindings.scopes.iter().enumerate() {
        let parent = scope
            .parent
            .map_or(String::new(), |parent| format!(" <{parent}"));
        writeln!(out, "{at} {:?}{parent}", scope.kind).expect("a string");
    }
    for d in &bindings.declarations {
        let at = d.span.start;
        writeln!(
            out,
            "{:?} {}@{}:{} in {}",
            d.kind, d.name, at.line, at.column, d.scope
        )
        .expect("a string");
    }
    for r in &bindings.references {
        let at = r.span.start;
        let to = r.declaration.map_or("?".to_owned(), |d| {
            let d = bindings.declarations[d].span.start;
            format!("{}:{}", d.line, d.column)
        });
        let call = if r.call { " call" } else { "" };
        let export = if r.export { " export" } else { "" };
        writeln!(
            out,
            "{}@{}:{} in {} -> {to}{call}{export}",
            r.name, at.line, at.column, r.scope
        )
        .expect("a string");
    }
    out
}

/// `text`, its dump and its findings, as one snapshot.
fn snapshot(name: &str, text: &str) {
    let all = format!(
        "{text}---\n{}---\n{}\n",
        dump(text),
        bound(text).1.join("\n")
    );
    sjs_testkit::snapshot!(name, &all);
}

fn findings(text: &str) -> Vec<String> {
    bound(text).1
}

#[test]
fn every_way_to_bind_a_name_at_module_level_and_in_functions() {
    snapshot(
        "bindings-module",
        r#"import { a, b as c } from "./x.js"
import * as ns from "./y.js"
import * as from from "./w.js"
import cfg from "./c.json" with { type: "json" }

export { a, c as d }
export { e as f } from "./z.js"
export const k = a
let { p, q: r = k, ...rest } = cfg
const [s, t = s, ...u] = [p]
export function fn(x, { y, z: [w] } = {}, ...more) {
  return x + y + w + more + fn + from
}
export async function* stream(n) {
  yield n
  yield* ns.all
  await rest
}
const g = async (h = 1) => h
const curried = (one) => (two) => one + two
"#,
    );
}

#[test]
fn a_class_binds_its_name_and_its_members_are_keys() {
    snapshot(
        "bindings-class",
        r"class A extends B.C {
  #priv = 1
  static count = A.n
  field = this.#priv + A
  static = count
  static;
  constructor(v) {
    super()
    this.v = v
  }
  get size() {
    return A
  }
  static {
    A.count = 2
  }
  [Symbol.iterator]() {
    return ns.it
  }
  async method(o) {
    return { o, key: o, [o]: 1 }
  }
}
",
    );
}

#[test]
fn statements_open_the_scopes_they_have() {
    snapshot(
        "bindings-statements",
        r"for (let i = 0; i < 3; i += 1) {
  a.b?.c(i)
}
for (const [e1] of xs) {
  e1?.z
}
for await (const v of of) {
  v
}
switch (k) {
  case k1: {
    const z1 = 1
    break
  }
  default: {
    f(z1)
  }
}
try {
  g()
} catch (err) {
  err
} finally {
  err
}
try {
  g()
} catch {
  g()
}
;[x1, [x2], { y1, z1: w1 = d1, ...r1 }, ...x3] = [y1]
typeof q
await aw
while (cond) {
  cond2
}
if (c1) {
  throw c2
}
c3 ? c4 : c5
c6[c7]
{
  function inner() {}
  inner()
}
inner()
new K(n)
`${t1}`
",
    );
}

#[test]
fn the_removed_forms_the_parser_recovers_bind_their_names_too() {
    snapshot(
        "bindings-removed-forms",
        r"const o = {
  m(p) {
    return p
  },
}
const h = x => x
const fe = function inner() {
  return inner
}
const ce = class Named {
  static me = Named
}
var old = inner
for (const key in o) {
  key
}
",
    );
}

#[test]
fn a_binding_used_before_its_declaration_has_run_is_e135() {
    let early = |name: &str, line: u32, column: u32, declared: u32| {
        format!("E135 @{line}:{column} `{name}` is used before its declaration on line {declared} has run (SPEC.md §8.1)")
    };
    assert_eq!(
        findings("u\nconst u = 1\n{\n  v\n}\nconst v = 1\nx = 1\nlet x = 2\n"),
        [
            early("u", 1, 1, 2),
            early("v", 4, 3, 6),
            early("x", 7, 1, 8)
        ],
        "a read, a read in a block, a write"
    );
    assert_eq!(
        findings("const a = a\nconst [b] = [b]\nconst [e = e] = []\nconst { f = g, g } = o\n"),
        [
            early("a", 1, 11, 1),
            early("b", 2, 14, 2),
            early("e", 3, 12, 3),
            early("g", 4, 13, 4)
        ],
        "in its own initializer, and in a default before its element"
    );
    assert_eq!(
        findings("const [c, d = c] = [1]\nconst { m, n = m } = o\nconst after = [c, d, m, n, after2]\nfunction after2() {}\n"),
        Vec::<String>::new(),
        "an element before, a declaration before, a function declared below"
    );
    assert_eq!(
        findings("for (const h of h) {\n}\nfor (const [i, j = i] of [i]) {\n  i + j\n}\nfor (let n = n; n < 1; n += 1) {\n}\n"),
        [early("h", 1, 17, 1), early("i", 3, 27, 3), early("n", 6, 14, 6)]
    );
    assert_eq!(
        findings("const l = (m = n, n) => m\nconst p = (q = q) => q\nconst r = (s, t = s, [u, w = u] = t) => w\n"),
        [early("n", 1, 16, 1), early("q", 2, 16, 2)],
        "a default that names a later parameter, or its own"
    );
    assert_eq!(
        findings("class R extends R {\n  static s = R\n  t = R\n  static {\n    R\n  }\n  m() {\n    return R\n  }\n}\nnew Later()\nclass Later {}\n"),
        [early("R", 1, 17, 1), early("Later", 11, 5, 12)],
        "a class in its own `extends`, not in its body"
    );
    assert_eq!(
        findings("const k = () => k\nconst lazy = () => late\nclass C {\n  f = late\n  m() {\n    return late\n  }\n}\nexport { late }\nconst late = 1\ntry {\n} catch (e) {\n  e\n}\n"),
        Vec::<String>::new(),
        "what runs later may name what is declared later, and an export list runs nothing"
    );
}

#[test]
fn a_function_called_before_what_it_uses_is_declared_is_e135_at_the_call() {
    let call = |line: u32, column: u32, f: &str, name: &str, declared: u32, used: u32| {
        format!("E135 @{line}:{column} `{f}` is called before `{name}` is declared on line {declared}: line {used} uses it (SPEC.md §8.1)")
    };
    assert_eq!(
        findings("function f() {\n  return x\n}\nf()\nconst x = 1\nf()\n"),
        [call(4, 1, "f", "x", 5, 2)],
        "called before, and not when the call is after"
    );
    assert_eq!(
        findings("const x = f()\nfunction f() {\n  return x\n}\n"),
        [call(1, 11, "f", "x", 1, 3)],
        "the call that would give it its value"
    );
    assert_eq!(
        findings("const g = () => h()\nfunction h() {\n  return k()\n}\nconst k = () => [y, z]\ng()\nconst y = 1\nconst z = 2\ng()\n"),
        [call(6, 1, "g", "y", 7, 5), call(6, 1, "g", "z", 8, 5)],
        "through the functions it calls, arrows included"
    );
    assert_eq!(
        findings("h()\nconst k = () => 1\nfunction h() {\n  return k()\n}\n"),
        [call(1, 1, "h", "k", 2, 4)],
        "the function it calls is itself a binding"
    );
    assert_eq!(
        findings(concat!(
            "function rec(n) {\n  rec(n)\n  const local = 1\n  helper()\n  function helper() {\n    return [local, rec]\n  }\n}\n",
            "rec(1)\nconst cb = () => {\n  f()\n}\nfunction f() {\n  return late\n}\nfunction outer() {\n  const inside = () => late\n  return [inside, f]\n}\nouter()\n",
            "const value = other(f)\nconst late = 1\nfunction even(n) {\n  return odd(n)\n}\nfunction odd(n) {\n  const m = n\n  return even(m)\n}\neven(1)\n",
        )),
        Vec::<String>::new(),
        "a call makes its function's bindings anew; a closure, a value and a callback run later; `other` is unknown"
    );
    assert_eq!(
        findings("const run = (f) => {\n  inner()\n  const mine = 1\n  function inner() {\n    return mine\n  }\n}\n"),
        [call(2, 3, "inner", "mine", 3, 5)],
        "inside a function as at module level"
    );
    assert_eq!(
        findings(concat!(
            "import { imported } from \"./i.js\"\n\nfunction f(given) {\n  unknown()\n",
            "  imported()\n  given()\n  return x\n}\nf(imported)\nconst x = 1\n",
        )),
        [call(9, 1, "f", "x", 10, 7)],
        "what it calls that is no function of the file is followed no further"
    );
}

#[test]
fn a_name_declared_twice_in_one_scope_is_e000() {
    let twice = |line: u32, column: u32, name: &str, first: u32| {
        format!("E000 @{line}:{column} `{name}` is already declared in this scope, on line {first}")
    };
    assert_eq!(
        findings("import { a } from \"./a.js\"\n\nconst a = 2\nfunction a() {}\nclass a {}\nlet b = 1\nconst [b] = []\n"),
        [twice(3, 7, "a", 1), twice(4, 10, "a", 1), twice(5, 7, "a", 1), twice(7, 8, "b", 6)]
    );
    assert_eq!(
        findings("const f = (p, q) => {\n  const p = 1\n  {\n    const q = 2\n  }\n}\nfunction g(r) {\n  function r() {}\n}\ntry {\n} catch (e) {\n  const e = 1\n}\n"),
        [
            twice(2, 9, "p", 1),
            shadow(4, 11, "q", 1),
            twice(8, 12, "r", 7),
            twice(12, 9, "e", 11)
        ],
        "a body may not declare a parameter's name, once; a block inside it shadows"
    );
    assert_eq!(
        findings("for (const i of xs) {\n  const i = 1\n}\n{\n  const a = 3\n  function dup() {}\n  function dup() {}\n}\nconst a = 1\nconst same = (a) => a\nclass K {\n  m(a) {\n    const K = a\n  }\n}\n"),
        [shadow(2, 9, "i", 1), shadow(5, 9, "a", 9), twice(7, 12, "dup", 6)],
        "other scopes are other names: in a block E107, in a function none"
    );
    assert_eq!(
        findings("const f = (p, p) => p\nconst [d, d] = []\n"),
        Vec::<String>::new(),
        "two names of one parameter list or one pattern are the parser's E000"
    );
}

fn shadow(line: u32, column: u32, name: &str, outer: u32) -> String {
    format!("E107 @{line}:{column} `{name}` shadows the binding declared on line {outer}: only a nested function may reuse an outer name (SPEC.md §4.2)")
}

#[test]
fn a_block_that_declares_a_name_of_its_function_is_e107() {
    assert_eq!(
        findings("const x = 1\n{\n  const x = 2\n  {\n    let x = 3\n  }\n}\n"),
        [shadow(3, 9, "x", 1), shadow(5, 9, "x", 3)],
        "each against the nearest binding around it"
    );
    assert_eq!(
        findings(concat!(
            "import { i } from \"./i.js\"\n\nconst f = (x, e) => {\n  for (const x of i) {\n    x\n  }\n",
            "  for (let i = 0; i < 1; i += 1) {\n  }\n  try {\n  } catch (e) {\n  }\n  if (x) {\n    const [f, { k: e }] = x\n  }\n}\n",
        )),
        [shadow(4, 14, "x", 3), shadow(10, 12, "e", 3), shadow(13, 20, "e", 3)],
        "a loop variable, a `catch` binding and a pattern's name over a parameter; `i` and `f` belong to the module, outside the function"
    );
    assert_eq!(
        findings("{\n  function late() {}\n  class Late {}\n}\nfunction late() {}\nclass Late {\n  static {\n    const late = 1\n  }\n}\n"),
        [shadow(2, 12, "late", 5), shadow(3, 9, "Late", 6), shadow(8, 11, "late", 5)],
        "a function and a class in a block, over names declared below; a `static` block is a block"
    );
    assert_eq!(
        findings(concat!(
            "import { i } from \"./i.js\"\n\nconst x = 1\nconst f = (x) => {\n  const i = x\n  return (i) => i\n}\n",
            "function g(f) {\n  function x() {}\n}\nclass K {\n  x = (f) => f\n  g(x, i) {\n    const K = 1\n  }\n}\n",
            "{\n  const fetch = 1\n  const own = fetch\n}\n{\n  const own = 2\n}\n",
        )),
        Vec::<String>::new(),
        "a nested function may shadow: a parameter, a body, a method; a global is no binding; blocks side by side share nothing"
    );
}

#[test]
fn a_name_resolves_to_the_nearest_declaration_around_it() {
    let (bindings, _) = bound("const a = 1\nconst f = (a) => {\n  {\n    const b = a\n  }\n}\n");
    let inner = bindings.scopes.len() - 1;
    let param = bindings.lookup(inner, "a").expect("declared");
    assert_eq!(bindings.declarations[param].span.start.line, 2);
    assert_eq!(
        bindings
            .lookup(0, "a")
            .map(|d| bindings.declarations[d].span.start.line),
        Some(1)
    );
    assert_eq!(
        bindings
            .lookup(inner, "b")
            .map(|d| bindings.declarations[d].scope),
        Some(inner)
    );
    assert_eq!(bindings.lookup(0, "b"), None);
    assert_eq!(bindings.lookup(inner, "nothing"), None);
    let spans: Vec<_> = bindings
        .scopes
        .iter()
        .map(|s| (s.span.start.line, s.span.end.line))
        .collect();
    assert_eq!(spans, [(1, 7), (2, 6), (2, 6), (3, 5)]);
}
