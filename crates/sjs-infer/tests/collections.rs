//! STORIES.md US-087: arrays, tuples, empty collections and what mutates them.

use sjs_bind::{bind, Disk, ModuleGraph};
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, infer_program, Facts};
use sjs_syntax::parse;
use sjs_testkit::fs::TempDir;
use sjs_types::{ClassId, Printer, Types};

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

/// The type inferred for `x` (`?` for none), the findings as `CODE @line:column text`,
/// and their messages.
fn checked(text: &str) -> (String, Vec<String>, Vec<String>) {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
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
    let messages = diagnostics.iter().map(|d| d.message.clone()).collect();
    let x = bindings.declarations.iter().position(|d| d.name == "x");
    let ty = x.and_then(|at| facts.declarations.get(&at));
    let printed = ty.map_or("?".to_owned(), |&ty| {
        Printer::new(&types, &class_name).print(ty)
    });
    (printed, found, messages)
}

fn typed(text: &str) -> String {
    let (printed, found, _) = checked(text);
    assert_eq!(found, Vec::<String>::new(), "{text}");
    printed
}

fn found(text: &str) -> Vec<String> {
    checked(text).1
}

const NONE: [&str; 0] = [];

#[test]
fn an_array_literal_is_a_tuple_for_a_const_nothing_mutates() {
    assert_eq!(typed("const x = [1, 2]\n"), "[1, 2]");
    assert_eq!(typed("const x = [1, \"a\"]\n"), "[1, \"a\"]");
    assert_eq!(
        typed("let x = [1, \"a\"]\n"),
        "readonly (int32 | string)[]",
        "a `let`: an array of the union"
    );
    assert_eq!(typed("let x = [1]\nx = [2, 3]\n"), "readonly int32[]");
    assert_eq!(
        typed("const x = [1, 2]\nconst y = x.length + x[0]\nconst [a, b] = x\n"),
        "[1, 2]",
        "read, not mutated"
    );
    assert_eq!(
        typed("const x = [[1, 2], { k: [3] }]\n"),
        "[readonly int32[], { k: readonly int32[] }]",
        "a literal inside it is an array"
    );
    assert_eq!(typed("const x = [...ys]\n"), "?");
    assert_eq!(typed("const x = [1, f()]\n"), "?");
    assert_eq!(
        typed("const y = [1, 2]\nconst x = y\n"),
        "[1, 2]",
        "through another binding"
    );
    assert_eq!(
        typed("const y = [1, \"a\"]\nconst [x] = y\n"),
        "int32",
        "a tuple gives each place its own type"
    );
    assert_eq!(
        typed("const [x, z] = [1, \"a\"]\n"),
        "int32",
        "and so does a literal a pattern takes from"
    );
}

#[test]
fn a_mutated_array_is_an_array_of_its_elements() {
    assert_eq!(
        typed("let y = true\ny = [1]\nconst x = y\nx.push(2)\n"),
        "boolean | int32[]",
        "each array a value may be"
    );
    for mutation in [
        "x.push(3)",
        "x.pop()",
        "x.shift()",
        "x.unshift(0)",
        "x.splice(0, 1)",
        "x.sort(by)",
        "x.reverse()",
        "x.fill(0)",
        "x.copyWithin(0, 1)",
        "x[0] = 5",
        "x[0] += 1",
        "x[1]++",
        "const f = () => {\n  x.push(4)\n}",
    ] {
        assert_eq!(
            typed(&format!("const x = [1, 2]\n{mutation}\n")),
            "int32[]",
            "{mutation}"
        );
    }
    for reading in [
        "x.map(by)",
        "x.at(0)",
        "x.includes(1)",
        "x.toSorted(by)",
        "f(x)",
        "y.push(x)",
        "y[0] = x",
        "o.x.push(1)",
    ] {
        assert_eq!(
            typed(&format!("const x = [1, 2]\n{reading}\n")),
            "[1, 2]",
            "{reading}"
        );
    }
    assert_eq!(typed("let x = [1]\nx.push(2)\n"), "int32[]");
    assert_eq!(
        typed("let x = true\nx = [1]\nx.push(2)\n"),
        "boolean | int32[]",
        "each array it may be"
    );
    assert_eq!(
        typed("const x = Object.keys({ a: 1 })\nx.sort()\n"),
        "\"a\"[]",
        "an array that is no literal too"
    );
    assert_eq!(typed("const y = [1]\ny.push(2)\nconst x = y\n"), "int32[]");
    assert_eq!(
        typed("const x = { list: [1] }\nx.list.push(2)\n"),
        "{ list: readonly int32[] }",
        "what a key holds is not followed yet"
    );
    assert_eq!(
        typed("const x = new Map([[1, 2]])\nx.set(1, 3)\nx.delete(1)\nx.clear()\n"),
        "Map<int32, int32>"
    );
    assert_eq!(
        checked("const x = { k: 1 }\nx.add(2)\n").0,
        "{ k: int32 }",
        "`set`, `add`, `delete` and `clear` mutate whatever has them"
    );
}

#[test]
fn passing_to_a_parameter_that_is_mutated_mutates() {
    let callee = "const fill = (xs, other) => {\n  xs.push(1)\n  return other\n}\n";
    assert_eq!(
        typed(&format!("{callee}const x = [1, 2]\nfill(x, 0)\n")),
        "int32[]"
    );
    assert_eq!(
        typed(&format!("{callee}const x = [1, 2]\nfill([9], x)\n")),
        "[1, 2]",
        "the other parameter is not mutated"
    );
    assert_eq!(
        typed(&format!("{callee}const x = [1, 2]\nfill([0], x)\n")),
        "[1, 2]"
    );
    assert_eq!(
        typed("function sortAll(xs) {\n  xs.sort(by)\n}\nconst x = [2, 1]\nsortAll(x)\n"),
        "int32[]",
        "a function declaration"
    );
    assert_eq!(
        typed("const inner = (zs) => {\n  zs.pop()\n}\nconst middle = (ys) => inner(ys)\nconst outer = (ws) => {\n  middle(ws)\n}\nconst x = [1, 2]\nouter(x)\n"),
        "int32[]",
        "through as many calls as it takes"
    );
    assert_eq!(
        typed("const a = (xs) => b(xs)\nconst b = (ys) => a(ys)\nconst x = [1, 2]\na(x)\n"),
        "[1, 2]",
        "functions that only pass it round mutate nothing"
    );
    assert_eq!(
        typed(
            "const read = (xs) => xs.length\nconst x = [1, 2]\nread(x)\nunknown(x)\nx.concat(x)\n"
        ),
        "[1, 2]"
    );
    assert_eq!(typed("const f = ({ xs }, ...rest) => {\n  xs.push(1)\n  rest.push(2)\n}\nconst x = [1, 2]\nf(x, x)\n"), "[1, 2]", "a pattern and a rest are no plain parameter");
    assert_eq!(typed("const f = (xs) => {\n  xs.push(1)\n}\nconst g = f\nconst x = [1, 2]\ng(x)\nf(x.slice())\nf([x])\n"), "[1, 2]", "only a call by the function's own name, of the binding's own name");
    assert_eq!(
        typed("let f = (xs) => {\n  xs.push(1)\n}\nconst x = [1, 2]\nf(x)\n"),
        "int32[]"
    );
}

#[test]
fn an_empty_collection_takes_its_type_from_what_is_put_into_it() {
    assert_eq!(typed("const x = new Map()\nx.set(1, later())\n"), "?");
    assert_eq!(
        typed("const x = []\nx.push(1, \"a\")\n"),
        "(int32 | string)[]"
    );
    assert_eq!(
        typed("const x = []\nx.unshift(1.5)\nx.push(2)\n"),
        "number[]"
    );
    assert_eq!(
        typed("const x = []\nx[0] = \"a\"\n"),
        "string[]",
        "an index assignment"
    );
    assert_eq!(typed("const x = []\nconst add = () => {\n  const later = () => {\n    x.push(true)\n  }\n}\n"), "boolean[]", "from a nested arrow: it is in the binding's scope");
    assert_eq!(
        typed("const x = []\nx.push({ kind: \"a\" })\nx.push([1])\n"),
        "({ kind: string } | readonly int32[])[]",
        "what is put in holds no literals"
    );
    assert_eq!(typed("let x = []\nx.push(1)\n"), "int32[]");
    assert_eq!(
        typed("let x = [\"a\"]\nx = []\nx.push(1)\n"),
        "(int32 | string)[]".replace("(int32 | string)[]", "int32[] | string[]"),
        "an empty array assigned later is filled the same way"
    );
    assert_eq!(
        typed("const x = new Map()\nx.set(\"a\", 1)\nx.set(\"b\", 2.5)\n"),
        "Map<string, number>"
    );
    assert_eq!(
        typed("const x = new Set()\nx.add(1)\nx.add(\"a\")\n"),
        "Set<int32 | string>"
    );
    assert_eq!(
        typed("const x = []\nx.push(later())\n"),
        "?",
        "filled with what has no type yet: no type yet, and no finding"
    );
    assert_eq!(typed("const x = new Map()\nx.set(k, 1)\n"), "?");
    assert_eq!(typed("const x = new Set()\nx.add(k)\n"), "?");
    assert_eq!(
        typed("const x = []\nx.set(1, 2)\nx.add(1)\n"),
        "?",
        "put into, but not as an array is"
    );
    assert_eq!(
        typed("const x = new Map()\nx.push(1)\nx.add(1)\nx.set(1)\nx.set(1, 2, 3)\n"),
        "?"
    );
    assert_eq!(
        typed("const x = new Set()\nx.push(1)\nx.add(1, 2)\nx.add()\n"),
        "?"
    );
}

#[test]
fn an_empty_collection_nothing_fills_is_e110() {
    assert_eq!(
        found("let x = [1]\nx.push(2)\nx += []\n"),
        ["E110 @3:6 []"],
        "`+=` gives no binding its value"
    );
    assert_eq!(
        found("const x = []\nconst f = () => {\n  this.x.push(1)\n}\n"),
        ["E110 @1:11 []"],
        "a binding is no field"
    );
    assert_eq!(checked("const x = []\n"), ("?".to_owned(), vec!["E110 @1:11 []".to_owned()], vec!["an empty `[]` has no type: it takes one from what is put into it where its binding or field is in scope (`push`, `unshift`, `name[i] = …`), and nothing is (SPEC.md §3.2)".to_owned()]));
    assert_eq!(
        found("const x = []\nx.push()\nx.pop()\nx.length\nx[0]\nx.unshift()\n"),
        ["E110 @1:11 []"],
        "`push()` with no argument puts nothing in"
    );
    assert_eq!(
        found("const x = new Map()\nconst y = new Set()\nx.get(1)\ny.has(1)\n"),
        ["E110 @1:11 new Map()", "E110 @2:11 new Set()"]
    );
    let messages = checked("const x = new Map()\nconst y = new Set()\n").2;
    assert!(
        messages[0].contains("`new Map()`") && messages[0].contains("(`set`)"),
        "{messages:?}"
    );
    assert!(
        messages[1].contains("`new Set()`") && messages[1].contains("(`add`)"),
        "{messages:?}"
    );
    assert_eq!(
        found("const x = [[]]\n"),
        ["E110 @1:12 []"],
        "the inner one is the value of no binding"
    );
    assert_eq!(
        found("f([])\nconst o = { a: [] }\nconst g = () => []\nconst t = [new Map(), new Set()]\n"),
        [
            "E110 @1:3 []",
            "E110 @2:16 []",
            "E110 @3:17 []",
            "E110 @4:12 new Map()",
            "E110 @4:23 new Set()"
        ]
    );
    assert_eq!(found("let x = [1]\nx = []\n"), ["E110 @2:5 []"]);
    assert_eq!(
        found("let x = [1]\nx = []\nx.push(2)\nlet y = new Set()\ny = new Set()\ny.add(1)\n"),
        NONE
    );
    assert_eq!(
        found("let x = 1\nx += []\no.k = []\nconst [a] = []\nconst { b } = new Map()\n"),
        [
            "E110 @2:6 []",
            "E110 @3:7 []",
            "E110 @4:13 []",
            "E110 @5:15 new Map()"
        ],
        "only `name = []` is a binding's value"
    );
    assert_eq!(found("const a = new Map([[1, 2]])\nconst b = new Set(xs)\nconst c = new Thing()\nconst d = [1]\nconst e = new Map\n"), ["E110 @5:11 new Map"], "with something in it, it is not empty");
    assert_eq!(
        found("const Map = f()\nconst Set = f()\nconst a = new Map()\nconst b = new Set()\n"),
        NONE,
        "a `Map` the file declares is not the collection"
    );
    assert_eq!(
        found("const x = []\nconst y = x\ny.push(1)\n"),
        ["E110 @1:11 []"],
        "filled by another name: not in a way that is followed"
    );
}

#[test]
fn a_field_is_filled_anywhere_in_its_class_body() {
    let class = |members: &str| found(&format!("class K {{\n{members}}}\n"));
    assert_eq!(class("  items = []\n  #seen = new Set()\n  byName = new Map()\n  add(v) {\n    this.items.push(v)\n    this.#seen.add(v)\n    this.byName.set(v, 1)\n  }\n"), NONE);
    assert_eq!(class("  #items = []\n  fill(v) {\n    const later = () => {\n      this.#items[0] = v\n    }\n  }\n"), NONE, "an index assignment, from an arrow in a method");
    assert_eq!(
        class("  items = []\n  static all = new Map()\n  #seen = new Set()\n"),
        [
            "E110 @2:11 []",
            "E110 @3:16 new Map()",
            "E110 @4:11 new Set()"
        ]
    );
    assert_eq!(class("  items = []\n  m(v) {\n    this.items.push()\n    this.items.pop()\n    this.other.push(v)\n    items.push(v)\n    this.items.add(v)\n    this.items[0] += v\n    this.items.length\n  }\n"), ["E110 @2:11 []", "E137 @6:10 other"], "nothing here puts anything into `items`, and `other` is no field");
    assert_eq!(class("  seen = new Set()\n  names = new Map()\n  m(v) {\n    this.seen.push(v)\n    this.seen[0] = v\n    this.names.add(v)\n  }\n"), ["E110 @2:10 new Set()", "E110 @3:11 new Map()"]);
    assert_eq!(found("class K {\n  items = []\n}\nclass L {\n  items = [1]\n  m(v) {\n    this.items.push(v)\n  }\n}\n"), ["E110 @2:11 []"], "another class's body is not this one's");
    assert_eq!(
        class("  static all = []\n  static {\n    this.all.unshift(1)\n  }\n"),
        NONE
    );
}

#[test]
fn the_length_of_an_array_is_not_assigned() {
    assert_eq!(
        found("const x = [1, 2]\nx.length = 0\n"),
        ["E133 @2:3 length"]
    );
    assert_eq!(
        found("let x = [1]\nx.length += 1\nx.length++\nx[\"length\"] = 0\n"),
        [
            "E133 @2:3 length",
            "E133 @3:3 length",
            "E133 @4:3 \"length\""
        ]
    );
    assert_eq!(
        found("const x = []\nx.push(1)\nx.length = 0\nconst n = x.length\n"),
        ["E133 @3:3 length"]
    );
    assert_eq!(
        found(
            "const o = { length: 1 }\no.length = 2\nconst s = \"text\"\nunknownName.length = 0\n"
        ),
        NONE,
        "a key of a shape named `length` is a key"
    );
    assert_eq!(
        checked("const x = [1]\nx.length = 0\n").2,
        ["assigning `length` is removed: use `splice` or `toSpliced` (SPEC.md §4.6)"]
    );
    assert_eq!(typed("const x = [1, 2]\nconst n = x.length\n"), "[1, 2]");
    assert_eq!(
        typed("const y = [1, 2]\nconst i = f()\nconst x = y.length\n"),
        "int32"
    );
}

/// The type of a name in each file of a program on disk, from the given roots.
fn program(files: &[(&str, &str)], roots: &[&str], wanted: &[(&str, &str)]) -> Vec<String> {
    let dir = TempDir::new("us087").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let roots: Vec<_> = roots.iter().map(|root| base.join(root)).collect();
    let (graph, _) = ModuleGraph::load(&Disk, &base, &roots).expect("loads");
    let (links, _) = graph.link();
    let mut types = Types::new();
    let modules = infer_program(&graph, &links, &mut types);
    let printer = Printer::new(&types, &class_name);
    wanted
        .iter()
        .map(|(file, name)| {
            let ty = modules.get(&base.join(file)).and_then(|module| {
                let at = module
                    .bindings
                    .declarations
                    .iter()
                    .position(|d| d.name == *name)?;
                module.facts.declarations.get(&at)
            });
            ty.map_or("?".to_owned(), |&ty| printer.print(ty))
        })
        .collect()
}

#[test]
fn what_one_module_mutates_is_mutated_in_all() {
    let lib = ("lib.js", "export const xs = [1, 2]\nexport const ys = [3, 4]\nexport const zs = [5]\nexport const fill = (list) => {\n  list.push(0)\n}\nexport const empty = []\n");
    assert_eq!(
        program(
            &[
                lib,
                (
                    "lib.test.js",
                    "import { xs, empty } from \"./lib.js\"\nxs.push(3)\nempty.push(\"a\")\n"
                )
            ],
            &["lib.js"],
            &[("lib.js", "xs"), ("lib.js", "ys"), ("lib.js", "empty")]
        ),
        ["[1, 2]", "[3, 4]", "?"],
        "without the test file nothing mutates them"
    );
    assert_eq!(
        program(
            &[lib, ("lib.test.js", "import { xs, empty } from \"./lib.js\"\nxs.push(3)\nempty.push(\"a\")\n")],
            &["lib.js", "lib.test.js"],
            &[("lib.js", "xs"), ("lib.js", "ys"), ("lib.test.js", "xs"), ("lib.js", "empty")],
        ),
        ["int32[]", "[3, 4]", "int32[]", "?"],
        "an array mutated only in a test file is mutated program-wide; an empty one is filled in its own scope only"
    );
    assert_eq!(
        program(
            &[
                lib,
                ("entry.js", "import { ys, zs, fill as put } from \"./re.js\"\nconst local = [7, 8]\nconst kept = [9]\nput(ys)\nput(local)\nconst pass = (list) => put(list)\npass(zs)\nexport { local, kept }\n"),
                ("re.js", "export { ys, zs, fill } from \"./lib.js\"\n"),
            ],
            &["entry.js"],
            &[("lib.js", "xs"), ("lib.js", "ys"), ("lib.js", "zs"), ("entry.js", "local"), ("entry.js", "kept"), ("entry.js", "ys")],
        ),
        ["[1, 2]", "int32[]", "int32[]", "int32[]", "[9]", "int32[]"],
        "passed to a function of another module that mutates its parameter, through re-exports and local functions"
    );
    assert_eq!(
        program(
            &[("entry.js", "import { gone } from \"./lib.js\"\nimport * as ns from \"./lib.js\"\nimport { bad } from \"./bad.js\"\nconst own = [1]\ngone.push(1)\nbad(own)\nns.fill(own)\nexport { own }\n"), lib, ("bad.js", "export const = 1\n")],
            &["entry.js"],
            &[("entry.js", "own"), ("lib.js", "xs")],
        ),
        ["[1]", "[1, 2]"],
        "what an import is bound to nowhere, a rejected module and a call through a namespace mutate nothing known"
    );
}
