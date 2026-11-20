//! STORIES.md US-069: where `this` may be written.

use sjs_bind::bind;
use sjs_diag::SourceFile;
use sjs_syntax::parse;

const OUTSIDE: &str = "`this` is used outside a class: only a method, an accessor, a field's value and a `static` block have one, and the arrows in them (SPEC.md §4.4)";
const DEFAULT: &str =
    "`this` is used in a parameter's default, which may not refer to it (SPEC.md §4.4)";
const EARLY: &str = "`this` is used before `super()` has been called (SPEC.md §4.5)";

/// The findings on a `this` of `text`: `CODE @line:column`, with the message checked
/// against the one of its kind.
fn found(text: &str) -> Vec<String> {
    let source = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    let (tree, _) = parse(&source);
    let (_, diagnostics) = bind(&source, &tree);
    diagnostics
        .iter()
        .filter(|d| source.slice(&d.span) == Some("this"))
        .map(|d| {
            let at = d.span.start;
            assert_eq!(d.path, "a.js");
            let kind = [OUTSIDE, DEFAULT, EARLY]
                .iter()
                .position(|m| d.message == *m);
            let kind = ["outside", "default", "early"][kind.expect("a known message")];
            format!("{} @{}:{} {kind}", d.code.as_str(), at.line, at.column)
        })
        .collect()
}

const NONE: [&str; 0] = [];

#[test]
fn this_outside_a_class_is_e126() {
    assert_eq!(
        found(concat!(
            "this\nconst a = this.x\nfunction f() {\n  return this\n}\nconst g = () => this\n",
            "const h = () => {\n  const i = () => this.y\n}\nconst o = { p: this, q: [this] }\n",
            "if (this) {\n  f(this, `${this}`)\n}\nconst e = function () {\n  this\n}\n",
            "const m = { n() {\n  this\n} }\nclass K extends this.B {}\n",
        )),
        [
            "E126 @1:1 outside",
            "E126 @2:11 outside",
            "E126 @4:10 outside",
            "E126 @6:17 outside",
            "E126 @8:19 outside",
            "E126 @10:16 outside",
            "E126 @10:26 outside",
            "E126 @11:5 outside",
            "E126 @12:5 outside",
            "E126 @12:14 outside",
            "E126 @15:3 outside",
            "E126 @18:3 outside",
            "E126 @20:17 outside",
        ],
        "at module level, in a function, in arrows however deep, in any expression, in what a class extends"
    );
}

#[test]
fn this_in_a_class_member_and_its_arrows_is_fine() {
    assert_eq!(
        found(concat!(
            "class K {\n  x = this.y\n  static s = this\n  #p = () => this\n  static {\n    this.t = 1\n  }\n",
            "  constructor() {\n    this.y = 1\n  }\n  m() {\n    const f = () => () => this\n    return [this, f, { v: this }]\n  }\n",
            "  get g() {\n    return this.x\n  }\n  set g(v) {\n    this.x = v\n  }\n  static n() {\n    return this\n  }\n",
            "  async *[Symbol.asyncIterator]() {\n    yield this\n  }\n}\n",
            "const o = { this: 1 }\no.this\no?.this\nclass L {\n  this() {}\n}\nconst { this: t } = o\n",
        )),
        NONE,
        "a field's value, a static block, the constructor, a method, an accessor, an arrow in any of them; a key named `this` is no `this`"
    );
}

#[test]
fn a_function_or_a_class_inside_a_method_has_its_own() {
    assert_eq!(
        found(concat!(
            "class K {\n  m() {\n    function inner() {\n      return this\n    }\n    const e = function () {\n      const a = () => this\n    }\n",
            "    class N {\n      n() {\n        return this\n      }\n      x = this\n    }\n    return [inner, e, N, this]\n  }\n",
            "  o() {\n    return { p() {\n      return this\n    } }\n  }\n}\n",
        )),
        ["E126 @4:14 outside", "E126 @7:23 outside", "E126 @19:14 outside"],
        "a `function` in a method has none, nor the arrows in it; a class in a method has its own again"
    );
}

#[test]
fn a_parameters_default_may_not_use_this() {
    assert_eq!(
        found(concat!(
            "class K {\n  m(a = this, { b = this.x }, [c = () => this]) {\n    const f = (d = this) => d\n    return [a, b, c, f]\n  }\n",
            "  set s(v = this) {}\n  constructor(e = this) {}\n}\nfunction g(h = this) {}\nconst i = (j = this) => j\n",
        )),
        [
            "E126 @2:9 default",
            "E126 @2:21 default",
            "E126 @2:42 default",
            "E126 @3:20 default",
            "E126 @6:13 default",
            "E126 @7:19 default",
            "E126 @9:16 default",
            "E126 @10:16 default",
        ],
        "of a method, a pattern, an arrow in a default, an arrow in a method, an accessor, a constructor; outside a class it is the same code"
    );
}

#[test]
fn this_before_super_in_a_derived_constructor_is_e000() {
    assert_eq!(
        found(concat!(
            "class A extends B {\n  constructor(a) {\n    this.a = a\n    const f = () => this\n    super(this.b)\n    this.c = f\n  }\n}\n",
            "class C extends B {\n  constructor() {\n    this.x = 1\n  }\n}\n",
            "class D extends B {\n  constructor() {\n    if (D) {\n      super()\n    }\n    this.x = 1\n    super()\n    this.y = 2\n  }\n",
            "  m() {\n    this.z = 3\n  }\n  x = this\n}\n",
            "class E {\n  constructor() {\n    this.x = 1\n  }\n}\n",
            "class F extends B {\n  constructor() {\n    const g = () => {\n      super()\n    }\n    this.x = g\n  }\n}\n",
            "class G extends B {\n  constructor() {\n    class H extends B {\n      constructor() {\n        super()\n        this.h = 1\n      }\n    }\n    this.g = H\n  }\n}\n",
            "class I extends B {\n  static constructor() {\n    this.x = 1\n  }\n}\n",
            "class J extends B {\n  constructor() {\n    B()\n    this.x = super.y\n    super()\n  }\n}\n",
        )),
        [
            "E000 @3:5 early",
            "E000 @4:21 early",
            "E000 @5:11 early",
            "E000 @11:5 early",
            "E000 @49:5 early",
            "E000 @60:5 early",
        ],
        "before the first `super()` as the text reads, its arguments included; all of a constructor that calls none; not a method, a field, a class without `extends`, a constructor whose `super()` is a nested class's"
    );
}
