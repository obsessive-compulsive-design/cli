//! STORIES.md US-042: a snapshot per family of statement forms (US-026, US-027) and
//! declarations, each case a source file and, under it, its canonical form; US-045 adds
//! the order of imports.

mod common;

use common::{module, stripped};

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
        if stripped(input).is_some() {
            assert_eq!(
                module(&out),
                out,
                "formatting {name} twice changes it:
{out}"
            );
        }
        text.push_str(&out);
        text.push('\n');
    }
    sjs_testkit::snapshot!(&format!("statements-{name}"), &text);
}

#[test]
fn blocks_and_if() {
    family(
        "blocks-and-if",
        &[
            "{\nf()\n}",
            "{}",
            "if(a){b()}",
            "if (a) { b() } else { c() }",
            "if (a) {\n  b()\n} else if (c) {\n  d()\n} else if (e) {} else {\n  g()\n}",
            "if (a) b()\nelse c()",
        ],
    );
}

#[test]
fn loops() {
    family(
        "loops",
        &[
            "while(a){b()}",
            "while (true) {}",
            "for(const x of xs){f(x)}",
            "for (let [k, v] of entries) {}",
            "for await (const chunk of stream) {\n  use(chunk)\n}",
            "for(let i=0;i<n;i++){f(i)}",
            "for (let i = n; i > 0; --i) {}",
            "for (let i = 0; i < n; i += 2) {}",
            "for (let i = 0; i < n; i -= 1) {}",
        ],
    );
}

#[test]
fn jumps() {
    family(
        "jumps",
        &[
            "function f() {\n  return\n}",
            "function f() {\n  return a + b\n}",
            "function f() {\n  return;\n}",
            "while (a) {\n  break\n}",
            "while (a) {\n  continue\n}",
            "throw new Error(\"x\")",
            "throw e;",
        ],
    );
}

#[test]
fn switch_statements() {
    family(
        "switch",
        &[
            "switch(a){case 1:{f()}}",
            "switch (a) {\n  case 1:\n  case 2: {\n    f()\n  }\n  default: {\n    g()\n  }\n}",
            "switch (a) {\n  case \"s\": {\n    break\n  }\n  case -1: {\n  }\n}",
            "switch (a) {}",
            "switch (a) {\n\n  case 1: {}\n\n\n  case 2: {}\n\n}",
        ],
    );
}

#[test]
fn try_statements() {
    family(
        "try",
        &[
            "try{f()}catch(e){g(e)}",
            "try {\n  f()\n} catch {\n  g()\n}",
            "try {\n  f()\n} finally {\n  h()\n}",
            "try {\n  f()\n} catch (e) {\n  g(e)\n} finally {\n  h()\n}",
            "try {} catch {}",
        ],
    );
}

#[test]
fn declarations() {
    family(
        "declarations",
        &[
            "const a=1",
            "let b = 2",
            "const {x,y:[z]}=o",
            "let [a, b = 2, ...rest] = c",
            "const f=(a)=>a+1",
            "const o = {\n  a: 1,\n}",
            "const t = `a\n  b\n\n  c`",
            "f()\ng();\n;\nh()",
        ],
    );
}

#[test]
fn functions_and_classes() {
    family(
        "functions-and-classes",
        &[
            "function f(a,b=1,...r){return a}",
            "async function g() {\n  await h()\n}",
            "function*gen(){yield 1}",
            "async function * agen() {}",
            "class A {}",
            "class B extends A {\n  static s = 1\n  #p\n  constructor(x) {\n    super()\n    this.x = x\n  }\n  get g() {\n    return this.x\n  }\n  set g(v) {}\n  static m() {}\n  async *gen() {}\n  #q() {}\n  static {\n    f()\n  }\n  [Symbol.iterator]() {}\n}",
            "class C extends ns.Base {}",
        ],
    );
}

#[test]
fn modules() {
    family(
        "modules",
        &[
            "import {a} from './a.js'",
            "import {a as b,c} from \"./a.js\"",
            "import * as ns from './n.js'",
            "import data from './d.json' with {type:'json'}",
            "import {a} from './a.js'\nimport {b} from './b.js'\nconst c = a + b",
            "export const k = 1",
            "export {k as j}",
            "export {k} from './k.js'",
            "export function f() {}\nexport class C {}",
        ],
    );
}

#[test]
fn import_order() {
    family(
        "imports",
        &[
            "import {a} from './a.js'\nimport {b} from './a/b.js'\nimport {c} from '../a.js'\nimport {d} from '../../d.js'",
            "import { b } from \"./a.js\"\nimport { a } from \"./a.js\"",
            "import { b as a, a as b } from \"./x.js\"",
            "import d from \"./d.json\" with { type: \"json\" }\nimport * as c from \"./c.js\"\nimport { e } from \"./e.js\"\nimport { b } from \"./b.js\"",
            "// header\nimport { c } from \"./c.js\"\n// about b\nimport { b } from \"./b.js\" // b's line\n\nimport { a } from \"./a.js\"\n\nuse(a, b, c)",
            "import {\n  zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz as z,\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa as a,\n} from \"./x.js\"",
        ],
    );
}

#[test]
fn removed_forms() {
    family(
        "removed-forms",
        &[
            "var v = 1",
            "do {} while (a)",
            "with (o) {}",
            "outer: for (const x of xs) {}",
            "debugger",
            "for (;;) {}",
            "for (const k in o) {}",
        ],
    );
}

#[test]
fn comments_and_blank_lines() {
    family(
        "comments-and-blank-lines",
        &[
            "// header\n\nimport {a} from './a.js'\n\n\n// about b\nconst b = a // trailing\n/* block */\nfunction f() {} /* after */\n",
            "function f() { // why\n  a()\n\n\n  b()\n  // end\n}",
            "class A {\n  // first\n  a = 1\n\n  // second\n  b = 2\n}",
            "f()\n// only the end\n",
            "// nothing\n// but comments\n",
            "/**\n * Doc.\n */\nexport function f() {}",
        ],
    );
}
