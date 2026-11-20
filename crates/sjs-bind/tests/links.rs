//! STORIES.md US-067: export and import resolution over real directories.

use sjs_bind::{Bound, Content, Disk, Links, ModuleGraph};
use sjs_testkit::fs::TempDir;
use std::path::{Path, PathBuf};

struct Program {
    _dir: TempDir,
    base: PathBuf,
    graph: ModuleGraph,
    links: Links,
    /// (file, code, line, column, message).
    found: Vec<(String, String, u32, u32, String)>,
}

fn program(files: &[(&str, &str)]) -> Program {
    let dir = TempDir::new("us067").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let (graph, _) =
        ModuleGraph::load(&Disk, &base, &[base.join("entry.js")]).expect("the root loads");
    let (links, diagnostics) = graph.link();
    let found = diagnostics
        .iter()
        .map(|d| {
            let at = d.span.start;
            (
                d.path.clone(),
                d.code.as_str().to_owned(),
                at.line,
                at.column,
                d.message.clone(),
            )
        })
        .collect();
    Program {
        _dir: dir,
        base,
        graph,
        links,
        found,
    }
}

impl Program {
    /// A binding as `file:line:column`, `namespace of file` or `json file`.
    fn show(&self, bound: Option<Bound>) -> String {
        let name = |path: &Path| self.graph.get(path).expect("a module").name.clone();
        match bound {
            None => "nothing".to_owned(),
            Some(Bound::Declaration { module, span }) => {
                format!(
                    "{}:{}:{}",
                    name(&module),
                    span.start.line,
                    span.start.column
                )
            }
            Some(Bound::Namespace(path)) => format!("namespace of {}", name(&path)),
            Some(Bound::Json(path)) => format!("json {}", name(&path)),
        }
    }

    fn import(&self, file: &str, local: &str) -> String {
        self.show(self.links.import(&self.base.join(file), local))
    }

    fn export(&self, file: &str, name: &str) -> String {
        self.show(self.links.export(&self.base.join(file), name))
    }
}

fn missing(
    file: &str,
    line: u32,
    column: u32,
    from: &str,
    name: &str,
) -> (String, String, u32, u32, String) {
    let message = format!("`{from}` does not export `{name}` (SPEC.md §5.1)");
    (file.to_owned(), "E206".to_owned(), line, column, message)
}

fn computed(file: &str, line: u32, column: u32, local: &str) -> (String, String, u32, u32, String) {
    let message = format!(
        "`{local}` is a namespace, read by the names of its exports only: `{local}.name` or `{local}[\"name\"]` (SPEC.md §4.6)"
    );
    (file.to_owned(), "E137".to_owned(), line, column, message)
}

#[test]
fn an_import_binds_to_the_declaration_through_re_exports() {
    let p = program(&[
        (
            "entry.js",
            concat!(
                "import { a, r as s, deep } from \"./lib.js\"\n",
                "import * as ns from \"./lib.js\"\n",
                "import cfg from \"./c.json\" with { type: \"json\" }\n",
                "export { a as b, ns, cfg, s }\n",
                "export let count = 1\n",
                "count += 1\n",
                "const local = 1\n",
                "export { local, local as other }\n",
                "export function f() {}\n",
                "export class K {}\n",
                "export const [x, { y }] = deep\n",
            ),
        ),
        (
            "lib.js",
            "export const a = 1\nconst q = 2\nexport { q as r }\nexport { deep } from \"./d1.js\"\n",
        ),
        ("d1.js", "export { deep } from \"./d2.js\"\n"),
        ("d2.js", "export { deep } from \"./d3.js\"\n"),
        ("d3.js", "export { deep } from \"./d4.js\"\n"),
        ("d4.js", "export { d as deep } from \"./sub/d5.js\"\n"),
        ("sub/d5.js", "export const d = 1\n"),
        ("c.json", "{}"),
    ]);
    assert_eq!(p.found, []);
    assert_eq!(p.import("entry.js", "a"), "lib.js:1:14");
    assert_eq!(p.import("entry.js", "s"), "lib.js:2:7", "renamed twice");
    assert_eq!(p.import("entry.js", "deep"), "sub/d5.js:1:14", "depth 5");
    assert_eq!(p.import("entry.js", "ns"), "namespace of lib.js");
    assert_eq!(p.import("entry.js", "cfg"), "json c.json");
    assert_eq!(p.import("entry.js", "r"), "nothing", "the local name binds");
    assert_eq!(p.import("entry.js", "count"), "nothing", "no import");
    assert_eq!(p.import("entry.js", "type"), "nothing", "an attribute");
    assert_eq!(p.export("entry.js", "deep"), "nothing", "an initializer");
    assert_eq!(p.import("nowhere.js", "a"), "nothing");
    assert_eq!(
        p.export("entry.js", "b"),
        "lib.js:1:14",
        "an import, exported"
    );
    assert_eq!(p.export("entry.js", "s"), "lib.js:2:7");
    assert_eq!(p.export("entry.js", "ns"), "namespace of lib.js");
    assert_eq!(p.export("entry.js", "cfg"), "json c.json");
    assert_eq!(p.export("entry.js", "count"), "entry.js:5:12", "a `let`");
    assert_eq!(p.export("entry.js", "local"), "entry.js:7:7");
    assert_eq!(p.export("entry.js", "other"), "entry.js:7:7");
    assert_eq!(p.export("entry.js", "f"), "entry.js:9:17");
    assert_eq!(p.export("entry.js", "K"), "entry.js:10:14");
    assert_eq!(p.export("entry.js", "x"), "entry.js:11:15");
    assert_eq!(p.export("entry.js", "y"), "entry.js:11:20");
    assert_eq!(p.export("entry.js", "a"), "nothing", "exported as `b` only");
    assert_eq!(p.export("lib.js", "q"), "nothing");
    assert_eq!(p.export("lib.js", "deep"), "sub/d5.js:1:14");
    assert_eq!(p.export("d4.js", "d"), "nothing");
    assert_eq!(p.export("c.json", "a"), "nothing");
    assert_eq!(p.export("nowhere.js", "a"), "nothing");
    assert_eq!(p.links.exports(&p.base.join("lib.js")), ["a", "deep", "r"]);
    assert_eq!(p.links.exports(&p.base.join("d4.js")), ["deep"]);
    assert_eq!(p.links.exports(&p.base.join("c.json")), Vec::<&str>::new());
    assert_eq!(
        p.links.exports(&p.base.join("nowhere.js")),
        Vec::<&str>::new()
    );
}

#[test]
fn a_name_the_module_does_not_export_is_e206_where_it_is_written() {
    let p = program(&[
        (
            "entry.js",
            concat!(
                "import { a, gone, lost as mine, q } from \"./lib.js\"\n",
                "import * as ns from \"./lib.js\"\n",
                "import { again } from \"./re.js\"\n",
                "export { nope } from \"./lib.js\"\n",
                "export { a as renamed, r as other, zip as a } from \"./lib.js\"\n",
                "ns.a\n",
                "ns.b\n",
                "ns[\"a\"]\n",
                "ns[\"c\"]\n",
                "ns?.d\n",
                "ns?.[\"e\"]\n",
                "again.a.b\n",
                "again.f()\n",
                "ns.r.s\n",
                "ns.q = 1\n",
            ),
        ),
        (
            "lib.js",
            "export const a = 1\nconst q = 2\nexport { q as r }\n",
        ),
        (
            "re.js",
            "import * as inner from \"./lib.js\"\nexport { inner as again }\nexport { gone } from \"./lib.js\"\n",
        ),
    ]);
    assert_eq!(
        p.found,
        [
            missing("entry.js", 1, 13, "lib.js", "gone"),
            missing("entry.js", 1, 19, "lib.js", "lost"),
            missing("entry.js", 1, 33, "lib.js", "q"),
            missing("entry.js", 4, 10, "lib.js", "nope"),
            missing("entry.js", 5, 36, "lib.js", "zip"),
            missing("entry.js", 7, 4, "lib.js", "b"),
            missing("entry.js", 9, 4, "lib.js", "c"),
            missing("entry.js", 10, 5, "lib.js", "d"),
            missing("entry.js", 11, 6, "lib.js", "e"),
            missing("entry.js", 13, 7, "lib.js", "f"),
            missing("entry.js", 15, 4, "lib.js", "q"),
            missing("re.js", 3, 10, "lib.js", "gone"),
        ],
        "a named import by the name asked for, a re-export, a key of a namespace, also one that came through a re-export; each where it is written, once"
    );
    assert_eq!(p.import("entry.js", "gone"), "nothing");
    assert_eq!(p.import("entry.js", "again"), "namespace of lib.js");
    assert_eq!(p.export("entry.js", "nope"), "nothing");
}

#[test]
fn a_namespace_read_by_a_key_that_is_no_string_is_e137() {
    let p = program(&[
        (
            "entry.js",
            concat!(
                "import * as ns from \"./lib.js\"\n",
                "import { a } from \"./lib.js\"\n",
                "const k = \"a\"\n",
                "ns[k]\n",
                "ns[0]\n",
                "ns[\"a\" + k]\n",
                "ns?.[k]\n",
                "ns[`a`]\n",
                "const f = (ns) => ns[k] + ns.zzz\n",
                "a[k]\n",
                "a.b\n",
                "a[\"b\"]\n",
                "k[k]\n",
                "k.length\n",
                "f(ns)\n",
                "ns\n",
            ),
        ),
        ("lib.js", "export const a = [1]\n"),
    ]);
    assert_eq!(
        p.found,
        [
            computed("entry.js", 4, 4, "ns"),
            computed("entry.js", 5, 4, "ns"),
            computed("entry.js", 6, 4, "ns"),
            computed("entry.js", 7, 6, "ns"),
            computed("entry.js", 8, 4, "ns"),
        ],
        "a name, a number, an expression, a template; a parameter named as the namespace, an import that is none and a constant are read as they like"
    );
}

#[test]
fn what_cannot_be_told_is_not_reported() {
    let p = program(&[
        (
            "entry.js",
            concat!(
                "import { x, undeclared } from \"./bad.js\"\n",
                "import { t } from \"./notext.js\"\n",
                "import { q } from \"./nowhere.js\"\n",
                "import { l } from \"lodash\"\n",
                "import * as far from \"./nowhere.js\"\n",
                "import * as broken from \"./bad.js\"\n",
                "import { round, shown } from \"./a.js\"\n",
                "import { w } from \"./rejected.js\"\n",
                "export { w2 } from \"./nowhere.js\"\n",
                "export { never }\n",
                "export { lazy as w3 } from \"./nowhere.js\"\n",
                "far.a\n",
                "broken.a\n",
                "broken[x]\n",
                "const lazy = import(\"./a.js\")\n",
                "const none = import(\"./nowhere.js\")\n",
            ),
        ),
        ("bad.js", "export const = 1\n"),
        (
            "a.js",
            "export { round } from \"./b.js\"\nexport { missing as shown }\n",
        ),
        ("b.js", "export { round } from \"./a.js\"\n"),
        (
            "rejected.js",
            "import { nothing } from \"./b.js\"\nexport const w = \n",
        ),
    ]);
    std::fs::write(p.base.join("notext.js"), b"\xff").expect("write");
    let reloaded =
        ModuleGraph::load(&Disk, &p.base, &[p.base.join("entry.js")]).expect("the root loads");
    let (links, diagnostics) = reloaded.0.link();
    let rejected = |file: &str| {
        let module = reloaded.0.get(&p.base.join(file)).expect("a module");
        match &module.content {
            Content::Script { parsed, .. } => parsed
                .iter()
                .any(|d| matches!(d.code.as_str(), "E000" | "E001" | "E002")),
            Content::Invalid(_) | Content::Json => true,
        }
    };
    assert!(rejected("bad.js") && rejected("rejected.js") && rejected("notext.js"));
    assert!(!rejected("entry.js") && !rejected("a.js"));
    let found: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.path.as_str(), d.code.as_str(), d.span.start.line))
        .collect();
    assert_eq!(
        found,
        [("entry.js", "E137", 14)],
        "nothing is said of what a file the grammar rejects exports, nor by one; of a path that names no module; of re-exports that go round; a namespace's key is literal whatever it names"
    );
    let entry = p.base.join("entry.js");
    for local in ["x", "t", "q", "l", "far", "round", "shown", "w"] {
        assert_eq!(links.import(&entry, local), None, "{local}");
    }
    assert_eq!(
        links.import(&entry, "broken"),
        Some(Bound::Namespace(p.base.join("bad.js")))
    );
    assert_eq!(links.export(&entry, "w2"), None);
    assert_eq!(links.export(&entry, "never"), None);
    assert_eq!(
        links.export(&entry, "w3"),
        None,
        "not the `lazy` of this file"
    );
    assert_eq!(links.export(&p.base.join("a.js"), "round"), None);
    assert_eq!(links.export(&p.base.join("rejected.js"), "w"), None);
    let imports = &reloaded.0.get(&entry).expect("entry").imports;
    let namespaces: Vec<_> = imports
        .iter()
        .filter_map(sjs_bind::Import::namespace)
        .collect();
    assert_eq!(
        namespaces,
        [Bound::Namespace(p.base.join("a.js"))],
        "`import(…)` of a module is its namespace; a static import and a path that names none are not"
    );
}
