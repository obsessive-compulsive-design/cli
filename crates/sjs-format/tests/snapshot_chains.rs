//! STORIES.md US-044: member chains, binary expressions and ternaries, each case a
//! source file and, under it, its canonical form, which formats to itself.

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
    sjs_testkit::snapshot!(&format!("chains-{name}"), &text);
}

#[test]
fn chains() {
    let long = "a".repeat(45);
    family(
        "chains",
        &[
            "a.b().c()",
            "a.b().c().d()",
            "f().a().b()",
            "a[0].b().c()",
            "a[0].b().c().d()",
            "a?.b()?.c().d()",
            "new A().b().c().d()",
            "(await a.b()).c().d()",
            "(await a.b().c().d()).e().f()",
            "a.b.c.d",
            "this.x.y().z().w()",
            "a.b().c().d().e",
            "a.b().c()[0].d()",
            "a.b(x).c(y, z).d({ k: 1 })",
            "const x = a.b((x) => x.c().d().e()).f()",
            "a.b()`t`.c().d()",
            &format!("const x = {long}.{long}().c()"),
            "const y = a\n  .b()\n  .c()\n  .d()",
        ],
    );
}

#[test]
fn binaries() {
    let long = "a".repeat(30);
    family(
        "binaries",
        &[
            "const x = a + b + c",
            &format!("const x = {long} + {long} + {long} + {long}"),
            &format!("const x = {long} * {long} + {long} * {long}"),
            &format!("const x = {long} ?? {long} ?? {long} ?? {long}"),
            &format!("const x = {long} && {long} || {long} && {long}"),
            &format!("if ({long} === {long} && {long} !== {long}) {{}}"),
            &format!("const x = ({long} + {long}) * ({long} - {long})"),
            &format!("const x = {long} ** {long} ** {long} ** {long}"),
            "const x = a.b().c().d() + e.f().g().h()",
        ],
    );
}

#[test]
fn ternaries() {
    let long = "a".repeat(30);
    family(
        "ternaries",
        &[
            "const x = a ? b : c",
            &format!("const x = {long} ? {long} : {long} + {long}"),
            &format!("const x = {long} ? {long} : {long} ? {long} : {long}"),
            &format!("const x = {long} ? {long} ? {long} : {long} : {long}"),
            "f(a ? b : c, d)",
            "const x = a.b().c().d() ? e : f",
        ],
    );
}
