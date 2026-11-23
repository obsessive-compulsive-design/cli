//! STORIES.md US-094: async typing.

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

#[test]
fn await_gives_what_a_promise_resolves_to() {
    let c = checked("const f = async () => await Promise.resolve(1)\nconst g = async () => f()\nconst { promise, resolve } = Promise.withResolvers()\nresolve(f())\nlet either = f()\neither = 2\nconst nested = async () => {\n  const a = await f()\n  const b = await g()\n  const c = await 5\n  const d = await either\n  const e = await promise\n  const u = await unknownThing\n  const s = await \"s\"\n  return [a, b]\n}\nconst top = await f()\n");
    assert_eq!(
        c.found,
        [
            "E118 @2:11 async",
            "E119 @10:19 5",
            "E119 @11:19 either",
            "E119 @14:19 \"s\""
        ],
        "a number, a union with a member that is no promise, a string; once each"
    );
    assert_eq!(c.of("f"), "f(): Promise<int32>");
    assert_eq!(
        c.of("g"),
        "g(): Promise<int32>",
        "a promise returned is waited for"
    );
    assert_eq!(c.of("promise"), "Promise<Promise<int32>>");
    assert_eq!(
        (c.of("a"), c.of("b"), c.of("d"), c.of("e")),
        (
            "int32".into(),
            "int32".into(),
            "int32".into(),
            "int32".into()
        )
    );
    assert_eq!((c.of("u"), c.of("top")), ("?".into(), "int32".into()));
    assert_eq!(c.of("nested"), "nested(): Promise<readonly int32[]>");
    // An instance of a class with `then` is awaited as a promise is.
    let c = checked("class T {\n  then(resolve) {\n    resolve(\"done\")\n    resolve()\n  }\n}\nclass Sub extends T {}\nclass Quiet {\n  then(resolve) {}\n}\nclass Odd {\n  then() {}\n}\nclass Plain {}\nclass Field {\n  then = 1\n}\nconst w = async () => {\n  const v = await new T()\n  const s = await new Sub()\n  const q = await new Quiet()\n  const o = await new Odd()\n  const p = await new Plain()\n  const fl = await new Field()\n  return v\n}\nnew T().then(() => 0)\n");
    assert_eq!(
        c.found,
        ["E119 @23:19 new Plain()", "E119 @24:20 new Field()"],
        "a `then` of a class is no chain on a promise; a class without the method is no promise"
    );
    assert_eq!((c.of("p"), c.of("fl")), ("Plain".into(), "Field".into()));
    assert_eq!(
        (c.of("v"), c.of("s")),
        ("string | undefined".into(), "string | undefined".into())
    );
    assert_eq!(
        (c.of("q"), c.of("o"), c.of("w")),
        (
            "unknown".into(),
            "unknown".into(),
            "w(): Promise<string | undefined>".into()
        )
    );
    assert_eq!(c.class("Odd"), "then(): undefined");
    assert!(c.related.iter().all(Vec::is_empty), "nothing is related");
}

#[test]
fn a_promise_that_is_dropped_floats() {
    let c = checked("const f = async () => await Promise.resolve(1)\nconst drop = (x) => 1\nconst keep = (x) => x\nconst wait = async (x) => await x\nconst hold = (x) => {\n  const kept = [x]\n  return kept.length\n}\nconst pick = (x, y) => (y ? (x) : 0)\nconst read = (x) => x === 1\nconst call = (x) => x()\nconst rest = (...xs) => 1\nconst fwd = (x) => keep(x)\nconst set = (x) => {\n  x = 1\n  return 1\n}\nconst store = (x) => {\n  let kept\n  kept = x\n  return 1\n}\nf()\n;[f(), 1]\nconst kept = f()\nconst later = async () => await kept\nconst ret = () => f()\ndrop(f())\ndrop(1)\ndrop(...[f()])\nrest(f())\nread(f())\nconst k = call(f)\nconst a = keep(f())\nconst b = wait(f())\nconst c = hold(f())\nconst d = pick(f(), true)\nlet e = f()\ne = f()\nPromise.all([f()])\nunknownThing(f())\nconst z = fwd(f())\nconst y = store(f())\nset(f())\nfor (const p of [f()]) {\n  p\n}\n");
    assert_eq!(
        c.found,
        ["E113 @23:1 f()",
            "E113 @24:3 f()",
            "E113 @28:6 f()",
            "E113 @32:6 f()",
            "E113 @40:1 Promise.all([f()])",
            "E113 @44:5 f()",
            "E113 @46:3 p"
        ],
        "stated and dropped, an element of an array that is, passed for a parameter nothing is done with"
    );
    assert_eq!(
        (c.of("kept"), c.of("ret")),
        ("Promise<int32>".into(), "ret(): Promise<int32>".into())
    );
}

#[test]
fn an_async_function_awaits() {
    let c = checked("async function none() {\n  return 1\n}\nasync function* ag() {\n  yield 1\n}\nconst arrow = async () => 1\nconst o = {\n  async m() {},\n  async() {},\n  async: 1,\n  async *[Symbol.asyncIterator]() {},\n  n() {},\n}\nclass K {\n  async m() {}\n  static async s() {\n    await none()\n  }\n  async() {}\n  static async *[Symbol.asyncIterator]() {}\n  get async() {\n    return 1\n  }\n}\nasync function outer() {\n  const inner = async () => await none()\n  const method = {\n    async m() {\n      await none()\n    },\n  }\n  class In {\n    async m() {\n      await none()\n    }\n  }\n}\nasync function looped() {\n  for await (const x of ag()) {\n  }\n}\nconst shaped = async () => ({ v: await none(), w: [await none()] })\nasync function plain() {\n  for (const x of [1]) {\n  }\n}\nfunction sync() {\n  return 1\n}\nawait none()\n");
    assert_eq!(
        c.found,
        [
            "E118 @1:1 async",
            "E118 @4:1 async",
            "E118 @7:15 async",
            "E118 @9:3 async",
            "E118 @12:3 async",
            "E118 @16:3 async",
            "E118 @21:10 async",
            "E118 @26:1 async",
            "E118 @44:1 async"
        ],
        "its own body: not what a function inside it awaits; `for await` awaits"
    );
}

#[test]
fn promises_are_not_made_or_chained_by_hand() {
    let c = checked("const f = async () => await Promise.resolve(1)\nconst p = f()\nnew Promise((r) => r(1))\nconst q = p.then((x) => x)\nconst r = p.catch(() => 0)\nconst s = p.finally(() => 0)\nlet either = f()\neither = { then: 1 }\neither.then\nconst o = { then: 1, value: 2 }\no.then\np.value\nconst mine = () => {\n  const Promise = Map\n  return new Promise()\n}\nnew (pick())()\n");
    assert_eq!(
        c.found,
        [
            "E127 @3:1 new Promise((r) => r(1))",
            "E128 @4:13 then",
            "E128 @5:13 catch",
            "E128 @6:13 finally",
            "E128 @9:8 then"
        ]
    );
}

#[test]
fn the_statics_of_promise_are_typed() {
    let c = checked("const f = async () => await Promise.resolve(1)\nconst ps = [f(), f()]\nlet list = [f()]\nlist = [f(), f()]\nconst all = Promise.all([f(), 1, \"s\"])\nconst all2 = Promise.all(ps)\nconst all3 = Promise.all(list)\nconst all4 = Promise.all([...ps, 1])\nconst lists = () => list\nconst all5 = Promise.all(lists())\nconst set = Promise.allSettled([f()])\nconst set2 = Promise.allSettled(list)\nconst any = Promise.any([f(), Promise.resolve(\"a\")])\nconst race = Promise.race(list)\nconst res = Promise.resolve()\nconst res2 = Promise.resolve(f())\nconst rej = Promise.reject(new Error(\"x\"))\nconst tr = Promise.try(() => 1)\nconst tr2 = Promise.try(f)\nconst { promise, resolve: done } = Promise.withResolvers()\ndone(1)\ndone(\"s\")\nconst plain = Promise.withResolvers()\nconst { reject } = Promise.withResolvers()\nconst no = [Promise.all(), Promise.all(unknownThing), Promise.all([unknownThing]), Promise.all(5)]\nconst no2 = [Promise.try(), Promise.try(1), Promise.resolve(unknownThing), Promise.other(1)]\n");
    assert_eq!(c.found, NONE);
    assert_eq!(
        c.of("all"),
        "Promise<[int32, int32, string]>",
        "place by place"
    );
    assert_eq!(
        (c.of("all2"), c.of("all3")),
        (
            "Promise<[int32, int32]>".into(),
            "Promise<readonly int32[]>".into()
        )
    );
    assert_eq!(c.of("all4"), "Promise<readonly int32[]>");
    assert_eq!(
        c.of("all5"),
        "Promise<readonly int32[]>",
        "of what a call gives"
    );
    assert_eq!(
        c.of("set"),
        "Promise<[{ status: \"fulfilled\", value: int32 } | { status: \"rejected\", reason: unknown }]>"
    );
    assert_eq!(
        c.of("set2"),
        "Promise<readonly ({ status: \"fulfilled\", value: int32 } | { status: \"rejected\", reason: unknown })[]>"
    );
    assert_eq!(
        (c.of("any"), c.of("race")),
        ("Promise<int32 | string>".into(), "Promise<int32>".into())
    );
    assert_eq!(
        (c.of("res"), c.of("res2"), c.of("rej")),
        (
            "Promise<undefined>".into(),
            "Promise<int32>".into(),
            "Promise<never>".into()
        )
    );
    assert_eq!(
        (c.of("tr"), c.of("tr2")),
        ("Promise<int32>".into(), "Promise<int32>".into())
    );
    assert_eq!(
        (c.of("promise"), c.of("done")),
        (
            "Promise<int32 | string>".into(),
            "done(a: int32 | string): undefined".into()
        )
    );
    assert_eq!(
        c.of("plain"),
        "{ promise: Promise<unknown>, reject: (a: unknown): undefined, resolve: (a: unknown): undefined }"
    );
    assert_eq!(
        (c.of("reject"), c.of("no"), c.of("no2")),
        (
            "reject(a: unknown): undefined".into(),
            "?".into(),
            "?".into()
        )
    );
}
