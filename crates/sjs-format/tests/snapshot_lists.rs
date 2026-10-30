//! STORIES.md US-043: each kind of list at widths 99, 100 and 101, every source laid out
//! at exactly 100 columns when flat, so that 99 breaks it and 100 and 101 do not; then
//! the story's edges at the canonical width.

mod common;

use common::{module, module_at};

/// The width of `text`'s first line when nothing breaks.
fn flat_width(text: &str) -> usize {
    module_at(text, usize::MAX)
        .lines()
        .next()
        .map_or(0, |line| line.chars().count())
}

/// `head`, a name of `x`s, then `tail`: exactly 100 columns when flat.
fn exactly_100(head: &str, tail: &str) -> String {
    let padding = 100 - head.chars().count() - tail.chars().count();
    format!("{head}{}{tail}", "x".repeat(padding))
}

fn widths(name: &str, cases: &[(&str, &str)]) {
    let mut text = String::new();
    for (head, tail) in cases {
        let input = exactly_100(head, tail);
        assert_eq!(flat_width(&input), 100, "{input} is not 100 columns flat");
        text.push_str("--- source\n");
        text.push_str(&input);
        text.push('\n');
        for width in [99, 100, 101] {
            text.push_str("--- at ");
            text.push_str(&width.to_string());
            text.push('\n');
            text.push_str(&module_at(&input, width));
        }
        text.push('\n');
    }
    sjs_testkit::snapshot!(&format!("lists-{name}"), &text);
}

#[test]
fn arrays() {
    widths(
        "arrays",
        &[(
            "const array = [first, second, third, fourth, fifth, sixth, seventh, eighth, ninth, tenth, ",
            "]",
        )],
    );
}

#[test]
fn objects() {
    widths(
        "objects",
        &[(
            "const object = { first: 1, second: 2, third: 3, fourth: 4, fifth: 5, sixth: 6, seventh: ",
            " }",
        )],
    );
}

#[test]
fn arguments() {
    widths(
        "arguments",
        &[
            (
                "call(first, second, third, fourth, fifth, sixth, seventh, eighth, ninth, tenth, eleventh, ",
                ")",
            ),
            (
                "const instance = new Thing(first, second, third, fourth, fifth, sixth, seventh, eighth, ",
                ")",
            ),
        ],
    );
}

#[test]
fn parameters() {
    widths(
        "parameters",
        &[
            (
                "function f(first, second, third, fourth, fifth, sixth, seventh, eighth, ninth, ...",
                ") {}",
            ),
            (
                "const f = (first, second, third = 3, fourth, fifth, sixth, seventh, eighth, ninth, ",
                ") => f",
            ),
        ],
    );
}

#[test]
fn patterns() {
    widths(
        "patterns",
        &[
            (
                "const [first, second, third, fourth, fifth, sixth, seventh, eighth, ninth, tenth, ",
                "] = x",
            ),
            (
                "const { first, second: alias, third = 3, fourth, fifth, sixth, seventh, ...",
                " } = x",
            ),
        ],
    );
}

#[test]
fn import_lists() {
    widths(
        "imports",
        &[
            (
                "import { a, b as alias, c, d, e, f, g, h, i, j, k, l, m, n, o, p, ",
                " } from \"./a.js\"",
            ),
            (
                "export { first, second as alias, third, fourth, fifth, sixth, seventh, eighth, ninth, ",
                " }",
            ),
        ],
    );
}

fn edges(name: &str, inputs: &[&str]) {
    let mut text = String::new();
    for input in inputs {
        text.push_str("--- source\n");
        text.push_str(input);
        if !input.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("--- formatted\n");
        let out = module(input);
        assert_eq!(
            module(&out),
            out,
            "formatting {name} twice changes it:\n{out}"
        );
        text.push_str(&out);
        text.push('\n');
    }
    sjs_testkit::snapshot!(&format!("lists-{name}"), &text);
}

#[test]
fn the_edges_of_the_story() {
    let long = "a".repeat(120);
    edges(
        "edges",
        &[
            &format!("const only = [{long}]"),
            &format!("f([{long}], b)"),
            "const kept = {\n  a: 1, b: 2 }",
            "const kept = { // why\n  a: 1 }",
            "const kept = { /* why */ a: 1 }",
            "f({\n  a: 1 }, b)",
            "const empty = {\n}",
            "const same = { a: 1,\n  b: 2 }",
            "function f(a, ...rest) {}",
            "const f = () => { return a }",
            "const g = () => { return { a } }",
            "const h = () => {\n  f()\n  return a\n}",
        ],
    );
}
