//! STORIES.md US-110: the control-flow graph of each statement form, as DOT.

use sjs_bind::{body_of, graphs, running, Cfg, ENTRY, EXIT, RAISE};
use sjs_diag::SourceFile;
use sjs_syntax::parse;
use sjs_testkit::snapshot;

/// The graphs of a text: the module's, then each function's.
fn built(text: &str) -> Vec<Cfg> {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("a source");
    // A removed form is refused, and has a tree all the same.
    let (tree, _) = parse(&source);
    graphs(&source, &tree)
}

fn dot(text: &str) -> String {
    let graphs: Vec<String> = built(text).iter().map(Cfg::to_dot).collect();
    graphs.join("\n")
}

/// The node of the last graph of a text that has a label.
fn node(graph: &Cfg, label: &str) -> usize {
    let found = graph.nodes.iter().position(|node| node.label == label);
    found.unwrap_or_else(|| panic!("no node `{label}`"))
}

#[test]
fn statements_in_order() {
    snapshot!(
        "cfg-sequence",
        &dot("const a = 1\nlet b = a\nb += 1\n{\n  b = 2\n}\n;\nfunction hoisted() {}\nclass C {}\nb = \"a \\\\ b\"\nconst long = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]\nconst é = \"0123456789012345678901234567é\"\n")
    );
    snapshot!("cfg-empty", &dot(""));
}

#[test]
fn an_if_branches() {
    snapshot!("cfg-if", &dot("if (a) {\n  b()\n}\nc()\n"));
    snapshot!(
        "cfg-if_else",
        &dot("if (a) {\n  b()\n} else if (c) {\n  d()\n} else {\n  e()\n}\nf()\n")
    );
    snapshot!(
        "cfg-if_constant",
        &dot("if (false) {\n  a()\n}\nif (true) {\n  b()\n} else {\n  c()\n}\nd()\n")
    );
}

#[test]
fn a_while_loops() {
    snapshot!(
        "cfg-while",
        &dot("while (a) {\n  if (b) {\n    continue\n  }\n  if (c) {\n    break\n  }\n  d()\n}\ne()\n")
    );
    snapshot!(
        "cfg-while_true",
        &dot("while (true) {\n  if (a) {\n    break\n  }\n}\nb()\nwhile (true) {\n  c()\n}\nd()\n")
    );
    snapshot!("cfg-while_false", &dot("while (false) {\n  a()\n}\nb()\n"));
}

#[test]
fn a_for_loops() {
    snapshot!(
        "cfg-for",
        &dot("for (let i = 0; i < n; i += 1) {\n  if (a) {\n    continue\n  }\n  b()\n}\nc()\n")
    );
    snapshot!("cfg-for_ever", &dot("for (;;) {\n  a()\n}\nb()\n"));
    snapshot!(
        "cfg-for_flow",
        &dot("for (let i = a ?? 0; b && c; i += d ? 1 : 2) {\n  e()\n}\n")
    );
    snapshot!(
        "cfg-for_of",
        &dot("for (const x of xs) {\n  if (x) {\n    break\n  }\n  a(x)\n}\nfor (const k in o) {\n  b(k)\n}\nfor await (const y of p ?? q) {\n  continue\n}\nc()\n")
    );
}

#[test]
fn a_switch_tests_its_cases_in_order() {
    snapshot!(
        "cfg-switch",
        &dot("switch (x) {\n  case 1:\n  case 2: {\n    a()\n    break\n  }\n  case 3: {\n    b()\n  }\n  default: {\n    c()\n  }\n  case 4: {\n    d()\n  }\n}\ne()\n")
    );
    snapshot!(
        "cfg-switch_signed",
        &dot("switch (x) {\n  case -1: {\n    a()\n  }\n  case 2n: {\n    b()\n  }\n}\n")
    );
    snapshot!(
        "cfg-switch_no_default",
        &dot("switch (a?.b) {\n  case 1: {\n    break\n  }\n}\nswitch (x) {}\ne()\n")
    );
}

#[test]
fn a_try_runs_its_finally_on_every_exit() {
    snapshot!(
        "cfg-try_catch",
        &dot("try {\n  a()\n  b()\n} catch (e) {\n  c(e)\n}\nd()\n")
    );
    snapshot!(
        "cfg-try_finally",
        &dot("function f() {\n  try {\n    return a()\n  } finally {\n    b()\n  }\n  c()\n}\n")
    );
    snapshot!(
        "cfg-try_catch_finally",
        &dot("function f() {\n  while (a) {\n    try {\n      if (b) {\n        break\n      }\n      if (c) {\n        continue\n      }\n      if (d) {\n        return 1\n      }\n      throw e\n    } catch {\n      g()\n    } finally {\n      h()\n    }\n  }\n  return 2\n}\n")
    );
    snapshot!(
        "cfg-try_nested",
        &dot("try {\n  try {\n    a()\n  } finally {\n    b()\n  }\n  try {\n    c()\n  } catch {\n    throw d\n  }\n} catch {\n  e()\n}\ntry {} catch {\n  f()\n}\n")
    );
    snapshot!(
        "cfg-try_finally_jumps",
        &dot("function f() {\n  try {\n    a()\n  } finally {\n    return 1\n  }\n}\nfunction g() {\n  try {\n    a()\n  } catch {\n    b()\n  } finally {\n    throw c\n  }\n}\n")
    );
}

#[test]
fn a_jump_leaves() {
    snapshot!(
        "cfg-throw_return",
        &dot("function f(a) {\n  if (a) {\n    throw a ?? b\n  }\n  if (b) {\n    return\n  }\n  return a ? 1 : 2\n}\n")
    );
    snapshot!(
        "cfg-labelled",
        &dot("outer: for (const a of x) {\n  for (const b of a) {\n    if (b) {\n      continue outer\n    }\n    switch (b) {\n      case 1: {\n        continue\n      }\n      default: {\n        break outer\n      }\n    }\n  }\n  c()\n}\nd()\n")
    );
    snapshot!("cfg-unbound", &dot("break\na()\ncontinue\nbreak nowhere\n"));
}

#[test]
fn an_expression_that_branches_has_a_node_for_each_part() {
    snapshot!(
        "cfg-logical",
        &dot("const a = b && c\nconst d = e || f\nconst g = h ?? i\nif (a && !(d || g)) {\n  j()\n}\nk((a && d) || g)\n")
    );
    snapshot!(
        "cfg-negation",
        &dot("if (!a) {\n  b()\n}\nif (-a) {\n  c()\n}\n")
    );
    snapshot!(
        "cfg-conditional",
        &dot("const a = b ? c : d\ne(a ? 1 : 2, (x) => (x ? 1 : 2))\n")
    );
    snapshot!(
        "cfg-optional_chain",
        &dot("const a = b?.c.d\ne.f?.(g)?.h\ni?.[j]\n")
    );
    snapshot!(
        "cfg-await_yield",
        &dot("async function f(p) {\n  const a = await p\n  return await a\n}\nfunction* g(xs) {\n  const a = yield 1\n  yield* xs\n}\n")
    );
}

#[test]
fn every_function_has_its_graph() {
    snapshot!(
        "cfg-functions",
        &dot("const f = (a) => a && b\nconst g = () => {\n  return 1\n}\nconst h = () => 2\nconst i = function () {\n  return 3\n}\nclass C {\n  static {\n    C.x = 1\n  }\n  constructor() {\n    this.y = 2\n  }\n  get z() {\n    return 4\n  }\n  m() {}\n}\nconst o = {\n  p: a ?? 5,\n  q() {\n    return b ?? 6\n  },\n}\n")
    );
    snapshot!(
        "cfg-removed",
        &dot("do {\n  a()\n} while (b)\nwith (a) {\n  b()\n}\ndebugger\n")
    );
}

#[test]
fn reachability_and_dominance() {
    let graphs =
        built("a()\nif (b) {\n  c()\n} else {\n  return\n}\nd()\nwhile (true) {\n  e()\n}\nf()\n");
    let graph = &graphs[0];
    let reached = graph.reachable();
    let of = |label: &str| node(graph, label);
    assert!(reached[ENTRY] && reached[EXIT] && reached[of("e()")]);
    assert!(!reached[RAISE] && !reached[of("f()")]);
    // Every path to `d()` goes through `a()` and `c()`; not every path to the exit.
    assert!(graph.dominates(of("a()"), of("d()")));
    assert!(graph.dominates(of("c()"), of("d()")));
    assert!(graph.dominates(of("d()"), of("d()")));
    assert!(!graph.dominates(of("c()"), EXIT));
    assert!(!graph.dominates(of("d()"), of("c()")));
    assert!(graph.dominates(of("a()"), EXIT));
    // Nothing dominates what is not reached, itself included.
    assert!(!graph.dominates(of("a()"), of("f()")));
    assert!(!graph.dominates(of("f()"), of("f()")));
}

#[test]
fn a_blocker_is_left_only_by_what_it_throws() {
    let graphs = built("try {\n  a()\n  b()\n} catch {\n  c()\n}\nd()\n");
    let graph = &graphs[0];
    let of = |label: &str| node(graph, label);
    let reached = graph.reachable_avoiding(&[of("a()")]);
    assert!(reached[of("a()")] && reached[of("c()")] && reached[of("d()")]);
    assert!(!reached[of("b()")]);
    let reached = graph.reachable_avoiding(&[of("a()"), of("c()")]);
    assert!(!reached[of("d()")] && !reached[EXIT]);
}

#[test]
fn the_node_of_a_text_is_the_smallest_that_holds_it() {
    let text = "const a = b ? c : d\ntry {\n  e()\n} finally {\n  f()\n}\n";
    let graphs = built(text);
    let graph = &graphs[0];
    let at = |part: &str| {
        let from = text.find(part).expect("the part");
        (from, from + part.len())
    };
    let labels = |range: (usize, usize)| -> Vec<&str> {
        let nodes = graph.nodes_holding(range);
        nodes
            .iter()
            .map(|node| graph.nodes[*node].label.as_str())
            .collect()
    };
    assert_eq!(labels(at("b")), ["b"]);
    assert_eq!(labels(at("c : d")), ["const a = b ? c : d"]);
    // A `finally` is built once for each way in: when the `try` ends, and when it throws.
    assert_eq!(labels(at("f()")), ["f()", "f()"]);
    assert_eq!(labels((0, text.len())), [""; 0]);
}

#[test]
fn what_runs_with_a_node_leaves_out_the_functions_written_in_it() {
    let source = SourceFile::new(
        "a.js",
        b"a(() => b(), { c: d(), e() {} })\nclass F {}\n".to_vec(),
    )
    .expect("a source");
    let (tree, _) = parse(&source);
    let texts = |nodes: Vec<sjs_syntax::NodeRef<'_>>| -> Vec<String> {
        let texts = nodes.iter().map(|node| source.slice(&node.span()));
        texts
            .map(|text| text.unwrap_or_default().to_owned())
            .collect()
    };
    let mut ran = texts(running(tree.root()));
    ran.sort();
    assert_eq!(
        ran,
        [
            "a(() => b(), { c: d(), e() {} })",
            "a(() => b(), { c: d(), e() {} })",
            "a(() => b(), { c: d(), e() {} })\nclass F {}\n",
            "c: d()",
            "d()",
            "{ c: d(), e() {} }"
        ]
    );
    assert_eq!(
        body_of(tree.root()).map(|body| matches!(body, sjs_syntax::Element::Node(_))),
        Some(true)
    );
}
