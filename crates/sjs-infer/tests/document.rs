//! STORIES.md US-100: determinism and `types.json`.

use sjs_bind::{Disk, ModuleGraph};
use sjs_diag::{Json, TypesDocument};
use sjs_infer::{infer_program, types_document, types_json, KINDS};
use sjs_testkit::fs::TempDir;
use sjs_types::Types;
use std::path::PathBuf;

const SCHEMA: &str = include_str!("../../../schema/types.schema.json");

/// The `types.json` of the files, found from `roots` in the order given.
fn written(dir: &TempDir, roots: &[String]) -> String {
    let base = dir.path().canonicalize().expect("canonical");
    let roots: Vec<PathBuf> = roots.iter().map(|root| base.join(root)).collect();
    let (graph, _) = ModuleGraph::load(&Disk, &base, &roots).expect("loads");
    let (links, _) = graph.link();
    types_json(&graph, &links)
}

fn program(files: &[(&str, &str)], roots: &[&str]) -> TypesDocument {
    let dir = TempDir::new("us100").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let roots: Vec<String> = roots.iter().map(|root| (*root).to_owned()).collect();
    let text = written(&dir, &roots);
    let document = TypesDocument::from_json(&text).expect("read back");
    assert_eq!(
        document.to_json(),
        text,
        "the reader gives what the writer was given"
    );
    document
}

/// `path: notation` of each export of each module, a module the grammar rejects
/// marked `!`.
fn listed(document: &TypesDocument) -> Vec<String> {
    let mut lines = Vec::new();
    for module in &document.modules {
        let mark = if module.syntax_error { "!" } else { "" };
        for export in &module.exports {
            let inferred = if export.inferred {
                ""
            } else {
                " (not inferred)"
            };
            lines.push(format!(
                "{}{mark}: {}{inferred}",
                module.path, export.notation
            ));
        }
    }
    lines
}

/// The text of the module at `at` of `count`: it takes from two modules after it,
/// and gives values of several kinds.
fn module_text(at: usize, count: usize) -> String {
    let next = [(at + 1) % count, (at + 7) % count];
    let later: Vec<usize> = next.into_iter().filter(|other| *other > at).collect();
    let imports: Vec<String> = later
        .iter()
        .map(|other| format!("import {{ double{other} }} from \"./m{other}.js\"\n"))
        .collect();
    let used: Vec<String> = later
        .iter()
        .map(|other| format!(" + double{other}({other}.5)"))
        .collect();
    let (imports, used) = (imports.concat(), used.concat());
    // Every fifth module has a generic function and a generic class too.
    let more = if at.is_multiple_of(5) {
        format!(
            "\nexport const pair{at} = (a, b) => [a, b]\n\nexport class Box{at} {{\n  held\n  constructor(held) {{\n    this.held = held\n  }}\n}}\n\nexport const box{at} = new Box{at}(\"b{at}\")\n"
        )
    } else {
        String::new()
    };
    format!(
        "{imports}\nexport const value{at} = {at}{used}\n\nexport const double{at} = (n) => n * 2\n{more}\nexport const shape{at} = {{ id: {at}, tag: \"m{at}\", list: [value{at}, 1.5] }}\n"
    )
}

#[test]
fn the_document_does_not_depend_on_the_order_modules_are_found_in() {
    let dir = TempDir::new("us100-shuffle").expect("a temporary directory");
    let count = 50;
    let mut names = Vec::new();
    for at in 0..count {
        let name = format!("m{at}.js");
        dir.write(&name, &module_text(at, count)).expect("the file");
        names.push(name);
    }
    let first = written(&dir, &names);
    let document = TypesDocument::from_json(&first).expect("read back");
    assert_eq!(document.modules.len(), count);
    assert_eq!(document.modules[0].path, "m0.js", "by path");
    let fifth = document
        .modules
        .iter()
        .find(|module| module.path == "m5.js");
    let exports = &fifth.expect("the module").exports;
    let exported: Vec<&str> = exports.iter().map(|e| e.notation.as_str()).collect();
    assert_eq!(
        exported,
        [
            "Box5<T>(held: T): Box5<T>",
            "box5: Box5<string>",
            "double5(n: number): number",
            "pair5<T, U>(a: T, b: U): readonly (T | U)[]",
            "shape5: { id: 5, tag: \"m5\", list: readonly number[] }",
            "value5: number"
        ],
        "by name; an export nothing calls is generic"
    );
    // A hundred other orders, each a shuffle of the one before.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    for round in 0..100 {
        for at in (1..names.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let other = usize::try_from(state >> 33).expect("fits") % (at + 1);
            names.swap(at, other);
        }
        assert!(
            written(&dir, &names) == first,
            "order {round} gives another document"
        );
    }
}

#[test]
fn every_export_has_its_notation_and_its_tree() {
    let document = program(
        &[
            ("a.js", "import { helper } from \"./b.js\"\nimport data from \"./c.json\" with { type: \"json\" }\n\nexport const greet = (name) => `hello ${name}`\n\nexport const used = greet(helper(\"a\"))\n\nexport const identity = (value) => value\n\nexport const later = notKnown()\n\nexport const config = data\n\nconst inner = 1\n\nexport { inner as renamed }\n"),
            ("b.js", "export const helper = (text) => text.length\n"),
            ("c.json", "{ \"a\": 1 }\n"),
        ],
        &["a.js"],
    );
    assert_eq!(
        listed(&document),
        [
            "a.js: config: unknown",
            "a.js: greet(name: int32): string",
            "a.js: identity<T>(value: T): T",
            "a.js: later: unknown (not inferred)",
            "a.js: renamed: 1",
            "a.js: used: string",
            "b.js: helper(text: string): int32"
        ],
        "a JSON module is no module of exports; what has no type is `unknown` and says so"
    );
    let empty = program(&[("only.js", "const hidden = 1\n")], &["only.js"]);
    assert_eq!(
        empty.to_json(),
        "{\n  \"version\": 1,\n  \"modules\": [\n    {\n      \"path\": \"only.js\",\n      \"syntaxError\": false,\n      \"exports\": []\n    }\n  ]\n}\n"
    );
    let nothing = TypesDocument::default();
    assert_eq!(
        TypesDocument::from_json(&nothing.to_json()),
        Ok(nothing),
        "an empty program"
    );
}

#[test]
fn a_module_the_grammar_rejects_is_listed_with_its_exports_unknown() {
    let document = program(
        &[
            ("a.js", "import { broken } from \"./bad.js\"\nimport data from \"./c.json\" with { type: \"json\" }\n\nexport const fine = 1\n\nexport const taken = broken\n\nexport { broken as passed } from \"./bad.js\"\n\nexport { data }\n"),
            ("bad.js", "const hidden = 2\n\nexport const broken = 1\n\nexport const also = (\n"),
            ("c.json", "1\n"),
        ],
        &["a.js"],
    );
    let lines = listed(&document);
    assert_eq!(
        lines[..4],
        [
            "a.js: data: unknown (not inferred)",
            "a.js: fine: 1",
            "a.js: passed: unknown (not inferred)",
            "a.js: taken: unknown (not inferred)"
        ],
        "what is bound to no declaration that has a type is not inferred"
    );
    assert!(
        !lines.iter().any(|line| line.contains("hidden")),
        "{lines:?}"
    );
    assert!(
        lines[4..].iter().all(|line| line.starts_with("bad.js!: ")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"bad.js!: broken: unknown (not inferred)".to_owned()),
        "{lines:?}"
    );
    let bad = &document.modules[1];
    let unknown = Json::map(vec![("kind", Json::text("unknown"))]);
    assert!(bad.syntax_error);
    assert!(bad.exports.iter().all(|export| export.structure == unknown));
}

/// Every object of a document that is a tree of a type: one with a `kind`.
fn trees(document: &Json) -> Vec<&Json> {
    let mut found = Vec::new();
    let mut pending = vec![document];
    while let Some(value) = pending.pop() {
        match value {
            Json::Map(inside) => {
                found.extend(value.get("kind").map(|_| value));
                pending.extend(inside.iter().map(|(_, member)| member));
            }
            Json::List(elements) => pending.extend(elements),
            _ => {}
        }
    }
    found
}

const EVERY_KIND: &str = "export const n = 1.5 + 1\nexport let i = 1\nexport const big = 10n\nexport let bigs = 20n\nexport let s = \"a\"\nexport let flag = true\nexport const nothing = undefined\nexport const lit = [\"a\", 1, 2n, true]\nexport const parsed = JSON.parse(\"1\")\nexport const thrower = () => {\n  throw new Error(\"no\")\n}\nexport const dict = Object.fromEntries([[\"a\", 1]])\nlet grown = [1]\ngrown.push(2)\nexport { grown }\nexport const fn = (a, b = 1, ...rest) => [a, b, rest]\nexport const called = fn(1)\nexport class Box {\n  held\n  constructor(held) {\n    this.held = held\n  }\n}\nexport const box = new Box(1)\nexport const pending = Promise.resolve(1)\nexport const walk = [1].values()\nexport function* gen() {\n  yield 1\n}\nexport async function* agen() {\n  yield await pending\n}\nexport const made = [gen(), agen()]\nexport const map = new Map([[\"a\", 1]])\nexport const set = new Set([1])\nconst key = { id: 1 }\nexport const weak = new WeakMap([[key, 1]])\nexport const found = map.get(\"a\")\nexport const absent = /a/v.exec(\"a\")\n";

#[test]
fn the_tree_of_a_type_says_its_kind_and_the_schema_lists_every_kind() {
    let dir = TempDir::new("us100-kinds").expect("a temporary directory");
    dir.write("a.js", EVERY_KIND).expect("the file");
    let text = written(&dir, &["a.js".to_owned()]);
    let document = Json::read(&text).expect("JSON");
    let kind = |tree: &&Json| tree.get("kind").and_then(Json::as_text).map(str::to_owned);
    let found: Vec<String> = trees(&document).iter().filter_map(kind).collect();
    for kind in KINDS {
        assert!(
            found.iter().any(|seen| seen == kind),
            "{kind} is not written"
        );
    }
    assert!(
        found.iter().all(|seen| KINDS.contains(&seen.as_str())),
        "{found:?}"
    );
    // The schema is JSON the compiler's own reader reads, and lists what is written.
    let schema = Json::read(SCHEMA).expect("the schema is JSON");
    let texts = |value: Option<&Json>| -> Vec<String> {
        let list = value.and_then(Json::as_list).unwrap_or_default();
        let texts = list.iter().filter_map(Json::as_text);
        texts.map(str::to_owned).collect()
    };
    let definition = |name: &str| schema.get("$defs").and_then(|defs| defs.get(name));
    let required = |name: &str| texts(definition(name).and_then(|d| d.get("required")));
    let of_type = definition("type").and_then(|ty| ty.get("properties"));
    let kinds = of_type.and_then(|p| p.get("kind"));
    assert_eq!(
        texts(kinds.and_then(|kind| kind.get("enum"))),
        KINDS,
        "the kinds of the schema"
    );
    assert_eq!(texts(schema.get("required")), ["version", "modules"]);
    assert_eq!(required("module"), ["path", "syntaxError", "exports"]);
    assert_eq!(
        required("export"),
        ["name", "inferred", "type", "structure"]
    );
    // Every member a tree is written with is one the schema describes.
    let mut members = Vec::new();
    for tree in trees(&document) {
        if let Json::Map(inside) = tree {
            members.extend(inside.iter().map(|(key, _)| key.clone()));
        }
    }
    members.sort();
    members.dedup();
    let mut known = Vec::new();
    if let Some(Json::Map(described)) = of_type {
        known.extend(described.iter().map(|(key, _)| key.clone()));
    }
    known.sort();
    assert_eq!(
        members, known,
        "the members of a tree, and those the schema describes"
    );
}

#[test]
fn a_tree_is_written_kind_by_kind() {
    let dir = TempDir::new("us100-tree").expect("a temporary directory");
    dir.write("a.js", "export const pick = (xs, at = 0) => xs[at]\n")
        .expect("the file");
    let base = dir.path().canonicalize().expect("canonical");
    let (graph, _) = ModuleGraph::load(&Disk, &base, &[base.join("a.js")]).expect("loads");
    let (links, _) = graph.link();
    let mut types = Types::new();
    let modules = infer_program(&graph, &links, &mut types);
    let document = types_document(&graph, &links, &modules, &mut types);
    let export = &document.modules[0].exports[0];
    assert_eq!(
        export.notation,
        "pick<T>(xs: readonly T[], at: int32): T | undefined"
    );
    assert_eq!(
        export.structure.write(),
        "{\n  \"kind\": \"function\",\n  \"typeParameters\": [\n    \"T\"\n  ],\n  \"parameters\": [\n    {\n      \"optional\": false,\n      \"type\": {\n        \"kind\": \"array\",\n        \"readonly\": true,\n        \"element\": {\n          \"kind\": \"variable\",\n          \"name\": \"T\"\n        }\n      }\n    },\n    {\n      \"optional\": false,\n      \"type\": {\n        \"kind\": \"int32\"\n      }\n    }\n  ],\n  \"rest\": null,\n  \"returns\": {\n    \"kind\": \"union\",\n    \"members\": [\n      {\n        \"kind\": \"variable\",\n        \"name\": \"T\"\n      },\n      {\n        \"kind\": \"undefined\"\n      }\n    ]\n  }\n}\n"
    );
}
