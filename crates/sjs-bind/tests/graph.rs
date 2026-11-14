//! STORIES.md US-060: the module graph over real directories.

use sjs_bind::{Content, Disk, Host, ModuleGraph};
use sjs_diag::Code;
use sjs_testkit::fs::TempDir;
use std::cell::RefCell;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

const MISSING: &str =
    "names no file: a path is spelled as its directory entries are, letter case included (SPEC.md §5.1)";

/// The disk, recording what is asked of it and refusing to read the files named.
#[derive(Default)]
struct Watched {
    refused: Vec<PathBuf>,
    reads: RefCell<Vec<PathBuf>>,
    listings: RefCell<Vec<PathBuf>>,
}

impl Host for Watched {
    fn canonical(&self, path: &Path) -> io::Result<PathBuf> {
        Disk.canonical(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_path_buf());
        if self.refused.iter().any(|refused| refused == path) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "refused"));
        }
        Disk.read(path)
    }

    fn names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        self.listings.borrow_mut().push(dir.to_path_buf());
        Disk.names(dir)
    }

    fn is_file(&self, path: &Path) -> bool {
        Disk.is_file(path)
    }
}

fn project(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let dir = TempDir::new("us060").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    (dir, base)
}

/// The graph from `entry.js`, with its findings as (file, line, column, message).
fn load(host: &dyn Host, base: &Path) -> (ModuleGraph, Vec<(String, u32, u32, String)>) {
    let (graph, diagnostics) =
        ModuleGraph::load(host, base, &[base.join("entry.js")]).expect("the root loads");
    let found = diagnostics
        .iter()
        .map(|d| {
            assert_eq!(d.code, Code::E205);
            let at = d.span.start;
            (d.path.clone(), at.line, at.column, d.message.clone())
        })
        .collect();
    (graph, found)
}

fn names(graph: &ModuleGraph) -> Vec<&str> {
    graph.modules().map(|module| module.name.as_str()).collect()
}

#[test]
fn the_closure_of_the_roots_imports_is_loaded_in_path_order() {
    let (_dir, base) = project(&[
        (
            "entry.js",
            concat!(
                "import { a } from \"./lib/a.js\"\n",
                "import cfg from \"./data/c.json\" with { type: \"json\" }\n\n",
                "const lazy = await import(\"./lazy.js\")\n",
            ),
        ),
        (
            "lib/a.js",
            "export { b } from \"../x/b.js\"\nexport const a = 1\n",
        ),
        ("x/b.js", "export const b = 2\n"),
        ("lazy.js", "export const z = 3\n"),
        ("data/c.json", "{\"k\": [1, 2]}\n"),
        ("unused.js", "export const u = 0\n"),
    ]);
    let (graph, found) = load(&Disk, &base);
    assert_eq!(found, Vec::new());
    assert_eq!(
        names(&graph),
        ["data/c.json", "entry.js", "lazy.js", "lib/a.js", "x/b.js"],
        "by canonical path; `unused.js` is no part of the program"
    );
    assert_eq!(graph.roots, [base.join("entry.js")]);
    let entry = graph.get(&base.join("entry.js")).expect("the root");
    assert_eq!(entry.path, base.join("entry.js"));
    let edges: Vec<_> = entry
        .imports
        .iter()
        .map(|i| {
            let at = i.span.start;
            (
                i.specifier.as_str(),
                i.dynamic,
                at.line,
                at.column,
                i.target.clone(),
            )
        })
        .collect();
    assert_eq!(
        edges,
        [
            ("./lib/a.js", false, 1, 19, Some(base.join("lib/a.js"))),
            (
                "./data/c.json",
                false,
                2,
                17,
                Some(base.join("data/c.json"))
            ),
            ("./lazy.js", true, 4, 27, Some(base.join("lazy.js"))),
        ]
    );
    assert!(
        matches!(&entry.content, Content::Script { source, tree, parsed }
        if source.path() == "entry.js" && tree.print(source) == source.text() && parsed.is_empty())
    );
    let json = graph
        .get(&base.join("data/c.json"))
        .expect("the JSON module");
    assert_eq!((&json.content, json.imports.len()), (&Content::Json, 0));
    assert_eq!(json.path, base.join("data/c.json"));
    let a = graph.get(&base.join("lib/a.js")).expect("a");
    assert_eq!(
        a.imports[0].target,
        Some(base.join("x/b.js")),
        "a re-export is an import"
    );
    assert!(graph.get(&base.join("unused.js")).is_none());
}

#[test]
fn a_file_reached_by_several_spellings_is_loaded_once() {
    let (dir, base) = project(&[
        (
            "entry.js",
            concat!(
                "import { a } from \"./a.js\"\n",
                "import { a as b } from \"././a.js\"\n",
                "import { a as c } from \"./sub/../a.js\"\n",
                "import { a as d } from \"./sub/up.js\"\n",
                "import { a as e } from \"./link.js\"\n",
                "import j from \"./j.json\" with { type: \"json\" }\n",
                "import k from \"./sub/../j.json\" with { type: \"json\" }\n",
            ),
        ),
        ("a.js", "export const a = 1\n"),
        ("sub/up.js", "export { a } from \"../a.js\"\n"),
        ("j.json", "[]"),
    ]);
    #[cfg(unix)]
    std::os::unix::fs::symlink(base.join("a.js"), dir.path().join("link.js")).expect("symlink");
    #[cfg(not(unix))]
    dir.write("link.js", "export const a = 1\n")
        .expect("the file");
    let host = Watched::default();
    let (graph, found) = load(&host, &base);
    assert_eq!(found, Vec::new());
    #[cfg(unix)]
    {
        assert_eq!(names(&graph), ["a.js", "entry.js", "j.json", "sub/up.js"]);
        let entry = graph.get(&base.join("entry.js")).expect("the root");
        let targets: Vec<_> = entry.imports.iter().map(|i| i.target.clone()).collect();
        let a = Some(base.join("a.js"));
        let j = Some(base.join("j.json"));
        let up = Some(base.join("sub/up.js"));
        assert_eq!(
            targets,
            [a.clone(), a.clone(), a.clone(), up, a, j.clone(), j]
        );
    }
    let reads = host.reads.borrow();
    assert_eq!(
        reads.len(),
        graph.modules().count(),
        "each file is read once"
    );
}

#[test]
fn a_path_that_names_no_module_is_e205_on_the_string() {
    let (_dir, base) = project(&[
        (
            "entry.js",
            concat!(
                "import { a } from \"./gone.js\"\n",
                "import { b } from \"./Lib.js\"\n",
                "import { c } from \"./dir.js\"\n",
                "import { d } from \"./\"\n",
                "import { e } from \"./lib.js/\"\n",
                "import { f } from \"./lib.js/f.js\"\n",
                "import { g } from \"./nowhere/g.js\"\n",
                "import { h } from \"./LIB/h.js\"\n",
                "import i from \"./empty.json\" with { type: \"json\" }\n",
                "import j from \"./bad.json\" with { type: \"json\" }\n",
                "import k from \"./bad.json\" with { type: \"json\" }\n",
                "import l from \"./gone.json\" with { type: \"json\" }\n",
                "export { m } from \"./gone.js\"\n",
                "const n = await import(\"./gone.js\")\n",
            ),
        ),
        ("lib.js", "export const b = 1\n"),
        ("dir.js/inner.js", "export const c = 1\n"),
        ("lib/h.js", "export const h = 1\n"),
        ("empty.json", ""),
        ("bad.json", "{\"a\": 1,}\n"),
    ]);
    let (graph, found) = load(&Disk, &base);
    let at = |line: u32, column: u32, path: &str, why: &str| {
        (
            "entry.js".to_owned(),
            line,
            column,
            format!("`{path}` {why}"),
        )
    };
    let directory = "names a directory, not a file (SPEC.md §5.1)";
    let not_json = "is not JSON (RFC 8259) (SPEC.md §5.1)";
    assert_eq!(
        found,
        [
            at(1, 19, "./gone.js", MISSING),
            at(2, 19, "./Lib.js", MISSING),
            at(3, 19, "./dir.js", directory),
            at(4, 19, "./", directory),
            at(5, 19, "./lib.js/", directory),
            at(6, 19, "./lib.js/f.js", MISSING),
            at(7, 19, "./nowhere/g.js", MISSING),
            at(8, 19, "./LIB/h.js", MISSING),
            at(9, 15, "./empty.json", not_json),
            at(10, 15, "./bad.json", not_json),
            at(11, 15, "./bad.json", not_json),
            at(12, 15, "./gone.json", MISSING),
            at(13, 19, "./gone.js", MISSING),
            at(14, 24, "./gone.js", MISSING),
        ]
    );
    assert_eq!(
        names(&graph),
        ["entry.js"],
        "what names no module adds none"
    );
    let entry = graph.get(&base.join("entry.js")).expect("the root");
    assert!(entry.imports.iter().all(|import| import.target.is_none()));
    assert_eq!(entry.imports.len(), 14);
}

#[cfg(unix)]
#[test]
fn a_link_to_nothing_or_to_a_file_of_another_kind_is_e205() {
    let (dir, base) = project(&[
        (
            "entry.js",
            concat!(
                "import { a } from \"./alias.js\"\n",
                "import { b } from \"./dangling.js\"\n",
                "import c from \"./code.json\" with { type: \"json\" }\n",
            ),
        ),
        ("data.json", "1\n"),
        ("real.js", "export const r = 1\n"),
    ]);
    let link = |target: &str, name: &str| {
        std::os::unix::fs::symlink(base.join(target), dir.path().join(name)).expect("symlink");
    };
    link("data.json", "alias.js");
    link("nothing.js", "dangling.js");
    link("real.js", "code.json");
    let (graph, found) = load(&Disk, &base);
    let whys: Vec<_> = found.iter().map(|f| (f.1, f.3.clone())).collect();
    let other = "is a link to a file of another kind (SPEC.md §5.1)";
    assert_eq!(
        whys,
        [
            (1, format!("`./alias.js` {other}")),
            (2, format!("`./dangling.js` {MISSING}")),
            (3, format!("`./code.json` {other}")),
        ]
    );
    assert_eq!(names(&graph), ["entry.js"]);
}

#[test]
fn a_file_that_cannot_be_read_is_e205_and_is_asked_for_once() {
    let (_dir, base) = project(&[
        (
            "entry.js",
            "import { a } from \"./a.js\"\nimport { s } from \"./secret.js\"\n",
        ),
        (
            "a.js",
            "import { s } from \"./secret.js\"\n\nexport const a = s\n",
        ),
        ("secret.js", "export const s = 1\n"),
    ]);
    let host = Watched {
        refused: vec![base.join("secret.js")],
        ..Watched::default()
    };
    let (graph, found) = load(&host, &base);
    let why = "`./secret.js` cannot be read: refused (SPEC.md §5.1)".to_owned();
    assert_eq!(
        found,
        [
            ("a.js".to_owned(), 1, 19, why.clone()),
            ("entry.js".to_owned(), 2, 19, why)
        ]
    );
    assert_eq!(names(&graph), ["a.js", "entry.js"]);
    let reads = host.reads.borrow();
    let asked = reads.iter().filter(|p| p.ends_with("secret.js")).count();
    assert_eq!(asked, 1);
}

#[test]
fn cycles_terminate_and_what_the_parser_reported_is_not_reported_again() {
    let (_dir, base) = project(&[
        (
            "entry.js",
            concat!(
                "import { e } from \"./entry.js\"\n",
                "import { p } from \"./peer.js\"\n",
                "import { l } from \"lodash\"\n",
                "import c from \"./gone.json\"\n",
                "import { w } from \"./gone.js\" with { type: \"json\" }\n",
                "export { j } from \"./gone.json\"\n",
                "const d = await import(\"./gone.json\")\n",
                "const t = \"./gone.js\"\n",
            ),
        ),
        (
            "peer.js",
            "import { e } from \"./entry.js\"\n\nexport const p = e\n",
        ),
    ]);
    let (graph, found) = load(&Disk, &base);
    assert_eq!(
        found,
        Vec::new(),
        "E201, E205 and E000 are the parser's on these"
    );
    assert_eq!(names(&graph), ["entry.js", "peer.js"]);
    let entry = graph.get(&base.join("entry.js")).expect("the root");
    let targets: Vec<_> = entry.imports.iter().map(|i| i.target.clone()).collect();
    assert_eq!(
        targets,
        [Some(base.join("entry.js")), Some(base.join("peer.js"))]
    );
    assert!(matches!(&entry.content, Content::Script { parsed, .. } if parsed.len() == 5));
}

#[test]
fn a_module_that_is_no_text_is_in_the_graph_with_its_e000() {
    let (dir, base) = project(&[("entry.js", "import { a } from \"./a.js\"\n")]);
    std::fs::write(
        dir.path().join("a.js"),
        b"import { b } from \"./b.js\"\n\xff",
    )
    .expect("write");
    let (graph, found) = load(&Disk, &base);
    assert_eq!(found, Vec::new());
    let a = graph.get(&base.join("a.js")).expect("a");
    assert!(matches!(&a.content, Content::Invalid(d) if d.code == Code::E000 && d.path == "a.js"));
    assert_eq!(a.imports, Vec::new());
}

#[test]
fn roots_are_canonical_counted_once_and_must_load() {
    let (_dir, base) = project(&[
        ("src/index.js", "import { t } from \"../tools/t.js\"\n"),
        ("tools/t.js", "export const t = 1\n"),
    ]);
    let roots = [
        base.join("src/index.js"),
        base.join("tools/t.js"),
        base.join("src/../src/./index.js"),
    ];
    let (graph, diagnostics) = ModuleGraph::load(&Disk, &base.join("src"), &roots).expect("loads");
    assert!(diagnostics.is_empty());
    assert_eq!(
        graph.roots,
        [base.join("src/index.js"), base.join("tools/t.js")]
    );
    let outside = base.join("tools/t.js").display().to_string();
    assert_eq!(
        names(&graph),
        ["index.js", outside.as_str()],
        "a file outside the base keeps its path"
    );
    let (graph, _) =
        ModuleGraph::load(&Disk, &base.join("no/such/base"), &roots[..1]).expect("loads");
    assert_eq!(
        names(&graph).len(),
        2,
        "a base that does not exist is used as given"
    );

    let gone = base.join("gone.js");
    let error =
        ModuleGraph::load(&Disk, &base, &[roots[0].clone(), gone.clone()]).expect_err("no root");
    assert_eq!(
        (error.path, error.error.kind()),
        (gone, io::ErrorKind::NotFound)
    );
    let host = Watched {
        refused: vec![roots[1].clone()],
        ..Watched::default()
    };
    let error = ModuleGraph::load(&host, &base, &roots).expect_err("unreadable");
    assert_eq!(
        (error.path, error.error.kind()),
        (roots[1].clone(), io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn ten_thousand_modules_load_with_one_read_each_and_one_listing() {
    const COUNT: usize = 10_000;
    let dir = TempDir::new("us060-many").expect("a temporary directory");
    let mut entry = String::new();
    for n in 0..COUNT {
        // Each module imports the next (a chain as deep as the program) and the root
        // imports every tenth (a wide fan).
        let next = (n + 1) % COUNT;
        let text = format!("import {{ v as w }} from \"./m{next}.js\"\n\nexport const v = w\n");
        dir.write(&format!("m{n}.js"), &text).expect("the file");
        if n % 10 == 0 {
            writeln!(entry, "import {{ v as v{n} }} from \"./m{n}.js\"").expect("a string");
        }
    }
    dir.write("entry.js", &entry).expect("the root");
    let base = dir.path().canonicalize().expect("canonical");
    let host = Watched::default();
    let (graph, found) = load(&host, &base);
    assert_eq!(found, Vec::new());
    assert_eq!(graph.modules().count(), COUNT + 1);
    assert_eq!(
        host.reads.borrow().len(),
        COUNT + 1,
        "linear: one read for each module"
    );
    assert_eq!(
        *host.listings.borrow(),
        [base],
        "and one listing for each directory"
    );
}

#[test]
fn the_tests_of_explicit_entries_are_those_beside_the_graphs_modules() {
    let (dir, base) = project(&[
        (
            "tools/entry.js",
            concat!(
                "import { a } from \"./lib/a.js\"\n",
                "import c from \"../data/c.json\" with { type: \"json\" }\n",
            ),
        ),
        ("tools/lib/a.js", "export const a = 1\n"),
        ("data/c.json", "{}\n"),
        ("tools/entry.test.js", ""),
        ("tools/b.test.js", ""),
        ("tools/lib/a.test.js", ""),
        ("tools/lib/a.test.json", ""),
        ("tools/lib/test.js", ""),
        ("tools/lib/deeper/d.test.js", ""),
        ("data/c.test.js", ""),
        ("src/other.test.js", ""),
        ("tools/dir.test.js/inner.js", ""),
    ]);
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        base.join("tools/b.test.js"),
        dir.path().join("tools/lib/z.test.js"),
    )
    .expect("symlink");
    let root = [base.join("tools/entry.js")];
    let (graph, _) = ModuleGraph::load(&Disk, &base, &root).expect("loads");
    assert_eq!(
        graph.test_files(&Disk),
        [
            base.join("data/c.test.js"),
            base.join("tools/b.test.js"),
            base.join("tools/entry.test.js"),
            base.join("tools/lib/a.test.js"),
        ],
        "sorted, each once, files only, not beneath a module's directory or elsewhere"
    );
    drop(dir);
}

#[test]
fn the_disk_lists_names_as_spelled_and_fails_on_what_is_no_directory() {
    let (_dir, base) = project(&[("Mixed.js", ""), ("sub/x.js", "")]);
    let mut listed = Disk.names(&base).expect("a directory");
    listed.sort();
    assert_eq!(listed, [OsString::from("Mixed.js"), OsString::from("sub")]);
    assert!(Disk.names(&base.join("Mixed.js")).is_err());
    assert!(Disk.is_file(&base.join("Mixed.js")) && !Disk.is_file(&base.join("sub")));
    assert_eq!(Disk.read(&base.join("Mixed.js")).expect("readable"), b"");
    assert!(Disk.canonical(&base.join("gone")).is_err());
}
