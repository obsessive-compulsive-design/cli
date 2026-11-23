//! STORIES.md US-095: the built-ins `Object` and `Array`.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::{parse, SyntaxTree};
use sjs_types::{ClassId, Printer, Type, Types};

struct Checked {
    /// `name: type` of each declaration that has one, by name.
    types: Vec<(String, String)>,
    /// The members of each class side that has a type, by the name of the side:
    /// `name: type`, each kind of member apart.
    members: Vec<(String, String)>,
    /// `CODE @line:column text`.
    found: Vec<String>,
    /// The related locations of each finding: `line:column note`.
    related: Vec<Vec<String>>,
}

impl Checked {
    fn of(&self, name: &str) -> String {
        // The last of the name: a binding, after the parameters of the classes.
        let mut types = self.types.iter().rev();
        let found = types.find(|(declared, _)| declared == name);
        found.map_or("?".to_owned(), |(_, ty)| ty.clone())
    }

    fn class(&self, name: &str) -> String {
        let found = self.members.iter().find(|(class, _)| class == name);
        found.map_or("?".to_owned(), |(_, members)| members.clone())
    }
}

/// What checking one inferred file gives.
fn shown(
    source: &SourceFile,
    tree: &SyntaxTree,
    bindings: &sjs_bind::Bindings,
    facts: &Facts,
    types: &mut Types,
) -> Checked {
    let diagnostics = check_expressions(source, tree, bindings, facts, types);
    let name = |class: ClassId| {
        let name = facts.class_names.get(&class);
        name.map_or(format!("Class{}", class.0), Clone::clone)
    };
    let printer = Printer::new(types, &name);
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
            let shown = if matches!(types.get(ty), Type::Function(_)) {
                printer.signature(&declaration.name, &[], ty)
            } else {
                printer.print(ty)
            };
            signatures.push((declaration.name.clone(), shown));
        }
    }
    let mut members = Vec::new();
    for (class, fields) in &facts.fields {
        let mut parts: Vec<String> = fields
            .iter()
            .map(|(key, ty)| format!("{key}: {}", printer.print(*ty)))
            .collect();
        if let Some(shape) = facts.classes.get(class) {
            let optional = |ty: &Option<_>| ty.map_or("?".to_owned(), |ty| printer.print(ty));
            let signatures = shape.signatures.iter();
            parts.extend(signatures.map(|(key, ty)| printer.signature(key, &[], *ty)));
            let getters = shape.getters.iter();
            parts.extend(getters.map(|(key, ty)| format!("get {key}: {}", optional(ty))));
            let setters = shape.setters.iter();
            parts.extend(setters.map(|(key, ty)| format!("set {key}: {}", optional(ty))));
        }
        members.push((name(*class), parts.join("; ")));
    }
    let related = diagnostics
        .iter()
        .map(|d| {
            let notes = d.related.iter();
            notes
                .map(|r| format!("{}:{} {}", r.span.start.line, r.span.start.column, r.note))
                .collect()
        })
        .collect();
    Checked {
        types: signatures,
        members,
        found,
        related,
    }
}

fn checked(text: &str) -> Checked {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    shown(&source, &tree, &bindings, &facts, &mut types)
}

const NONE: [&str; 0] = [];

/// The types of the names, in a text no finding is made on.
fn clean(text: &str, names: &[&str]) -> Vec<String> {
    let checked = checked(text);
    assert_eq!(checked.found, NONE, "{text}");
    names.iter().map(|name| checked.of(name)).collect()
}

const SETUP: &str = "const o = { a: 1, b: \"s\" }\nconst d = Object.fromEntries(Object.entries(o))\nconst xs = [1, 2]\n";

#[test]
fn every_kept_static_of_the_ambient_table_is_typed() {
    let samples = [
        (
            "Object.keys",
            "Object.keys(o)",
            "readonly (\"a\" | \"b\")[]",
        ),
        (
            "Object.values",
            "Object.values(o)",
            "readonly (1 | \"s\")[]",
        ),
        (
            "Object.entries",
            "Object.entries(o)",
            "readonly ([\"a\", 1] | [\"b\", \"s\"])[]",
        ),
        (
            "Object.fromEntries",
            "Object.fromEntries(Object.entries(o))",
            "{ [key: string]: int32 | string }",
        ),
        ("Object.hasOwn", "Object.hasOwn(o, \"a\")", "boolean"),
        (
            "Object.groupBy",
            "Object.groupBy(xs, (x, i) => `${x}${i}`)",
            "{ [key: string]: readonly int32[] }",
        ),
        ("Object.freeze", "Object.freeze(o)", "{ a: 1, b: \"s\" }"),
        ("Object.isFrozen", "Object.isFrozen(o)", "boolean"),
        (
            "Object.assign",
            "Object.assign(o, { a: 2 })",
            "{ a: 1, b: \"s\" }",
        ),
        ("Array.isArray", "Array.isArray(o)", "boolean"),
        ("Array.from", "Array.from(xs)", "readonly int32[]"),
        (
            "Array.fromAsync",
            "Array.fromAsync([Promise.resolve(1)])",
            "Promise<readonly int32[]>",
        ),
        (
            "Array.of",
            "Array.of(1, \"s\")",
            "readonly (int32 | string)[]",
        ),
    ];
    let table = sjs_bind::ambient::Table::shipped();
    for global in ["Object", "Array"] {
        let members = &table.global(global).expect("in the table").members;
        for member in members {
            let name = format!("{global}.{member}");
            let sample = samples.iter().find(|(sampled, ..)| *sampled == name);
            let (_, call, ty) = sample.unwrap_or_else(|| panic!("no sample of {name}"));
            let typed = clean(&format!("{SETUP}const typed = {call}\n"), &["typed"]);
            assert_eq!(typed, [*ty], "{name}");
        }
    }
    assert_eq!(
        samples.len(),
        13,
        "and no sample of what the table does not keep"
    );
}

#[test]
fn the_statics_of_object_and_array_take_what_they_are_given() {
    assert_eq!(
        clean(&format!("{SETUP}const k = Object.keys(d)\nconst v = Object.values(d)\nconst e = Object.entries(d)\nconst lists = Object.fromEntries([[\"a\", 1]])\nconst m = Array.from(xs, (x, i) => `${{x}}${{i}}`)\nconst g = Object.groupBy(\"ab\", (ch) => ch)\n"), &["k", "v", "e", "lists", "m", "g"]),
        ["readonly string[]", "readonly (int32 | string)[]", "readonly [string, int32 | string][]", "{ [key: string]: int32 | string }", "readonly string[]", "{ [key: string]: readonly string[] }"],
        "a dictionary gives strings for keys; a callback is passed the element and its place"
    );
    let untyped = "const a = Object.keys(5)\nconst b = Object.values(unknownThing)\nconst c = Object.fromEntries([1])\nconst d = Object.fromEntries(5)\nconst e = Object.groupBy(5, (x) => x)\nconst f = Object.other(1)\nconst g = Array.of()\nconst h = Array.of(unknownThing)\nconst i = Array.from(5)\nconst j = Array.from([1], notKnown)\nconst k = Array.other(1)\nconst l = Object.freeze()\nconst m = Object.entries([1])\nconst n = Object.groupBy(unknownThing, (x) => x)\nconst p = Array.of(...[1])\nconst q = Object.fromEntries([[]])\nconst r = Array.from((s, t, u) => 1)\n";
    let c = checked(untyped);
    assert_eq!(c.found, ["E110 @16:31 []"]);
    for name in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "p", "q", "r",
    ] {
        assert_eq!(c.of(name), "?", "{name}");
    }
    // A name the file declares is not the built-in.
    assert_eq!(
        clean("const Object = { keys: (x) => 1 }\nconst k = Object.keys(2)\nconst Array = { from: (x, f) => 1 }\nconst a = Array.from(\"ab\", (y) => y / 2)\nconst c = unknownThing(1, (p, q, r) => 1)\n", &["k"]),
        ["int32"]
    );
}

#[test]
fn every_method_of_an_array_is_typed() {
    let source = "const xs = [1, 2, 3]\nconst ws = [\"a\", \"b\"]\nlet ys = [1.5]\nys = [2.5]\nconst nest = [[1], [[\"a\"]]]\nconst at = xs.at(-1)\nconst cc = xs.concat([\"s\"], true, ...ws)\nconst cw = ys.copyWithin(0, 1)\nconst en = xs.entries()\nconst ev = xs.every((x) => x > 1)\nconst fl = ys.fill(0)\nconst fi = xs.filter((x, i) => i > 0)\nconst fd = xs.find((x) => x > 1)\nconst fx = xs.findIndex((x) => x > 1)\nconst fa = xs.findLast((x) => x > 1)\nconst fli = xs.findLastIndex((x) => x > 1)\nconst f1 = nest.flat()\nconst f2 = nest.flat(2)\nconst f0 = nest.flat(0)\nconst f9 = nest.flat(99)\nconst fm = xs.flatMap((x) => [x, `${x}`])\nconst fm2 = xs.flatMap((x) => x)\nconst fe = xs.forEach((x) => x)\nconst inc = xs.includes(1.5)\nconst io = xs.indexOf(1)\nconst jn = xs.join(\",\")\nconst ks = xs.keys()\nconst li = xs.lastIndexOf(2)\nconst mp = xs.map((x, i, all) => all.length + x + i)\nconst po = ys.pop()\nconst pu = ys.push(1)\nconst rd = xs.reduce((acc, x) => acc + x, 0)\nconst r2 = xs.reduce((acc, x, i, all) => acc + i + all.length)\nconst rs = xs.reduceRight((acc, x) => `${acc}${x}`, \"\")\nconst rv = ys.reverse()\nconst sh = ys.shift()\nconst sl = xs.slice(1)\nconst so = xs.some((x) => x > 1)\nconst st = ys.sort((a, b) => a - b)\nconst sp = ys.splice(0, 1)\nconst tl = xs.toLocaleString()\nconst tr = xs.toReversed()\nconst ts = xs.toSorted((a, b) => b - a)\nconst tw = ws.toSorted()\nconst sw = ws.sort()\nconst tp = xs.toSpliced(0, 1)\nconst tg = xs.toString()\nconst us = ys.unshift(1)\nconst vs = xs.values()\nconst wi = xs.with(0, 9)\nconst len = xs.length\n";
    let c = checked(source);
    assert_eq!(c.found, NONE);
    let expected = [
        ("at", "int32 | undefined"),
        ("cc", "readonly (int32 | string | boolean)[]"),
        ("cw", "number[]"),
        ("en", "Iterator<[int32, int32]>"),
        ("ev", "boolean"),
        ("fl", "number[]"),
        ("fi", "readonly int32[]"),
        ("fd", "int32 | undefined"),
        ("fx", "int32"),
        ("fa", "int32 | undefined"),
        ("fli", "int32"),
        ("f1", "readonly (int32 | readonly string[])[]"),
        ("f2", "readonly (int32 | string)[]"),
        (
            "f0",
            "readonly (readonly int32[] | readonly (readonly string[])[])[]",
        ),
        ("f9", "readonly (int32 | string)[]"),
        ("fm", "readonly (int32 | string)[]"),
        ("fm2", "readonly int32[]"),
        ("fe", "undefined"),
        ("inc", "boolean"),
        ("io", "int32"),
        ("jn", "string"),
        ("ks", "Iterator<int32>"),
        ("li", "int32"),
        ("mp", "readonly number[]"),
        ("po", "number | undefined"),
        ("pu", "int32"),
        ("rd", "number"),
        ("r2", "number"),
        ("rs", "string"),
        ("rv", "number[]"),
        ("sh", "number | undefined"),
        ("sl", "readonly int32[]"),
        ("so", "boolean"),
        ("st", "number[]"),
        ("sp", "readonly number[]"),
        ("tl", "string"),
        ("tr", "readonly int32[]"),
        ("ts", "readonly int32[]"),
        ("tw", "readonly string[]"),
        ("sw", "string[]"),
        ("tp", "readonly int32[]"),
        ("tg", "string"),
        ("us", "int32"),
        ("vs", "Iterator<int32>"),
        ("wi", "readonly int32[]"),
        ("len", "int32"),
    ];
    for (name, ty) in expected {
        assert_eq!(c.of(name), ty, "{name}");
    }
    // What is not known yet has no type, and nothing is said of it.
    let c = checked("const xs = [1]\nconst a = xs.map(notKnown)\nconst b = xs.flatMap(notKnown)\nconst c = xs.reduce(notKnown, 0)\nconst d = xs.reduce((acc, x) => acc, later())\nconst e = xs.concat(unknownThing)\nconst f = xs.flat(depth())\nconst g = xs.flat(-1)\nconst h = xs.flat(\"1\")\nlet either = [1]\neither = 2\nconst i = either.map((x) => x)\nconst j = unknownThing.map((x) => x)\nconst k = xs.indexOf(unknownThing)\nconst l = f().g(1)\nconst one = [1]\nconst short = [one]\nconst m = Object.fromEntries(short)\nconst n = xs.push(() => 1)\n");
    for name in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "l", "m"] {
        assert_eq!(c.of(name), "?", "{name}");
    }
    assert_eq!(c.of("n"), "int32", "a function pushed is no callback");
    assert_eq!(
        (c.found.clone(), c.of("k")),
        (vec!["E137 @12:18 map".to_owned()], "int32".to_owned())
    );
}

#[test]
fn what_an_array_is_asked_wrongly_is_found() {
    let c = checked("const xs = [1, 2, 3]\nconst ws = [\"a\"]\nlet ys = [1.5]\nys = [2.5]\nconst io = xs.indexOf(\"s\")\nconst li = xs.lastIndexOf(true)\nconst inc = ws.includes(1)\nconst st = ys.sort()\nconst ts = xs.toSorted()\nconst mixed = [1, \"a\"].toSorted()\nconst nope = xs.nope\nconst read = xs.map\nxs.nope = 1\nconst wrong = ws.map((w) => w / 2)\nconst more = ws.map((w, i, all, extra) => w)\nconst cmp = ws.toSorted((a, b, c) => 0)\nconst ok = [xs.indexOf(1.5), ws.includes(\"b\"), xs.indexOf()]\n");
    assert_eq!(
        c.found,
        [
            "E138 @5:23 \"s\"",
            "E138 @6:27 true",
            "E138 @7:25 1",
            "E133 @8:15 sort",
            "E133 @9:15 toSorted",
            "E133 @10:24 toSorted",
            "E137 @11:17 nope",
            "E138 @12:17 map",
            "E129 @13:4 nope",
            "E138 @14:31 /",
            "E136 @15:33 extra",
            "E136 @16:32 c"
        ],
        "a search for what it cannot hold, a sort with no comparator, a key it has not, a callback misused"
    );
    assert_eq!(c.related[9], ["14:15 passed a `string` here"]);
    assert_eq!(
        (c.of("io"), c.of("st"), c.of("ts")),
        ("int32".into(), "number[]".into(), "readonly int32[]".into())
    );
    let _ = c.class("none");
}
