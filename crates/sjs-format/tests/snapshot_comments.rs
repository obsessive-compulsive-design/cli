//! STORIES.md US-046: comments in every syntactic position, each case a source file and,
//! under it, its canonical form, which formats to itself.

mod common;

use common::module;

fn family(name: &str, inputs: &[&str]) {
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
    sjs_testkit::snapshot!(&format!("comments-{name}"), &text);
}

#[test]
fn statements() {
    family(
        "statements",
        &[
            "//x\nf()",
            "f() //x",
            "f() /* x */",
            "a()\n// between\nb()",
            "a() // first\n// above b\nb() // second",
            "function f() {\n  a()\n  // end\n}",
            "f() // no final newline",
            "/* a */ /* b */ f()",
            "if (a) /* c */ {\n  b()\n} // tail\nelse {\n  c()\n}",
            "switch (a) {\n  case 1:\n  // between labels\n  case 2: {\n    f()\n  }\n}",
            "class A {\n  // first\n  a = 1 // trailing\n  /** m */\n  m() {}\n}",
            "f(); // after a semicolon",
            "//////\n//***\n// ok\nf()",
            "for (let i = 0; /* c */ i < n; i++) {}",
        ],
    );
}

#[test]
fn expressions() {
    family(
        "expressions",
        &[
            "f(a, // x\n  b)",
            "f(\n  // about a\n  a,\n  b)",
            "f(/* a */ a, b /* b */, /* c */ c)",
            "f(a /* last */)",
            "f(a, // last\n)",
            "f(\n  a,\n  // dangling\n)",
            "f( // none\n)",
            "const o = [\n  /* dangling */\n]",
            "const o = {\n  a: 1, // a\n  // b\n  b: 2,\n}",
            "const o = { a /* k */: 1, b: /* v */ 2 }",
            "const { a /* a */, b } = o",
            "function f(a /* a */, b = /* d */ 1) {}",
            "x = a + /* c */ b",
            "x = a + // c\n  b",
            "x = a\n  // c\n  ? b : c",
            "x = a.b() // c\n  .c()\n  .d()",
            "x = /* c */ -a",
            "x = (a) /* c */ => a",
            "x = /* c */ (a)",
            "x = (a\n  // own\n) + 1",
            "x = f(/** doc */ a)",
            "new A(a, // a\n  b)",
            "x = a /* long\n  block */ + b",
            "import { a /* a */, b } from \"./a.js\"",
            "const a = [1\n  // own before the comma\n  , 2]",
        ],
    );
}

#[test]
fn doc_comments() {
    let long = "word ".repeat(30);
    family(
        "docs",
        &[
            "/** Short. */\nexport function f() {}",
            "/**Short.*/\nexport function f() {}",
            &format!("/** {long}*/\nexport function f() {{}}"),
            &format!("/**\n * {long}\n *\n * Second paragraph.\n */\nexport function f() {{}}"),
            "/**\n * One\n * two.\n *\n *\n * @param a  the first\n * @returns it\n */\nexport function f(a) {}",
            "/**\n * @example\n * ```js\n * const x = f()\n *\n *   expect(x).toBe(1)\n * ```\n * - item one\n * - item two\n * # Heading\n * | a | b |\n * > quote\n * 1. first\n *   indented\n */\nexport function f() {}",
            "/**\n *\n * Doc.\n *\n */\nexport function f() {}",
            "/**\n Doc without stars.\n */\nexport function f() {}",
            "/** Summary on the first line.\n * More.\n * **Bold** stays. */\nexport function g() {}",
            &format!("/**\n * {0}ab\n */\nexport function f() {{}}\nclass A {{\n  /**\n   * {0}ab\n   */\n  m() {{}}\n}}", "word ".repeat(19)),
            "/***/\nf()",
            "f()\n/**\n * The end.\n */",
            &format!("/**\n * {}\n */\nexport function f() {{}}", "x".repeat(120)),
        ],
    );
}
