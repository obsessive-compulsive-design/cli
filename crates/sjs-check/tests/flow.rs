//! STORIES.md US-110: what the flow of control shows (E117, E108, E109, E114, and
//! E000 for a `this` a path reaches before `super()`).

use sjs_check::check_flow;
use sjs_diag::SourceFile;
use sjs_syntax::parse;

const NONE: [&str; 0] = [];

/// `CODE @line:column` of each finding, in the order given.
fn found(text: &str) -> Vec<String> {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("a source");
    let (tree, _) = parse(&source);
    let found = check_flow(&source, &tree);
    assert!(found.iter().all(|found| found.path == "a.js"));
    found
        .iter()
        .map(|d| {
            format!(
                "{:?} @{}:{}",
                d.code, d.span.start.line, d.span.start.column
            )
        })
        .collect()
}

#[test]
fn code_after_a_jump_is_never_reached() {
    assert_eq!(
        found("function f(a) {\n  return a\n  f(1)\n}\n"),
        ["E117 @3:3"]
    );
    assert_eq!(
        found("function f(a) {\n  throw a\n  f(1)\n}\n"),
        ["E117 @3:3"]
    );
    assert_eq!(
        found("while (a) {\n  if (b) {\n    break\n    c()\n  }\n  continue\n  d()\n}\ne()\n"),
        ["E117 @4:5", "E117 @7:3"]
    );
    assert_eq!(
        found("function f(a) {\n  if (a) {\n    return 1\n  }\n  return 2\n}\nf(1)\n"),
        NONE
    );
}

#[test]
fn code_after_a_loop_that_cannot_exit_is_never_reached() {
    assert_eq!(found("while (true) {\n  f()\n}\ng()\n"), ["E117 @4:1"]);
    assert_eq!(
        found("while (true) {\n  if (f()) {\n    break\n  }\n}\ng()\n"),
        NONE
    );
    // The `break` is the `switch`'s: the loop goes on.
    assert_eq!(
        found("while (true) {\n  switch (x) {\n    case 1: {\n      break\n    }\n    default: {\n      f()\n    }\n  }\n}\ng()\n"),
        ["E117 @11:1"]
    );
    assert_eq!(
        found("function f(x) {\n  while (true) {\n    switch (x) {\n      case 1: {\n        return 1\n      }\n      default: {\n        x += 1\n      }\n    }\n  }\n}\n"),
        NONE
    );
}

#[test]
fn a_branch_whose_condition_is_written_false_is_never_reached() {
    assert_eq!(
        found("if (false) {\n  f()\n  g()\n}\nh()\n"),
        ["E117 @1:12"]
    );
    assert_eq!(
        found("if (true) {\n  f()\n} else {\n  g()\n}\nwhile (false) {\n  h()\n}\n"),
        ["E117 @3:8", "E117 @6:15"]
    );
    assert_eq!(
        found("if (true) {\n  f()\n} else if (a) {\n  g()\n}\n"),
        ["E117 @3:8"]
    );
    assert_eq!(found("if (a && true) {\n  f()\n} else {\n  g()\n}\n"), NONE);
    assert_eq!(
        found("if (!true) {\n  f()\n}\nif ((false)) {\n  g()\n}\nif (a && false) {\n  h()\n}\nif (true || a) {\n  i()\n} else {\n  j()\n}\n"),
        ["E117 @1:12", "E117 @4:14", "E117 @7:17", "E117 @12:8"]
    );
}

#[test]
fn a_stretch_of_unreachable_code_is_one_finding() {
    assert_eq!(
        found("function f() {\n  return 1\n  f()\n  if (a) {\n    f()\n  }\n  while (true) {\n    f()\n  }\n}\n"),
        ["E117 @3:3"]
    );
    // Each branch is a stretch of its own.
    assert_eq!(
        found("function f(a) {\n  if (false) {\n    a()\n  } else {\n    return 1\n    a()\n  }\n  a()\n}\n"),
        ["E117 @2:14", "E117 @6:5", "E117 @8:3"]
    );
    assert_eq!(
        found("function f(a) {\n  switch (a) {\n    case 1: {\n      return 1\n      f()\n    }\n    default: {\n      throw a\n      f()\n    }\n  }\n}\n"),
        ["E117 @5:7", "E117 @9:7"]
    );
}

#[test]
fn a_hoisted_function_declaration_is_reached_where_it_is_called() {
    assert_eq!(
        found("function f() {\n  return g()\n  function g() {\n    return 1\n  }\n}\n"),
        NONE
    );
    assert_eq!(
        found("function f() {\n  return g()\n  function g() {\n    return 1\n    f()\n  }\n  f()\n  f()\n}\n"),
        ["E117 @5:5", "E117 @7:3"],
        "what is dead in a function is its own finding"
    );
    assert_eq!(
        found("function f() {\n  return 1\n  class C {}\n}\n"),
        ["E117 @3:3"],
        "a class is declared where it stands"
    );
}

#[test]
fn a_try_is_left_through_its_finally() {
    assert_eq!(
        found("function f() {\n  try {\n    return 1\n  } finally {\n    g()\n  }\n  h()\n}\n"),
        ["E117 @7:3"]
    );
    assert_eq!(
        found(
            "function f() {\n  try {\n    return g()\n  } catch {\n    h()\n  }\n  return 2\n}\n"
        ),
        NONE,
        "what the `try` evaluates may throw"
    );
    assert_eq!(
        found("try {} catch {\n  f()\n}\ntry {\n  f()\n} catch {\n  throw g\n  h()\n} finally {\n  i()\n}\n"),
        ["E117 @1:14", "E117 @8:3"]
    );
    assert_eq!(
        found("function f() {\n  while (a) {\n    try {\n      break\n    } finally {\n      g()\n      continue\n      h()\n    }\n    i()\n  }\n  return 1\n}\n"),
        ["E117 @8:7", "E117 @10:5"]
    );
}

#[test]
fn a_finally_may_throw_and_may_not_return() {
    assert_eq!(
        found("function f(a) {\n  try {\n    a()\n  } finally {\n    throw a\n  }\n}\n"),
        NONE
    );
    assert_eq!(
        found("function f(a) {\n  try {\n    return a()\n  } catch {\n    return 1\n  } finally {\n    if (a) {\n      return 2\n    }\n  }\n}\n"),
        ["E109 @8:7"]
    );
    assert_eq!(
        found("function f(a) {\n  try {\n    a()\n  } finally {\n    return 1\n    a()\n  }\n  a()\n}\n"),
        ["E109 @5:5", "E117 @6:5", "E117 @8:3"]
    );
    // The `return` of a function written in a `finally` is that function's.
    assert_eq!(
        found("function f(a) {\n  try {\n    a()\n  } finally {\n    const g = () => {\n      return 1\n    }\n    const h = function () {\n      return 2\n    }\n    const o = {\n      m() {\n        return 3\n      },\n      p: 4,\n    }\n    a(g, h, o, () => 5)\n  }\n}\n"),
        NONE
    );
}

#[test]
fn a_case_block_ends_abruptly_or_is_last() {
    assert_eq!(
        found("function f(x) {\n  switch (x) {\n    case 1: {\n      x = 2\n    }\n    case 2:\n    case 3: {\n      break\n    }\n    case 4: {\n      return 1\n    }\n    case 5: {\n      throw x\n    }\n    case 6: {}\n    default: {\n      x = 3\n    }\n  }\n  while (x) {\n    switch (x) {\n      case 1: {\n        continue\n      }\n      default: {}\n    }\n  }\n  return x\n}\n"),
        ["E108 @3:5", "E108 @16:5"]
    );
    // Both branches return: the end is not reached. One branch: it is.
    assert_eq!(
        found("function f(x) {\n  switch (x) {\n    case 1: {\n      if (x) {\n        return 1\n      } else {\n        return 2\n      }\n    }\n    case 2: {\n      if (x) {\n        return 1\n      }\n    }\n    default: {\n      return 3\n    }\n  }\n}\n"),
        ["E108 @10:5"]
    );
    // A `finally` is built for each way into it, and judged once.
    assert_eq!(
        found("try {\n  f()\n} finally {\n  switch (x) {\n    case 1: {\n      g()\n    }\n    default: {\n      h()\n    }\n  }\n}\n"),
        ["E108 @5:5"]
    );
    assert_eq!(
        found("function f(x) {\n  return 1\n  switch (x) {\n    case 1: {\n      g()\n    }\n    default: {\n      h()\n    }\n  }\n}\n"),
        ["E117 @3:3"],
        "what is not reached does not fall through"
    );
}

#[test]
fn a_field_without_an_initializer_is_assigned_on_every_constructor_path() {
    assert_eq!(
        found("class A {\n  a\n  b\n  c\n  d\n  #e\n  static s\n  f = 1\n  constructor(v) {\n    this.a = v\n    if (v) {\n      this.b = 1\n    }\n    this.c += 1\n    other.d = 1\n    this.#e = 2\n    v = this.d\n    this.d.x = 1\n    this[\"d\"] = 1\n  }\n  m() {\n    this.d = 1\n  }\n}\n"),
        ["E114 @3:3", "E114 @4:3", "E114 @5:3"],
        "in a branch, by `+=`, in a method"
    );
    assert_eq!(
        found("class A {\n  static;\n  static s\n  t = 1\n}\n"),
        ["E114 @2:3"],
        "a field named `static` is no static field"
    );
    assert_eq!(
        found("class A {\n  a\n  m() {\n    this.a = 1\n  }\n}\n"),
        ["E114 @2:3"],
        "no constructor"
    );
    assert_eq!(
        found("class A {\n  a\n  b = undefined\n  constructor() {\n    this.a = 1\n  }\n}\nclass B extends A {\n  c\n  constructor() {\n    super()\n    this.c = 2\n  }\n}\n"),
        NONE
    );
    assert_eq!(
        found("class A {\n  a\n  b\n  c\n  d\n  e\n  constructor(v) {\n    if (v) {\n      this.a = 1\n      this.b = 1\n    } else {\n      this.a = 2\n    }\n    for (const x of v) {\n      this.c = x\n    }\n    this.d = v ? 1 : 2\n    v && (this.e = 1)\n  }\n}\n"),
        ["E114 @3:3", "E114 @4:3", "E114 @6:3"],
        "in one branch, in a loop, behind `&&`"
    );
    assert_eq!(
        found("class A {\n  a\n  b\n  c\n  d\n  constructor(v) {\n    if (v === 0) {\n      throw v\n    }\n    this.a = 1\n    try {\n      this.b = v()\n    } catch {\n      this.b = 0\n    }\n    try {\n      this.c = v()\n    } catch {\n      v()\n    }\n    const f = () => {\n      this.d = 1\n    }\n    if (v === 1) {\n      return\n    }\n    this.d = f\n  }\n}\n"),
        ["E114 @4:3", "E114 @5:3"],
        "a path that throws assigns nothing and ends nowhere; one that returns early does"
    );
    assert_eq!(
        found("class A {\n  [Symbol.iterator]\n  constructor() {\n    while (true) {\n      f()\n    }\n  }\n}\nconst B = class {\n  x\n}\n"),
        ["E114 @10:3"],
        "a constructor that does not end leaves nothing unassigned"
    );
}

#[test]
fn a_derived_constructor_calls_super_on_every_path_to_this() {
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    if (a) {\n      super(1)\n    }\n    this.p = 1\n  }\n}\n"),
        ["E000 @6:5"]
    );
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    if (a) {\n      super(1)\n      this.p = 1\n    } else {\n      super(2)\n      this.p = 2\n    }\n    this.p = 3\n    return this\n  }\n}\n"),
        NONE
    );
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    for (const x of a) {\n      super(x)\n    }\n    a(this)\n    a.this = 1\n    a?.this\n    a({ this: 1 })\n    a(() => this)\n  }\n}\n"),
        ["E000 @6:7"],
        "a property named `this` is no `this`, and an arrow runs later"
    );
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    if (a) {\n      super(1)\n    } else {\n      super.m(1)\n    }\n    this.p = 1\n  }\n}\n"),
        ["E000 @8:5"],
        "`super.m()` is no call of `super`"
    );
    // Where one node evaluates both, the order of the text is the order they run in.
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    if (a) {\n      super(1)\n    } else if (a === 0) {\n      super(this.q)\n    } else {\n      a(super(2), this.q)\n    }\n  }\n}\n"),
        ["E000 @6:13"]
    );
    // The text before the first `super()` is the binder's, and so is a constructor
    // that calls none; a class that extends nothing has `this` from the start.
    assert_eq!(
        found("class B extends A {\n  constructor(a) {\n    this.p = 1\n    super(1)\n  }\n}\nclass C extends A {\n  constructor(a) {\n    this.p = 1\n  }\n  m(a) {\n    if (a) {\n      super.m(1)\n    }\n    return this\n  }\n  static constructor(a) {\n    if (a) {\n      super.m(1)\n    }\n    return this\n  }\n}\nclass D {\n  constructor(a) {\n    if (a) {\n      a(1)\n    }\n    this.p = 1\n  }\n}\n"),
        NONE
    );
}
