//! STORIES.md US-093: iteration protocol typing.

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

#[test]
fn a_loop_binds_what_iterating_its_iterable_gives() {
    assert_eq!(
        clean("const xs = [1, 2]\nconst m = new Map()\nm.set(\"a\", 1)\nconst s = new Set()\ns.add(true)\nfor (const x of xs) {\n  const a = x\n}\nfor (const [k, v] of m) {\n  const b = [k, v]\n}\nfor (const e of s) {\n  const c = e\n}\nfor (const ch of \"abc\") {\n  const d = ch\n}\nfor (let l of [1.5]) {\n  l = 2\n}\nfor (const u of unknownThing) {\n  const w = u\n}\n", &["x", "a", "k", "v", "b", "e", "ch", "d", "l", "u", "w"]),
        ["int32", "int32", "string", "int32", "[string, int32]", "boolean", "string", "string", "number", "?", "?"],
        "an array its elements, a `Map` its pairs, a `Set` its members, a string strings"
    );
    assert_eq!(
        clean("const sp = [...\"ab\"]\nconst xs = [1, 2]\nconst sp2 = [...xs, \"z\"]\nconst s = new Set()\ns.add(true)\nconst af = Array.from(s)\nconst itf = Iterator.from(xs)\nconst no = [...5]\nconst two = Array.from([1], (x) => x)\nconst un = Array.from(unknownThing)\n", &["sp", "sp2", "af", "itf", "no", "two", "un"]),
        ["readonly string[]", "readonly (int32 | string)[]", "readonly boolean[]", "Iterator<int32>", "?", "?", "?"],
        "a spread, `Array.from` and `Iterator.from` hold what is iterated"
    );
    let c = checked("for (const n of 5) {}\nfor (const o of { a: 1 }) {}\nconst w = new WeakMap()\nfor (const e of w) {}\nlet either = [1]\neither = 2\nfor (const x of either) {}\nfor (const f of (a) => a) {}\nfor (const i of Iterator.from(\"ab\")) {\n  const each = i\n}\nconst pending = async () => 1\nfor (const p of [pending()]) {\n  const promised = p\n}\n");
    assert_eq!(
        c.of("promised"),
        "Promise<int32>",
        "awaited by `for await` only"
    );
    assert_eq!(c.of("each"), "string", "an iterator what it gives");
    assert_eq!(
        c.found,
        [
            "E138 @1:17 5",
            "E138 @2:17 { a: 1 }",
            "E138 @7:17 either",
            "E138 @8:17 (a) => a"
        ],
        "a number, a shape, a union with a member that is none, a function"
    );
}

#[test]
fn a_generator_yields_the_union_of_its_yields() {
    let c = checked("function* gen() {\n  yield 1\n  yield \"a\"\n  yield* [true]\n  return 2n\n}\nfunction* none() {}\nasync function* ag() {\n  yield 1\n  return \"done\"\n}\nfunction* bare() {\n  const got = yield\n}\nfunction* nested() {\n  yield 1\n  const arrow = () => 2\n  class K {\n    *m() {\n      yield \"no\"\n    }\n  }\n}\nfunction* outer() {\n  yield* gen()\n}\nfunction* del() {\n  yield* 5\n}\nfunction* un() {\n  yield f()\n}\nfunction* und() {\n  yield* f()\n}\nconst g = gen()\nfor (const v of gen()) {\n  const got = v\n}\nfor (const w of ag()) {}\nconst run = async () => {\n  for await (const y of ag()) {\n    const one = y\n  }\n  for await (const z of [pending(), 1]) {\n    const two = z\n  }\n  for await (const q of 5) {}\n}\nconst pending = async () => \"p\"\n");
    assert_eq!(
        c.found,
        ["E138 @40:17 ag()", "E138 @48:25 5"],
        "an async generator takes `for await`"
    );
    assert_eq!(
        c.of("gen"),
        "gen(): Generator<int32 | string | boolean, bigint>"
    );
    assert_eq!(
        c.of("none"),
        "none(): Generator<never, undefined>",
        "yielding nothing"
    );
    assert_eq!(
        c.of("ag"),
        "ag(): AsyncGenerator<int32, string>",
        "with what it returns"
    );
    assert_eq!(c.of("bare"), "bare(): Generator<undefined, undefined>");
    assert_eq!(
        c.of("nested"),
        "nested(): Generator<int32, undefined>",
        "its own yields only"
    );
    assert_eq!(
        c.of("outer"),
        "outer(): Generator<int32 | string | boolean, undefined>"
    );
    assert_eq!(
        (c.of("del"), c.of("un"), c.of("und")),
        ("?".into(), "?".into(), "?".into())
    );
    assert_eq!(
        (c.of("g"), c.of("v")),
        (
            "Generator<int32 | string | boolean, bigint>".into(),
            "int32 | string | boolean".into()
        )
    );
    assert_eq!(
        (c.of("y"), c.of("one"), c.of("z"), c.of("two")),
        (
            "int32".into(),
            "int32".into(),
            "int32 | string".into(),
            "int32 | string".into()
        )
    );
}

#[test]
fn a_class_with_a_symbol_iterator_is_iterable() {
    let c = checked("class Bag {\n  *[Symbol.iterator]() {\n    yield* this.items\n  }\n  items = [1, 2]\n}\nclass Stream {\n  async *[Symbol.asyncIterator]() {\n    yield \"a\"\n  }\n  *[Symbol.iterator]() {\n    yield 1\n  }\n}\nclass Later {\n  async *[Symbol.asyncIterator]() {\n    yield true\n  }\n}\nclass A {\n  [Symbol.iterator]() {\n    return new B()\n  }\n}\nclass B {\n  [Symbol.iterator]() {\n    return new A()\n  }\n}\nclass Odd {\n  [Symbol.iterator]() {\n    return this\n  }\n}\nclass Plain {}\nclass Bad {\n  [Symbol.iterator]() {\n    return 1\n  }\n}\nclass Untyped {\n  [Symbol.iterator]() {\n    return f()\n  }\n}\nfor (const b of new Bag()) {\n  const got = b\n}\nconst all = [...new Bag()]\nfor (const s of new Stream()) {\n  const sync = s\n}\nconst run = async () => {\n  for await (const t of new Stream()) {\n    const later = t\n  }\n  for await (const u of new Bag()) {\n    const plain = u\n  }\n  for await (const l of new Later()) {\n    const flag = l\n  }\n}\nfor (const c of new A()) {}\nfor (const o of new Odd()) {}\nfor (const n of new Untyped()) {}\nfor (const p of new Plain()) {}\nfor (const q of new Bad()) {}\nfor (const r of new Later()) {}\n");
    assert_eq!(
        c.found,
        [
            "E138 @67:17 new Plain()",
            "E138 @68:17 new Bad()",
            "E138 @69:17 new Later()"
        ],
        "no `[Symbol.iterator]`, one that gives a number, an async one alone"
    );
    assert_eq!(
        c.class("Bag"),
        "items: readonly int32[]; [Symbol.iterator](): Generator<int32, undefined>"
    );
    assert_eq!(
        (c.of("got"), c.of("all"), c.of("sync")),
        ("int32".into(), "readonly int32[]".into(), "int32".into())
    );
    assert_eq!(
        (c.of("later"), c.of("plain"), c.of("flag")),
        ("string".into(), "int32".into(), "boolean".into())
    );
    assert_eq!(
        (c.of("c"), c.of("o"), c.of("n")),
        ("?".into(), "?".into(), "?".into())
    );
}

#[test]
fn the_helpers_are_typed_on_every_iterator() {
    let c = checked("function* gen() {\n  yield 1\n  yield 2\n}\nconst m = gen().map((x) => `${x}`)\nconst f = gen().filter((x) => x > 1)\nconst t = gen().take(1).drop(1)\nconst fm = gen().flatMap((x) => [x, \"s\"])\nconst r = gen().reduce((acc, x) => acc + x, 0)\nconst r2 = gen().reduce((acc, x) => acc)\nconst rs = gen().reduce((acc, x, i) => `${acc}${i}`, \"\")\nconst arr = gen().toArray()\nconst fe = gen().forEach((x, i) => i)\nconst so = gen().some((x) => x > 1)\nconst ev = gen().every((x) => x > 1)\nconst fi = gen().find((x) => x > 1)\nconst chain = Iterator.from([1]).map((x) => x * 2).toArray()\nconst other = gen().next()\nconst rd = gen().reduce((acc, x) => acc, later())\nconst rn = gen().reduce(notKnown, 0)\nconst nf = gen().map(notKnown)\nconst bad = gen().flatMap((x) => 1)\nconst xs = [1]\nconst no = xs.map((x) => x)\nlet o = gen()\no = [1]\nconst either = o.map((x) => x)\nlet e = gen()\ne = Iterator.from(\"a\")\nconst both = e.map((x) => x)\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.of("m"), c.of("f"), c.of("t")),
        (
            "Iterator<string>".into(),
            "Iterator<int32>".into(),
            "Iterator<int32>".into()
        )
    );
    assert_eq!(
        (c.of("fm"), c.of("r"), c.of("r2"), c.of("rs")),
        (
            "Iterator<int32 | string>".into(),
            "number".into(),
            "int32".into(),
            "string".into()
        )
    );
    assert_eq!(
        (c.of("arr"), c.of("fe"), c.of("so"), c.of("ev")),
        (
            "readonly int32[]".into(),
            "undefined".into(),
            "boolean".into(),
            "boolean".into()
        )
    );
    assert_eq!(
        (c.of("fi"), c.of("chain")),
        ("int32 | undefined".into(), "readonly number[]".into())
    );
    assert_eq!(c.of("both"), "Iterator<int32 | string>");
    for later in ["other", "rd", "rn", "nf", "bad", "no", "either"] {
        assert_eq!(c.of(later), "?", "{later}");
    }
    // On an async iterator: what ends the iteration gives its result in time.
    let c = checked("async function* ag() {\n  yield 1\n}\nconst m = ag().map((x) => x + 1)\nconst arr = ag().toArray()\nconst fi = ag().find((x) => x > 0)\nconst fe = ag().forEach((x) => x)\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.of("m"), c.of("arr")),
        (
            "AsyncIterator<number>".into(),
            "Promise<readonly int32[]>".into()
        )
    );
    assert_eq!(
        (c.of("fi"), c.of("fe")),
        (
            "Promise<int32 | undefined>".into(),
            "Promise<undefined>".into()
        )
    );
    // A callback is passed the element and its place, and no more.
    let source = "function* gen() {\n  yield \"a\"\n}\nconst ex = gen().map((x, i, more) => x)\nconst half = gen().map((x) => x / 2)\nconst second = gen().reduce((acc) => acc, (x) => x * 2)\n";
    let c = checked(source);
    assert_eq!(c.found, ["E136 @4:29 more", "E138 @5:33 /"]);
    assert_eq!(
        c.related,
        [
            vec!["4:12 passed a `string` here", "4:12 passed a `int32` here"],
            vec!["5:14 passed a `string` here"]
        ],
        "with where the callback is passed it"
    );
}
