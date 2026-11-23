//! STORIES.md US-098: `RegExp`, `Error`, `Date` and `Intl`.

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
fn a_regex_literal_carries_its_groups_and_its_flags() {
    let source = "const re = /(?<year>\\d{4})-(\\d\\d)|(?<year>\\d\\d)(?:x)[a\\(]\\((?=b)(?<=c)(?<!d)(?!e)/dgv\nconst plain = /a/v\nconst m = re.exec(\"2026-01\")\nconst t = re.test(\"x\")\nconst p = plain.exec(\"a\")\nconst a = [re.source, re.flags, re.global, re.lastIndex, re.toString(), re.hasIndices]\nconst o = { re: undefined }\nlet maybe = o\nmaybe = { re }\nconst q = maybe.re?.exec(\"x\")\n";
    let c = checked(source);
    assert_eq!(c.found, NONE);
    assert_eq!(
        c.of("re"),
        "RegExp<{ \"1\": string | undefined, \"2\": string | undefined, \"3\": string | undefined, year: string | undefined }, \"dgv\">",
        "a group by its number, a named one by its name too; a name in two alternatives once"
    );
    assert_eq!(c.of("plain"), "RegExp<{}, \"v\">");

    let found = "{ \"0\": string, \"1\": string | undefined, \"2\": string | undefined, \"3\": string | undefined, groups: { year: string | undefined }, index: int32, indices: readonly ([int32, int32] | undefined)[], input: string }";
    assert_eq!(
        c.of("m"),
        format!("{found} | null"),
        "with `indices`, for the `d` flag"
    );
    assert_eq!(c.of("q"), format!("{found} | null | undefined"));
    assert_eq!(
        c.of("p"),
        "{ \"0\": string, groups: undefined, index: int32, input: string } | null"
    );
    assert_eq!(
        (c.of("t"), c.of("a")),
        (
            "boolean".into(),
            "[string, string, boolean, int32, string, boolean]".into()
        )
    );
    let c = checked("const re = /(a)(?<b>c)/v\nconst plain = /a/v\nconst text = \"a-b\"\nre.lastIndex = 0\nre.compile()\nconst n = new RegExp(\"a\")\nconst c = RegExp(\"a\")\nconst r = text.replace(re, (all, one, two, at, input, groups) => `${all}${at}${groups.b}${one}${input}`)\nconst r2 = text.replace(\"a\", (all, at, input) => `${all}${at}${input}`)\nconst r3 = text.replaceAll(plain, (all, at, input, extra) => all)\nconst r4 = text.replace(plain, (all) => all / 2)\nconst nope = re.nope\nconst x = [plain.exec, n.exec(\"a\"), unknownThing.exec(\"a\"), f().exec(\"a\"), text.match(re)]\nconst other = [1].replace(re, (all, more) => all)\nconst un = text.replace(unknownThing, (all, at, input) => all)\nconst first = text.replace((a, b, c, d) => a, re)\nconst second = text.padEnd(5, (a, b, c, d) => a)\nconst third = unknownThing.replace(re, (a) => a)\nclass P {\n  a\n  b\n  constructor(a, b) {\n    this.a = a\n    this.b = b\n  }\n  exec() {\n    return 1\n  }\n  static has(o) {\n    return #none in o\n  }\n}\nconst mine = new P(1, \"s\").exec()\n");
    assert_eq!(
        c.of("mine"),
        "int32",
        "an `exec` of a class of the program is its own"
    );
    assert_eq!(
        c.found,
        [
            "E133 @4:4 lastIndex",
            "E133 @5:4 compile",
            "E133 @6:11 new RegExp(\"a\")",
            "E133 @7:11 RegExp(\"a\")",
            "E125 @8:80 groups.b",
            "E125 @8:91 one",
            "E136 @10:52 extra",
            "E138 @11:45 /",
            "E137 @12:17 nope",
            "E138 @13:18 exec",
            "E137 @14:19 replace"
        ],
        "the removed members and makings; a callback past what `replace` passes it"
    );
    assert_eq!(
        (c.of("r"), c.of("r2"), c.of("x")),
        ("string".into(), "string".into(), "?".into())
    );
    let _ = (c.class("none"), c.related);
}

#[test]
fn errors_are_classes_a_class_may_extend() {
    let source = "const e = new Error(\"m\", { cause: 1 })\nconst a = [e.message, e.name, e.stack, e.cause, `${e}`]\nclass Mine extends Error {\n  code = 1\n  constructor(message) {\n    super(message)\n  }\n  describe() {\n    return `${this.message}${this.code}`\n  }\n}\nclass Bare extends TypeError {}\nclass Far extends ns.Error {}\nconst m = new Mine(\"x\")\nconst b = [m.message, m.code, new Bare(\"y\").name, new AggregateError([e]).errors, m instanceof Error, m.describe()]\nconst kinds = [new EvalError(\"a\"), new RangeError(\"a\"), new ReferenceError(\"a\"), new SyntaxError(\"a\"), new TypeError(\"a\"), new URIError(\"a\")]\nconst far = new Far().message\nconst copy = structuredClone(e)\nlet slot = e\nslot = new TypeError(\"t\")\nconst box = { last: e }\nbox.last = new RangeError(\"r\")\n";
    assert_eq!(
        clean(
            source,
            &["e", "a", "m", "b", "kinds", "far", "copy", "slot"]
        ),
        [
            "Error",
            "[string, string, string | undefined, unknown, string]",
            "Mine",
            "[string, int32, string, readonly unknown[], boolean, string]",
            "[EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError]",
            "?",
            "Error",
            "Error | TypeError"
        ]
    );
    let c = checked("const e = new Error(\"m\")\nclass Mine extends Error {}\nconst bad = [e.nope, new Mine().code, new TypeError(\"t\").errors]\nclass Local extends RangeError {\n  m() {\n    return this.nope\n  }\n}\n");
    assert_eq!(
        c.found,
        [
            "E137 @3:16 nope",
            "E137 @3:33 code",
            "E137 @3:58 errors",
            "E137 @6:17 nope"
        ]
    );
    assert_eq!(
        found("const Error = f()\nclass Mine extends Error {\n  m() {\n    return this.nope\n  }\n}\n"),
        NONE,
        "a name the file declares is not the class of the ambient library"
    );
}

#[test]
fn a_date_is_made_and_read_in_the_kept_ways() {
    let source = "const d = new Date(0)\nconst iso = new Date(\"2026-01-31T12:00:00Z\")\nconst a = [d.getTime(), d.toISOString(), d.valueOf(), d.getUTCFullYear(), d.getUTCMilliseconds(), Date.now()]\nconst text = \"x\"\nconst loose = new Date(text)\nconst both = new Date(d.getTime() + 1)\n";
    assert_eq!(
        clean(source, &["d", "iso", "a", "loose", "both"]),
        [
            "Date",
            "Date",
            "[number, string, number, number, number, number]",
            "Date",
            "Date"
        ],
        "a text that is no literal is the one parse left unchecked"
    );
    for iso in [
        "2026",
        "2026-01",
        "2026-01-31",
        "2026-01-31T12:00",
        "2026-01-31T12:00:00",
        "2026-01-31T12:00:00.5",
        "2026-01-31T12:00:00.123Z",
        "2026-01-31T12:00+01:00",
        "2026-01-31T12:00:00-08:30",
        "2026T12:00Z",
    ] {
        assert_eq!(
            found(&format!("const d = new Date(\"{iso}\")\n")),
            NONE,
            "{iso}"
        );
    }
    for other in [
        "Jan 31 2026",
        "",
        "26-01-31",
        "2026-1-31",
        "2026-01-31-01",
        "2026/01/31",
        "20260131",
        "2026-01-31 12:00",
        "2026-01-31T",
        "2026-01-31T12",
        "2026-01-31T1:00",
        "2026-01-31T12:00:00:00",
        "2026-01-31T12:00.5",
        "2026-01-31T12:00:00.",
        "2026-01-31T12:00:00.x",
        "2026-01-31T12:00+01",
        "2026-01-31T12:00+01:0",
        "2026-01-31T12:00+01:00:00",
        "2026-01-31T12:00Zx",
        "2026-01-31T12:00X",
        "x026-01-31",
        "2026-0x-31",
        "2026-01-31T12:0x",
        "2026-01-31T12:00+0x:00",
        "2026-01-312",
    ] {
        let found = found(&format!("const d = new Date(\"{other}\")\n"));
        assert_eq!(found.len(), 1, "{other}");
        assert!(found[0].starts_with("E133 @1:20 "), "{other}: {found:?}");
    }
    let c = checked("const d = new Date(0)\nconst now = new Date()\nconst b = [d.getFullYear(), d.setTime(1), d.getTimezoneOffset(), d.toString(), d.setUTCHours(1)]\nconst called = Date()\nconst mine = new Other()\nconst n = [new unknownThing.Date(), Date.other()]\n");
    assert_eq!(
        c.found,
        [
            "E133 @2:13 new Date()",
            "E133 @3:14 getFullYear",
            "E133 @3:31 setTime",
            "E133 @3:45 getTimezoneOffset",
            "E137 @3:68 toString",
            "E133 @3:82 setUTCHours"
        ],
        "no argument, the setters and the local getters; any other key is not one"
    );
    assert_eq!(found("const Date = f()\nconst d = new Date()\nconst RegExp = f()\nconst r = new RegExp(\"a\")\n"), NONE);
}

fn found(text: &str) -> Vec<String> {
    checked(text).found
}

#[test]
fn intl_is_typed_from_its_data_file() {
    let source = "const c = new Intl.Collator(\"en\")\nconst names = [\"b\", \"a\"]\nconst a = [c.compare(\"a\", \"b\"), names.toSorted(c.compare), c.resolvedOptions()]\nconst nf = new Intl.NumberFormat(\"en\")\nconst b = [nf.format(1), nf.formatToParts(1), new Intl.PluralRules(\"en\").select(1), new Intl.DisplayNames(\"en\").of(\"US\"), Intl.supportedValuesOf(\"currency\"), new Intl.Locale(\"en\").language, new Intl.Locale(\"en\").region, Intl.getCanonicalLocales(\"en\")]\n";
    assert_eq!(
        clean(source, &["c", "a", "nf", "b"]),
        [
            "Intl.Collator",
            "[number, readonly string[], unknown]",
            "Intl.NumberFormat",
            "[string, readonly unknown[], string, string | undefined, readonly string[], string, string | undefined, readonly string[]]"
        ]
    );
    let c = checked("const c = new Intl.Collator(\"en\")\nconst bad = [c.nope, Intl.nope(1), new Intl.Nope(), c.resolvedOptions]\nconst mine = new Other.Collator()\nconst none = Intl.nope(1)\n");
    assert_eq!(c.of("none"), "?");
    assert_eq!(c.found, ["E137 @2:16 nope", "E138 @2:55 resolvedOptions"]);
    assert_eq!(
        clean("const Intl = { Collator: 1, f: (x) => x }\nconst c = Intl.Collator\nconst d = Intl.f(1)\n", &["c", "d"]),
        ["1", "int32"],
        "a name the file declares is not the `Intl` of the ambient library"
    );
}
