//! STORIES.md US-070: module purity and evaluation order, over real directories.

use sjs_bind::{Disk, ModuleGraph};
use sjs_testkit::fs::TempDir;
use std::path::PathBuf;

fn program(files: &[(&str, &str)], roots: &[&str]) -> (TempDir, ModuleGraph) {
    let dir = TempDir::new("us070").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let roots: Vec<PathBuf> = roots.iter().map(|root| base.join(root)).collect();
    let (graph, _) = ModuleGraph::load(&Disk, &base, &roots).expect("the roots load");
    (dir, graph)
}

/// Where a module of this text is first impure: `line:column text`, or `pure`.
fn impure(text: &str) -> String {
    let (_dir, graph) = program(&[("entry.js", text)], &["entry.js"]);
    let module = graph.modules().next().expect("the module");
    module.impure().map_or("pure".to_owned(), |span| {
        let covered = &text[span.byte_start..span.byte_end];
        format!("{}:{} {covered}", span.start.line, span.start.column)
    })
}

#[test]
fn a_module_that_only_declares_is_pure() {
    for text in [
        "",
        "import { a } from \"./a.js\"\nimport * as ns from \"./a.js\"\nexport { a, ns }\nexport { b } from \"./a.js\"\n",
        "const a = 1\nlet b = \"s\"\nconst c = true\nconst d = 1n\nconst e = /x/v\nconst f = undefined\nconst g = `text`\n",
        "const f = () => g()\nconst h = async (x) => {\n  await x\n}\nfunction g() {\n  return f()\n}\nexport function i() {}\n",
        "class K {\n  x = make()\n  static y = 1\n  static z = [() => run()]\n  m() {\n    return run()\n  }\n}\nexport class L extends K {}\n",
        "const o = { a: 1, b: [2, { c: () => f() }], d: -1, [k]: 3, \"e\": o }\nexport const p = [o, [], {}]\n",
        "const a = -1\nconst b = (2 + 3) * a\nconst c = a > b ? a : b\nconst d = !c\nexport const e = typeof d\n",
        "const e = function () {\n  run()\n}\nconst k = class {\n  m() {}\n}\nconst o = { m() {\n  run()\n} }\n",
    ] {
        assert_eq!(impure(text), "pure", "{text}");
    }
}

#[test]
fn what_runs_at_the_top_level_makes_a_module_impure_from_there() {
    let cases = [
        ("const a = 1\nrun()\nconst b = other()\n", "2:1 run()"),
        ("const a = make()\n", "1:11 make()"),
        (
            "export const a = [1, { b: make(2) }, later()]\n",
            "1:27 make(2)",
        ),
        (
            "const a = Object.freeze({ b: 1 })\n",
            "1:11 Object.freeze({ b: 1 })",
        ),
        ("const a = new Map()\n", "1:11 new Map()"),
        ("const a = await load\n", "1:11 await load"),
        ("let a = 1\na = 2\n", "2:1 a = 2"),
        ("let a = 1\na += 1\n", "2:1 a += 1"),
        (
            "const a = import(\"./entry.js\")\n",
            "1:11 import(\"./entry.js\")",
        ),
        ("const a = 1 + f()\n", "1:15 f()"),
        ("const a = b ? c() : d\n", "1:15 c()"),
        ("const a = (-f())\n", "1:13 f()"),
        ("const a = o.b\n", "1:11 o.b"),
        ("const a = o[0]\n", "1:11 o[0]"),
        ("const a = [...b]\n", "1:12 ...b"),
        ("const a = { ...b }\n", "1:13 ...b"),
        ("const a = `x${b}`\n", "1:11 `x${b}`"),
        ("const a = tag`x`\n", "1:11 tag`x`"),
        ("const { a } = b\n", "1:7 { a }"),
        ("const [a] = b\n", "1:7 [a]"),
        ("const a = { [k()]: 1 }\n", "1:14 k()"),
        (
            "class K {\n  static {\n    run()\n  }\n}\n",
            "2:3 static {\n    run()\n  }",
        ),
        (
            "class K {\n  a = f()\n  static b = 1\n  static c = g()\n}\n",
            "4:14 g()",
        ),
        ("export class K {\n  static {}\n}\n", "2:3 static {}"),
        ("const k = class {\n  static a = f()\n}\n", "2:14 f()"),
        ("if (a) {\n}\n", "1:1 if (a) {\n}"),
        ("for (const a of b) {\n}\n", "1:1 for (const a of b) {\n}"),
        ("{\n  const a = 1\n}\n", "1:1 {\n  const a = 1\n}"),
        ("a\n", "1:1 a"),
        ("throw a\n", "1:1 throw a"),
        ("export const a = 1\nexport const b = f()\n", "2:18 f()"),
    ];
    for (text, at) in cases {
        assert_eq!(impure(text), at, "{text}");
    }
}

#[test]
fn a_json_module_and_a_file_that_is_no_text_are_pure() {
    let (dir, _) = program(&[("entry.js", "")], &["entry.js"]);
    std::fs::write(dir.path().join("notext.js"), b"run()\n\xff").expect("write");
    dir.write("c.json", "[1]").expect("write");
    dir.write(
        "entry.js",
        "import c from \"./c.json\" with { type: \"json\" }\nimport { t } from \"./notext.js\"\nexport { c, t }\n",
    )
    .expect("write");
    let base = dir.path().canonicalize().expect("canonical");
    let (graph, _) = ModuleGraph::load(&Disk, &base, &[base.join("entry.js")]).expect("loads");
    let found: Vec<_> = graph
        .modules()
        .map(|module| (module.name.as_str(), module.impure().is_none()))
        .collect();
    assert_eq!(
        found,
        [("c.json", true), ("entry.js", true), ("notext.js", true)]
    );
}

fn order(files: &[(&str, &str)], roots: &[&str]) -> Vec<String> {
    let (_dir, graph) = program(files, roots);
    let order = graph.evaluation_order();
    assert_eq!(order.len(), graph.modules().count(), "every module once");
    assert_eq!(order, graph.evaluation_order(), "the same on every run");
    order.iter().map(|module| module.name.clone()).collect()
}

#[test]
fn modules_are_evaluated_in_es_module_order() {
    assert_eq!(order(&[("entry.js", "")], &["entry.js"]), ["entry.js"]);
    assert_eq!(
        order(
            &[
                ("entry.js", "import { x } from \"./z.js\"\nimport { a } from \"./a.js\"\nexport { m } from \"./m.js\"\nimport { x } from \"./nowhere.js\"\nimport { x } from \"./a.js\"\n"),
                ("z.js", "import { x } from \"./shared.js\"\nimport { x } from \"./a.js\"\n"),
                ("a.js", "import { x } from \"./shared.js\"\nexport const a = 1\n"),
                ("m.js", "import c from \"./c.json\" with { type: \"json\" }\nexport const m = c\n"),
                ("shared.js", ""),
                ("c.json", "{}"),
            ],
            &["entry.js"]
        ),
        ["shared.js", "a.js", "z.js", "c.json", "m.js", "entry.js"],
        "what a module imports before it, in the order of its imports, each once: not the order of the paths"
    );
    assert_eq!(
        order(
            &[
                ("entry.js", "import { x } from \"./a.js\"\n"),
                ("a.js", "import { x } from \"./b.js\"\n"),
                (
                    "b.js",
                    "import { x } from \"./a.js\"\nimport { x } from \"./entry.js\"\nimport { x } from \"./b.js\"\n"
                ),
            ],
            &["entry.js"]
        ),
        ["b.js", "a.js", "entry.js"],
        "a cycle ends where it closes"
    );
    assert_eq!(
        order(
            &[
                ("one.js", "import { x } from \"./shared.js\"\n"),
                (
                    "two.js",
                    "import { x } from \"./own.js\"\nimport { x } from \"./shared.js\"\nimport { x } from \"./one.js\"\n"
                ),
                ("shared.js", ""),
                ("own.js", ""),
            ],
            &["two.js", "one.js", "two.js"]
        ),
        ["own.js", "shared.js", "one.js", "two.js"],
        "the roots in the order given; a root another root has imported is not evaluated again"
    );
}

#[test]
fn a_dynamic_import_does_not_affect_the_order() {
    assert_eq!(
        order(
            &[
                ("entry.js", "import { x } from \"./z.js\"\nconst lazy = import(\"./a.js\")\nconst late = import(\"./late.js\")\n"),
                ("a.js", "import { x } from \"./dep.js\"\nconst again = import(\"./entry.js\")\nconst more = import(\"./more.js\")\n"),
                ("z.js", "const same = import(\"./late.js\")\n"),
                ("late.js", "import { x } from \"./z.js\"\n"),
                ("dep.js", ""),
                ("more.js", ""),
            ],
            &["entry.js"]
        ),
        ["z.js", "entry.js", "late.js", "dep.js", "a.js", "more.js"],
        "what only `import(…)` reaches comes after the roots' modules, as found, with its own imports before it"
    );
}
