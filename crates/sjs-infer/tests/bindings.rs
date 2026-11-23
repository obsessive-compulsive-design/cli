//! STORIES.md US-084: the types of bindings, through destructuring too.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Printer, Type, TypeId, Types};
use std::collections::BTreeMap;

fn class_name(class: ClassId) -> String {
    format!("Class{}", class.0)
}

/// What a test knows before inference: the types of names, and the fields of class 0.
type Names = Vec<(&'static str, TypeId)>;
type Known<'a> = &'a dyn Fn(&mut Types) -> (Names, Names);

/// The type inferred for each of `names`, printed; `?` for one that has none.
fn inferred_with(text: &str, names: &[&str], known: Known<'_>) -> Vec<String> {
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
    facts.fields.insert(ClassId(0), fields.into());
    let before = facts.declarations.clone();
    let facts = infer_bindings(&source, &tree, &bindings, &facts, &mut types);
    for (declaration, ty) in before {
        assert_eq!(
            facts.declarations.get(&declaration),
            Some(&ty),
            "what was known stays"
        );
    }
    let printer = Printer::new(&types, &class_name);
    let by_name = |name: &&str| {
        // The first declaration of the name, wherever it is.
        let found = bindings.declarations.iter().position(|d| d.name == *name);
        let ty = found.and_then(|at| facts.declarations.get(&at));
        ty.map_or("?".to_owned(), |&ty| printer.print(ty))
    };
    names.iter().map(by_name).collect()
}

fn nothing(_: &mut Types) -> (Names, Names) {
    (Vec::new(), Vec::new())
}

fn inferred(text: &str, names: &[&str]) -> Vec<String> {
    inferred_with(text, names, &nothing)
}

fn one(text: &str) -> String {
    inferred(text, &["x"]).remove(0)
}

#[test]
fn a_const_has_the_type_of_its_value() {
    assert_eq!(one("const x = 1\n"), "1");
    assert_eq!(one("const x = \"a\"\n"), "\"a\"");
    assert_eq!(one("const x = 1.5\n"), "1.5");
    assert_eq!(one("const x = undefined\n"), "undefined");
    assert_eq!(one("const x = [1, 2]\n"), "[1, 2]");
    assert_eq!(one("export const x = `a${1}`\n"), "string");
    assert_eq!(
        one("const x = { kind: \"circle\", r: 1, \"a-b\": true }\n"),
        "{ \"a-b\": true, kind: \"circle\", r: 1 }"
    );
    assert_eq!(
        one("const y = 2\nconst x = { y, z: [y] }\n"),
        "{ y: 2, z: readonly int32[] }"
    );
    assert_eq!(one("const x = {}\n"), "{}");
    assert_eq!(
        one("const y = 1\nconst x = y\n"),
        "1",
        "through another binding"
    );
    assert_eq!(
        one("const x = y\nconst y = 1\n"),
        "1",
        "whichever comes first"
    );
    assert_eq!(
        one("const a = 1\nconst b = a\nconst c = b\nconst d = c\nconst x = d\n"),
        "1"
    );
    assert_eq!(one("const x = 1 + 2\n"), "number");
    assert_eq!(one("const x = 1n * 2n\n"), "bigint");
    assert_eq!(one("const x = \"a\" + \"b\"\n"), "string");
    for later in [
        "const x = f()\n",
        "const x = y\n",
        "const x = { ...o }\n",
        "const x = { [k]: 1 }\n",
        "const x = { m() {} }\n",
        "const x = { a: f() }\n",
        "const x = \"a\" - \"b\"\n",
        "const x = 1 + \"a\"\n",
        "const x = 1n + 1\n",
        "const x = 1 + f()\n",
        "const x = f() + 1\n",
        "for (const x of xs) {\n}\n",
        "const f = (x) => x\n",
    ] {
        assert_eq!(one(later), "?", "{later}");
    }
    assert_eq!(
        inferred(
            "const f = () => {\n  const inner = 1\n  {\n    const deep = \"a\"\n  }\n}\n",
            &["inner", "deep", "f"]
        ),
        ["1", "\"a\"", "(): undefined"],
        "in any scope"
    );
}

#[test]
fn a_let_is_the_union_of_what_it_is_assigned_without_literals() {
    assert_eq!(one("let x = 1\n"), "int32");
    assert_eq!(one("let x = 1\nx = 2\n"), "int32");
    assert_eq!(
        one("let x = 1\nx = 1.5\n"),
        "number",
        "after a non-int32 assignment"
    );
    assert_eq!(one("let x = undefined\nx = \"a\"\n"), "string | undefined");
    assert_eq!(
        one("let x = \"a\"\nx = 1\nx = true\nx = 2n\n"),
        "bigint | int32 | string | boolean".replace("bigint | int32", "int32 | bigint")
    );
    assert_eq!(
        one("let x = 1\nfor (const y of ys) {\n  x = x + 1\n}\n"),
        "number",
        "assigned `x + 1` in a loop"
    );
    assert_eq!(
        one("let x = 1\nconst f = () => {\n  x = \"a\"\n}\n"),
        "int32 | string",
        "from a nested function"
    );
    assert_eq!(one("let x = 1\nx += 1\n"), "number");
    assert_eq!(one("let x = 1\nx -= 1\n"), "number");
    assert_eq!(one("let x = \"a\"\nx += \"b\"\n"), "string");
    assert_eq!(one("let x = 1n\nx *= 2n\n"), "bigint");
    assert_eq!(
        one("let x = 1\nfor (let i = 0; i < 1; i++) {\n  x++\n}\n"),
        "number"
    );
    assert_eq!(
        inferred(
            "for (let i = 0; i < 9; i++) {\n}\nfor (let j = 0n; j < 9n; j--) {\n}\nfor (let k = 0; k < 9; ++k) {\n}\nfor (let m = 0; m < 9; f(m)) {\n}\nlet n = 1\n--n\n",
            &["i", "j", "k", "m", "n"]
        ),
        ["number", "bigint", "number", "int32", "number"],
        "stepped after or before the name; a header that only calls assigns nothing"
    );
    assert_eq!(inferred("", &["x"]), ["?"], "an empty file");
    assert_eq!(
        one("let x = undefined\nx ??= 1\n"),
        "int32 | undefined",
        "it was `undefined` before"
    );
    assert_eq!(one("let x = 1\nx ??= \"a\"\n"), "int32 | string");
    assert_eq!(
        one("let y = 1\nlet x = y\ny = \"a\"\n"),
        "int32 | string",
        "what another `let` comes to hold"
    );
    assert_eq!(
        inferred("let a = 1\nlet b = \"s\"\na = b\nb = a\n", &["a", "b"]),
        ["int32 | string", "int32 | string"]
    );
    assert_eq!(one("let x = [1]\n"), "readonly int32[]");
    assert_eq!(one("let x = { a: 1 }\n"), "{ a: int32 }");
    for later in [
        "let x = 1\nx = f()\n",
        "let x = 1\nx += \"a\"\n",
        "let x = \"a\"\nx -= \"b\"\n",
        "let x = \"a\"\nx++\n",
        "let x = 1\nx += f()\n",
        "let x = 1\nx ??= f()\n",
        "let x = 1\n;[x] = [2]\n",
        "let x = 1\n;({ x } = o)\n",
        "let x = 1\n;(x) = 2\n",
        "let x = f()\nx = 1\n",
        "for (let x of xs) {\n  x = 1\n}\n",
        "let x = 1\nfor (const y of ys) {\n  x = [x]\n}\n",
    ] {
        assert_eq!(one(later), "?", "{later}");
    }
    assert_eq!(
        inferred(
            "let x = 1\nx = [x]\nconst y = 2\nlet z = y\n",
            &["x", "y", "z"]
        ),
        ["?", "2", "int32"],
        "a type that grows without end is given up on, alone"
    );
    assert_eq!(
        inferred(
            "let x = 1\nx = f()\nconst y = x\nlet z = \"a\"\nz = x\n",
            &["y", "z"]
        ),
        ["?", "?"],
        "and so is what depends on an unknown"
    );
    assert_eq!(
        inferred(
            "undeclared = 1\nundeclared += 1\nfetch++\n",
            &["undeclared"]
        ),
        ["?"]
    );
}

#[test]
fn a_pattern_takes_from_a_shape_by_key() {
    let from = "const o = { a: \"x\", b: 1, c: undefined, d: true }\n";
    let names = ["a", "n", "c", "rest", "e"];
    let get = |pattern: &str| inferred(&format!("{from}const {pattern} = o\n"), &names);
    assert_eq!(
        get("{ a, b: n, c, ...rest }"),
        ["string", "int32", "undefined", "{ d: true }", "?"]
    );
    assert_eq!(
        get("{ ...rest }"),
        [
            "?",
            "?",
            "?",
            "{ a: \"x\", b: 1, d: true, c: undefined }",
            "?"
        ]
    );
    assert_eq!(
        get("{ a = 1 }")[0],
        "string",
        "the default is dead: `undefined` cannot come"
    );
    assert_eq!(
        get("{ c = 1 }")[2],
        "int32",
        "`undefined` only: the default"
    );
    assert_eq!(get("{ c = f() }")[2], "?");
    assert_eq!(
        get("{ e }")[4],
        "?",
        "a key the shape lacks is the checker's"
    );
    assert_eq!(
        inferred("const { a = 1 } = { a: \"x\" }\n", &["a"]),
        ["string"],
        "straight from the literal"
    );
    assert_eq!(
        inferred(
            "let { a, b = 2 } = { a: 1, b: undefined }\na = \"s\"\n",
            &["a", "b"]
        ),
        ["int32 | string", "int32"],
        "a `let` pattern, assigned later"
    );
    let known = |types: &mut Types| {
        let or_undefined = types.union([TypeId::STRING, TypeId::UNDEFINED]);
        let instance = types.intern(Type::Class(ClassId(0), Vec::new()));
        let dictionary = types.intern(Type::Dictionary(TypeId::NUMBER));
        let either = types.union([instance, TypeId::UNDEFINED]);
        let names = vec![
            ("maybe", or_undefined),
            ("k", instance),
            ("d", dictionary),
            ("u", either),
        ];
        (
            names,
            vec![("size", TypeId::INT32), ("label", or_undefined)],
        )
    };
    let head =
        "const maybe = f()\nconst k = f()\nconst d = f()\nconst u = f()\nconst o = { m: maybe }\n";
    let get = |body: &str, names: &[&str]| inferred_with(&format!("{head}{body}"), names, &known);
    assert_eq!(get("const { m } = o\n", &["m"]), ["string | undefined"]);
    assert_eq!(
        get("const { m = 1 } = o\n", &["m"]),
        ["int32 | string"],
        "the default joins what is there"
    );
    assert_eq!(
        get("const { m = undefined } = o\n", &["m"]),
        ["string | undefined"]
    );
    assert_eq!(
        get(
            "const { size, label = \"none\", hidden, ...others } = k\n",
            &["size", "label", "hidden", "others"]
        ),
        ["int32", "string", "?", "?"],
        "the public fields of a class instance"
    );
    assert_eq!(
        get(
            "const { anything, other = 1 } = d\n",
            &["anything", "other"]
        ),
        ["number | undefined", "number"]
    );
    assert_eq!(get("const { size } = u\nconst { length } = \"text\"\nconst [first] = k\nconst [...all] = o\nconst { ...more } = d\n", &["size", "length", "first", "all", "more"]), ["?"; 5], "what is no shape here is a later pass's");
}

#[test]
fn a_pattern_takes_from_a_tuple_or_an_array_by_place() {
    let known = |types: &mut Types| {
        let tuple = types.intern(Type::Tuple(vec![
            TypeId::INT32,
            TypeId::STRING,
            TypeId::BOOLEAN,
        ]));
        let array = types.intern(Type::Array {
            element: TypeId::STRING,
            mutated: true,
        });
        let or_undefined = types.union([TypeId::NUMBER, TypeId::UNDEFINED]);
        let gaps = types.intern(Type::Tuple(vec![or_undefined, TypeId::UNDEFINED]));
        (vec![("t", tuple), ("xs", array), ("g", gaps)], Vec::new())
    };
    let head = "const t = f()\nconst xs = f()\nconst g = f()\n";
    let get = |body: &str, names: &[&str]| inferred_with(&format!("{head}{body}"), names, &known);
    assert_eq!(
        get("const [a, b, c, d] = t\n", &["a", "b", "c", "d"]),
        ["int32", "string", "boolean", "undefined"]
    );
    assert_eq!(
        get("const [a, b, ...rest] = t\n", &["b", "rest"]),
        ["string", "[boolean]"]
    );
    assert_eq!(
        get(
            "const [...rest] = t\nconst [a, b, c, ...none] = t\n",
            &["rest", "none"]
        ),
        ["[int32, string, boolean]", "[]"]
    );
    assert_eq!(
        get("const [a, b, c] = xs\n", &["a", "c"]),
        ["string | undefined", "string | undefined"],
        "an element may be absent"
    );
    assert_eq!(
        get("const [a = 1, ...rest] = xs\n", &["a", "rest"]),
        ["int32 | string", "readonly string[]"]
    );
    assert_eq!(
        get("const [a = \"d\", b = 2, c = 3] = g\n", &["a", "b", "c"]),
        ["number | string", "int32", "int32"]
    );
    assert_eq!(
        get(
            "const [[a, { b }], ...[c]] = [[1, { b: \"x\" }], true]\n",
            &["a", "b", "c"]
        ),
        ["int32 | { b: \"x\" } | undefined", "?", "boolean"],
        "the literal a pattern takes from is a tuple; a literal inside it an array, any element maybe absent"
    );
    assert_eq!(
        get("const { p: [a, { q = 5, ...r }], ...s } = { p: t, z: 1 }\nconst [{ deep: [d] }] = xs\n", &["a", "q", "r", "s", "d"]),
        ["int32", "?", "?", "{ z: 1 }", "?"],
        "patterns in patterns"
    );
    let nested = |types: &mut Types| {
        let inner = types.intern(Type::Object(vec![
            sjs_types::Property {
                key: "q".to_owned(),
                ty: TypeId::UNDEFINED,
                literal: false,
            },
            sjs_types::Property {
                key: "w".to_owned(),
                ty: TypeId::STRING,
                literal: false,
            },
        ]));
        let pair = types.intern(Type::Tuple(vec![TypeId::INT32, inner]));
        (vec![("t", pair)], Vec::new())
    };
    assert_eq!(
        inferred_with(
            "const t = f()\nconst [a, { q = 5, ...r }, ...[none = 1]] = t\n",
            &["a", "q", "r", "none"],
            &nested
        ),
        ["int32", "int32", "{ w: string }", "int32"]
    );
    assert_eq!(
        get("const [a] = \"text\"\nconst { b } = t\n", &["a", "b"]),
        ["?", "?"]
    );
}
