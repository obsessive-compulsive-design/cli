//! STORIES.md US-113: predicate functions (SPEC.md §3.5).

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_infer::{check_expressions, infer_bindings, Facts};
use sjs_syntax::parse;
use sjs_types::{ClassId, Printer, Types};

struct Checked {
    /// `name: type` of each declaration that has a type.
    types: Vec<(String, String)>,
    /// `CODE @line:column` of each finding.
    found: Vec<String>,
}

impl Checked {
    /// The type of the declaration `name`: `?` for one that has none.
    fn of(&self, name: &str) -> &str {
        let found = self.types.iter().find(|(declared, _)| declared == name);
        found.map_or("?", |(_, ty)| ty.as_str())
    }
}

fn checked(text: &str) -> Checked {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (bindings, _) = bind(&source, &tree);
    let mut types = Types::new();
    let facts = infer_bindings(&source, &tree, &bindings, &Facts::default(), &mut types);
    let diagnostics = check_expressions(&source, &tree, &bindings, &facts, &mut types);
    let name = |class: ClassId| facts.class_names.get(&class).cloned().unwrap_or_default();
    let printer = Printer::new(&types, &name);
    let declared = facts.declarations.iter();
    // What the ambient library declares is no declaration of the file.
    let types = declared
        .filter_map(|(&at, &ty)| {
            Some((
                bindings.declarations.get(at)?.name.clone(),
                printer.print(ty),
            ))
        })
        .collect();
    let found = diagnostics
        .iter()
        .map(|d| {
            format!(
                "{:?} @{}:{}",
                d.code, d.span.start.line, d.span.start.column
            )
        })
        .collect();
    Checked { types, found }
}

/// The types of the names given, in order.
fn types(text: &str, names: &[&str]) -> Vec<String> {
    let c = checked(text);
    assert_eq!(c.found, [""; 0], "{text}");
    names.iter().map(|name| c.of(name).to_owned()).collect()
}

/// A value that is a string, a number or `undefined`, and one that may be anything.
const HEAD: &str = "const flag = Math.random() < 0.5\nconst other = Math.random() < 0.5\nconst v = flag ? `${flag}` : other ? Math.random() : undefined\nconst raw = JSON.parse(`${flag}`)\n";

fn with(body: &str, names: &[&str]) -> Vec<String> {
    types(&format!("{HEAD}{body}"), names)
}

#[test]
fn a_function_that_returns_a_test_of_its_parameter_is_that_test() {
    let declared = "function isString(x) {\n  return typeof x === \"string\"\n}\n";
    let arrow = "const isString = (x) => typeof x === \"string\"\n";
    let block = "const isString = (x) => {\n  return (typeof x === \"string\")\n}\n";
    for written in [declared, arrow, block] {
        assert_eq!(
            with(
                &format!("{written}const a = isString(v) ? v : \"\"\nconst b = isString(v) ? 0 : v\nconst c = !isString(v) ? 0 : v\nconst d = isString(v) && v !== \"\" ? v : \"\"\nconst e = isString(v) || v === undefined ? 0 : v\n"),
                &["a", "b", "c", "d", "e"]
            ),
            ["string", "number | undefined", "string | 0", "string", "number"],
            "{written}"
        );
    }
    // Early exits, and a path of keys passed.
    assert_eq!(
        with(
            &format!("{declared}function f() {{\n  const o = {{ v }}\n  if (!isString(o.v)) {{\n    return 0\n  }}\n  const a = o.v\n  return a.length\n}}\nexport const r = f()\n"),
            &["a"]
        ),
        ["string"]
    );
}

#[test]
fn what_a_predicate_returns_is_read_as_a_condition_is() {
    let text = "const isAbsent = (x) => !(typeof x === \"string\" || typeof x === \"number\")\nconst isText = (x) => typeof x === \"string\" && x !== \"\"\nconst isPlain = (x) => typeof x === \"string\" || x === undefined\nconst a = isAbsent(v) ? v : 0\nconst b = isAbsent(v) ? 0 : v\nconst c = isText(v) ? v : \"\"\nconst d = isText(v) ? 0 : v\nconst e = isPlain(v) ? v : \"\"\nconst g = isPlain(v) ? 0 : v\n";
    assert_eq!(
        with(text, &["a", "b", "c", "d", "e", "g"]),
        [
            "0 | undefined",
            "number | string",
            "string",
            "number | \"\" | undefined",
            "string | undefined",
            "number"
        ]
    );
}

#[test]
fn a_predicate_may_ask_its_parameter_for_its_truth() {
    let text = "const s = flag ? { n: 1 } : undefined\nconst isThere = (x) => !!x\nconst a = isThere(s) ? s : 0\nconst b = isThere(s) ? 0 : s\n";
    assert_eq!(with(text, &["a", "b"]), ["0 | { n: 1 }", "0 | undefined"]);
    // Only `!` turns the answer round: `typeof x` asks nothing of `x`.
    let c = checked(&format!("{HEAD}const s = flag ? {{ n: 1 }} : undefined\nconst kind = (x) => typeof x\nconst a = kind(s) ? s : 0\n"));
    assert_eq!(c.of("a"), "0 | { n: 1 } | undefined");
}

#[test]
fn a_predicate_over_another_parameter_and_one_that_calls_another() {
    let text = "function isSet(label, x, strict = true) {\n  return x !== undefined\n}\nconst isString = (x) => typeof x === \"string\"\nconst isWord = (x) => isSet(\"word\", x) && isString(x)\nconst isNumber = (x) => !isWord(x) && isSet(\"number\", x)\nconst a = isSet(\"a\", v) ? v : 0\nconst b = isWord(v) ? v : \"\"\nconst c = isWord(v) ? 0 : v\nconst d = isNumber(v) ? v : 0\nconst e = isSet(v, \"e\") ? v : 0\n";
    assert_eq!(
        with(text, &["a", "b", "c", "d", "e"]),
        [
            "number | string",
            "string",
            "number | undefined",
            "number",
            "number | string | undefined"
        ],
        "a default parameter does not unmake a predicate; the first parameter is tested by none"
    );
    // A parameter that is no plain name is passed over; a predicate that calls
    // itself is followed only so far.
    let text = "const second = ({ a }, x, ...rest) => x !== undefined\nfunction again(x) {\n  return again(x) && x !== undefined\n}\nconst a = second({ a: 1 }, v, v) ? v : 0\nconst b = again(v) ? v : 0\n";
    assert_eq!(
        with(text, &["a", "b"]),
        ["number | string", "number | string"]
    );
}

#[test]
fn a_predicate_of_checks_narrows_what_may_be_anything_to_the_shape() {
    // SPEC.md §6.3.
    let text = "function isNamed(data) {\n  return typeof data === \"object\" && data !== null && Object.hasOwn(data, \"name\") && typeof data.name === \"string\"\n}\nfunction f() {\n  if (!isNamed(raw)) {\n    return 0\n  }\n  const a = raw\n  const b = raw.name\n  return b.length\n}\nexport const r = f()\nconst box = { raw }\nconst c = isNamed(box.raw) ? box.raw.name : \"\"\n";
    assert_eq!(
        with(text, &["a", "b", "r", "c"]),
        ["{ name: unknown }", "string", "int32", "string"]
    );
}

#[test]
fn any_other_body_is_no_predicate() {
    let text = "function two(x) {\n  const kind = typeof x\n  return kind === \"string\"\n}\nfunction none(x) {\n  if (typeof x === \"string\") {\n    return true\n  }\n  return false\n}\nfunction nothing(x) {\n  return\n}\nconst blank = (x) => {}\nconst told = (x) => flag\nconst a = two(v) ? v : 0\nconst b = none(v) ? v : 0\nconst c = told(v) ? v : 0\nconst d = told(flag) ? v : 0\nconst o = { is: (x) => typeof x === \"string\" }\nconst e = o.is(v) ? v : 0\nnothing(v)\nblank(v)\n";
    assert_eq!(
        with(text, &["a", "b", "c", "d", "e"]),
        ["number | string | undefined"; 5]
    );
}

#[test]
fn a_predicate_passed_to_filter_tells_what_is_kept() {
    let text = "const isString = (x) => typeof x === \"string\"\nfunction isSet(x) {\n  return x !== undefined\n}\nconst xs = [v, v]\nconst a = xs.filter(isString)\nconst b = xs.filter(isSet)\nconst c = xs.filter((x) => typeof x === \"number\")\nconst d = xs.filter((x, at) => at > 0)\nconst e = xs.filter(({ length }) => length === 1)\nconst keep = [isString]\nconst g = xs.filter(keep[0])\n";
    let c = checked(&format!("{HEAD}{text}"));
    let of = |name: &str| c.of(name).to_owned();
    assert_eq!(
        ["a", "b", "c", "d"].map(of),
        [
            "readonly string[]",
            "readonly (number | string)[]",
            "readonly number[]",
            "readonly (number | string | undefined)[]"
        ]
    );
    assert_eq!(
        c.of("g"),
        c.of("d"),
        "what is no name and no arrow tells nothing"
    );
}
