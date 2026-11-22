//! STORIES.md US-085: object literals, and the types of imports.

use sjs_bind::{bind, Disk, ModuleGraph};
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, infer_program, Facts};
use sjs_syntax::parse;
use sjs_testkit::fs::TempDir;
use sjs_types::{ClassId, Printer, Type, TypeId, Types};
use std::collections::BTreeMap;

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

type Names = Vec<(&'static str, TypeId)>;
/// What a test knows before inference: the types of names, and the fields of class 0.
type Known<'a> = &'a dyn Fn(&mut Types) -> (Names, Names);

fn nothing(_: &mut Types) -> (Names, Names) {
    (Vec::new(), Vec::new())
}

/// The type inferred for `x`, printed (`?` for none), and the findings on object
/// literals: `CODE @line:column text`.
fn checked(text: &str, known: Known<'_>) -> (String, Vec<String>) {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let (given, fields) = known(&mut types);
    let mut facts = Facts::default();
    for (name, ty) in given {
        let declaration = bindings.lookup(0, name).expect("a declared name");
        facts.declarations.insert(declaration, ty);
    }
    let fields: BTreeMap<String, TypeId> = fields
        .into_iter()
        .map(|(k, ty)| (k.to_owned(), ty))
        .collect();
    facts.fields.insert(ClassId(0), fields);
    facts.class_names.insert(ClassId(0), "Point".to_owned());
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    let found = check_expressions(&source, &tree, &bindings, &facts, &mut types)
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
    let x = bindings.declarations.iter().position(|d| d.name == "x");
    let ty = x.and_then(|at| facts.declarations.get(&at));
    let printed = ty.map_or("?".to_owned(), |&ty| {
        Printer::new(&types, &class_name).print(ty)
    });
    (printed, found)
}

fn typed(text: &str) -> String {
    let (printed, found) = checked(text, &nothing);
    assert_eq!(found, Vec::<String>::new(), "{text}");
    printed
}

const NONE: [&str; 0] = [];

#[test]
fn a_literal_has_its_exact_shape_with_literals_retained() {
    assert_eq!(
        typed("const x = { kind: \"circle\", r: 1 }\n"),
        "{ kind: \"circle\", r: 1 }"
    );
    assert_eq!(
        typed("const x = { a: { kind: \"k\", deep: { n: 1n } }, b: [1] }\n"),
        "{ a: { kind: \"k\", deep: { n: 1n } }, b: readonly int32[] }",
        "nested literals are retained"
    );
    assert_eq!(typed("const x = { 1: \"a\", 1.5: \"b\", 0x10: \"c\", 2n: \"d\", \"s t\": 1, if: 2, true: 3, null: 4 }\n"), "{ \"1\": \"a\", \"1.5\": \"b\", \"16\": \"c\", \"2\": \"d\", if: 2, null: 4, \"s t\": 1, true: 3 }", "every key as ES spells it");
    assert_eq!(
        typed("const a = undefined\nconst b = 1\nconst x = { a, b }\n"),
        "{ b: 1, a: undefined }",
        "shorthand, of an undefined binding too"
    );
    assert_eq!(typed("const x = { a: f() }\n"), "?");
    assert_eq!(typed("const x = { a }\n"), "?");
}

#[test]
fn a_let_and_a_mutated_object_hold_no_literals() {
    assert_eq!(
        typed("let x = { kind: \"circle\", at: { r: 1 } }\n"),
        "{ at: { r: int32 }, kind: string }",
        "assigned to a `let`"
    );
    assert_eq!(
        typed("let x = undefined\nx = { kind: \"circle\" }\n"),
        "{ kind: string } | undefined"
    );
    assert_eq!(
        typed("const x = { kind: \"circle\", r: 1 }\nx.r = 2\n"),
        "{ kind: string, r: int32 }",
        "written into"
    );
    for mutation in [
        "x.at.r = 2",
        "x[\"r\"] += 1",
        "x.r++",
        "x.at[k] ??= 1",
        "for (const i of xs) {\n  x.r -= 1\n}",
        "const f = () => {\n  --x.r\n}",
    ] {
        let text = format!("const x = {{ r: 1, at: {{ r: 1 }} }}\n{mutation}\n");
        assert_eq!(typed(&text), "{ at: { r: int32 }, r: int32 }", "{mutation}");
    }
    for reading in [
        "x.r",
        "f(x.r)",
        "const y = x.r + 1",
        "y.r = 2",
        "y[k] = x.r",
        "const z = x.r === 1",
        "f().r = x",
    ] {
        let text = format!("const y = {{ r: 2 }}\nconst x = {{ r: 1 }}\n{reading}\n");
        assert_eq!(typed(&text), "{ r: 1 }", "{reading}");
    }
}

#[test]
fn a_spread_merges_and_later_keys_win() {
    let head = "const a = { p: 1, q: \"a\" }\nconst b = { q: true, r: 2n }\nconst c = 3\n";
    let of = |literal: &str| typed(&format!("{head}const x = {literal}\n"));
    assert_eq!(of("{ ...a, ...b, c }"), "{ c: 3, p: 1, q: true, r: 2n }");
    assert_eq!(of("{ ...b, ...a }"), "{ p: 1, q: \"a\", r: 2n }");
    assert_eq!(
        of("{ q: 0, ...a, p: \"over\" }"),
        "{ p: \"over\", q: \"a\" }"
    );
    assert_eq!(of("{ ...a, q: 0, ...{ z: 1 }, }"), "{ p: 1, q: 0, z: 1 }");
    assert_eq!(
        of("{ ...undefined, ...a }"),
        "{ p: 1, q: \"a\" }",
        "`undefined` spreads nothing"
    );
    assert_eq!(of("{ ...{} }"), "{}");
    assert_eq!(of("{ ...o }"), "?");
}

#[test]
fn a_spread_takes_from_each_kind_of_value() {
    let known = |types: &mut Types| {
        let shape = |types: &mut Types, key: &str, ty| {
            types.intern(Type::Object(vec![sjs_types::Property {
                key: key.to_owned(),
                ty,
                literal: false,
            }]))
        };
        let (one, two) = (
            shape(types, "a", TypeId::INT32),
            shape(types, "b", TypeId::STRING),
        );
        let either = types.union([one, two]);
        let maybe = types.union([one, TypeId::UNDEFINED]);
        let instance = types.intern(Type::Class(ClassId(0), Vec::new()));
        let other = types.intern(Type::Class(ClassId(1), Vec::new()));
        let dictionary = types.intern(Type::Dictionary(TypeId::NUMBER));
        let numbers = types.intern(Type::Array {
            element: TypeId::NUMBER,
            mutated: false,
        });
        let or_array = types.union([one, numbers]);
        let names = vec![
            ("u", either),
            ("m", maybe),
            ("k", instance),
            ("other", other),
            ("d", dictionary),
            ("s", TypeId::STRING),
            ("w", or_array),
            ("raw", TypeId::UNKNOWN),
        ];
        (names, vec![("x", TypeId::NUMBER), ("y", TypeId::NUMBER)])
    };
    let head = "const u = f()\nconst m = f()\nconst k = f()\nconst other = f()\nconst d = f()\nconst s = f()\nconst w = f()\nconst raw = f()\n";
    let of = |literal: &str| checked(&format!("{head}const x = {literal}\n"), &known);
    assert_eq!(
        of("{ z: 1, ...u }").0,
        "{ z: 1, a: int32 } | { z: 1, b: string }",
        "a union spreads each member"
    );
    assert_eq!(
        of("{ ...u, a: \"x\" }").0,
        "{ a: \"x\" } | { a: \"x\", b: string }"
    );
    assert_eq!(
        of("{ ...u, ...m }").0,
        "{ a: int32 } | { b: string } | { a: int32, b: string }"
    );
    assert_eq!(of("{ ...m }").0, "{} | { a: int32 }");
    assert_eq!(
        of("{ ...k, y: \"s\" }"),
        ("{ y: \"s\", x: number }".to_owned(), Vec::new()),
        "a class instance: its public fields"
    );
    assert_eq!(
        of("{ ...other }"),
        ("?".to_owned(), Vec::new()),
        "a class whose fields are not known yet"
    );
    assert_eq!(
        of("{ ...d }"),
        ("?".to_owned(), Vec::new()),
        "a dictionary has no shape"
    );
    assert_eq!(
        of("{ ...[1] }"),
        ("?".to_owned(), vec!["E138 @9:13 ...[1]".to_owned()])
    );
    assert_eq!(
        of("{ a: 1, ...s, ...raw }").1,
        ["E138 @9:19 ...s", "E138 @9:25 ...raw"]
    );
    assert_eq!(
        of("{ ...w }").1,
        ["E138 @9:13 ...w"],
        "one member that does not spread"
    );
    assert_eq!(of("{ ...1, ...true, ...\"a\" }").1.len(), 3);
    assert_eq!(of("[{ ...u, a: 1 }]").0, "[{ a: 1 } | { a: 1, b: string }]");
    assert!(of("{ ...{ ...u, ...u } }").0.contains(" | "));
    let mutated = checked(
        &format!("{head}const x = {{ ...u, kind: \"k\" }}\nx.kind = \"z\"\n"),
        &known,
    );
    assert_eq!(
        mutated.0, "{ a: int32, kind: string } | { b: string, kind: string }",
        "written into: every shape it may be loses its literals"
    );
}

#[test]
fn an_array_of_class_instances_does_not_spread() {
    let head = "";
    let unspreadable = checked(
        &format!("{head}const ks = f()\nconst x = {{ ...ks }}\n"),
        &|types: &mut Types| {
            let instance = types.intern(Type::Class(ClassId(0), Vec::new()));
            let instances = types.intern(Type::Array {
                element: instance,
                mutated: true,
            });
            (vec![("ks", instances)], Vec::new())
        },
    );
    assert_eq!(unspreadable.1, ["E138 @2:13 ...ks"]);
}

#[test]
fn a_literal_of_very_many_shapes_is_given_up_on() {
    // Two shapes each time: past 64 alternatives the literal is given up on.
    let names = ["u0", "u1", "u2", "u3", "u4", "u5", "u6"];
    let h = names.map(|name| format!("const {name} = f()\n")).concat();
    let spreads = names.map(|name| format!("...{name}, ")).concat();
    let wide = |types: &mut Types| {
        let mut given: Names = Vec::new();
        for (n, name) in names.into_iter().enumerate() {
            let shape = |types: &mut Types, key: String| {
                types.intern(Type::Object(vec![sjs_types::Property {
                    key,
                    ty: TypeId::INT32,
                    literal: false,
                }]))
            };
            let (one, two) = (shape(types, format!("a{n}")), shape(types, format!("b{n}")));
            given.push((name, types.union([one, two])));
        }
        (given, Vec::new())
    };
    assert_eq!(
        checked(&format!("{h}const x = {{ {spreads} }}\n"), &wide),
        ("?".to_owned(), Vec::new())
    );
    let six = names[..6]
        .iter()
        .map(|name| format!("...{name}, "))
        .collect::<Vec<_>>()
        .concat();
    assert_eq!(
        checked(&format!("{h}const x = {{ {six} }}\n"), &wide)
            .0
            .matches(" | ")
            .count(),
        63
    );
}

#[test]
fn a_computed_key_is_a_literal_the_checker_knows() {
    let head = "const k = \"key\"\nconst n = 7\nconst b = 2n\nconst t = true\nlet loose = \"l\"\nconst list = [1]\n";
    let of = |literal: &str| checked(&format!("{head}const x = {literal}\n"), &nothing);
    assert_eq!(
        of("{ [k]: 1, [n]: 2, [b]: 3, [t]: 4, [\"s\"]: 5, [1.5]: 6, [-1]: 7 }"),
        (
            "{ \"-1\": 7, \"1.5\": 6, \"2\": 3, \"7\": 2, key: 1, s: 5, true: 4 }".to_owned(),
            Vec::new()
        )
    );
    assert_eq!(
        of("{ [loose]: 1 }"),
        ("?".to_owned(), vec!["E000 @7:14 loose".to_owned()]),
        "a `let` holds no literal"
    );
    assert_eq!(
        of("{ [list]: 1, [undefined]: 2, [`a${k}`]: 3, [n + 1]: 4, ok: 5 }").1,
        [
            "E000 @7:14 list",
            "E000 @7:25 undefined",
            "E000 @7:41 `a${k}`",
            "E000 @7:55 n + 1"
        ]
    );
    assert_eq!(
        of("{ [later()]: 1, [unknown]: 2 }"),
        ("?".to_owned(), Vec::new()),
        "one of no type yet is not judged yet"
    );
    assert_eq!(of("{ a: 1, [k]: f() }"), ("?".to_owned(), Vec::new()));
}

#[test]
fn a_computed_key_spelled_already_is_e134() {
    let head = "const k = \"a\"\nconst n = 1\n";
    let of = |literal: &str| checked(&format!("{head}const x = {literal}\n"), &nothing).1;
    assert_eq!(of("{ a: 1, [k]: 2 }"), ["E134 @3:20 k"]);
    assert_eq!(
        of("{ [k]: 1, a: 2 }"),
        ["E134 @3:21 a"],
        "a key spelled outright after a computed one"
    );
    assert_eq!(
        of("{ [k]: 1, [\"a\"]: 2, [k]: 3 }"),
        ["E134 @3:22 \"a\"", "E134 @3:32 k"]
    );
    assert_eq!(
        of("{ 1: 1, [n]: 2, [1.0]: 3, \"1\": 4 }"),
        ["E134 @3:20 n", "E134 @3:28 1.0"],
        "as ES compares keys; a key spelled outright twice is the parser's finding"
    );
    assert_eq!(
        of("{ a: 1, ...o, [k]: 2 }"),
        NONE,
        "after a spread the later value is deliberate"
    );
    assert_eq!(of("{ [k]: 1, ...o, a: 2 }"), NONE);
    assert_eq!(
        of("{ a: 1, b: { [k]: 2 }, [n]: 3 }"),
        NONE,
        "another literal, other keys"
    );
    assert_eq!(of("{ [k]: 1, b: { a: 1, [k]: 2 } }"), ["E134 @3:33 k"]);
    let text = format!("{head}const x = {{ a: 1, [k]: 2 }}\nconst y = [{{ a: 1, [k]: x }}]\n`${{{{ a: 1, [k]: 2 }}}}`\n");
    let (_, found) = checked(&text, &nothing);
    assert_eq!(
        found,
        [
            "E134 @3:20 k",
            "E134 @4:21 k",
            "E125 @5:4 { a: 1, [k]: 2 }",
            "E134 @5:13 k"
        ],
        "each literal once, wherever it is typed from"
    );
    let source = SourceFile::new(
        "a.js",
        format!("{head}const x = {{ a: 1, [k]: 2, ...[1], [x]: 3 }}\n").into_bytes(),
    )
    .expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let found = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "the key `a` is spelled twice in this literal (SPEC.md §4.6)",
            "a `readonly int32[]` is spread into an object literal: only a shape, a class instance or `undefined` spreads there (SPEC.md §4.6)",
        ]
    );
    let (_, found) = checked("const k = [1]\nconst x = { [k]: 1 }\n", &nothing);
    assert_eq!(found, ["E000 @2:14 k"]);
    let source =
        SourceFile::new("a.js", b"const k = [1]\nconst x = { [k]: 1 }\n".to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let found = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    assert_eq!(
        found.iter().map(|d| d.message.as_str()).collect::<Vec<_>>(),
        ["a computed key must be a literal the checker can tell: a string, number, bigint or boolean literal, or a `const` of one (SPEC.md §4.6)"]
    );
}

/// The types of the names of each module of a program on disk: `file name: type`.
fn program(files: &[(&str, &str)], wanted: &[(&str, &str)]) -> Vec<String> {
    let dir = TempDir::new("us085").expect("a temporary directory");
    for (path, text) in files {
        dir.write(path, text).expect("the file");
    }
    let base = dir.path().canonicalize().expect("canonical");
    let (graph, _) = ModuleGraph::load(&Disk, &base, &[base.join("entry.js")]).expect("loads");
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
fn an_import_has_the_type_of_what_it_is_bound_to() {
    let lib = ("lib.js", "export const a = 1\nexport let count = 0\ncount += 1\nconst hidden = \"h\"\nexport { hidden as shown }\nexport const box = { kind: \"box\" }\n");
    assert_eq!(
        program(
            &[
                ("entry.js", "import { a, count, shown as s, box, far } from \"./re.js\"\nimport cfg from \"./c.json\" with { type: \"json\" }\nexport const sum = a + count\nexport const copy = { ...box, s }\nconst t = `${cfg}`\n"),
                ("re.js", "export { a, count, shown, box } from \"./lib.js\"\nexport { deep as far } from \"./deep.js\"\n"),
                ("deep.js", "import { a } from \"./lib.js\"\nexport const deep = [a]\n"),
                lib,
                ("c.json", "{}"),
            ],
            &[("entry.js", "a"), ("entry.js", "count"), ("entry.js", "s"), ("entry.js", "box"), ("entry.js", "far"), ("entry.js", "cfg"), ("entry.js", "sum"), ("entry.js", "copy"), ("deep.js", "a"), ("nowhere.js", "a")],
        ),
        ["1", "number", "\"h\"", "{ kind: \"box\" }", "[1]", "unknown", "number", "{ kind: \"box\", s: \"h\" }", "1", "?"],
        "through re-exports and renames; a JSON import is `unknown`"
    );
}

#[test]
fn a_namespace_import_has_the_shape_of_its_modules_exports() {
    let lib = ("lib.js", "export const a = 1\nexport let count = 0\ncount += 1\nconst hidden = \"h\"\nexport { hidden as shown }\n");
    assert_eq!(
        program(
            &[
                ("entry.js", "import * as ns from \"./lib.js\"\nimport * as outer from \"./outer.js\"\nimport * as fn from \"./fn.js\"\nimport * as bad from \"./bad.js\"\nimport * as none from \"./nowhere.js\"\nexport const copy = { ...ns, extra: true }\n"),
                lib,
                ("outer.js", "import * as inner from \"./lib.js\"\nimport data from \"./c.json\" with { type: \"json\" }\nexport { inner, data }\nexport const own = 2\n"),
                ("fn.js", "export const typed = 1\nexport function untyped(nothing) {\n  return nothing.size\n}\n"),
                ("bad.js", "export const = 1\n"),
                ("c.json", "[]"),
            ],
            &[("entry.js", "ns"), ("entry.js", "outer"), ("entry.js", "fn"), ("entry.js", "bad"), ("entry.js", "none"), ("entry.js", "copy"), ("bad.js", "x")],
        ),
        [
            "{ a: 1, shown: \"h\", count: number }",
            "{ own: 2, data: unknown, inner: { a: 1, shown: \"h\", count: number } }",
            "?",
            "?",
            "?",
            "{ a: 1, extra: true, shown: \"h\", count: number }",
            "?",
        ],
        "a key for each export; a namespace exported again; none until every export has a type, nor from a file the grammar rejects"
    );
    assert_eq!(
        program(
            &[
                ("entry.js", "import * as me from \"./entry.js\"\nimport * as other from \"./other.js\"\nexport { me }\nexport const x = 1\n"),
                ("other.js", "import * as entry from \"./entry.js\"\nimport { x } from \"./entry.js\"\nimport * as own from \"./other.js\"\nexport { own }\nexport const y = 2\nexport const z = x\n"),
            ],
            &[("entry.js", "me"), ("entry.js", "other"), ("other.js", "entry"), ("other.js", "own"), ("other.js", "x"), ("other.js", "z"), ("other.js", "y")],
        ),
        ["?", "?", "?", "?", "?", "?", "2"],
        "a namespace that holds itself has no shape, nor have modules that import one another"
    );
}
