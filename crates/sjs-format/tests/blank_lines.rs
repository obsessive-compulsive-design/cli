//! STORIES.md US-042: the blank-line matrix, 0 to 3 blank lines in each position the
//! rules name, and the story's edges.

mod common;

use common::module;

fn blanks(count: usize) -> String {
    "\n".repeat(count)
}

#[test]
fn between_top_level_items_there_is_exactly_one() {
    for count in 0..=3 {
        let source = format!("const a = 1\n{}const b = 2\n", blanks(count));
        assert_eq!(
            module(&source),
            "const a = 1\n\nconst b = 2\n",
            "{count} blank lines"
        );
        let imports = format!(
            "import {{ a }} from \"./a.js\"\n{}import {{ b }} from \"./b.js\"\n{}f()\n",
            blanks(count),
            blanks(count)
        );
        assert_eq!(
            module(&imports),
            "import { a } from \"./a.js\"\nimport { b } from \"./b.js\"\n\nf()\n",
            "{count} blank lines: none between imports, one after them"
        );
    }
}

#[test]
fn after_an_opening_brace_there_is_none() {
    for count in 0..=3 {
        let source = format!("function f() {{\n{}  a()\n}}\n", blanks(count));
        assert_eq!(
            module(&source),
            "function f() {\n  a()\n}\n",
            "{count} blank lines"
        );
        let class = format!("class A {{\n{}  a = 1\n}}\n", blanks(count));
        assert_eq!(
            module(&class),
            "class A {\n  a = 1\n}\n",
            "{count} blank lines"
        );
    }
}

#[test]
fn before_a_closing_brace_there_is_none() {
    for count in 0..=3 {
        let source = format!("function f() {{\n  a()\n{}}}\n", blanks(count));
        assert_eq!(
            module(&source),
            "function f() {\n  a()\n}\n",
            "{count} blank lines"
        );
        let switch = format!(
            "switch (a) {{\n  case 1: {{\n    f()\n{}  }}\n{}}}\n",
            blanks(count),
            blanks(count)
        );
        assert_eq!(
            module(&switch),
            "switch (a) {\n  case 1: {\n    f()\n  }\n}\n",
            "{count} blank lines"
        );
    }
}

#[test]
fn between_statements_there_is_at_most_one() {
    for count in 0..=3 {
        let source = format!("function f() {{\n  a()\n{}  b()\n}}\n", blanks(count));
        let expected = if count == 0 {
            "function f() {\n  a()\n  b()\n}\n"
        } else {
            "function f() {\n  a()\n\n  b()\n}\n"
        };
        assert_eq!(module(&source), expected, "{count} blank lines");
    }
}

#[test]
fn a_function_declaration_gets_its_blank_line_above_its_comment() {
    for count in 0..=3 {
        let source = format!(
            "function f() {{\n  a()\n{}  // g\n  function g() {{}}\n}}\n",
            blanks(count)
        );
        assert_eq!(
            module(&source),
            "function f() {\n  a()\n\n  // g\n  function g() {}\n}\n",
            "{count} blank lines"
        );
    }
}

#[test]
fn a_file_that_is_only_comments() {
    assert_eq!(module("// a\n// b\n"), "// a\n// b\n");
    assert_eq!(module("\n\n// a\n\n\n// b\n\n\n"), "// a\n\n// b\n");
    assert_eq!(module("/* a */"), "/* a */\n");
}

#[test]
fn no_trailing_newline_plus_a_trailing_comment() {
    assert_eq!(module("f() // x"), "f()  // x\n");
    assert_eq!(module("f() /* x */"), "f()  /* x */\n");
    assert_eq!(module("f()\n// x"), "f()\n// x\n");
}

#[test]
fn an_empty_clause_block() {
    assert_eq!(
        module("switch (a) {\n  case 1: {\n  }\n  default: {\n\n  }\n}\n"),
        "switch (a) {\n  case 1: {}\n  default: {}\n}\n"
    );
    assert_eq!(module("try {\n} catch {\n}\n"), "try {} catch {}\n");
}

#[test]
fn blank_lines_inside_a_multi_line_template_are_untouched() {
    let source = "const t = `a\n\n\n\n  b   \n`\nf()\n";
    assert_eq!(module(source), "const t = `a\n\n\n\n  b   \n`\n\nf()\n");
    let nested = "function f() {\n  return `\n\n\n  x`\n}\n";
    assert_eq!(module(nested), nested);
}

#[test]
fn the_output_always_ends_in_exactly_one_newline() {
    for source in ["a()", "a()\n", "a()\n\n\n", "// c", "{}\n\n", "class A {}"] {
        let out = module(source);
        assert!(
            out.ends_with('\n') && !out.ends_with("\n\n"),
            "{source:?} → {out:?}"
        );
    }
    assert_eq!(module(""), "", "an empty file stays empty");
}
