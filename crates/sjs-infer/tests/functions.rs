//! STORIES.md US-089: function signatures from bodies and call sites.

use sjs_bind::{bind, Disk, ModuleGraph};
use sjs_diag::{Diagnostics, SourceFile};
use sjs_infer::{check_expressions, infer_bindings, infer_program, Facts};
use sjs_syntax::parse;
use sjs_testkit::fs::TempDir;
use sjs_types::{ClassId, Printer, Type, Types};
use std::time::Instant;

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
    let found = diagnostics
        .iter()
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

/// The signature of `f`, or the type of `x`, in a text no finding is made on.
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
fn the_examples_of_the_spec_infer_their_signatures() {
    let greet = "export const greet = (name, opts) => (opts.loud ? name + \"!\" : name)\ngreet(\"a\", { loud: true })\ngreet(\"b\", { loud: false })\n";
    assert_eq!(
        clean(greet, "greet"),
        "greet(name: string, opts: { loud: boolean }): string"
    );
    let parse = "const config = load()\nconst failure = load()\nexport function parse(text) {\n  if (text.length === 0) {\n    return { ok: false, error: failure }\n  }\n  return { ok: true, value: config }\n}\nparse(\"a\")\n";
    assert_eq!(
        clean(parse, "parse"),
        "parse(text: string): { ok: true, value: Config } | { ok: false, error: ParseError }"
    );
}

#[test]
fn a_parameter_is_what_is_passed_for_it_at_every_call() {
    let partly = checked("const j = (a) => -a\nj(1)\nj(...xs)\nconst k = (a) => -a\nk(1)\nk(later())\nconst m = (a = later()) => a\nm(1)\nconst n = (a, b) => -b\nn(1)\n");
    assert_eq!(
        [partly.of("j"), partly.of("k"), partly.of("m"), partly.of("n")],
        ["?", "?", "?", "?"],
        "one call with a spread, one argument or a default of no type yet, a parameter one call leaves out"
    );
    assert_eq!(partly.found, ["E136 @10:1 n(1)"]);
    assert_eq!(
        clean("let f = (a) => a\nf(1)\nf(\"s\")\nf(2)\n", "f"),
        "f(a: int32 | string): int32 | string"
    );
    assert_eq!(
        clean(
            "function f(a, b) {\n  return a > 0 ? b : !b\n}\nf(1, true)\nconst g = () => f(1.5, false)\n",
            "f"
        ),
        "f(a: number, b: boolean): boolean",
        "from any scope"
    );
    assert_eq!(
        clean("let f = (a) => a\nf({ kind: \"k\" })\nf([1])\n", "f"),
        "f(a: { kind: string } | readonly int32[]): { kind: string } | readonly int32[]",
        "what is passed holds no literals"
    );
    assert_eq!(
        clean("const f = (a, b = 0) => b\nf(1, \"s\")\nf(2)\n", "f"),
        "f<T>(a: T, b?: int32 | string): int32 | string",
        "the default joins; optional when a call leaves it out"
    );
    assert_eq!(
        clean("const f = (a, b = 0) => b\nf(1, 2)\n", "f"),
        "f<T>(a: T, b: int32): int32",
        "passed at every call: not optional"
    );
    assert_eq!(
        clean("const f = (a = \"d\") => a\n", "f"),
        "f(a: string): string",
        "a default alone types it"
    );
    assert_eq!(
        clean(
            "const f = (a, ...rest) => rest\nf(1, 2, \"s\")\nf(1)\n",
            "f"
        ),
        "f<T>(a: T, ...rest: (int32 | string)[]): readonly (int32 | string)[]"
    );
    assert_eq!(
        clean("const f = (...rest) => rest\nf()\n", "f"),
        "f(...rest: never[]): readonly never[]",
        "a rest nothing is passed for"
    );
    let untyped = checked("const f = (a) => -a\nconst g = (a, { b }) => a\ng(1, { b: 2 })\nconst h = (a) => -a\nh(later())\nconst i = (a, ...rest) => a\ni(...xs)\ni(1, ...xs)\n");
    assert_eq!(
        [
            untyped.of("f"),
            untyped.of("g"),
            untyped.of("h"),
            untyped.of("i")
        ],
        ["?", "?", "?", "?"],
        "nothing passed, a pattern, an argument of no type yet, a spread"
    );
    assert_eq!(untyped.found, NONE);
    assert_eq!(
        clean("const f = (a) => -a\nconst x = f\n", "x"),
        "?",
        "a function with a parameter of no type has no type yet"
    );
    assert_eq!(
        clean("const f = (a) => 1\nconst x = [f(later()), f(1)]\n", "x"),
        "[int32, int32]",
        "but its calls have what it returns"
    );
    assert_eq!(clean("let f = (a) => a\nf(1)\n", "f"), "f(a: int32): int32");
    assert_eq!(
        clean(
            "const f = (a) => {\n  a = \"s\"\n  return a\n}\nf(1)\n",
            "f"
        ),
        "?",
        "a parameter that is assigned is a later pass's"
    );
}

#[test]
fn a_function_returns_the_union_of_its_returns() {
    assert_eq!(
        clean("const f = (a) => {\n  if (a === 1) {\n    return 1\n  } else if (a === 2) {\n    return 2\n  } else if (a === 3) {\n    return 3\n  } else {\n    return \"s\"\n  }\n}\nf(1)\n", "f"),
        "f<T>(a: T): int32 | string",
        "a chain of `else if`s that ends in an `else`"
    );
    assert_eq!(
        clean("const f = () => 1\n", "f"),
        "f(): int32",
        "an uncalled function is inferred all the same"
    );
    assert_eq!(clean("function f() {}\n", "f"), "f(): undefined");
    assert_eq!(
        clean("function f() {\n  later()\n}\n", "f"),
        "f(): undefined"
    );
    assert_eq!(
        clean("function f() {\n  return\n}\n", "f"),
        "f(): undefined"
    );
    assert_eq!(
        clean("function f() {\n  throw new Error(\"no\")\n}\n", "f"),
        "f(): never",
        "a function that only throws"
    );
    assert_eq!(
        clean(
            "const f = (a) => {\n  if (a) {\n    return 1\n  }\n  return \"s\"\n}\nf(true)\n",
            "f"
        ),
        "f(a: boolean): int32 | string"
    );
    assert_eq!(clean("const f = (a) => {\n  if (a) {\n    return 1\n  } else if (later()) {\n    return 2n\n  } else {\n    throw new Error(\"no\")\n  }\n}\nf(true)\n", "f"), "f(a: boolean): int32 | bigint");
    assert_eq!(clean("const f = (k) => {\n  switch (k) {\n    case 1: {\n      return \"one\"\n    }\n    default: {\n      return undefined\n    }\n  }\n}\nf(1)\n", "f"), "f(k: int32): string | undefined");
    assert_eq!(
        clean(
            "function f() {\n  try {\n    return 1\n  } catch {\n    return \"s\"\n  }\n}\n",
            "f"
        ),
        "f(): int32 | string"
    );
    assert_eq!(
        clean(
            "function f() {\n  try {\n    later()\n  } finally {\n    return 1\n  }\n}\n",
            "f"
        ),
        "f(): int32"
    );
    assert_eq!(
        clean("function f() {\n  {\n    return 1\n  }\n}\n", "f"),
        "f(): int32"
    );
    assert_eq!(
        clean(
            "function f() {\n  return inner\n\n  function inner() {\n    return \"s\"\n  }\n}\n",
            "f"
        ),
        "f(): (): string",
        "a function declared after the `return` is hoisted; its returns are its own"
    );
    assert_eq!(clean("const f = () => {\n  const g = () => {\n    return 1\n  }\n  class K {\n    m() {\n      return 2\n    }\n  }\n}\n", "f"), "f(): undefined");
    assert_eq!(clean("const f = async () => 1\nasync function g() {\n  return \"s\"\n}\nconst x = [f(), g()]\n", "x"), "[Promise<int32>, Promise<string>]");
    assert_eq!(
        clean("function* f() {\n  yield 1\n}\nconst x = f()\n", "x"),
        "Generator<int32, undefined>",
        "a generator yields what its body does"
    );
    assert_eq!(clean("const f = () => later()\nconst g = () => {\n  return later()\n}\nconst x = [f(), g()]\n", "x"), "?");
    assert_eq!(
        clean("const f = () => ({ kind: \"a\", n: 1 })\n", "f"),
        "f(): { kind: \"a\", n: 1 }",
        "the literals of a shape are kept"
    );
}

#[test]
fn a_value_on_some_paths_only_is_e111() {
    assert_eq!(
        found("function f(a) {\n  if (a) {\n    return 1\n  }\n}\n"),
        ["E111 @3:5 return 1"],
        "the body may run off its end"
    );
    assert_eq!(
        found("function f(a) {\n  if (a) {\n    return\n  }\n  return 1\n}\n"),
        ["E111 @3:5 return"],
        "at the `return` without a value"
    );
    assert_eq!(found("const f = (a) => {\n  if (a) {\n    return 1\n  } else if (later()) {\n    return 2\n  }\n}\n"), ["E111 @3:5 return 1"], "an `else if` without an `else`");
    assert_eq!(
        found("const f = (k) => {\n  switch (k) {\n    case 1: {\n      return 1\n    }\n  }\n}\n"),
        ["E111 @4:7 return 1"],
        "a `switch` without a `default`"
    );
    assert_eq!(found("const f = (k) => {\n  switch (k) {\n    case 1: {\n      return 1\n    }\n    default: {\n      later()\n    }\n  }\n}\n"), ["E111 @4:7 return 1"]);
    assert_eq!(
        found("function f() {\n  try {\n    return 1\n  } catch {\n    later()\n  }\n}\n"),
        ["E111 @3:5 return 1"]
    );
    assert_eq!(
        found("function f() {\n  try {\n    later()\n  } catch {\n    return 1\n  }\n}\n"),
        ["E111 @5:5 return 1"]
    );
    assert_eq!(
        found("function f() {\n  while (later()) {\n    return 1\n  }\n}\n"),
        ["E111 @3:5 return 1"]
    );
    assert_eq!(found("class K {\n  m(a) {\n    if (a) {\n      return 1\n    }\n  }\n  get g() {\n    return 1\n  }\n}\nconst o = function (a) {\n  if (a) {\n    return 1\n  }\n}\n"), ["E111 @4:7 return 1", "E111 @13:5 return 1"], "a method and a function expression too");
    assert_eq!(found("function f(a) {\n  if (a) {\n    return\n  }\n  later()\n}\nconst g = (a) => {\n  if (a) {\n    throw new Error(\"no\")\n  }\n  return 1\n}\nconst h = () => {}\nconst i = (a) => -a\n"), NONE);
    assert_eq!(
        checked("function f(a) {\n  if (a) {\n    return 1\n  }\n}\n").diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
        ["this function returns a value on some paths and nothing on others: return a value on every path, or on none (SPEC.md §4.3)"]
    );
}

#[test]
fn recursion_is_solved_round_by_round() {
    assert_eq!(
        clean(
            "const fact = (n) => (n < 2 ? 1 : n * fact(n - 1))\nfact(5)\n",
            "fact"
        ),
        "fact(n: number): number"
    );
    assert_eq!(clean("function count(n) {\n  if (n === 0) {\n    return \"done\"\n  }\n  return count(n - 1)\n}\ncount(3)\n", "count"), "count(n: number): string");
    assert_eq!(
        clean("const even = (n) => (n === 0 ? true : odd(n - 1))\nconst odd = (n) => (n === 0 ? false : even(n - 1))\neven(4)\n", "odd"),
        "odd(n: number): boolean",
        "two functions that call one another"
    );
    assert_eq!(
        clean("const loop = () => loop()\n", "loop"),
        "loop(): never",
        "a function that only calls itself returns nothing ever"
    );
    let grows = checked("const wrap = (n) => [wrap(n)]\nwrap(1)\nconst fine = () => 1\n");
    assert_eq!(
        [grows.of("wrap"), grows.of("fine")],
        ["?".to_owned(), "fine(): int32".to_owned()],
        "a type that grows without end is given up on, alone"
    );
}

#[test]
fn a_call_has_the_type_of_what_it_calls() {
    assert_eq!(
        clean("const f = () => \"s\"\nconst x = f()\n", "x"),
        "string"
    );
    assert_eq!(
        clean("const x = f()\nfunction f() {\n  return 1\n}\n", "x"),
        "int32",
        "a function declaration is hoisted"
    );
    assert_eq!(clean("const a = () => 1\nconst b = () => \"s\"\nconst pick = later() ? a : b\nconst x = pick()\n", "x"), "int32 | string", "through a union of two functions");
    assert_eq!(
        clean("const f = () => () => 1\nconst x = f()()\n", "x"),
        "int32",
        "an arrow that is a value is typed from its body"
    );
    assert_eq!(
        clean("const o = { f: () => 1 }\nconst x = o.f()\n", "x"),
        "int32",
        "a function in a literal too"
    );
    assert_eq!(
        clean("const x = [later(), list.map(later), ns.f(1)]\n", "x"),
        "?"
    );
}

#[test]
fn a_count_of_arguments_the_function_does_not_take_is_e136() {
    let f = "const f = (a, b = 1, c = 2) => a\nf(0)\nf(0, 1, 2)\n";
    assert_eq!(
        found(&format!("{f}f()\nf(1, 2, 3, 4)\n")),
        ["E136 @4:1 f()", "E136 @5:1 f(1, 2, 3, 4)"]
    );
    assert_eq!(
        found("function g(a, ...rest) {\n  return rest\n}\ng(1)\ng(1, 2, 3, 4, 5)\ng()\n"),
        ["E136 @6:1 g()"],
        "a rest takes any number more"
    );
    assert_eq!(
        found("const h = () => 1\nh(1)\nconst i = (a) => a\ni(1)\ni(...xs)\nh(...xs)\n"),
        ["E136 @2:1 h(1)"],
        "a spread passes a number that is not known"
    );
    assert_eq!(
        found("const a = () => 1\nconst pick = later() ? a : a\nconst alias = a\nalias(1)\n"),
        ["E136 @4:1 alias(1)"],
        "by the type of what is called, when it is no function by name"
    );
    assert_eq!(found("const a = (p, q = 1) => p\na(0)\na(0, 1)\nconst alias = a\nalias()\nalias(1)\nalias(1, 2)\nalias(1, 2, 3)\n"), ["E136 @5:1 alias()", "E136 @8:1 alias(1, 2, 3)"]);
    assert_eq!(
        found("const r = (...rest) => rest\nr(1)\nconst alias = r\nalias()\nalias(1, 2, 3)\n"),
        NONE
    );
    let messages: Vec<String> = checked(&format!("{f}f()\nconst h = () => 1\nh(1)\nfunction g(a, ...rest) {{\n  return rest\n}}\ng(1)\ng()\n")).diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(
        messages,
        [
            "`f` takes 1 to 3 argument(s) and is passed 0 (SPEC.md §4.4)",
            "`h` takes 0 argument(s) and is passed 1 (SPEC.md §4.4)",
            "`g` takes 1 or more argument(s) and is passed 0 (SPEC.md §4.4)",
        ]
    );
}

#[test]
fn calling_what_is_no_function_is_e139() {
    assert_eq!(
        found("const n = 1\nn()\nconst s = \"a\"\ns(1)\nconst o = { k: 2 }\no()\no.k()\n"),
        ["E139 @2:1 n", "E139 @4:1 s", "E139 @6:1 o", "E139 @7:1 o.k"]
    );
    assert_eq!(
        found("const f = () => 1\nconst either = later() ? f : 2\neither()\nf()()\n"),
        ["E139 @3:1 either", "E139 @4:1 f()"],
        "one member that is none; what a call gives"
    );
    assert_eq!(
        found("later()\nns.f()\nlist.map(later)\nconst f = () => 1\nf()\n"),
        NONE
    );
    assert_eq!(
        found("function f() {}\nnew f()\nconst n = 1\nnew n()\nnew (later() ? n : n)()\n"),
        ["E139 @2:5 f", "E139 @4:5 n", "E139 @5:5 (later() ? n : n)"],
        "`new` on a function or a typed value"
    );
    assert_eq!(found("class K {}\nimport { L } from \"./l.js\"\nnew K()\nnew L()\nnew Map([[1, 2]])\nnew ns.Thing()\nconst Later = later()\nnew Later()\nconst arrow = () => 1\n"), NONE);
    assert_eq!(
        found("const arrow = () => 1\nnew arrow()\n"),
        ["E139 @2:5 arrow"]
    );
    let messages: Vec<String> = checked("const n = 1\nn()\nnew n()\n")
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        messages,
        [
            "`n` is called, and is a `1`: only a function is called (SPEC.md §4.4)",
            "`new` on `n`, which is no class (SPEC.md §4.4)",
        ]
    );
}

#[test]
fn a_use_the_union_cannot_take_names_the_calls_that_made_it() {
    let twice = self::checked("const y = (1 - \"a\") * 2\nconst z = [1 - \"b\"]\n");
    assert_eq!(
        twice.found,
        ["E138 @1:14 -", "E138 @2:14 -"],
        "typed from more than one place, reported once"
    );
    let class = self::checked("const config = load()\nconst half = (x) => x / 2\nhalf(config)\n");
    let notes: Vec<&str> = class
        .diagnostics
        .iter()
        .flat_map(|d| d.related.iter().map(|r| r.note.as_str()))
        .collect();
    assert_eq!(
        notes,
        ["passed a `class #0` here"],
        "a class by its name, when it has one"
    );
    let checked = checked("const half = (x) => x / 2\nhalf(\"a\")\nhalf(1)\n");
    assert_eq!(checked.found, ["E138 @1:23 /"]);
    let finding = checked.diagnostics.iter().next().expect("a finding");
    assert_eq!(finding.message, "`/` takes two numbers or two bigints: these are a `int32 | string` and a `2` (SPEC.md §4.1)");
    let related: Vec<(&str, u32, u32, &str)> = finding
        .related
        .iter()
        .map(|related| {
            (
                related.path.as_str(),
                related.span.start.line,
                related.span.start.column,
                related.note.as_str(),
            )
        })
        .collect();
    assert_eq!(
        related,
        [
            ("a.js", 2, 6, "passed a `string` here"),
            ("a.js", 3, 6, "passed a `int32` here")
        ]
    );
    let none = self::checked("const n = 1\nconst x = n / \"a\"\n");
    assert!(
        none.diagnostics.iter().all(|d| d.related.is_empty()),
        "what is no parameter has no call sites"
    );
}

fn program(
    files: &[(&str, &str)],
    roots: &[&str],
) -> (TempDir, Vec<(String, String, Diagnostics)>) {
    let dir = TempDir::new("us089").expect("a temporary directory");
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

#[test]
fn a_function_called_from_two_modules_has_both_in_its_parameter() {
    let (_dir, typed) = program(
        &[
            ("entry.js", "import { show, twice } from \"./lib.js\"\nimport { other } from \"./other.js\"\nexport const a = show(\"text\")\nexport const b = [twice(1), other]\n"),
            ("other.js", "import { show as display } from \"./re.js\"\nexport const other = display(1)\n"),
            ("re.js", "export { show } from \"./lib.js\"\n"),
            ("lib.js", "export const show = (value) => value * 2\nexport function twice(n) {\n  return n + n\n}\n"),
        ],
        &["entry.js"],
    );
    let of = |name: &str| {
        typed
            .iter()
            .find(|(declared, ..)| declared == name)
            .map(|(_, ty, _)| ty.as_str())
    };
    assert_eq!(
        of("lib.js value"),
        Some("int32 | string"),
        "a `string` from one module and a number from another"
    );
    assert_eq!(of("lib.js twice"), Some("(a: int32): number"));
    assert_eq!(
        of("entry.js b"),
        Some("[number, ?]").map(|_| "?"),
        "a call of a function whose body has no type has none"
    );
    let lib = typed
        .iter()
        .find(|(declared, ..)| declared == "lib.js show")
        .expect("show");
    let findings: Vec<_> = lib.2.iter().collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(
        (
            findings[0].code.as_str(),
            findings[0].span.start.line,
            findings[0].span.start.column
        ),
        ("E138", 1, 38)
    );
    let mut related: Vec<(&str, u32, u32, &str)> = findings[0]
        .related
        .iter()
        .map(|related| {
            (
                related.path.as_str(),
                related.span.start.line,
                related.span.start.column,
                related.note.as_str(),
            )
        })
        .collect();
    related.sort_unstable();
    assert_eq!(
        related,
        [
            ("entry.js", 3, 23, "passed a `string` here"),
            ("other.js", 2, 30, "passed a `int32` here")
        ],
        "both call sites, each in its module"
    );
}

#[test]
fn an_imported_function_is_called_as_it_is_declared() {
    let (_dir, typed) = program(
        &[
            ("entry.js", "import { size, pair } from \"./lib.js\"\nexport const a = size(\"abc\")\nexport const b = pair(1, \"s\")\nexport const c = size()\nexport const d = pair(...xs)\n"),
            ("lib.js", "export const size = (text) => text.length\nexport function pair(left, right = 0) {\n  return [left, right]\n}\n"),
        ],
        &["entry.js"],
    );
    let of = |name: &str| {
        typed
            .iter()
            .find(|(declared, ..)| declared == name)
            .map(|(_, ty, _)| ty.as_str())
    };
    assert_eq!(of("entry.js a"), Some("int32"));
    assert_eq!(of("entry.js b"), Some("readonly (int32 | string)[]"));
    assert_eq!(of("lib.js size"), Some("(a: string): int32"));
    assert_eq!(
        of("lib.js pair"),
        Some("<T>(a: T, b: int32 | string): readonly (int32 | string | T)[]")
    );
    let entry = typed
        .iter()
        .find(|(declared, ..)| declared == "entry.js a")
        .expect("a");
    let found: Vec<_> = entry
        .2
        .iter()
        .map(|d| (d.code.as_str(), d.span.start.line))
        .collect();
    assert_eq!(
        found,
        [("E136", 4)],
        "the count of arguments is judged where the call is"
    );
}

#[test]
fn ten_thousand_call_sites_resolve_in_linear_time() {
    let sites = |count: usize| -> String {
        let mut text = String::from("let f = (a) => a\n");
        for at in 0..count {
            text.push_str(if at % 2 == 0 { "f(1)\n" } else { "f(\"s\")\n" });
        }
        text
    };
    let timed = |count: usize| {
        let text = sites(count);
        let start = Instant::now();
        let checked = checked(&text);
        assert_eq!(checked.of("f"), "f(a: int32 | string): int32 | string");
        assert_eq!(checked.found, NONE);
        start.elapsed()
    };
    let (small, large) = (timed(2_500), timed(10_000));
    assert!(
        large < small * 16 + std::time::Duration::from_millis(500),
        "four times the call sites took {large:?}, against {small:?}: more than linear"
    );
}
