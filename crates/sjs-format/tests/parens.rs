//! STORIES.md US-041: between every pair of operators, parentheses stay exactly where
//! dropping them would change the parse, the formatted text parses to the same tree as
//! its source (US-050's equivalence), and formatting it again changes nothing.

mod common;

use common::{formatted, stripped, without_first_pair};

/// How many `(` a text holds.
fn pairs(text: &str) -> usize {
    text.matches('(').count()
}

const OPERATORS: &[&str] = &[
    "??",
    "||",
    "&&",
    "|",
    "^",
    "&",
    "===",
    "!==",
    "<",
    ">",
    "<=",
    ">=",
    "instanceof",
    "<<",
    ">>",
    ">>>",
    "+",
    "-",
    "*",
    "/",
    "%",
    "**",
];

/// Checks one source: equivalence, minimality and idempotence. Returns whether the
/// parentheses were kept.
fn check(text: &str) -> bool {
    let original = stripped(text).unwrap_or_else(|| panic!("{text} parses clean"));
    let out = formatted(text);
    assert_eq!(
        stripped(&out).as_deref(),
        Some(original.as_str()),
        "{text} formats to {out}, which parses differently"
    );
    assert_eq!(formatted(&out), out, "{out} is not stable");
    let without = without_first_pair(text).unwrap_or_else(|| panic!("{text} has a pair"));
    let needed = stripped(&without).as_deref() != Some(original.as_str());
    let kept = pairs(&out) == pairs(text);
    assert_eq!(
        kept, needed,
        "{text} formats to {out}; without the pair: {without}"
    );
    kept
}

#[test]
fn every_pair_of_operators_keeps_parentheses_exactly_where_the_parse_needs_them() {
    let mut kept = 0;
    let mut dropped = 0;
    for outer in OPERATORS {
        for inner in OPERATORS {
            for text in [
                format!("x = (a {inner} b) {outer} c"),
                format!("x = a {outer} (b {inner} c)"),
            ] {
                if check(&text) {
                    kept += 1;
                } else {
                    dropped += 1;
                }
            }
        }
    }
    let cases = OPERATORS.len() * OPERATORS.len() * 2;
    assert_eq!(kept + dropped, cases);
    assert!(
        kept > cases / 3 && dropped > cases / 3,
        "{kept} kept, {dropped} dropped"
    );
}

#[test]
fn the_edges_of_the_story_keep_or_drop_as_it_says() {
    for (text, kept) in [
        ("x = (a ?? b) || c", true),
        ("x = (a || b) ?? c", true),
        ("x = ((a))", false),
        ("x = (-1) ** 2", true),
        ("x = (await p).x", true),
        ("x = (() => 1)()", true),
        ("x = typeof (a + b)", true),
        ("x = -(-a)", true),
        ("x = (a + b) * c", true),
        ("x = (a * b) + c", false),
        ("x = (a ** b) ** c", true),
        ("x = a ** (b ** c)", false),
        ("x = (a = 1) + 2", true),
        ("x = a + (b = 1)", true),
        ("x = (a ? b : c) ? d : e", true),
        ("x = a ? (b ? c : d) : e", false),
        ("x = (a?.b).c", true),
        ("x = (a.b).c", false),
        ("x = (new A()).b", false),
        ("x = (a + b).c", true),
        ("x = !(a + b)", true),
        ("x = !(a)", false),
        ("(a)++", false),
        ("x = f((a))", false),
        ("x = f((b + c))", false),
        ("x = [(a)]", false),
        ("x = { a: (b) }", false),
        ("x = () => (a)", false),
        ("x = () => ({})", true),
        ("x = () => ({}).a", true),
    ] {
        assert_eq!(check(text), kept, "{text} → {}", formatted(text));
    }
    assert_eq!(formatted("x = ((a))"), "x = a");
    assert_eq!(formatted("x = -(-a)"), "x = -(-a)");
    assert_eq!(formatted("x = (a * b) + c"), "x = a * b + c");
}
