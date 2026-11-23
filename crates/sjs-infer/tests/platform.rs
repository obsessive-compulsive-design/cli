//! STORIES.md US-099: the platform globals, from their IDL.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::platform::{interfaces, platform, spec, Kind, Spec};
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

fn found(text: &str) -> Vec<String> {
    checked(text).found
}

/// Every name a type of the data files is written with.
fn names(spec: &Spec, found: &mut Vec<String>) {
    match spec {
        Spec::Name(name) => found.push(name.clone()),
        Spec::Array(inner) | Spec::Generic(_, inner) => names(inner, found),
        Spec::Tuple(parts) | Spec::Union(parts) => {
            for part in parts {
                names(part, found);
            }
        }
        Spec::Function(parameters, returns) => {
            for (_, _, ty) in parameters {
                names(ty, found);
            }
            names(returns, found);
        }
    }
}

#[test]
fn the_data_files_are_read_whole() {
    let tables = platform();
    let primitives = [
        "string",
        "number",
        "int32",
        "bigint",
        "boolean",
        "undefined",
        "null",
        "unknown",
    ];
    let known = |name: &String| {
        let interface = tables
            .iter()
            .any(|table| table.kind != Kind::Function && table.name == *name);
        primitives.contains(&name.as_str())
            || interface
            || ["Uint8Array", "ArrayBuffer"].contains(&name.as_str())
    };
    let mut nullable = Vec::new();
    for table in tables {
        let name = &table.name;
        assert!(
            table.source.contains(" §") || table.source.contains("interface"),
            "{name} cites its IDL"
        );
        assert_eq!(table.call.is_some(), table.kind == Kind::Function, "{name}");
        assert!(
            table.kind == Kind::Interface || table.constructor.is_none(),
            "{name}"
        );
        let base = table.extends.iter();
        assert!(
            base.clone()
                .all(|base| tables.iter().any(|other| other.name == *base)),
            "{name}"
        );
        let written = table.members.iter().map(|member| member.what.clone());
        let constructor = table
            .constructor
            .iter()
            .map(|new| format!("{new} => unknown"));
        for what in written.chain(table.call.clone()).chain(constructor) {
            let read = spec(&what).unwrap_or_else(|| panic!("{name}: `{what}` is not read"));
            let mut used = Vec::new();
            names(&read, &mut used);
            assert!(
                used.iter().all(known),
                "{name}: `{what}` names what is none: {used:?}"
            );
        }
        for member in &table.members {
            let nulls = member.what.contains("| null");
            assert_eq!(
                nulls, member.nullable,
                "{name}.{}: `| null` and `# nullable` go together",
                member.name
            );
            nullable.extend(nulls.then(|| format!("{name}.{}", member.name)));
        }
    }
    // What the IDL of §6.2 makes nullable, each found by its marker.
    assert_eq!(
        nullable,
        [
            "Headers.get",
            "Request.body",
            "Response.body",
            "FormData.get",
            "URL.parse",
            "URLSearchParams.get",
            "URLPattern.exec",
            "Event.target",
            "Event.currentTarget",
            "WritableStreamWriter.desiredSize",
            "MessageEvent.source",
        ]
    );
    // Every global of §6.2 in the ambient table has its table here, of its kind, and
    // a namespace lists the members the ambient table keeps.
    let ambient = sjs_bind::ambient::Table::shipped();
    let mut globals = 0;
    for global in ambient.globals().filter(|global| global.spec == "§6.2") {
        globals += 1;
        let table = tables.iter().find(|table| table.name == global.name);
        let table = table.unwrap_or_else(|| panic!("{} has no table", global.name));
        let upper = global.name.starts_with(char::is_uppercase);
        let function = !upper && global.members.is_empty();
        let kind = match (upper, function) {
            (true, _) => Kind::Interface,
            (false, true) => Kind::Function,
            (false, false) => Kind::Namespace,
        };
        assert_eq!(table.kind, kind, "{}", global.name);
        let listed: Vec<&String> = table.members.iter().map(|member| &member.name).collect();
        if kind == Kind::Namespace {
            let kept: Vec<&String> = global.members.iter().collect();
            assert_eq!(listed, kept, "{}", global.name);
        }
    }
    assert_eq!(globals, 39, "the globals of §6.2");
}

#[test]
fn the_notation_of_the_data_files_is_read() {
    let tables = interfaces("# a comment\nstray = \"before any table\"\n[Thing]\nidl = \"Spec §1, interface Thing\"\nextends = \"Base\"\nsample = \"make(\\\"a\\\")\"\nnew = \"(a: string)\"\nstatic of = \"() => Thing\"\nsize = \"int32 | null\" # nullable\n\"[Symbol.iterator]\" = \"() => Iterator<string>\"\nbare = unquoted\n\n[namespace things]\nfirst = \"Thing\"\n[function make]\ncall = \"(a: string) => Thing\"\nnot a line\n");
    assert_eq!(tables.len(), 3);
    let thing = &tables[0];
    assert_eq!(
        (thing.name.as_str(), thing.kind),
        ("Thing", Kind::Interface)
    );
    assert_eq!(thing.source, "Spec §1, interface Thing");
    assert_eq!(thing.extends.as_deref(), Some("Base"));
    assert_eq!(thing.sample.as_deref(), Some("make(\"a\")"));
    assert_eq!(thing.constructor.as_deref(), Some("(a: string)"));
    let members: Vec<(&str, &str, bool, bool)> = thing
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.what.as_str(), m.is_static, m.nullable))
        .collect();
    assert_eq!(
        members,
        [
            ("of", "() => Thing", true, false),
            ("size", "int32 | null", false, true),
            ("[Symbol.iterator]", "() => Iterator<string>", false, false),
            ("bare", "unquoted", false, false)
        ]
    );
    assert_eq!(
        (tables[1].name.as_str(), tables[1].kind),
        ("things", Kind::Namespace)
    );
    assert_eq!(
        (tables[2].name.as_str(), tables[2].kind),
        ("make", Kind::Function)
    );
    assert_eq!(tables[2].call.as_deref(), Some("(a: string) => Thing"));
}

#[test]
fn the_types_of_the_data_files_are_read() {
    let name = |name: &str| Spec::Name(name.to_owned());
    assert_eq!(spec(" string "), Some(name("string")));
    assert_eq!(
        spec("a_b1[][]"),
        Some(Spec::Array(Box::new(Spec::Array(Box::new(name("a_b1"))))))
    );
    assert_eq!(
        spec("[A, B]"),
        Some(Spec::Tuple(vec![name("A"), name("B")]))
    );
    assert_eq!(
        spec("A | B | C"),
        Some(Spec::Union(vec![name("A"), name("B"), name("C")]))
    );
    assert_eq!(
        spec("Promise<A[]>"),
        Some(Spec::Generic(
            "Promise".to_owned(),
            Box::new(Spec::Array(Box::new(name("A"))))
        ))
    );
    assert_eq!(
        spec("(a: A, b?: B | C, ...rest: D) => (E | F)[]"),
        Some(Spec::Function(
            vec![
                (false, false, name("A")),
                (true, false, Spec::Union(vec![name("B"), name("C")])),
                (false, true, name("D"))
            ],
            Box::new(Spec::Array(Box::new(Spec::Union(vec![
                name("E"),
                name("F")
            ]))))
        ))
    );
    assert_eq!(
        spec("() => A"),
        Some(Spec::Function(Vec::new(), Box::new(name("A"))))
    );
    for wrong in [
        "",
        "|",
        "A |",
        "A[",
        "[A",
        "Promise<A",
        "Promise<>",
        "(a) => A",
        "(a: A",
        "(a: A) =>",
        "(: A) => B",
        "A B",
        "(a: A, b: B)",
        "()",
        "<A>",
        "A | | B",
        "(a: ) => A",
    ] {
        assert_eq!(spec(wrong), None, "{wrong}");
    }
    // What is not written right, and a name that is none, are `unknown`.
    let mut types = Types::new();
    let any = sjs_infer::platform::typed(&mut types, &name("Nothing"));
    assert_eq!(any, sjs_types::TypeId::UNKNOWN);
    let listed =
        sjs_infer::platform::typed(&mut types, &spec("AsyncIterator<bigint>").expect("read"));
    let none = |_: ClassId| String::new();
    assert_eq!(
        Printer::new(&types, &none).print(listed),
        "AsyncIterator<bigint>"
    );
}

#[test]
fn the_globals_of_the_platform_are_typed_from_their_idl() {
    let source = "const h = new Headers()\nconst r = fetch(\"https://example.com/\")\nconst r2 = fetch(new Request(\"https://example.com/\"))\nconst r3 = fetch(new URL(\"https://example.com/\"), { method: \"POST\" })\nconst a = [h.get(\"x\"), h.has(\"x\"), h.set(\"a\", \"b\"), [...h]]\nconst b = [performance.now(), navigator.hardwareConcurrency, crypto.randomUUID(), atob(\"a\"), console.log(1, \"a\"), crypto.subtle]\nconst t = setTimeout(() => 1, 10)\nclearTimeout(t)\nclearTimeout()\nconst i = setInterval((x) => x, 10, 1)\nclearInterval(i)\nconst u = URL.parse(\"x\")\nconst c = [URL.canParse(\"x\"), Response.json({ a: 1 }), AbortSignal.timeout(5), new URL(\"https://a.b/\").searchParams.get(\"q\")]\nconst port = new MessageChannel().port1\nport.addEventListener(\"message\", (event) => [event.data, event.origin, event.type])\nconst target = new EventTarget()\ntarget.addEventListener(\"ready\", (event) => event.type)\nconst e = [[1, 2].forEach(console.log), new File([\"a\"], \"a\").size, new CustomEvent(\"x\").type]\nclass Bus extends EventTarget {}\nconst bus = new Bus().dispatchEvent(new Event(\"x\"))\nconst run = async () => {\n  const res = await fetch(\"x\")\n  const body = await res.json()\n  return [res.status, res.headers.get(\"a\"), body, res.body]\n}\nlet names = [\"a\"]\nnames.push(\"b\")\nconst key = crypto.subtle.generateKey(\"x\", true, names)\nconst blob = new Blob([\"a\"])\nconst sent = new WebSocket(\"wss://x/\").send(blob)\nconst text = new TextDecoder().decode(new TextEncoder().encode(\"a\"))\nqueueMicrotask(() => 1)\n";
    let c = checked(source);
    assert_eq!(c.found, NONE);
    assert_eq!(
        (c.of("r"), c.of("r2"), c.of("r3")),
        (
            "Promise<Response>".into(),
            "Promise<Response>".into(),
            "Promise<Response>".into()
        )
    );
    assert_eq!(
        c.of("a"),
        "[string | null, boolean, undefined, readonly [string, string][]]"
    );
    assert_eq!(
        c.of("b"),
        "[number, int32, string, string, undefined, SubtleCrypto]"
    );
    assert_eq!(
        (c.of("t"), c.of("i"), c.of("u")),
        ("Timeout".into(), "Interval".into(), "URL | null".into())
    );
    assert_eq!(c.of("c"), "[boolean, Response, AbortSignal, string | null]");
    assert_eq!(c.of("bus"), "boolean");
    assert_eq!(
        found("const ws = new WebSocket(\"wss://x/\")\nws.addEventListener(\"message\", (event) => event.data.nope)\nws.addEventListener(\"message\", (event) => event.nope)\n"),
        ["E137 @2:54 nope", "E133 @3:49 nope"],
        "the data of a message is `unknown`: nothing is read from it"
    );
    assert_eq!(c.of("e"), "[undefined, number, string]");
    assert_eq!(c.of("run"), "run(): Promise<readonly unknown[]>");
    assert_eq!(
        (c.of("body"), c.of("key"), c.of("text")),
        ("unknown".into(), "Promise<unknown>".into(), "string".into())
    );
    assert_eq!(c.class("typeof URL"), "canParse(a: string, b?: string | URL): boolean; parse(a: string, b?: string | URL): URL | null");
}

#[test]
fn what_the_platform_is_passed_is_what_it_takes() {
    let c = checked("const t = setTimeout(() => 1, 10)\nconst i = setInterval(() => 1, 10)\nclearInterval(t)\nclearTimeout(i)\nclearTimeout(1)\nconst r = fetch(1)\nconst h = new Headers()\nconst a = [h.get(1), h.nope, Headers.nope, console.nope, new URL(1), URL.canParse(true), atob(unknownThing), h.has(...[1])]\nh.nope = 1\nconst b = [new Blob([\"a\"]).slice(\"a\"), crypto.getRandomValues([1]), AbortSignal.any(1), new TextEncoder().encode(1)]\nconst fn = (x) => x\nconst k = [queueMicrotask(1), queueMicrotask(fn), setTimeout(fn, \"soon\"), btoa(), fetch(\"a\", 1, 2)]\nconst mine = (s) => s\nmine(1)\nconst own = { get: (k) => k }\nown.get(1)\n");
    assert_eq!(
        c.found,
        [
            "E138 @3:15 t",
            "E138 @4:14 i",
            "E138 @5:14 1",
            "E138 @6:17 1",
            "E138 @8:18 1",
            "E133 @8:24 nope",
            "E133 @8:38 nope",
            "E138 @8:66 1",
            "E138 @8:83 true",
            "E129 @9:3 nope",
            "E138 @10:34 \"a\"",
            "E138 @10:63 [1]",
            "E138 @10:85 1",
            "E138 @10:114 1",
            "E138 @12:27 1",
            "E138 @12:66 \"soon\"",
            "E136 @12:75 btoa()",
            "E136 @12:83 fetch(\"a\", 1, 2)"
        ],
        "a handle of the other timer, a number for text, a member that is not listed"
    );
    assert_eq!(c.of("r"), "Promise<Response>");
    assert_eq!(
        found("const fetch = (x) => x\nconst r = fetch(1)\nconst console = { log: (x) => x }\nconst l = console.log(1)\n"),
        NONE,
        "a name the file declares is not the global"
    );
    let _ = (c.class("none"), c.related);
}
