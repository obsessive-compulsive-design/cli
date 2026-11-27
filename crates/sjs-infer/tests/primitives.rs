//! STORIES.md US-096: the built-ins of the primitives, `Math`, `JSON` and
//! `structuredClone`.

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
fn every_kept_static_of_the_ambient_table_is_typed() {
    let samples = [
        ("String.raw", "String.raw({ raw: [\"a\"] })", "string"),
        ("String.fromCodePoint", "String.fromCodePoint(97)", "string"),
        ("Number.isFinite", "Number.isFinite(1)", "boolean"),
        ("Number.isInteger", "Number.isInteger(1.5)", "boolean"),
        ("Number.isNaN", "Number.isNaN(1)", "boolean"),
        ("Number.isSafeInteger", "Number.isSafeInteger(1)", "boolean"),
        ("Number.parseFloat", "Number.parseFloat(\"1\")", "number"),
        ("Number.parseInt", "Number.parseInt(\"1\", 10)", "number"),
        ("BigInt.asIntN", "BigInt.asIntN(8, 1n)", "bigint"),
        ("BigInt.asUintN", "BigInt.asUintN(8, 1n)", "bigint"),
        ("Math.imul", "Math.imul(2, 3)", "int32"),
        ("Math.clz32", "Math.clz32(1)", "int32"),
        ("Math.random", "Math.random()", "number"),
        ("Math.trunc", "Math.trunc(1.5)", "int32"),
        ("Math.floor", "Math.floor(1.5)", "int32"),
        ("Math.ceil", "Math.ceil(1.5)", "int32"),
        ("Math.round", "Math.round(1.5)", "int32"),
        ("JSON.parse", "JSON.parse(\"1\")", "unknown"),
        (
            "JSON.stringify",
            "JSON.stringify(1, undefined, 2)",
            "string",
        ),
        ("JSON.rawJSON", "JSON.rawJSON(\"1\")", "unknown"),
        ("JSON.isRawJSON", "JSON.isRawJSON(1)", "boolean"),
    ];
    let table = sjs_bind::ambient::Table::shipped();
    let mut used = 0;
    for global in ["String", "Number", "BigInt", "Math", "JSON"] {
        let members = &table.global(global).expect("in the table").members;
        for member in members {
            let name = format!("{global}.{member}");
            let sample = samples.iter().find(|(sampled, ..)| *sampled == name);
            used += usize::from(sample.is_some());
            // A constant is read; any other member of `Math` is a function of numbers.
            let constant = member.starts_with(char::is_uppercase) || member == "NaN";
            let (call, ty) = match sample {
                Some((_, call, ty)) => ((*call).to_owned(), *ty),
                None if constant => (name.clone(), "number"),
                None => {
                    assert_eq!(global, "Math", "no sample of {name}");
                    (format!("{name}(1.5)"), "number")
                }
            };
            let typed = clean(&format!("const typed = {call}\n"), &["typed"]);
            assert_eq!(typed, [ty], "{name}");
        }
    }
    assert_eq!(
        used,
        samples.len(),
        "and no sample of what the table does not keep"
    );
    assert_eq!(
        clean("const Math = { PI: \"s\", max: (x) => \"m\" }\nconst a = Math.PI\nconst b = Math.max(1)\nconst c = other.Math.PI\n", &["a", "b", "c"]),
        ["\"s\"", "string", "?"],
        "a name the file declares is not the built-in"
    );
}

#[test]
fn the_methods_of_strings_numbers_and_bigints_are_typed() {
    let source = "const s = \"abc\"\nlet n = 1.5\nn = 2\nconst big = 10n\nlet either = \"a\"\neither = 1\nconst o = { s: undefined }\nlet maybe = o\nmaybe = { s }\nconst a = [s.at(0), s.charAt(0), s.codePointAt(0), s.concat(\"d\"), s.endsWith(\"c\"), s.includes(\"b\"), s.indexOf(\"b\"), s.isWellFormed(), s.match(/a/v), s.matchAll(/a/gv), s.split(\"\"), s.trim(), s.length, s.search(/a/v), s.lastIndexOf(\"a\"), s.startsWith(\"a\")]\nconst b = [n.toFixed(2), n.toPrecision(2), n.toString(16), big.toString(2), (1).toFixed(1), either.toString()]\nconst c = [maybe.s?.trim(), [s].at(0)?.at(0)]\n";
    assert_eq!(
        clean(source, &["a", "b", "c"]),
        [
            "[string | undefined, string, int32 | undefined, string, boolean, boolean, int32, boolean, readonly string[] | null, Iterator<readonly string[]>, readonly string[], string, int32, int32, int32, boolean]",
            "[string, string, string, string, string, string]",
            "[string | undefined, string | undefined]"
        ]
    );
    for method in sjs_infer_methods() {
        let typed = clean(
            &format!("const s = \"abc\"\nconst typed = s.{method}()\n"),
            &["typed"],
        );
        assert_ne!(typed, ["?"], "{method}");
    }
    let c = checked("const s = \"abc\"\nlet n = 1.5\nn = 2\nconst big = 10n\nlet either = \"a\"\neither = 1\nconst c = [s.substr(1), s.charCodeAt(0), s.localeCompare(\"b\"), s.bold(), s.nope, n.toExponential(1), big.toLocaleString(), s.trim, n.at(0), either.trim(), n.substr(1)]\nconst d = [true.toString(), unknownThing.trim()]\n");
    assert_eq!(
        c.found,
        [
            "E133 @7:14 substr",
            "E133 @7:27 charCodeAt",
            "E133 @7:44 localeCompare",
            "E133 @7:66 bold",
            "E137 @7:76 nope",
            "E137 @7:84 toExponential",
            "E137 @7:106 toLocaleString",
            "E138 @7:126 trim",
            "E137 @7:134 at",
            "E137 @7:148 trim",
            "E137 @7:158 substr"
        ],
        "the removed methods of a string; a key a string or a number has not"
    );
    assert_eq!((c.of("c"), c.of("d")), ("?".into(), "?".into()));
}

/// The kept methods of a string, as SPEC.md §6.1 has them: every ES2025 one but the
/// removed.
fn sjs_infer_methods() -> [&'static str; 32] {
    [
        "at",
        "charAt",
        "codePointAt",
        "concat",
        "endsWith",
        "includes",
        "indexOf",
        "isWellFormed",
        "lastIndexOf",
        "match",
        "matchAll",
        "normalize",
        "padEnd",
        "padStart",
        "repeat",
        "replace",
        "replaceAll",
        "search",
        "slice",
        "split",
        "startsWith",
        "substring",
        "toLocaleLowerCase",
        "toLocaleUpperCase",
        "toLowerCase",
        "toString",
        "toUpperCase",
        "toWellFormed",
        "trim",
        "trimEnd",
        "trimStart",
        "valueOf",
    ]
}

#[test]
fn the_removed_ways_of_the_statics_are_found() {
    let c = checked("let n = 1.5\nn = 2\nclass K {}\nlet mixed = { a: 1 }\nmixed = new K()\nconst a = Number.parseInt(\"1\")\nconst b = Number.parseInt(\"1\", 10)\nconst c = JSON.stringify(n, (k, v) => v)\nconst d = JSON.stringify(n, undefined, 2)\nconst e = JSON.stringify(n, [\"a\"])\nconst f = [structuredClone({ a: 1 }), structuredClone([1]), structuredClone(new Map())]\nconst g = structuredClone(new K())\nconst h = structuredClone(() => 1)\nconst i = structuredClone(mixed)\nconst j = [structuredClone(), structuredClone(unknownThing), Number.other(1), JSON.other(1), String.other(1), BigInt.other(1)]\n");
    assert_eq!(
        c.found,
        [
            "E133 @6:11 Number.parseInt(\"1\")",
            "E133 @8:29 (k, v) => v",
            "E133 @10:29 [\"a\"]",
            "E138 @12:27 new K()",
            "E138 @13:27 () => 1",
            "E138 @14:27 mixed"
        ],
        "no radix, a replacer, a clone of what a clone is not"
    );
    assert_eq!(
        (c.of("a"), c.of("c"), c.of("d")),
        ("number".into(), "string".into(), "string".into())
    );
    assert_eq!(
        (c.of("g"), c.of("h"), c.of("j")),
        ("K".into(), "h(): int32".into(), "?".into())
    );
    assert_eq!(c.of("i"), "{ a: int32 } | K");
    let _ = (c.class("K"), c.related);
}
