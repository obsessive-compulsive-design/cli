//! Tests for the generator: rendering of every construct, production coverage, determinism,
//! grammatical validity of generated text, and shrinking.

use std::collections::BTreeSet;

use super::generate::{Options, Production, ProgramGen, RESERVED};
use super::tree::{
    Decl, Else, Expr, ForUpdate, Import, Item, Literal, Member, MemberKey, Param, Pattern, Program,
    Property, Stmt, TemplatePart,
};
use crate::prng::Rng;
use crate::prop::{check_with, Config, Gen};

fn id(name: &str) -> Expr {
    Expr::Ident(name.to_owned())
}

fn num(text: &str) -> Expr {
    Expr::Literal(Literal::Number(text.to_owned()))
}

// A test helper that exists to be short; the tree really does box its children.
#[allow(clippy::unnecessary_box_returns)]
fn b(e: Expr) -> Box<Expr> {
    Box::new(e)
}

/// One tree that uses every construct of the renderer once; its text is the snapshot a
/// reviewer reads, and the mutation gate's witness for every rendering branch.
#[allow(clippy::too_many_lines)]
fn everything() -> Program {
    let pattern = Pattern::Object(
        vec![
            ("alpha".to_owned(), None, None),
            (
                "beta".to_owned(),
                Some(Box::new(Pattern::Array(
                    vec![(Pattern::Ident("gamma".to_owned()), Some(num("1")))],
                    Some("rest".to_owned()),
                ))),
                None,
            ),
            ("delta".to_owned(), None, Some(id("item"))),
        ],
        Some("others".to_owned()),
    );
    let items = vec![
        Item::Import(Import::Named(
            vec![
                ("alpha".to_owned(), None),
                ("beta".to_owned(), Some("gamma".to_owned())),
            ],
            "./util.js".to_owned(),
        )),
        Item::Import(Import::Namespace(
            "shapes".to_owned(),
            "./shapes/circle.js".to_owned(),
        )),
        Item::Import(Import::Json(
            "config".to_owned(),
            "./config.json".to_owned(),
        )),
        Item::Export(Decl::Const(
            pattern,
            Expr::Object(vec![
                Property::Shorthand("alpha".to_owned()),
                Property::Named("beta".to_owned(), num("2")),
                Property::StringKey(
                    "a \"key\"".to_owned(),
                    Expr::Literal(Literal::Str("line\nbreak\t\\".to_owned())),
                ),
                Property::Computed(
                    Expr::Literal(Literal::Str("k".to_owned())),
                    Expr::Literal(Literal::Bool(true)),
                ),
                Property::Spread(id("delta")),
            ]),
        )),
        Item::ExportList(
            vec![
                ("alpha".to_owned(), None),
                ("beta".to_owned(), Some("gamma".to_owned())),
            ],
            None,
        ),
        Item::ExportList(
            vec![("delta".to_owned(), None)],
            Some("../lib/parse.js".to_owned()),
        ),
        Item::Decl(Decl::Let(
            Pattern::Ident("count".to_owned()),
            Expr::Literal(Literal::Undefined),
        )),
        Item::Decl(Decl::Function {
            is_async: true,
            is_generator: true,
            name: "walk".to_owned(),
            params: vec![
                Param::Plain(Pattern::Ident("left".to_owned()), None),
                Param::Plain(
                    Pattern::Ident("right".to_owned()),
                    Some(Expr::Literal(Literal::Bool(false))),
                ),
                Param::Rest("more".to_owned()),
            ],
            body: vec![
                Stmt::Expr(Expr::Yield(None, false)),
                Stmt::Expr(Expr::Yield(Some(b(id("left"))), false)),
                Stmt::Expr(Expr::Yield(
                    Some(b(Expr::Unary("await", b(id("right"))))),
                    true,
                )),
                Stmt::Return(Some(Expr::Ternary(
                    b(Expr::Binary(b(id("left")), "??", b(id("right")))),
                    b(num("1")),
                    b(num("2")),
                ))),
            ],
        }),
        Item::Decl(Decl::Class {
            name: "Widget".to_owned(),
            extends: Some("shapes.Shape".to_owned()),
            members: vec![
                Member::Constructor(
                    vec![Param::Plain(Pattern::Ident("width".to_owned()), None)],
                    vec![
                        Stmt::Expr(Expr::SuperCall(vec![id("width")])),
                        Stmt::Expr(Expr::Assign(
                            b(Expr::Member(b(Expr::This), "width".to_owned(), false)),
                            "=",
                            b(id("width")),
                        )),
                    ],
                ),
                Member::Field {
                    is_static: true,
                    key: MemberKey::Ident("total".to_owned()),
                    value: Some(num("0")),
                },
                Member::Field {
                    is_static: false,
                    key: MemberKey::Private("secret".to_owned()),
                    value: None,
                },
                Member::Method {
                    is_static: true,
                    is_async: true,
                    is_generator: true,
                    key: MemberKey::Symbol("asyncIterator"),
                    params: vec![],
                    body: vec![Stmt::Expr(Expr::Call(
                        b(Expr::SuperMember("render".to_owned())),
                        vec![],
                        false,
                    ))],
                },
                Member::Method {
                    is_static: false,
                    is_async: false,
                    is_generator: false,
                    key: MemberKey::Private("hidden".to_owned()),
                    params: vec![],
                    body: vec![],
                },
                Member::Getter(
                    false,
                    MemberKey::Ident("area".to_owned()),
                    vec![Stmt::Return(Some(Expr::Member(
                        b(Expr::This),
                        "width".to_owned(),
                        false,
                    )))],
                ),
                Member::Setter(
                    true,
                    MemberKey::Ident("area".to_owned()),
                    Param::Plain(Pattern::Ident("value".to_owned()), None),
                    vec![],
                ),
                Member::StaticBlock(vec![Stmt::Expr(Expr::Assign(
                    b(Expr::Member(b(id("Widget")), "total".to_owned(), false)),
                    "+=",
                    b(num("1")),
                ))]),
            ],
        }),
        Item::Stmt(Stmt::Block(vec![Stmt::Decl(Decl::Const(
            Pattern::Ident("inner".to_owned()),
            num("3"),
        ))])),
        Item::Stmt(Stmt::If(
            Expr::Binary(b(id("alpha")), "===", b(id("null"))),
            vec![Stmt::Throw(Expr::New(
                "Error".to_owned(),
                vec![Expr::Literal(Literal::Str("empty".to_owned()))],
            ))],
            Some(Box::new(Else::If(
                Expr::Binary(b(id("alpha")), "!==", b(num("0"))),
                vec![Stmt::Expr(Expr::Call(b(id("alpha")), vec![], true))],
                Some(Box::new(Else::Block(vec![Stmt::Expr(Expr::Call(
                    b(Expr::Member(b(id("console")), "log".to_owned(), false)),
                    vec![id("alpha")],
                    false,
                ))]))),
            ))),
        )),
        Item::Stmt(Stmt::ForOf {
            is_await: true,
            is_let: true,
            pattern: Pattern::Array(
                vec![
                    (Pattern::Ident("key".to_owned()), None),
                    (Pattern::Ident("value".to_owned()), None),
                ],
                None,
            ),
            iterable: Expr::Call(
                b(Expr::Member(b(id("Object")), "entries".to_owned(), false)),
                vec![id("shape")],
                false,
            ),
            body: vec![Stmt::Continue],
        }),
        Item::Stmt(Stmt::For {
            var: "index".to_owned(),
            init: num("0"),
            test: Expr::Binary(
                b(id("index")),
                "<",
                b(Expr::Member(b(id("items")), "length".to_owned(), false)),
            ),
            update: ForUpdate::AddAssign(num("1")),
            body: vec![Stmt::Break],
        }),
        Item::Stmt(Stmt::For {
            var: "i".to_owned(),
            init: num("9"),
            test: Expr::Binary(b(id("i")), ">", b(num("0"))),
            update: ForUpdate::SubAssign(num("1")),
            body: vec![],
        }),
        Item::Stmt(Stmt::For {
            var: "j".to_owned(),
            init: num("0"),
            test: Expr::Binary(b(id("j")), "<", b(num("2"))),
            update: ForUpdate::PostInc,
            body: vec![],
        }),
        Item::Stmt(Stmt::For {
            var: "k".to_owned(),
            init: num("2"),
            test: Expr::Binary(b(id("k")), ">", b(num("0"))),
            update: ForUpdate::PostDec,
            body: vec![],
        }),
        Item::Stmt(Stmt::For {
            var: "m".to_owned(),
            init: num("0"),
            test: Expr::Binary(b(id("m")), "<=", b(num("2"))),
            update: ForUpdate::PreInc,
            body: vec![],
        }),
        Item::Stmt(Stmt::For {
            var: "n".to_owned(),
            init: num("2"),
            test: Expr::Binary(b(id("n")), ">=", b(num("0"))),
            update: ForUpdate::PreDec,
            body: vec![],
        }),
        Item::Stmt(Stmt::While(
            Expr::Unary("!", b(id("done"))),
            vec![Stmt::Expr(Expr::Assign(
                b(id("done")),
                "??=",
                b(Expr::Literal(Literal::Bool(true))),
            ))],
        )),
        Item::Stmt(Stmt::Switch(
            id("token"),
            vec![
                (
                    vec![Literal::Str("a".to_owned()), Literal::Str("b".to_owned())],
                    vec![Stmt::Break],
                ),
                (
                    vec![Literal::BigInt("1n".to_owned())],
                    vec![Stmt::Return(None)],
                ),
            ],
            Some(vec![Stmt::Expr(Expr::Call(
                b(id("fallback")),
                vec![],
                false,
            ))]),
        )),
        Item::Stmt(Stmt::Try(
            vec![Stmt::Expr(Expr::Call(b(id("risky")), vec![], false))],
            Some((Some("error".to_owned()), vec![Stmt::Throw(id("error"))])),
            Some(vec![Stmt::Expr(Expr::Call(
                b(id("cleanup")),
                vec![],
                false,
            ))]),
        )),
        Item::Stmt(Stmt::Try(vec![], Some((None, vec![])), None)),
        Item::Stmt(Stmt::Try(vec![], None, Some(vec![]))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("total")),
            "**=",
            b(Expr::Binary(
                b(Expr::Unary("-", b(num("2")))),
                "**",
                b(Expr::Binary(b(num("3")), "**", b(num("2")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("total")),
            "*=",
            b(Expr::Binary(
                b(Expr::Binary(b(num("1")), "+", b(num("2")))),
                "*",
                b(Expr::Binary(b(num("3")), "-", b(num("4")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("total")),
            "/=",
            b(Expr::Binary(
                b(Expr::Binary(b(num("8")), "/", b(num("2")))),
                "%",
                b(num("3")),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("total")),
            "-=",
            b(Expr::Binary(
                b(Expr::Binary(b(id("a")), "<<", b(num("1")))),
                ">>>",
                b(Expr::Binary(b(id("b")), ">>", b(num("2")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("total")),
            "%=",
            b(Expr::Binary(
                b(Expr::Binary(b(id("a")), "|", b(id("b")))),
                "&",
                b(Expr::Binary(b(id("c")), "^", b(id("d")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("flag")),
            "=",
            b(Expr::Binary(
                b(Expr::Binary(b(id("a")), "||", b(id("b")))),
                "??",
                b(Expr::Binary(
                    b(id("c")),
                    "&&",
                    b(Expr::Binary(b(id("d")), "??", b(id("e")))),
                )),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("flag")),
            "=",
            b(Expr::Binary(
                b(Expr::Binary(b(id("a")), "instanceof", b(id("Widget")))),
                "&&",
                b(Expr::Binary(
                    b(Expr::Unary("typeof", b(id("b")))),
                    "===",
                    b(Expr::Literal(Literal::Str("string".to_owned()))),
                )),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("bits")),
            "=",
            b(Expr::Unary(
                "~",
                b(Expr::Unary("+", b(Expr::Paren(b(num("5")))))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("list")),
            "=",
            b(Expr::Array(
                vec![
                    num("1"),
                    id("rest"),
                    Expr::Arrow {
                        is_async: false,
                        params: vec![],
                        body: Ok(b(num("0"))),
                    },
                ],
                vec![false, true, false],
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("empty")),
            "=",
            b(Expr::Object(vec![])),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("make")),
            "=",
            b(Expr::Arrow {
                is_async: true,
                params: vec![Param::Plain(Pattern::Ident("x".to_owned()), None)],
                body: Ok(b(Expr::Object(vec![Property::Shorthand("x".to_owned())]))),
            }),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("run")),
            "=",
            b(Expr::Arrow {
                is_async: false,
                params: vec![],
                body: Err(vec![Stmt::Return(Some(Expr::Call(
                    b(Expr::Arrow {
                        is_async: false,
                        params: vec![],
                        body: Ok(b(num("1"))),
                    }),
                    vec![],
                    false,
                )))]),
            }),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("text")),
            "=",
            b(Expr::Template(vec![
                TemplatePart::Text("n=".to_owned()),
                TemplatePart::Substitution(Expr::Binary(b(id("a")), "+", b(num("1")))),
                TemplatePart::Text(" done".to_owned()),
            ])),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("raw")),
            "=",
            b(Expr::StringRaw(vec![TemplatePart::Text("\\d+".to_owned())])),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("re")),
            "=",
            b(Expr::Call(
                b(Expr::Member(
                    b(Expr::Regex("a+b".to_owned(), "gv".to_owned())),
                    "test".to_owned(),
                    false,
                )),
                vec![id("text")],
                false,
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("shape")),
            "=",
            b(Expr::New(
                "shapes.Circle".to_owned(),
                vec![num("1"), num("2")],
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("lazy")),
            "=",
            b(Expr::DynamicImport("./util.js".to_owned())),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("deep")),
            "=",
            b(Expr::Index(
                b(Expr::Index(
                    b(Expr::Member(b(id("a")), "b".to_owned(), true)),
                    b(num("0")),
                    true,
                )),
                b(id("key")),
                false,
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("called")),
            "=",
            b(Expr::Call(
                b(Expr::Member(
                    b(Expr::Paren(b(Expr::Unary("await", b(id("promise")))))),
                    "then".to_owned(),
                    false,
                )),
                vec![id("alpha")],
                true,
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("neg")),
            "=",
            b(Expr::Binary(
                b(Expr::Unary("-", b(id("a")))),
                "**",
                b(Expr::Unary("-", b(id("b")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("cond")),
            "=",
            b(Expr::Ternary(
                b(Expr::Assign(b(id("x")), "=", b(num("1")))),
                b(Expr::Ternary(b(id("y")), b(num("2")), b(num("3")))),
                b(Expr::Arrow {
                    is_async: false,
                    params: vec![],
                    body: Ok(b(num("4"))),
                }),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Assign(
            b(id("sum")),
            "=",
            b(Expr::Binary(
                b(Expr::Binary(b(num("1")), "-", b(num("2")))),
                "-",
                b(Expr::Binary(b(num("3")), "-", b(num("4")))),
            )),
        ))),
        Item::Stmt(Stmt::Expr(Expr::Call(
            b(Expr::Member(
                b(Expr::Member(b(id("a")), "b".to_owned(), false)),
                "c".to_owned(),
                false,
            )),
            vec![Expr::Assign(b(id("z")), "+=", b(num("1")))],
            false,
        ))),
    ];
    Program {
        items,
        comments: Vec::new(),
    }
}

#[test]
fn every_construct_renders_as_the_snapshot_shows() {
    crate::snapshot!("program-everything", &everything().to_string());
}

#[test]
#[allow(clippy::too_many_lines)]
fn precedence_parenthesises_exactly_where_needed() {
    let render = |e: Expr| {
        Program {
            items: vec![Item::Stmt(Stmt::Expr(Expr::Assign(b(id("r")), "=", b(e))))],
            comments: Vec::new(),
        }
        .to_string()
    };
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Binary(b(num("1")), "+", b(num("2")))),
            "*",
            b(num("3"))
        )),
        "r = (1 + 2) * 3\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(num("1")),
            "+",
            b(Expr::Binary(b(num("2")), "*", b(num("3"))))
        )),
        "r = 1 + 2 * 3\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(num("1")),
            "-",
            b(Expr::Binary(b(num("2")), "-", b(num("3"))))
        )),
        "r = 1 - (2 - 3)\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Binary(b(num("2")), "**", b(num("3")))),
            "**",
            b(num("2"))
        )),
        "r = (2 ** 3) ** 2\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(num("2")),
            "**",
            b(Expr::Binary(b(num("3")), "**", b(num("2"))))
        )),
        "r = 2 ** 3 ** 2\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Unary("-", b(num("2")))),
            "**",
            b(num("2"))
        )),
        "r = (-2) ** 2\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(num("2")),
            "**",
            b(Expr::Unary("-", b(num("2"))))
        )),
        "r = 2 ** -2\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Binary(b(id("a")), "||", b(id("b")))),
            "??",
            b(id("c"))
        )),
        "r = (a || b) ?? c\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(id("a")),
            "??",
            b(Expr::Binary(b(id("b")), "&&", b(id("c"))))
        )),
        "r = a ?? (b && c)\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Binary(b(id("a")), "??", b(id("b")))),
            "||",
            b(id("c"))
        )),
        "r = (a ?? b) || c\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(id("a")),
            "||",
            b(Expr::Binary(b(id("b")), "??", b(id("c"))))
        )),
        "r = a || (b ?? c)\n"
    );
    assert_eq!(
        render(Expr::Binary(
            b(Expr::Binary(b(id("a")), "??", b(id("b")))),
            "??",
            b(id("c"))
        )),
        "r = a ?? b ?? c\n"
    );
    assert_eq!(
        render(Expr::Unary(
            "typeof",
            b(Expr::Binary(b(id("a")), "+", b(id("b"))))
        )),
        "r = typeof (a + b)\n"
    );
    assert_eq!(
        render(Expr::Unary("-", b(Expr::Unary("-", b(num("2")))))),
        "r = - -2\n",
        "`--2` would lex as a decrement"
    );
    assert_eq!(
        render(Expr::Unary("+", b(Expr::Unary("+", b(id("a")))))),
        "r = + +a\n"
    );
    assert_eq!(
        render(Expr::Unary("-", b(Expr::Unary("+", b(id("a")))))),
        "r = -+a\n",
        "different signs need no space"
    );
    assert_eq!(
        render(Expr::Unary("!", b(Expr::Unary("!", b(id("a")))))),
        "r = !!a\n"
    );
    assert_eq!(
        render(Expr::Member(
            b(Expr::Binary(b(id("a")), "+", b(id("b")))),
            "c".to_owned(),
            false
        )),
        "r = (a + b).c\n"
    );
    assert_eq!(
        render(Expr::Call(
            b(Expr::Arrow {
                is_async: false,
                params: vec![],
                body: Ok(b(num("1")))
            }),
            vec![],
            false
        )),
        "r = (() => 1)()\n"
    );
    assert_eq!(
        render(Expr::Ternary(
            b(Expr::Binary(b(id("a")), "??", b(id("b")))),
            b(id("c")),
            b(id("d"))
        )),
        "r = a ?? b ? c : d\n"
    );
    assert_eq!(
        render(Expr::Ternary(
            b(Expr::Ternary(b(id("a")), b(id("b")), b(id("c")))),
            b(id("d")),
            b(id("e"))
        )),
        "r = (a ? b : c) ? d : e\n"
    );
    assert_eq!(
        render(Expr::Array(
            vec![Expr::Assign(b(id("a")), "=", b(num("1")))],
            vec![false]
        )),
        "r = [a = 1]\n"
    );
    assert_eq!(
        render(Expr::New("W".to_owned(), vec![Expr::Yield(None, false)])),
        "r = new W(yield)\n"
    );
    assert_eq!(
        render(Expr::Index(
            b(id("a")),
            b(Expr::Assign(b(id("i")), "=", b(num("0")))),
            false
        )),
        "r = a[i = 0]\n"
    );
}

/// An arrow with the expression body `e`.
fn arrow(e: Expr) -> Expr {
    Expr::Arrow {
        is_async: false,
        params: Vec::new(),
        body: Ok(b(e)),
    }
}

#[test]
fn an_arrow_body_that_begins_with_a_brace_is_parenthesised() {
    let render = |e: Expr| {
        Program {
            items: vec![Item::Stmt(Stmt::Expr(Expr::Assign(b(id("r")), "=", b(e))))],
            comments: Vec::new(),
        }
        .to_string()
    };
    let object = || Expr::Object(vec![Property::Shorthand("a".to_owned())]);
    for (body, expected) in [
        (object(), "r = () => ({ a })\n"),
        (
            Expr::Ternary(b(object()), b(id("x")), b(id("y"))),
            "r = () => ({ a } ? x : y)\n",
        ),
        (
            Expr::Binary(b(object()), "===", b(id("x"))),
            "r = () => ({ a } === x)\n",
        ),
        (
            Expr::Assign(b(object()), "=", b(id("o"))),
            "r = () => ({ a } = o)\n",
        ),
        (
            Expr::Member(b(object()), "a".to_owned(), false),
            "r = () => ({ a }.a)\n",
        ),
        (
            Expr::Index(b(object()), b(num("0")), false),
            "r = () => ({ a }[0])\n",
        ),
        (
            Expr::Call(b(object()), Vec::new(), false),
            "r = () => ({ a }())\n",
        ),
        // An object that is not the first token needs nothing.
        (
            Expr::Ternary(b(id("x")), b(object()), b(id("y"))),
            "r = () => x ? { a } : y\n",
        ),
        (Expr::Paren(b(object())), "r = () => ({ a })\n"),
        (id("x"), "r = () => x\n"),
    ] {
        assert_eq!(render(arrow(body)), expected);
    }
}

#[test]
fn strings_escape_what_the_subset_needs() {
    let render = |text: &str| {
        Program {
            comments: Vec::new(),
            items: vec![Item::Stmt(Stmt::Throw(Expr::Literal(Literal::Str(
                text.to_owned(),
            ))))],
        }
        .to_string()
    };
    assert_eq!(render("plain"), "throw \"plain\"\n");
    assert_eq!(render("say \"hi\""), "throw \"say \\\"hi\\\"\"\n");
    assert_eq!(render("a\\b"), "throw \"a\\\\b\"\n");
    assert_eq!(render("l\nr\rt\t"), "throw \"l\\nr\\rt\\t\"\n");
    assert_eq!(render("é 😀"), "throw \"é 😀\"\n");
}

fn generate(seed: u64, options: Options) -> (Program, BTreeSet<Production>) {
    ProgramGen { options }.generate_with_coverage(&mut Rng::from_seed(seed))
}

#[test]
fn every_production_appears_in_the_generated_corpus() {
    let mut seen = BTreeSet::new();
    let options = Options {
        max_depth: 5,
        max_items: 8,
        doubles: true,
        comments: false,
    };
    for seed in 0..600 {
        seen.extend(generate(seed, options).1);
    }
    let missing: Vec<Production> = Production::ALL
        .iter()
        .copied()
        .filter(|p| !seen.contains(p))
        .collect();
    assert_eq!(missing, Vec::<Production>::new());
    assert_eq!(Production::ALL.len(), 148);
    let listed: BTreeSet<Production> = Production::ALL.iter().copied().collect();
    assert_eq!(listed.len(), Production::ALL.len(), "no duplicates");
}

#[test]
fn generation_is_deterministic_per_seed() {
    let options = Options::default();
    assert_eq!(generate(7, options).0, generate(7, options).0);
    assert_eq!(
        generate(7, options).0.to_string(),
        generate(7, options).0.to_string()
    );
    assert_ne!(generate(7, options).0, generate(8, options).0);
}

/// Every program of the 10 000-program corpus (seeds 0..10 000, default options) has a
/// recorded FNV-1a hash, so a failure anywhere can name "program N" and anyone can
/// regenerate it. The manifest is a snapshot; regenerate it with `SJS_UPDATE_SNAPSHOTS=1`.
#[test]
fn the_corpus_manifest_matches() {
    let options = Options::default();
    let mut manifest = String::new();
    for seed in 0..10_000u64 {
        let text = generate(seed, options).0.to_string();
        let line = format!("{seed} {}\n", fnv1a(text.as_bytes()));
        manifest.push_str(&line);
    }
    crate::snapshot!("program-corpus-manifest", &manifest);
}

fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    format!("{hash:016x}")
}

/// A crude token scan: enough to prove no removed word, no `==`/`!=`, no `var`, no `++`
/// outside a `for` header, no ASI-hazard line start and no braceless body got through.
fn scan_for_removed_syntax(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut previous_ends_expression = false;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        let number = index + 1;
        if line.is_empty() {
            previous_ends_expression = false;
            continue;
        }
        if line.starts_with("import ") {
            previous_ends_expression = false;
            continue;
        }
        for word in [
            "var ",
            "with ",
            "void ",
            "delete ",
            "debugger",
            "arguments",
            "eval(",
            " in ",
            "export default",
            "do {",
        ] {
            if outside_strings(line).contains(word) {
                problems.push(format!("{number}: {word:?} in {raw:?}"));
            }
        }
        let code = outside_strings(line);
        if code.contains("==") && code.replace("===", "").replace("!==", "").contains("==") {
            problems.push(format!("{number}: loose equality in {raw:?}"));
        }
        if code.contains("!=") && code.replace("!==", "").contains("!=") {
            problems.push(format!("{number}: loose inequality in {raw:?}"));
        }
        if (code.contains("++") || code.contains("--")) && !code.starts_with("for (") {
            problems.push(format!(
                "{number}: increment outside a for header in {raw:?}"
            ));
        }
        if code.contains("||=") || code.contains("&&=") {
            problems.push(format!("{number}: logical assignment in {raw:?}"));
        }
        if code.ends_with(';') {
            problems.push(format!("{number}: semicolon in {raw:?}"));
        }
        if previous_ends_expression && line.starts_with(['(', '[', '`', '+', '-', '/']) {
            problems.push(format!("{number}: ASI hazard in {raw:?}"));
        }
        let balanced = code.matches('(').count() == code.matches(')').count();
        for head in ["if (", "while (", "for (", "for await ("] {
            if balanced && code.starts_with(head) && !code.ends_with('{') {
                problems.push(format!("{number}: braceless body in {raw:?}"));
            }
        }
        previous_ends_expression = !code.ends_with(['{', '}', ':']) && !code.starts_with("case ");
    }
    problems
}

/// The line with string, template and regex contents blanked, so scans see only code.
fn outside_strings(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == '\\' {
                    chars.next();
                } else if ch == q {
                    quote = None;
                    out.push(q);
                }
            }
            None => {
                if ch == '"' || ch == '`' {
                    quote = Some(ch);
                    out.push(ch);
                } else if ch == '/'
                    && chars.peek().is_some_and(|c| *c != '/' && *c != ' ')
                    && out.ends_with(['(', ',', '=', ' '])
                {
                    quote = Some('/');
                    out.push(ch);
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out
}

#[test]
fn generated_programs_contain_no_removed_syntax() {
    let options = Options {
        max_depth: 5,
        max_items: 8,
        doubles: true,
        comments: false,
    };
    for seed in 0..400 {
        let text = generate(seed, options).0.to_string();
        let problems = scan_for_removed_syntax(&text);
        assert!(
            problems.is_empty(),
            "seed {seed}:\n{}\n---\n{text}",
            problems.join("\n")
        );
        assert!(
            text.ends_with('\n') && !text.contains("\n\n\n"),
            "seed {seed}: layout"
        );
    }
    assert_eq!(
        scan_for_removed_syntax(
            "var a = 1\na == b\nc != d\ni++\nx ||= y\nz;\nlet q = 1\n(q).r()\nif (q) r()\n"
        )
        .len(),
        8
    );
    assert_eq!(
        scan_for_removed_syntax(
            "const s = \"var a == b\"\nfor (let i = 0; i < 2; i++) {\n  s()\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn generated_identifiers_never_collide_with_reserved_names() {
    let reserved: BTreeSet<&str> = RESERVED.iter().copied().collect();
    let options = Options {
        max_depth: 4,
        max_items: 6,
        doubles: false,
        comments: false,
    };
    let mut names_seen = BTreeSet::new();
    for seed in 0..300 {
        let text = generate(seed, options).0.to_string();
        for word in outside_strings(&text.replace('\n', " "))
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '#'))
        {
            let bare = word.trim_start_matches('#');
            if bare.is_empty() || bare.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                continue;
            }
            names_seen.insert(bare.to_owned());
        }
    }
    let generated_names: Vec<&str> = super::generate::NAMES_FOR_TESTS.to_vec();
    for name in generated_names {
        assert!(!reserved.contains(name), "{name} is reserved");
        assert!(name.chars().all(|c| c.is_ascii_lowercase()), "{name}");
    }
    let keywords_used: Vec<&String> = names_seen
        .iter()
        .filter(|n| reserved.contains(n.as_str()))
        .collect();
    let allowed: BTreeSet<&str> = ["Error", "Symbol", "String", "Object", "console", "null"]
        .into_iter()
        .collect();
    let keyword_names: Vec<&&String> = keywords_used
        .iter()
        .filter(|n| {
            !allowed.contains(n.as_str()) && n.chars().next().is_some_and(char::is_uppercase)
        })
        .collect();
    assert_eq!(
        keyword_names,
        Vec::<&&String>::new(),
        "a generated program used a global as a name"
    );
}

#[test]
fn numbers_stay_int32_unless_doubles_are_requested() {
    for seed in 0..300 {
        let text = generate(
            seed,
            Options {
                max_depth: 5,
                max_items: 8,
                doubles: false,
                comments: false,
            },
        )
        .0
        .to_string();
        for token in outside_strings(&text.replace('\n', " "))
            .split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_'))
        {
            if token.chars().next().is_some_and(|c| c.is_ascii_digit()) && !token.ends_with('n') {
                assert!(
                    !token.contains('.') && !token.contains('e'),
                    "seed {seed}: {token} is not an int32"
                );
            }
        }
    }
    let with_doubles: BTreeSet<Production> = (0..200)
        .flat_map(|seed| {
            generate(
                seed,
                Options {
                    max_depth: 5,
                    max_items: 8,
                    doubles: true,
                    comments: false,
                },
            )
            .1
        })
        .collect();
    assert!(with_doubles.contains(&Production::Double));
    let without: BTreeSet<Production> = (0..200)
        .flat_map(|seed| {
            generate(
                seed,
                Options {
                    max_depth: 5,
                    max_items: 8,
                    doubles: false,
                    comments: false,
                },
            )
            .1
        })
        .collect();
    assert!(!without.contains(&Production::Double));
}

#[test]
fn regex_literals_always_carry_v_and_never_u() {
    let mut seen_regex = false;
    for seed in 0..300 {
        let text = generate(
            seed,
            Options {
                max_depth: 5,
                max_items: 8,
                doubles: false,
                comments: false,
            },
        )
        .0
        .to_string();
        for line in text.lines() {
            if let Some(start) = line.find("= /") {
                seen_regex = true;
                let rest = &line[start + 2..];
                let end = rest[1..].find('/').expect("closing slash") + 1;
                let flags = &rest[end + 1..];
                let flags: String = flags.chars().take_while(char::is_ascii_lowercase).collect();
                assert!(
                    flags.ends_with('v') && !flags.contains('u'),
                    "seed {seed}: {line}"
                );
            }
        }
    }
    assert!(seen_regex);
}

#[test]
fn shrinking_removes_one_item_or_statement_at_a_time() {
    let gen = ProgramGen {
        options: Options::default(),
    };
    let program = Program {
        comments: Vec::new(),
        items: vec![
            Item::Stmt(Stmt::While(id("a"), vec![Stmt::Break, Stmt::Continue])),
            Item::Decl(Decl::Function {
                is_async: false,
                is_generator: false,
                name: "f".to_owned(),
                params: vec![],
                body: vec![Stmt::Return(None)],
            }),
            Item::Import(Import::Namespace("ns".to_owned(), "./util.js".to_owned())),
            Item::Stmt(Stmt::Switch(
                id("t"),
                vec![(vec![Literal::Bool(true)], vec![Stmt::Break])],
                None,
            )),
            Item::Stmt(Stmt::Try(vec![], Some((None, vec![])), None)),
        ],
    };
    let candidates = gen.shrink(&program);
    assert_eq!(
        candidates.len(),
        5 + 2 + 1 + 1,
        "five item removals, two loop-body removals, one function-body removal, one case removal; a try with only a catch cannot lose it"
    );
    assert!(candidates
        .iter()
        .all(|c| c.items.len() == 4 || c.items.len() == 5));
    let exported = Program {
        comments: Vec::new(),
        items: vec![Item::Export(Decl::Function {
            is_async: false,
            is_generator: false,
            name: "g".to_owned(),
            params: vec![],
            body: vec![Stmt::Return(None)],
        })],
    };
    assert_eq!(
        gen.shrink(&exported),
        vec![
            Program {
                items: vec![],
                comments: Vec::new()
            },
            Program {
                comments: Vec::new(),
                items: vec![Item::Export(Decl::Function {
                    is_async: false,
                    is_generator: false,
                    name: "g".to_owned(),
                    params: vec![],
                    body: vec![],
                })],
            },
        ],
        "an exported declaration stays exported when its body shrinks"
    );
    assert_eq!(
        gen.shrink(&Program {
            items: vec![],
            comments: Vec::new()
        }),
        Vec::<Program>::new()
    );
    assert!(gen.shrink(&everything()).len() > 50);
}

#[test]
fn a_failing_property_shrinks_to_a_one_item_program() {
    let gen = ProgramGen {
        options: Options {
            max_depth: 4,
            max_items: 8,
            doubles: false,
            comments: false,
        },
    };
    let failure = check_with(&Config::new(200), &gen, &|p: &Program| {
        if p.to_string().contains("while (") {
            Err("has a while".to_owned())
        } else {
            Ok(())
        }
    })
    .expect_err("some program has a while loop");
    let minimal: Program = {
        let mut rng = Rng::from_seed(failure.seed);
        let original = gen.generate(&mut rng);
        let mut current = original;
        let mut steps = 0;
        loop {
            let next = gen
                .shrink(&current)
                .into_iter()
                .find(|c| c.to_string().contains("while ("));
            steps += 1;
            assert!(steps < 10_000, "shrinking must make progress");
            match next {
                Some(next) => current = next,
                None => break current,
            }
        }
    };
    assert_eq!(minimal.items.len(), 1, "{minimal}");
    assert!(failure.minimal.contains("While"), "{}", failure.minimal);
}

#[test]
fn bound_names_cover_every_declaration_and_pattern_shape() {
    use crate::program::tree::{Decl, Expr, Literal, Pattern};
    let n = |s: &str| s.to_owned();
    assert_eq!(
        Decl::Function {
            is_async: false,
            is_generator: true,
            name: n("f"),
            params: Vec::new(),
            body: Vec::new(),
        }
        .bound_names(),
        ["f"]
    );
    assert_eq!(
        Decl::Class {
            name: n("C"),
            extends: None,
            members: Vec::new(),
        }
        .bound_names(),
        ["C"]
    );
    assert_eq!(
        Decl::Let(
            Pattern::Ident(n("a")),
            Expr::Literal(Literal::Number(n("1")))
        )
        .bound_names(),
        ["a"]
    );
    let nested = Pattern::Object(
        vec![
            (n("a"), None, None),
            (
                n("b"),
                Some(Box::new(Pattern::Array(
                    vec![
                        (Pattern::Ident(n("c")), None),
                        (
                            Pattern::Ident(n("d")),
                            Some(Expr::Literal(Literal::Number(n("2")))),
                        ),
                    ],
                    Some(n("e")),
                ))),
                None,
            ),
        ],
        Some(n("f")),
    );
    assert_eq!(
        Decl::Const(nested, Expr::Literal(Literal::Number(n("1")))).bound_names(),
        ["a", "c", "d", "e", "f"]
    );
}

/// A name is exported once per program (an ES2025 early error otherwise), whether by
/// an exported declaration or an export list, aliases included.
#[test]
fn generated_programs_never_export_a_name_twice() {
    use crate::program::tree::Item;
    let options = Options::default();
    let mut renamed = 0;
    let mut kept_private = 0;
    for seed in 0..600u64 {
        let program = generate(seed, options).0;
        let mut seen = BTreeSet::new();
        for item in &program.items {
            let names: Vec<String> = match item {
                Item::Export(decl) => decl.bound_names(),
                Item::ExportList(names, _) => names
                    .iter()
                    .map(|(name, alias)| {
                        if alias.is_some() {
                            renamed += 1;
                        }
                        alias.clone().unwrap_or_else(|| name.clone())
                    })
                    .collect(),
                Item::Decl(decl) if seen.iter().any(|s| decl.bound_names().contains(s)) => {
                    kept_private += 1;
                    Vec::new()
                }
                Item::Decl(_) | Item::Import(_) | Item::Stmt(_) => Vec::new(),
            };
            // A pattern that binds one name twice is its own early error, left to the
            // pattern story; here each item counts each name once.
            let names: BTreeSet<String> = names.into_iter().collect();
            for name in names {
                assert!(
                    seen.insert(name.clone()),
                    "seed {seed}: `{name}` exported twice\n{program}"
                );
            }
        }
    }
    assert!(renamed > 0, "some export list got a fresh alias");
    assert!(
        kept_private > 0,
        "some declaration stayed private because its name was already exported"
    );
}

/// A member a field before it would join, there being no ASI inside a class body:
/// `x = 1` then `[Symbol.iterator]() {}` is an index expression, and `x = 1` then
/// `*g() {}` a multiplication.
fn joins_a_field_before_it(member: &Member) -> bool {
    matches!(
        member,
        Member::Method {
            key: MemberKey::Symbol(_),
            ..
        } | Member::Method {
            is_static: false,
            is_async: false,
            is_generator: true,
            ..
        }
    )
}

#[test]
fn no_generated_class_member_continues_the_field_before_it() {
    let mut ordered = 0;
    for seed in 0..600u64 {
        let program = generate(seed, Options::default()).0;
        for item in &program.items {
            let (Item::Decl(Decl::Class { members, .. })
            | Item::Export(Decl::Class { members, .. })) = item
            else {
                continue;
            };
            let first_field = members
                .iter()
                .position(|member| matches!(member, Member::Field { .. }));
            let Some(first_field) = first_field else {
                continue;
            };
            if members[..first_field].iter().any(joins_a_field_before_it) {
                ordered += 1;
            }
            assert!(
                !members[first_field..].iter().any(joins_a_field_before_it),
                "seed {seed}: a member after a field would continue it\n{program}"
            );
        }
    }
    assert!(ordered > 10, "the ordering was exercised {ordered} times");
    assert!(joins_a_field_before_it(&Member::Method {
        is_static: false,
        is_async: false,
        is_generator: true,
        key: MemberKey::Ident("g".to_owned()),
        params: Vec::new(),
        body: Vec::new(),
    }));
    assert!(!joins_a_field_before_it(&Member::Method {
        is_static: true,
        is_async: false,
        is_generator: true,
        key: MemberKey::Ident("g".to_owned()),
        params: Vec::new(),
        body: Vec::new(),
    }));
}

#[test]
fn comments_are_inserted_only_when_asked_and_leave_the_code_in_place() {
    let plain = ProgramGen {
        options: Options::default(),
    };
    let with = ProgramGen {
        options: Options {
            comments: true,
            ..Options::default()
        },
    };
    for seed in 0..40 {
        let program = with.generate(&mut Rng::from_seed(seed));
        assert_eq!(
            program.items,
            plain.generate(&mut Rng::from_seed(seed)).items,
            "seed {seed}: the same program, comments apart"
        );
        assert!(!program.comments.is_empty(), "seed {seed}: some comment");
        let text = program.to_string();
        let bare = Program {
            items: program.items.clone(),
            comments: Vec::new(),
        }
        .to_string();
        let mut lines = text.lines();
        for line in bare.lines() {
            assert!(
                lines.any(|with| {
                    with == line
                        || (!line.is_empty()
                            && with.starts_with(line)
                            && with[line.len()..].starts_with(" // "))
                }),
                "seed {seed}: {line:?} is not in\n{text}"
            );
        }
    }
    let text = with.generate(&mut Rng::from_seed(7)).to_string();
    crate::snapshot!("program-with-comments", &text);
    let above: Vec<String> = (0..40)
        .flat_map(|seed| with.generate(&mut Rng::from_seed(seed)).comments)
        .filter(|comment| !comment.trailing)
        .map(|comment| comment.text)
        .collect();
    let some = |test: &dyn Fn(&str) -> bool| above.iter().any(|text| test(text));
    assert!(
        some(&|text| text.starts_with("//") && !text.starts_with("// ") && text != "//////"),
        "a `//x`"
    );
    assert!(some(&|text| text == "//////"), "a ruler");
    assert!(some(&|text| text.starts_with("/* ")), "a block comment");
    assert!(
        some(&|text| text.starts_with("// ")),
        "a `// x` above a line"
    );
    assert!(
        some(&|text| text.starts_with("/**\n") && text.contains("@param")),
        "a doc comment with a tag"
    );
    assert!(some(&|text| text.contains("@example")), "an example");
}
