//! STORIES.md US-090: generalization and instantiation.

use sjs_bind::{bind, Disk, ModuleGraph};
use sjs_diag::{Diagnostics, SourceFile};
use sjs_infer::{check_expressions, infer_bindings, infer_program, Facts};
use sjs_syntax::parse;
use sjs_testkit::fs::TempDir;
use sjs_types::{ClassId, Printer, Type, Types};

fn class_name(class: ClassId) -> String {
    ["Config", "ParseError"]
        .get(class.0 as usize)
        .map_or(format!("Class{}", class.0), |name| (*name).to_owned())
}

struct Checked {
    /// `name: type` of each declaration that has one, by name.
    types: Vec<(String, String)>,
    /// `CODE @line:column text`.
    found: Vec<String>,
    diagnostics: Diagnostics,
}

impl Checked {
    fn of(&self, name: &str) -> String {
        let found = self.types.iter().find(|(declared, _)| declared == name);
        found.map_or("?".to_owned(), |(_, ty)| ty.clone())
    }
}

/// Infers and checks `text`, in which `config` is a `Config` and `failure` a
/// `ParseError` when it declares them.
fn checked(text: &str) -> Checked {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let mut facts = Facts::default();
    for (name, class) in [("config", 0), ("failure", 1)] {
        let declaration = bindings.declarations.iter().position(|d| d.name == name);
        let ty = types.intern(Type::Class(ClassId(class), Vec::new()));
        facts
            .declarations
            .extend(declaration.map(|declaration| (declaration, ty)));
    }
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let printer = Printer::new(&types, &class_name);
    // An `async` function that never awaits is another story's finding (US-094).
    let found = diagnostics
        .iter()
        .filter(|d| d.code.as_str() != "E118")
        .map(|d| {
            let covered = source.slice(&d.span).expect("in the file");
            format!(
                "{} @{}:{} {covered}",
                d.code.as_str(),
                d.span.start.line,
                d.span.start.column
            )
        })
        .collect();
    let mut signatures = Vec::new();
    for (at, declaration) in bindings.declarations.iter().enumerate() {
        if let Some(&ty) = facts.declarations.get(&at) {
            let names: Vec<&str> = facts.parameters.get(&at).map_or(Vec::new(), |parameters| {
                parameters
                    .iter()
                    .map(|p| p.map_or("_", |p| bindings.declarations[p].name.as_str()))
                    .collect()
            });
            let shown = if matches!(types.get(ty), Type::Function(_)) {
                printer.signature(&declaration.name, &names, ty)
            } else {
                printer.print(ty)
            };
            signatures.push((declaration.name.clone(), shown));
        }
    }
    Checked {
        types: signatures,
        found,
        diagnostics,
    }
}

fn program(
    files: &[(&str, &str)],
    roots: &[&str],
) -> (TempDir, Vec<(String, String, Diagnostics)>) {
    let dir = TempDir::new("us090").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let roots: Vec<_> = roots.iter().map(|root| base.join(root)).collect();
    let (graph, _) = ModuleGraph::load(&Disk, &base, &roots).expect("loads");
    let (links, _) = graph.link();
    let mut types = Types::new();
    let modules = infer_program(&graph, &links, &mut types);
    let mut out = Vec::new();
    for module in graph.modules() {
        let Some(inferred) = modules.get(&module.path) else {
            continue;
        };
        let sjs_bind::Content::Script { source, tree, .. } = &module.content else {
            continue;
        };
        let diagnostics = check_expressions(
            source,
            tree,
            &inferred.bindings,
            &inferred.facts,
            &mut types,
        );
        let printer = Printer::new(&types, &class_name);
        for (at, declaration) in inferred.bindings.declarations.iter().enumerate() {
            let ty = inferred
                .facts
                .declarations
                .get(&at)
                .map_or("?".to_owned(), |&ty| printer.print(ty));
            out.push((
                format!("{} {}", module.name, declaration.name),
                ty,
                diagnostics.clone(),
            ));
        }
    }
    (dir, out)
}
/// The signature of a function, or the type of a binding, in a text no finding is
/// made on.
fn clean(text: &str, name: &str) -> String {
    let checked = checked(text);
    assert_eq!(checked.found, Vec::<String>::new(), "{text}");
    checked.of(name)
}

fn found(text: &str) -> Vec<String> {
    checked(text).found
}

const NONE: [&str; 0] = [];

#[test]
fn a_parameter_the_body_treats_opaquely_is_a_type_parameter() {
    assert_eq!(
        clean("export const id = (x) => x\n", "id"),
        "id<T>(x: T): T"
    );
    assert_eq!(
        clean("export const same = (a, b) => a === b\n", "same"),
        "same<T, U>(a: T, b: U): boolean",
        "compared with `===`"
    );
    assert_eq!(
        clean("export const g = (x) => (x !== null ? x : 0)\n", "g"),
        "g<T>(x: T): int32 | T",
        "through parentheses and a branch of a conditional"
    );
    assert_eq!(
        clean("export const pair = (a, b) => [a, b]\n", "pair"),
        "pair<T, U>(a: T, b: U): readonly (T | U)[]",
        "stored in an array"
    );
    assert_eq!(
        clean(
            "export const h = (x) => {\n  const o = { x }\n  o.x = x\n  return o\n}\n",
            "h"
        ),
        "h<T>(x: T): { x: T }",
        "stored in a field of what the function makes"
    );
    assert_eq!(
        clean("export const index = (k, v) => {\n  const m = new Map()\n  m.set(k, v)\n  return m\n}\nconst i = index(\"a\", 1)\n", "index"),
        "index<T, U>(k: T, v: U): Map<T, U>",
        "stored in a `Map` the function makes"
    );
    assert_eq!(
        clean("export const f = (a, b, c) => [c, -a]\n", "c"),
        "U",
        "the type parameters count the parameters that are generic"
    );
    assert_eq!(
        clean("function walk(n, acc) {\n  if (n === 0) {\n    return acc\n  }\n  return walk(n - 1, acc)\n}\nconst r = walk(3, \"s\")\n", "walk"),
        "walk<T>(n: number, acc: T): T",
        "passed on to the function itself"
    );
    assert_eq!(
        clean("function outer() {\n  const inner = (x) => x\n  return inner(1)\n}\nconst r = outer()\n", "r"),
        "int32"
    );
    for constrained in [
        "export const f = (x) => x.size\n",
        "export const f = (x) => x()\n",
        "export const f = (x) => new x()\n",
        "export const f = (x) => (x ? 1 : 2)\n",
        "export const f = (x) => -x\n",
        "export const f = (x) => {\n  x = 1\n  return x\n}\n",
        "export const f = (x) => x + 1\n",
        "export const f = (xs) => xs[0] < 1\n",
        "export const f = (xs) => xs[0].size\n",
        "export const f = (xs) => g(xs[0])\n",
        "export const f = (xs) => {\n  seen.push(xs[0])\n}\n",
        "export const f = (o) => o[\"k\"] + o[0]\n",
    ] {
        assert_eq!(clean(constrained, "f"), "?", "{constrained}");
    }
}

#[test]
fn a_parameter_nothing_says_anything_of_is_e112() {
    assert_eq!(found("const id = (x) => x\n"), ["E112 @1:13 x"]);
    assert_eq!(
        found("function id(x) {\n  return x\n}\nconst f = (a, b) => [a]\n"),
        ["E112 @1:13 x", "E112 @4:12 a", "E112 @4:15 b"],
        "a declaration; a parameter the body does not use"
    );
    assert_eq!(
        found("const f = (x) => 1\nexport { f }\nconst g = (x) => 1\nconst h = g\nconst i = (x) => 1\ni(...xs)\nconst j = (x = 1, ...rest) => [x, rest]\nconst k = (x) => x.size\n"),
        NONE,
        "exported in a list, named as a value, called, with a default, a rest, constrained"
    );
    assert_eq!(found("let l = (x) => x\n"), ["E112 @1:10 x"]);
    let messages: Vec<String> = checked("const id = (x) => x\n")
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(messages, ["nothing says what `x` is: the body does nothing with it that tells, and nothing calls the function (SPEC.md §4.4)"]);
}

#[test]
fn a_parameter_read_by_place_is_an_array_of_a_type_parameter() {
    assert_eq!(
        clean("export const first = (xs) => xs[0]\n", "first"),
        "first<T>(xs: readonly T[]): T | undefined"
    );
    let called =
        checked("const first = (xs) => xs[0]\nconst a = first([1, 2])\nconst b = first([\"s\"])\n");
    assert_eq!(
        [called.of("a"), called.of("b")],
        ["int32 | undefined", "string | undefined"]
    );
    assert_eq!(called.found, NONE);
    assert_eq!(
        clean(
            "export const at = (xs, i) => xs[i]\nconst r = at([1], 0)\n",
            "at"
        ),
        "at<T>(xs: readonly T[], i: int32): T | undefined",
        "the place is what the calls pass"
    );
    assert_eq!(
        clean("export const f = (xs) => xs[0] === xs[1]\n", "f"),
        "f<T>(xs: readonly T[]): boolean"
    );
    assert_eq!(
        clean("let first = (xs) => xs[0]\nfirst([1])\n", "first"),
        "first(xs: readonly int32[]): int32 | undefined",
        "bound to a `let`, it is what is passed"
    );
}

#[test]
fn each_call_of_a_generic_function_is_its_own() {
    let called = checked("const id = (x) => x\nconst a = id(1)\nconst b = id(\"s\")\nconst c = id({ k: true })\nconst d = id(...xs)\nconst e = id(later())\n");
    assert_eq!(
        [
            called.of("id"),
            called.of("a"),
            called.of("b"),
            called.of("c"),
            called.of("d"),
            called.of("e")
        ],
        [
            "id<T>(x: T): T",
            "int32",
            "string",
            "{ k: boolean }",
            "?",
            "?"
        ]
    );
    assert_eq!(called.found, NONE);
    assert_eq!(
        clean(
            "export const pair = (a, b) => [a, b]\nconst p = pair(1, 2)\n",
            "p"
        ),
        "readonly int32[]",
        "a union of the same thing twice is the thing"
    );
    assert_eq!(
        clean(
            "export const box = (v) => ({ v, all: [v], two: [v, 1] })\nconst b = box(\"s\")\n",
            "b"
        ),
        "{ all: readonly string[], two: readonly (int32 | string)[], v: string }"
    );
    assert_eq!(
        clean("export const index = (k, v) => {\n  const m = new Map()\n  m.set(k, v)\n  return m\n}\nconst i = index(\"a\", 1)\n", "i"),
        "Map<string, int32>"
    );
    assert_eq!(
        clean("export const first = (xs) => xs[0]\nconst t = [1, \"s\"]\nconst a = first(t)\nconst b = first(1)\n", "a"),
        "int32 | string | undefined",
        "a tuple for an array; what is no array says nothing"
    );
    let nested = checked(
        "export const wrap = (x) => (y) => [x, y]\nconst w = wrap(1)\nconst z = w(\"s\")\n",
    );
    assert_eq!(
        [nested.of("wrap"), nested.of("w"), nested.of("z")],
        [
            "wrap<T>(x: T): <U>(a: U): readonly (T | U)[]",
            "w<U>(a: U): readonly (int32 | U)[]",
            "readonly (int32 | string)[]"
        ],
        "a generic that returns a generic"
    );
    assert_eq!(nested.found, NONE);
    assert_eq!(
        found("export const id = (x) => x\nconst q = id()\n"),
        ["E136 @2:11 id()"]
    );
    assert_eq!(
        clean(
            "let id = (x) => x\nconst a = id(1)\nconst b = id(\"s\")\n",
            "a"
        ),
        "int32 | string",
        "a generic bound to a `let` is monomorphic"
    );
    assert_eq!(
        clean(
            "export const f = (x) => x\nexport const g = (y) => f(y)\nconst r = g(1)\n",
            "g"
        ),
        "g<T>(y: T): T",
        "passed to a generic function, it is generic still"
    );
}

#[test]
fn an_arrow_that_is_a_value_is_typed_from_its_body() {
    assert_eq!(clean("const x = [(a) => a]\n", "x"), "[<T>(a: T): T]");
    assert_eq!(
        clean("const x = [(xs, a) => (a < 1 ? xs[0] : a)]\n", "x"),
        "[<T>(a: readonly T[], b: number): number | T | undefined]"
    );
    assert_eq!(
        clean("const x = [(a, b) => a < b, async (a) => a]\n", "x"),
        "[(a: number | bigint | string, b: number | bigint | string): boolean, <T>(a: T): Promise<T>]"
    );
    assert_eq!(
        clean(
            "const half = (n) => n / 2\nhalf(1)\nconst x = [(a) => half(a)]\n",
            "x"
        ),
        "[(a: int32): number]"
    );
    for untold in [
        "const x = [(a) => a.size]\n",
        "const x = [({ a }) => a]\n",
        "const x = [(a = 1) => a]\n",
        "const x = [(...a) => a]\n",
        "const x = [(a) => {\n  seen.push(a)\n}]\n",
        "const x = [(a) => later(a)[0]]\n",
        "const half = (n) => n / 2\nconst x = [(a) => half(a)]\n",
    ] {
        assert_eq!(clean(untold, "x"), "?", "{untold}");
    }
}

#[test]
fn compose_is_what_it_is_passed() {
    let typed = checked("const inc = (n) => n + 1\ninc(1)\nconst show = (n) => `${n}`\nshow(1)\nconst compose = (f, g) => (x) => f(g(x))\nconst both = compose(show, inc)\nconst r = both(2)\n");
    assert_eq!(
        [typed.of("compose"), typed.of("r")],
        [
            "compose(f: (a: int32): string, g: (a: int32): number): (a: int32): string",
            "string"
        ]
    );
    assert_eq!(typed.found, NONE);
    assert_eq!(
        clean(
            "export const compose = (f, g) => (x) => f(g(x))\n",
            "compose"
        ),
        "?",
        "a parameter that is called is what the calls pass: none here"
    );
    assert_eq!(
        clean("export const apply = (f, x) => f(x)\n", "x"),
        "T",
        "passed to what has no type, it is opaque"
    );
}

#[test]
fn what_is_stored_outside_the_function_is_what_the_calls_pass() {
    let stored = checked("const store = new Map()\nexport const put = (k, v) => {\n  store.set(k, v)\n  return v\n}\nput(\"a\", 1)\n");
    assert_eq!(
        [stored.of("store"), stored.of("put")],
        ["Map<string, int32>", "put(k: string, v: int32): int32"]
    );
    assert_eq!(stored.found, NONE);
    assert_eq!(
        clean(
            "const box = { v: 0 }\nexport const set = (x) => {\n  box.v = x\n}\nset(1)\n",
            "set"
        ),
        "set(x: int32): undefined",
        "a field of what is declared outside"
    );
    assert_eq!(
        clean(
            "export const keep = (x) => {\n  this.kept = x\n  return x\n}\nkeep(1)\n",
            "keep"
        ),
        "keep(x: int32): int32",
        "what it is stored into is no name"
    );
    assert_eq!(
        clean(
            "export const keep = (x) => {\n  let last = x\n  last = x\n  return last\n}\n",
            "x"
        ),
        "T",
        "a binding of the function's own"
    );
}

#[test]
fn an_ordered_parameter_is_a_number_a_bigint_or_a_string() {
    assert_eq!(
        clean("export const max = (a, b) => (a < b ? b : a)\n", "max"),
        "max(a: number | bigint | string, b: number | bigint | string): number | bigint | string",
        "ordering one against the other is no finding"
    );
    assert_eq!(
        clean("export const small = (a) => a < 10\n", "small"),
        "small(a: number): boolean",
        "what it is ordered against says which"
    );
    assert_eq!(
        clean(
            "export const f = (a, b, c) => [10n >= a, b <= \"m\", c > 1.5]\n",
            "f"
        ),
        "f(a: bigint, b: string, c: number): readonly boolean[]"
    );
    assert_eq!(
        clean(
            "const big = (a, b) => (a > b ? a : b)\nconst x = big(1, 2)\n",
            "big"
        ),
        "big(a: int32, b: int32): int32",
        "then the call sites say"
    );
    assert_eq!(
        found("const less = (a, b) => a < b\nless(\"a\", \"b\")\nless(1, 2)\n"),
        ["E138 @1:26 <"]
    );
    assert_eq!(
        found("export const f = (a, b) => a < b && a.size\n"),
        NONE,
        "ordered and read: what the calls pass"
    );
}

#[test]
fn passing_to_a_function_that_is_not_generic_constrains() {
    let passed =
        checked("const half = (n) => n / 2\nexport const pass = (x) => half(x)\nhalf(4)\n");
    assert_eq!(
        [passed.of("half"), passed.of("pass")],
        ["half(n: int32): number", "pass(x: int32): number"]
    );
    assert_eq!(passed.found, NONE);
    let wrong = checked("const half = (n) => n / 2\nexport const pass = (x) => half(x)\nhalf(4)\nconst y = pass(\"s\")\nconst z = pass(later())\n");
    assert_eq!(wrong.found, ["E138 @4:16 \"s\""]);
    let messages: Vec<String> = wrong
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        messages,
        ["a `string` is passed where `pass` takes a `int32` (SPEC.md §4.4)"]
    );
    assert_eq!(
        clean(
            "const half = (n) => n / 2\nconst pass = (x) => half(x)\n",
            "pass"
        ),
        "?",
        "until the function says what it takes"
    );
    assert_eq!(
        clean(
            "export const a = (x) => b(x)\nexport const b = (y) => a(y)\n",
            "a"
        ),
        "?",
        "two that pass to one another say nothing"
    );
    assert_eq!(
        clean(
            "const pick = (o, d) => o.pick(d)\npick({ pick: (a) => a }, 1)\n",
            "pick"
        ),
        "pick<T>(o: { pick: <T>(a: T): T }, d: T): T",
        "passed to a method of another parameter"
    );
}

#[test]
fn a_generic_function_is_generic_where_it_is_imported() {
    let (_dir, typed) = program(
        &[
            ("entry.js", "import { id, first } from \"./lib.js\"\nexport const a = id(1)\nexport const b = id(\"s\")\nexport const c = first([true])\n"),
            ("lib.js", "export const id = (x) => x\nexport function first(xs) {\n  return xs[0]\n}\n"),
        ],
        &["entry.js"],
    );
    let of = |name: &str| {
        let found = typed.iter().find(|(declared, ..)| declared == name);
        found.map(|(_, ty, _)| ty.as_str())
    };
    assert_eq!(of("lib.js id"), Some("<T>(a: T): T"));
    assert_eq!(
        of("lib.js first"),
        Some("<T>(a: readonly T[]): T | undefined")
    );
    assert_eq!(
        [of("entry.js a"), of("entry.js b"), of("entry.js c")],
        [Some("int32"), Some("string"), Some("boolean | undefined")]
    );
    assert!(typed.iter().all(|(_, _, found)| found.is_empty()));
}
#[test]
fn a_function_in_a_generic_function_has_type_parameters_of_its_own() {
    let nested = checked("export const f = (x) => {\n  const g = (q) => [q, x]\n  return g\n}\nconst h = f(1)\nconst r = h(\"s\")\n");
    assert_eq!(
        [
            nested.of("f"),
            nested.of("g"),
            nested.of("h"),
            nested.of("r")
        ],
        [
            "f<T>(x: T): <U>(a: U): readonly (T | U)[]",
            "g<U>(q: U): readonly (T | U)[]",
            "h<U>(a: U): readonly (int32 | U)[]",
            "readonly (int32 | string)[]"
        ]
    );
    assert_eq!(nested.found, NONE);
    assert_eq!(
        clean("export const f = (x) => new Box(x)\n", "x"),
        "T",
        "passed to `new`, of a class that is not the file's"
    );
    assert_eq!(
        clean("export const f = async (x) => x\nconst p = f(1)\n", "p"),
        "Promise<int32>"
    );
    assert_eq!(
        clean(
            "export const f = (x) => {\n  const t = [x, 1]\n  return t\n}\nconst r = f(\"s\")\n",
            "r"
        ),
        "[string, 1]"
    );
    assert_eq!(
        clean("export const f = (x) => {\n  const t = [x, 1]\n  const m = new Map()\n  m.set(x, x)\n  const p = { x }\n  return [(y) => [y, t, m, p]]\n}\n", "f"),
        "f<T>(x: T): readonly (<U>(a: U): readonly ({ x: T } | [T, 1] | U | Map<T, T>)[])[]",
        "an arrow in it that names what holds its type parameter"
    );
    assert_eq!(
        clean("export const f = (x) => {\n  const g = async (q) => [q, x]\n  const u = x !== null ? x : 0\n  return [(y) => [y, g, u]]\n}\n", "g"),
        "g<U>(q: U): Promise<readonly (T | U)[]>"
    );
    assert_eq!(
        clean("export const f = (x) => {\n  const g = (...rest) => [x, rest]\n  g(1)\n  return g\n}\nconst h = f(\"s\")\n", "h"),
        "h(...rest: int32[]): readonly (string | readonly int32[])[]",
        "one with a rest"
    );
    assert_eq!(
        clean(
            "export const f = (x) => [(y) => [y, x], (z) => z]\nconst r = f(1)\n",
            "r"
        ),
        "readonly ((<T>(a: T): T) | (<U>(a: U): readonly (int32 | U)[]))[]",
        "an arrow generic in a parameter of the same name keeps it"
    );
}
#[test]
fn what_holds_a_type_parameter_keeps_it_from_the_arrows_inside() {
    for (holder, held) in [
        ("[x, 1]", "[T, 1]"),
        ("{ x }", "{ x: T }"),
        ("(q) => [q, x]", "(<U>(a: U): readonly (T | U)[])"),
        ("x !== null ? x : 0", "int32 | T"),
    ] {
        let text = format!(
            "export const f = (x) => {{\n  const t = {holder}\n  return [(y) => [y, t]]\n}}\n"
        );
        let wanted = format!("f<T>(x: T): readonly (<U>(a: U): readonly ({held} | U)[])[]");
        assert_eq!(clean(&text, "f"), wanted);
    }
    assert_eq!(
        clean("export const f = (x) => {\n  const g = (a, k) => [a.length, k()]\n  return g([x], () => x)\n}\n", "g"),
        "g<T>(a: readonly T[], k: (): T): readonly (int32 | T)[]",
        "a type parameter met twice is one"
    );
    for untold in [
        "const x = [(a) => [a[0], a.size]]\n",
        "const x = [(a) => [a < 1, a.size]]\n",
        "const x = [(a) => [a < 1, -a]]\n",
        "const x = [(a) => [a[0], a < 1]]\n",
    ] {
        assert_eq!(clean(untold, "x"), "?", "{untold}");
    }
    assert_eq!(
        clean(
            "export const f = (x) => {\n  let n = 0\n  n += x\n  return n\n}\n",
            "x"
        ),
        "?",
        "added to a binding, it is no part of it"
    );
}

#[test]
fn a_union_of_functions_is_instantiated_each() {
    let both = "const a = (x) => x\nconst b = (x) => [x]\n";
    assert_eq!(
        clean(
            &format!("{both}const pick = later() ? a : b\nconst r = pick(1)\n"),
            "r"
        ),
        "int32 | readonly int32[]"
    );
    assert_eq!(
        clean("const a = (x) => x\nconst c = (n) => n > 1\nc(1.5)\nconst pick = later() ? c : a\nconst r = pick(1)\n", "r"),
        "int32 | boolean",
        "one that is generic and one that is not"
    );
    assert_eq!(
        clean(
            &format!("{both}const pick = later() ? a : b\nconst r = pick(...xs)\n"),
            "r"
        ),
        "?"
    );
}
