//! STORIES.md US-116: `int32` refinement (SPEC.md §3.5, §4.1).

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

/// An array, its length, a number that is no `int32`, and a function whose
/// parameter is an `int32`.
const HEAD: &str =
    "const xs = [1.5, 2.5].slice(0)\nconst size = xs.length\nconst real = Math.random()\n";

fn types(body: &str, names: &[&str]) -> Vec<String> {
    let c = checked(&format!("{HEAD}{body}"));
    assert_eq!(c.found, [""; 0], "{body}");
    names.iter().map(|name| c.of(name).to_owned()).collect()
}

#[test]
fn an_int32_is_what_is_known_to_be_one() {
    assert_eq!(
        types(
            "let a = 5\nlet b = 2147483647\nlet c = 2147483648\nlet d = -2147483648\nlet e = 1.5\nconst g = xs.indexOf(1.5)\nconst h = xs.findIndex((x) => x > 1)\nlet i = 0\ni = size\ni = g\n",
            &["a", "b", "c", "d", "e", "size", "g", "h", "i"]
        ),
        ["int32", "int32", "number", "int32", "number", "int32", "int32", "int32", "int32"],
        "a literal in range, a length, a place found; a `let` given only those stays one"
    );
    assert_eq!(
        types(
            "const a = size & 1\nconst b = size | 2\nconst c = size ^ 3\nconst d = ~size\nconst e = size << 2\nconst g = size >> 1\nconst h = size % 2\nconst i = 1 << 31\nconst j = size >>> 1\n",
            &["a", "b", "c", "d", "e", "g", "h", "i", "j"]
        ),
        ["int32", "int32", "int32", "int32", "int32", "int32", "int32", "int32", "number"]
    );
    // Arithmetic may leave the range.
    assert_eq!(
        types(
            "const a = size * 2\nconst b = size + 1\nconst c = size - 1\nconst d = -size\nconst e = size / 2\nlet m = 0\nm += 1\nlet n = 0\nn++\n",
            &["a", "b", "c", "d", "e", "m", "n"]
        ),
        ["number"; 7]
    );
}

#[test]
fn a_whole_number_of_what_is_in_range_is_an_int32() {
    assert_eq!(
        types(
            "const a = Math.trunc(size)\nconst b = Math.floor(size / 2)\nconst c = Math.ceil(1.5)\nconst d = Math.round(-1.5)\nconst e = Math.trunc((size) / 4 / -1)\nconst g = Math.floor((size / 1))\nconst flag = real < 0.5\nconst h = Math.round(flag ? 1.5 : size)\n",
            &["a", "b", "c", "d", "e", "g", "h"]
        ),
        ["int32"; 7]
    );
    assert_eq!(
        types(
            "const a = Math.trunc(real)\nconst b = Math.floor(real * 10)\nconst c = Math.ceil(3000000000)\nconst d = Math.round(size / 0.5)\nconst e = Math.trunc(size / size)\nconst g = Math.floor(real / 2)\nconst h = Math.round(size * 2)\nconst i = Math.trunc(size / real)\nconst j = Math.sqrt(size)\nconst k = Math.trunc(-2147483648.5)\nconst m = Math.floor(size - 1)\n",
            &["a", "b", "c", "d", "e", "g", "h", "i", "j", "k", "m"]
        ),
        ["number"; 11],
        "what may be any number, or leave the range: divided by less than one, by what may be none, multiplied"
    );
    let c = checked(&format!(
        "{HEAD}const a = Math.trunc()\nconst b = Math.floor(later())\n"
    ));
    assert_eq!([c.of("a"), c.of("b")], ["number", "number"]);
}

#[test]
fn the_variable_of_a_loop_that_counts_up_to_an_int32_is_one() {
    let text = "function f(n) {\n  let total = 0\n  for (let a = 0; a < xs.length; a += 1) {\n    total += xs[a]\n  }\n  for (let b = 0; b < n; b++) {\n    total += b\n  }\n  for (let c = 1; c < size; ++c) {\n    total += c\n  }\n  return total\n}\nexport const r = f(size)\n";
    assert_eq!(types(text, &["a", "b", "c"]), ["int32"; 3]);
    // Not when the bound is no `int32`, the test no `<`, the step no one, the
    // start no `int32`, or the body adds to it; not outside the loop.
    let text = "function f(n) {\n  let total = 0\n  for (let a = 0; a < real; a += 1) {\n    total += a\n  }\n  for (let b = 0; b <= n; b += 1) {\n    total += b\n  }\n  for (let c = 0; c < n; c += 2) {\n    total += c\n  }\n  for (let d = 0.5; d < n; d += 1) {\n    total += d\n  }\n  for (let e = 0; e < n; e += 1) {\n    e += 1\n  }\n  for (let g = 0; g < n; g--) {\n    total += g\n  }\n  for (let h = 0; n > h; h += 1) {\n    total += h\n  }\n  let i = 0\n  for (let j = 0; i < n; i += 1) {\n    total += j\n  }\n  let m = 0\n  while (m < n) {\n    m += 1\n  }\n  for (let o = 0; o < later(); o += 1) {\n    total += o\n  }\n  return total\n}\nexport const r = f(size)\n";
    let c = checked(&format!("{HEAD}{text}"));
    assert_eq!(
        ["a", "b", "c", "d", "e", "g", "h", "i", "m", "o"].map(|name| c.of(name).to_owned()),
        ["number"; 10]
    );
}
