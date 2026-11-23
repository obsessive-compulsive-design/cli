//! STORIES.md US-097: `Map`, `Set`, `WeakMap` and the typed arrays.

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
    for (class, fields) in facts.fields.iter() {
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

#[test]
fn a_collection_holds_what_it_is_made_with_and_what_is_put_into_it() {
    let source = "const m = new Map([[\"a\", 1], [\"b\", 2]])\nconst e = new Map()\ne.set(1, \"x\")\nconst grown = new Map([[\"a\", 1]])\ngrown.set(\"b\", \"two\")\nconst s = new Set([1, 2])\nconst more = new Set(\"ab\")\nmore.add(1)\nconst w = new WeakMap()\nconst key = { id: 1 }\nw.set(key, 1)\nconst made = new WeakMap([[key, \"v\"]])\nconst copy = new Map(m)\nconst entries = new Map(Object.entries({ a: 1 }))\nconst list = [[\"a\", 1]]\nconst loose = new Map(list)\nconst mixed = new Map([[\"a\", 1], ...m])\n";
    assert_eq!(
        clean(
            source,
            &["m", "e", "grown", "s", "more", "w", "made", "copy", "entries"]
        ),
        [
            "Map<string, int32>",
            "Map<int32, string>",
            "Map<string, int32 | string>",
            "Set<int32>",
            "Set<int32 | string>",
            "WeakMap<{ id: int32 }, int32>",
            "WeakMap<{ id: int32 }, string>",
            "Map<string, int32>",
            "Map<string, int32>"
        ],
        "from pairs written out, from an iterable of pairs, and from what `set` and `add` put in"
    );
    let c = checked("const a = new Map(5)\nconst b = new Set(unknownThing)\nconst c = new Map([[1]])\nconst d = new Map([1])\nconst e = new Map(unknownThing)\nconst f = new Set(5)\nconst g = new Map([[1, 2]], 3)\nconst h = new Map([[unknownThing, 1]])\nconst i = new Map([[1, unknownThing]])\nconst j = new Map([[1, 2]])\nj.set(unknownThing, 1)\nconst k = new Set([1])\nk.add(unknownThing)\nconst l = new Other([1])\nconst n = new Map([[1, 2], [3]])\nconst o = new Map([[1, 2, 3]])\nconst p = new Map([[...k]])\nconst one = [1]\nconst short = [one]\nconst q = new Map(short)\nconst r = new Map([Object.entries({ a: 1 })[0]])\nconst w = new WeakMap()\nw.set({ a: 1 }, unknownThing)\nconst pushed = new Map([[1, 2]])\npushed.push(1)\n");
    assert_eq!(c.of("pushed"), "Map<int32, int32>");
    for name in [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "n", "o", "p", "q", "r", "w",
    ] {
        assert_eq!(c.of(name), "?", "{name}");
    }
    assert_eq!(c.found, ["E137 @25:8 push"]);
    assert_eq!(
        clean(
            "const Map = f()\nconst mine = new Map([[1, 2]])\n",
            &["mine"]
        ),
        ["?"],
        "a name the file declares is not the collection"
    );
    let many: Vec<String> = (0..60).map(|at| format!("class C{at} {{}}\n")).collect();
    let many = many.concat();
    assert_eq!(
        clean(&format!("{many}const last = new C59()\n"), &["last"]),
        ["C59"],
        "the classes of a file are numbered from the first, whatever the ambient library has"
    );
    let empty = checked("const w = new WeakMap()\nclass K {\n  seen = new WeakMap()\n  m(k) {\n    this.seen.set(k, 1)\n  }\n}\n");
    assert_eq!(
        empty.found,
        ["E110 @1:11 new WeakMap()"],
        "an empty one nothing is put into"
    );
}

#[test]
fn the_methods_of_the_collections_are_typed() {
    let source = "const m = new Map([[\"a\", 1], [\"b\", 2]])\nconst s = new Set([1, 2])\nconst w = new WeakMap()\nconst key = { id: 1 }\nw.set(key, 1)\nlet k = \"a\"\nk = \"b\"\nconst o = { m: undefined }\nlet maybe = o\nmaybe = { m }\nconst a = [m.get(\"a\"), m.get(\"a\") ?? 0, m.has(\"a\"), m.size, m.set(\"c\", 3), m.delete(\"a\"), m.clear(), m.get(k)]\nconst b = [m.keys(), m.values(), m.entries(), m.forEach((v, key2, all) => `${key2}${v}${all.size}`)]\nconst c = [s.add(3), s.has(1.5), s.size, s.union(new Set([\"x\"])), s.intersection(s), s.difference(s), s.symmetricDifference(new Set([true])), s.isSubsetOf(s), s.isSupersetOf(s), s.isDisjointFrom(s), s.values(), s.keys(), s.entries(), s.delete(1), s.clear(), s.forEach((v, again, all) => v + again + all.size)]\nconst d = [w.get(key), w.has(key), w.delete(key), w.set(key, 2)]\nconst g = Map.groupBy([1, 2], (x, i) => `${x}${i}`)\nconst h = [maybe.m?.get(\"a\"), maybe.m?.size]\n";
    assert_eq!(
        clean(source, &["a", "b", "c", "d", "g", "h"]),
        [
            "[int32 | undefined, int32, boolean, int32, Map<string, int32>, boolean, undefined, int32 | undefined]",
            "[Iterator<string>, Iterator<int32>, Iterator<[string, int32]>, undefined]",
            "[Set<int32>, boolean, int32, Set<int32 | string>, Set<int32>, Set<int32>, Set<int32 | boolean>, boolean, boolean, boolean, Iterator<int32>, Iterator<int32>, Iterator<[int32, int32]>, boolean, undefined, undefined]",
            "[int32 | undefined, boolean, boolean, WeakMap<{ id: int32 }, int32>]",
            "Map<string, readonly int32[]>",
            "[int32 | undefined, int32 | undefined]"
        ]
    );
    let c = checked("const m = new Map([[\"a\", 1]])\nconst s = new Set([1])\nconst w = new WeakMap()\nconst key = { id: 1 }\nw.set(key, 1)\nw.set(\"no\", 2)\nconst bad = [m.get(1), s.has(\"x\"), m.delete(true), m.nope, w.size, w.keys(), m.get, w.has(1)]\nfor (const x of w) {}\nconst un = [s.union(5), s.union(), m.get(unknownThing), m.get(), Map.groupBy(5, (x) => x), Map.groupBy([1], notKnown), Map.groupBy([1], 5), Map.groupBy(), unknownThing.get(1), f().get(1)]\nm.forEach((v, key2, all, extra) => v)\nlet either = m\neither = 1\nconst e = either.get(\"a\")\nm.size = 1\n");
    assert_eq!(
        c.found,
        [
            "E138 @6:7 \"no\"",
            "E138 @7:20 1",
            "E138 @7:30 \"x\"",
            "E138 @7:45 true",
            "E137 @7:54 nope",
            "E137 @7:62 size",
            "E137 @7:70 keys",
            "E138 @7:80 get",
            "E138 @7:91 1",
            "E137 @8:17 w",
            "E136 @10:26 extra",
            "E137 @13:18 get"
        ],
        "a key it cannot hold, a `WeakMap` key that is no object, what a `WeakMap` has not"
    );
    assert_eq!((c.of("un"), c.of("e")), ("?".into(), "?".into()));
    let _ = (c.class("none"), c.related);
}

#[test]
fn typed_arrays_and_buffers_are_classes_of_the_ambient_library() {
    let source = "const u = new Uint8Array(8)\nconst big = new BigInt64Array(2)\nconst buf = new ArrayBuffer(8)\nconst view = new DataView(buf)\nconst f = Uint8Array.from([1, 2])\nconst a = [u[0], u.length, u.at(0), u.buffer, u.map((x, i) => x + i), u.join(\",\"), u.subarray(1), big[0], big.at(0), Uint8Array.BYTES_PER_ELEMENT, ArrayBuffer.isView(u)]\nconst b = [view.getInt8(0), view.getBigInt64(0), view.setInt8(0, 1), buf.byteLength, buf.slice(0), buf.resize(1), buf.resizable, view.buffer, view.byteOffset]\nu[0] = 1\nfor (const x of u) {\n  const each = x\n}\nconst arr = [...u]\nconst cl = [structuredClone(u), structuredClone(buf)]\nconst sum = u.reduce((acc, x) => acc + x, 0)\nconst is = u instanceof Uint8Array\nconst all = [Float64Array.of(1.5), Uint8Array.fromHex(\"ff\"), Uint8Array.fromBase64(\"/w==\"), u.keys(), u.entries(), u.values(), u.find((x) => x > 1), u.findIndex((x) => x > 1), u.every((x) => x > 1), u.fill(0), u.set([1]), u.toString(), big.reduce((acc, x) => acc + x)]\nconst kept = `${u}`\n";
    assert_eq!(
        clean(source, &["u", "big", "buf", "view", "f", "a", "b", "each", "arr", "cl", "sum", "is", "all"]),
        [
            "Uint8Array",
            "BigInt64Array",
            "ArrayBuffer",
            "DataView",
            "Uint8Array",
            "[number | undefined, int32, number | undefined, ArrayBuffer, Uint8Array, string, Uint8Array, bigint | undefined, bigint | undefined, int32, boolean]",
            "[number, bigint, undefined, int32, ArrayBuffer, undefined, boolean, ArrayBuffer, int32]",
            "number",
            "readonly number[]",
            "[Uint8Array, ArrayBuffer]",
            "number",
            "boolean",
            "[Float64Array, Uint8Array, Uint8Array, Iterator<int32>, Iterator<[int32, number]>, Iterator<number>, number | undefined, int32, boolean, Uint8Array, undefined, string, bigint]"
        ]
    );
    let table = sjs_bind::ambient::Table::shipped();
    let typed: Vec<&str> = table
        .globals()
        .filter(|global| {
            global
                .members
                .iter()
                .any(|member| member == "BYTES_PER_ELEMENT")
        })
        .map(|global| global.name.as_str())
        .collect();
    assert_eq!(typed.len(), 12, "the typed arrays of the table");
    for name in typed {
        let made = clean(&format!("const made = new {name}(4)\nconst size = {name}.BYTES_PER_ELEMENT\nconst from = {name}.from([1])\n"), &["made", "size", "from"]);
        assert_eq!(made, [name, "int32", name], "{name}");
    }
    let c = checked("const u = new Uint8Array(8)\nconst buf = new ArrayBuffer(8)\nconst view = new DataView(buf)\nu[0] = \"s\"\nconst bad = [u.nope, u.push(1), view.nope, buf.length, u.toBase64()]\nconst wrong = u.map((x) => x.length)\nconst more = u.map((x, i, extra) => x)\nconst dv = DataView.from(1)\nconst other = Uint8Array.other(1)\nconst un = [ArrayBuffer.other(1), Uint8Array.SIZE, other.Uint8Array.BYTES_PER_ELEMENT, u[\"length\"], u[\"nope\"]]\nconst Float32Array = f()\nconst mine = [Float32Array.BYTES_PER_ELEMENT, Float32Array.from([1])]\n");
    assert_eq!(
        c.found,
        [
            "E138 @4:8 \"s\"",
            "E137 @5:16 nope",
            "E137 @5:24 push",
            "E137 @5:38 nope",
            "E137 @5:48 length",
            "E137 @5:58 toBase64",
            "E137 @6:30 length",
            "E136 @7:27 extra",
            "E137 @10:103 \"nope\""
        ]
    );
    assert_eq!((c.of("un"), c.of("mine")), ("?".into(), "?".into()));
    assert_eq!((c.of("dv"), c.of("other")), ("?".into(), "?".into()));
}
