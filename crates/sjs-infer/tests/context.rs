//! STORIES.md US-091: contextual typing of callbacks.

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

#[allow(dead_code)]
fn program(
    files: &[(&str, &str)],
    roots: &[&str],
) -> (TempDir, Vec<(String, String, Diagnostics)>) {
    let dir = TempDir::new("us091").expect("a temporary directory");
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

/// The related locations of each finding: `line:column note`.
fn related(text: &str) -> Vec<Vec<String>> {
    let checked = checked(text);
    let notes = |d: &sjs_diag::Diagnostic| -> Vec<String> {
        let each = d.related.iter().map(|r| {
            assert_eq!(r.path, "a.js");
            format!("{}:{} {}", r.span.start.line, r.span.start.column, r.note)
        });
        each.collect()
    };
    checked.diagnostics.iter().map(notes).collect()
}

fn messages(text: &str) -> Vec<String> {
    let checked = checked(text);
    checked
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

const INC: &str = "const inc = (n) => n + 1\nexport const one = inc(1)\n";

#[test]
fn a_callback_is_given_what_its_function_calls_it_with() {
    let run = "const run = (f) => f(1)\nexport const r = run((n) => n * 2)\n";
    assert_eq!(clean(run, "run"), "run(f: (a: int32): number): number");
    assert_eq!(clean(run, "r"), "number");
    let apply = "const apply = (f, x) => f(x)\nexport const r = apply((n) => n + 1, 1)\n";
    assert_eq!(
        clean(apply, "apply"),
        "apply(f: (a: int32): number, x: int32): number",
        "what is passed on to it is what this call passes"
    );
    let twice = "const twice = (f, x) => f(f(x))\nexport const r = twice((n) => n / 2, 1)\n";
    assert_eq!(
        clean(twice, "twice"),
        "twice(f: (a: number): number, x: number): number",
        "every call of it counts: what it is given, and what it gives"
    );
    let both = "const run = (f) => [f(1), f(\"a\")]\nexport const r = run((v) => v * 2)\n";
    assert_eq!(found(both), ["E138 @2:31 *"]);
    assert_eq!(
        messages(both),
        ["`*` takes two numbers or two bigints: these are a `int32 | string` and a `2` (SPEC.md §4.1)"]
    );
    assert_eq!(
        related(both),
        [["1:23 passed a `int32` here", "1:29 passed a `string` here"]]
    );
    assert_eq!(
        found("import { other } from \"./o.js\"\nexport const r = other((n) => n + 1)\n"),
        NONE,
        "nothing is known of what an import calls"
    );
    assert_eq!(
        found("const n = 1\nexport const r = n((a) => a + 1)\n"),
        ["E139 @2:18 n"],
        "what is no function expects nothing"
    );
}

#[test]
fn a_use_the_expectation_cannot_support_names_where_it_comes_from() {
    let operator = "const run = (f) => f(\"a\")\nexport const r = run((n) => n * 2)\n";
    assert_eq!(found(operator), ["E138 @2:31 *"]);
    assert_eq!(related(operator), [["1:22 passed a `string` here"]]);
    let read = "const run = (f, o) => f(o)\nexport const r = run((p) => p.missing, { k: 1 })\n";
    assert_eq!(found(read), ["E137 @2:31 missing"]);
    assert_eq!(related(read), [["1:25 passed a `{ k: int32 }` here"]]);
    let deeper =
        "const run = (f, o) => f(o)\nexport const r = run((p) => p.o.missing, { o: { k: 1 } })\n";
    assert_eq!(found(deeper), ["E137 @2:33 missing"]);
    assert_eq!(
        related(deeper),
        [NONE],
        "what is read from is no name: nothing says where it comes from"
    );
    let parameter = "const get = (o) => o.missing\nexport const g = get({ k: 1 })\n";
    assert_eq!(found(parameter), ["E137 @1:22 missing"]);
    assert_eq!(
        related(parameter),
        [["2:22 passed a `{ k: int32 }` here"]],
        "a parameter of a function declared by name: its calls"
    );
}

#[test]
fn a_callback_may_take_less_than_it_is_given_and_no_more() {
    let fewer = "const run = (f) => f(1, 2)\nexport const r = run((a) => a + 1)\nexport const s = run(() => 1)\n";
    assert_eq!(
        clean(fewer, "run"),
        "run(f: ((): int32) | ((a: int32): number)): number"
    );
    let more = "const run = (f) => f(1)\nexport const r = run((a, b) => a)\n";
    assert_eq!(found(more), ["E136 @2:26 b"]);
    assert_eq!(
        messages(more),
        ["nothing is ever passed for `b`: this function is given 1 argument(s) (SPEC.md §4.4)"]
    );
    assert_eq!(related(more), [["1:22 passed a `int32` here"]]);
    assert_eq!(checked(more).of("run"), "?", "the arrow has no type");
    let named = "const add = (a, b) => a + b\nexport const three = add(1, 2)\nconst run = (f) => f(1)\nexport const r = run(add)\n";
    assert_eq!(
        found(named),
        ["E136 @3:20 f(1)"],
        "a function that needs more than it is given is still a finding"
    );
    let alone = "const one = (a) => a + 1\nexport const r = one(1, 2)\n";
    assert_eq!(
        found(alone),
        ["E136 @2:18 one(1, 2)"],
        "what is no parameter takes what it takes"
    );
    let member = "const o = { f: (a) => a }\nexport const r = o.f(1, 2)\n";
    assert_eq!(found(member), ["E136 @2:18 o.f(1, 2)"]);
}

#[test]
fn a_callback_passed_on_is_what_the_function_it_reaches_takes() {
    let via = format!(
        "{INC}const run = (f) => f(1)\nexport const a = run(inc)\nconst via = (g) => run(g)\n"
    );
    let passed = format!("{via}export const b = via((n) => n * 3)\n");
    assert_eq!(clean(&passed, "via"), "via(g: (a: int32): number): number");
    assert_eq!(
        clean(&passed, "run"),
        "run(f: (a: int32): number): number",
        "the parameter passed on is no call site of what it is passed to"
    );
    let wrong = format!(
        "{via}export const b = via((n) => n.missing)\nexport const c = via((s) => s - \"a\")\n"
    );
    assert_eq!(found(&wrong), ["E137 @6:31 missing", "E138 @7:31 -"]);
    assert_eq!(
        related(&wrong),
        [
            ["6:18 expected to take a `int32` here"],
            ["7:18 expected to take a `int32` here"]
        ]
    );
    let more = format!("{via}export const c = via((x, y) => x)\n");
    assert_eq!(found(&more), ["E136 @6:26 y"]);
    assert_eq!(related(&more), [["6:18 expected to take a `int32` here"]]);
}

#[test]
fn a_generic_callback() {
    let id =
        "export const id = (x) => x\nconst apply = (f, x) => f(x)\nexport const r = apply(id, 1)\n";
    assert_eq!(clean(id, "apply"), "apply<T>(f: <T>(a: T): T, x: T): T");
    assert_eq!(clean(id, "r"), "int32");
    let arrow = "const apply = (f, x) => f(x)\nexport const r = apply((v) => v, \"s\")\n";
    assert_eq!(
        clean(arrow, "apply"),
        "apply(f: (a: string): string, x: string): string",
        "an arrow that could be generic is what it is expected to be"
    );
    let slot =
        "export const id = (x) => x\nconst o = { f: id }\no.f = (m) => m * 2\nexport const p = o\n";
    assert_eq!(
        found(slot),
        NONE,
        "a generic function says nothing of what an arrow is passed"
    );
    let unbound = "const apply = (f, x) => f(x)\nexport const a = apply\nexport const r = apply((v) => v * 2, other)\n";
    assert_eq!(
        found(unbound),
        NONE,
        "passed a type parameter nothing says anything of, it has no type"
    );
}

#[test]
fn nested_callbacks() {
    let outer = "const outer = (f) => f(1)\n";
    let inner = "const inner = (g) => g(\"s\")\n";
    let nested = "export const r = outer((a) => inner((b) => a + b.length))\n";
    for text in [
        format!("{outer}{inner}{nested}"),
        format!("{inner}{outer}{nested}"),
    ] {
        assert_eq!(
            clean(&text, "outer"),
            "outer(f: (a: int32): number): number"
        );
        assert_eq!(
            clean(&text, "inner"),
            "inner(g: (a: string): number): number"
        );
    }
    // Not through a binding: whichever function is typed first asks for its arrow.
    let bare = "outer((a) => inner((b) => a + b.length))\n";
    for text in [
        format!("{outer}{inner}{bare}"),
        format!("{inner}{outer}{bare}"),
    ] {
        assert_eq!(
            clean(&text, "outer"),
            "outer(f: (a: int32): number): number"
        );
        assert_eq!(
            clean(&text, "inner"),
            "inner(g: (a: string): number): number"
        );
    }
    let block = format!(
        "{outer}{inner}export const r = outer((a) => {{\n  inner((b) => a - b)\n  return a\n}})\n"
    );
    assert_eq!(
        found(&block),
        ["E138 @4:18 -"],
        "in a statement of the outer one that it does not return"
    );
    let declared = format!(
        "{inner}export const f = (a) => inner((b) => b.length + a)\nexport const r = f(1)\n"
    );
    assert_eq!(
        clean(&declared, "f"),
        "f(a: int32): number",
        "inside a function declared by name"
    );
}

#[test]
fn a_callback_inside_what_it_is_given_has_no_type_while_it_is_typed() {
    let walk = "const walk = (f, n) => (n === 0 ? f(0) : f(walk((m) => m + 1, n - 1)))\nexport const w = walk((k) => k * 2, 3)\n";
    assert_eq!(
        clean(walk, "walk"),
        "walk(f: (a: number): number, n: number): number"
    );
}

#[test]
fn a_spread_or_a_rest_leaves_the_count_open() {
    let rest = "const sum = (...ns) => ns.length\nexport const two = sum(1, 2)\nconst o = { f: sum }\no.f = (a, b) => a + b\nexport const p = o\n";
    assert_eq!(found(rest), NONE, "more may be passed to a rest");
    let spread = "const run = (f, xs) => f(...xs)\nexport const r = run((a) => a + 1, [1])\n";
    assert_eq!(found(spread), NONE, "what a spread passes is not counted");
    let mixed =
        "const run = (f, xs) => [f(...xs), f(1)]\nexport const r = run((a, b) => a + 1, [1])\n";
    assert_eq!(
        found(mixed),
        ["E136 @1:35 f(1)"],
        "a parameter past the other calls is typed alone, and that call passes too few"
    );
    let passed = "const gs = [1]\nconst run = (g, f) => [g, f(1)]\nexport const r = run(...gs, (a, b) => a)\n";
    assert_eq!(
        found(passed),
        NONE,
        "passed after a spread, which parameter it is for is not known"
    );
}

#[test]
fn an_arrow_stored_where_a_function_is_held() {
    let get = "const get = (o) => o.k\nexport const one = get({ k: 1 })\nconst h = { f: get }\n";
    let slot = format!("{get}h.f = (q) => q.missing\nexport const p = h\n");
    assert_eq!(found(&slot), ["E137 @4:16 missing"]);
    assert_eq!(
        related(&slot),
        [["4:1 expected to take a `{ k: int32 }` here"]]
    );
    let fits = format!("{get}h.f = (q) => q.k + 1\nexport const p = h\n");
    assert_eq!(
        found(&fits),
        ["E138 @4:7 (q) => q.k + 1"],
        "it gives a number, not an int32"
    );
    let name = "let f = (n) => n + 1\nexport const one = f(1)\nf = (m) => m - \"a\"\n";
    assert_eq!(found(name), ["E138 @3:14 -"]);
    assert_eq!(related(name), [["3:1 expected to take a `int32` here"]]);
    let union = "const len = (s) => s.length\nconst inc = (n) => n + 1\nexport const a = [len(\"a\"), inc(1)]\nexport const pick = (flag) => {\n  const o = { f: flag ? inc : len }\n  o.f = (v) => v * 2\n  return o\n}\nexport const p = pick(true)\n";
    assert_eq!(found(union), ["E138 @6:18 *"]);
    assert_eq!(
        messages(union),
        ["`*` takes two numbers or two bigints: these are a `int32 | string` and a `2` (SPEC.md §4.1)"],
        "an expected union of functions: what any of them takes"
    );
    assert_eq!(
        related(union),
        [["6:3 expected to take a `int32 | string` here"]]
    );
    let number = "const o = { k: 1 }\no.k = (m) => m\nexport const p = o\n";
    assert_eq!(
        found(number),
        ["E138 @2:7 (m) => m"],
        "where no function is held, the arrow is typed alone"
    );
    assert_eq!(
        found("let k = 1\nk = (m) => m * 2\nexport const p = () => k\n"),
        NONE,
        "nor where what is held has no type"
    );
}

#[test]
fn an_arrow_returned_beside_other_functions() {
    let pick = format!("{INC}export const pick = (flag) => {{\n  if (flag) {{\n    return inc\n  }}\n  return (m) => m - 1\n}}\nexport const p = pick(true)\n");
    assert_eq!(
        clean(&pick, "pick"),
        "pick(flag: boolean): (a: int32): number"
    );
    let wrong = format!("{INC}export const pick = (flag) => {{\n  if (flag) {{\n    return inc\n  }}\n  return (m) => m - \"a\"\n}}\nexport const p = pick(true)\n");
    assert_eq!(found(&wrong), ["E138 @7:19 -"]);
    assert_eq!(related(&wrong), [["5:12 expected to take a `int32` here"]]);
    let three = "const len = (s) => s.length\nconst inc = (n) => n + 1\nexport const a = [len(\"a\"), inc(1)]\nexport const pick = (k) => {\n  if (k === 0) {\n    return inc\n  }\n  if (k === 1) {\n    return (v) => v * 2\n  }\n  return len\n}\nexport const p = pick(0)\n";
    assert_eq!(found(three), ["E138 @9:21 *"]);
    assert_eq!(
        related(three),
        [["11:10 expected to take a `int32 | string` here"]],
        "what all the others take"
    );
    let alone = "export const make = () => {\n  return (n) => n + 1\n}\n";
    assert_eq!(found(alone), NONE, "beside nothing, nothing is expected");
}
