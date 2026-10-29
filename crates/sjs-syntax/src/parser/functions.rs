//! Function declarations and arrow functions (SPEC.md §2 `FunctionDecl`, `ArrowFn`,
//! `Block`, §4.4): `[async] function [*] name(params) { … }` at the top level of the
//! module or of a block, and `[async] (params) => expr | block` in expression position.
//! Parameters are binding patterns with defaults and a trailing rest; a name declared
//! twice across the parameters is E000. Removed forms are E000 with their replacement:
//! an arrow parameter without parentheses (fix: parenthesise it) and a `function`
//! expression (write an arrow or a declaration). `yield` outside a generator and
//! `await` inside a function that is not `async` are E000 wherever they occur.

use sjs_diag::Span;

use super::{bindings, expressions, FnContext, Parser, Scope};
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

const LPAREN: TokenKind = TokenKind::Punct(Punct::LParen);
const RPAREN: TokenKind = TokenKind::Punct(Punct::RParen);
const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);
const ARROW: TokenKind = TokenKind::Punct(Punct::Arrow);
const ELLIPSIS: TokenKind = TokenKind::Punct(Punct::Ellipsis);
const STAR: TokenKind = TokenKind::Punct(Punct::Star);
const FUNCTION: TokenKind = TokenKind::Keyword(Keyword::Function);

/// `[async] function [*] name(params) block`, at `async` or `function`. As a
/// declaration (`expression` false) the name is required; as the removed expression
/// form it is optional, and the caller has reported it. Returns the name and its span.
pub(super) fn function(parser: &mut Parser<'_>, expression: bool) -> Option<(String, Span)> {
    parser.start_node(if expression {
        NodeKind::FunctionExpr
    } else {
        NodeKind::FunctionDecl
    });
    let is_async = parser.at_word("async");
    if is_async {
        parser.bump(false);
    }
    parser.bump(false);
    let is_generator = parser.eat(&STAR);
    let name = if parser.peek(false).kind == TokenKind::Ident {
        let token = parser.bump(false).clone();
        Some((parser.text(&token).to_owned(), token.span))
    } else {
        if !expression {
            let span = parser.peek(false).span;
            parser.error(
                span,
                "a `function` declaration needs a name: an anonymous function is written as an arrow (SPEC.md §4.4)",
            );
        }
        None
    };
    let mark = parser.contexts.len();
    parser.contexts.push(FnContext::Function {
        is_async,
        is_generator,
    });
    if params(parser).is_some() {
        body(parser);
    }
    parser.contexts.truncate(mark);
    parser.finish_node();
    name
}

/// `[async] (params) => expr | block` in expression position, at `async`, `(` or the
/// lone parameter name. An expression body is an assignment expression: a `,` after it
/// belongs to the enclosing list (or is the removed comma operator).
pub(super) fn arrow(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::ArrowFunction);
    let is_async = parser.at_word("async") && parser.peek_second().kind != ARROW;
    if is_async {
        parser.bump(false);
    }
    let mark = parser.contexts.len();
    parser.contexts.push(FnContext::Arrow { is_async });
    let ok = if parser.peek(false).kind == LPAREN {
        params(parser).is_some()
    } else {
        lone_parameter(parser)
    };
    if ok && parser.peek(false).kind == ARROW {
        let arrow = parser.bump(false).span;
        if parser.peek(false).kind == LBRACE {
            body(parser);
        } else if expressions::assignment(parser).is_missing() {
            parser.error(arrow, "expected an expression or a block after `=>`");
        }
    }
    parser.contexts.truncate(mark);
    parser.finish_node();
}

/// `a =>` without parentheses: E000 with the parenthesised replacement, parsed anyway.
fn lone_parameter(parser: &mut Parser<'_>) -> bool {
    parser.start_node(NodeKind::Params);
    let token = parser.bump(false).clone();
    let name = parser.text(&token).to_owned();
    parser.error_with_fix(
        token.span,
        "arrow parameters are always parenthesised (SPEC.md §2 `ArrowFn`)",
        name.clone(),
        format!("({name})"),
    );
    parser.start_node(NodeKind::Param);
    parser.finish_node();
    parser.finish_node();
    true
}

/// Tokens the arrow scan reads ahead before giving up on an unmatched `(`.
pub(super) const ARROW_SCAN_LIMIT: usize = 65_536;

/// True when the `(` at lookahead offset `start` is closed by a `)` that `=>` follows.
/// Lookahead is lexed as division, so a regex default holding a parenthesis could
/// mislead the count until the expression grammar lands; the scan stops at the end of
/// the file and gives up after [`ARROW_SCAN_LIMIT`] tokens.
pub(super) fn arrow_ahead(parser: &mut Parser<'_>, start: usize) -> bool {
    let mut depth: u32 = 0;
    let mut offset = start;
    loop {
        let kind = parser.peek_nth(offset).kind.clone();
        match kind {
            TokenKind::Eof => return false,
            LPAREN => depth += 1,
            RPAREN => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return parser.peek_nth(offset + 1).kind == ARROW;
                }
            }
            _ => {}
        }
        offset += 1;
        if offset > ARROW_SCAN_LIMIT {
            return false;
        }
    }
}

/// `( pattern [= default], …, ...rest )`. When the list closed, how many parameters
/// it holds and whether the last is a rest parameter.
pub(super) fn params(parser: &mut Parser<'_>) -> Option<(usize, bool)> {
    parser.start_node(NodeKind::Params);
    if !parser.eat(&LPAREN) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `(` to open the parameter list");
        parser.finish_node();
        return None;
    }
    let mut names: Vec<(String, Span)> = Vec::new();
    let mut count = 0;
    let mut has_rest = false;
    loop {
        match parser.peek(false).kind.clone() {
            RPAREN => break,
            ELLIPSIS => {
                parser.start_node(NodeKind::RestParam);
                parser.bump(false);
                let ok = one_param(parser, &mut names);
                parser.finish_node();
                if !ok {
                    parser.finish_node();
                    return None;
                }
                count += 1;
                has_rest = true;
                if parser.peek(false).kind != RPAREN {
                    let span = parser.peek(false).span;
                    let message = if parser.peek(false).kind == TokenKind::Punct(Punct::Eq) {
                        "a rest parameter takes no default"
                    } else {
                        "a rest parameter must be last"
                    };
                    parser.error(span, message);
                    parser.finish_node();
                    return None;
                }
                break;
            }
            _ => {
                parser.start_node(NodeKind::Param);
                let ok = one_param(parser, &mut names);
                if ok {
                    bindings::default_value(parser);
                }
                parser.finish_node();
                if !ok {
                    parser.finish_node();
                    return None;
                }
                count += 1;
            }
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RPAREN {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `)` in the parameter list");
            parser.finish_node();
            return None;
        }
    }
    parser.bump(false);
    parser.finish_node();
    Some((count, has_rest))
}

/// One parameter's pattern; a name any earlier parameter bound is E000.
fn one_param(parser: &mut Parser<'_>, names: &mut Vec<(String, Span)>) -> bool {
    let mut own = Vec::new();
    if !bindings::pattern(parser, &mut own) {
        return false;
    }
    for (name, span) in own {
        if names.iter().any(|(earlier, _)| *earlier == name) {
            let message = format!("parameter `{name}` is declared twice");
            parser.error(span, message);
        }
        names.push((name, span));
    }
    true
}

/// A `Block` body: `{` items `}`.
fn body(parser: &mut Parser<'_>) {
    if parser.peek(false).kind != LBRACE {
        let span = parser.peek(false).span;
        parser.error(
            span,
            "expected `{` to open the function body (SPEC.md §2 `Block`)",
        );
        return;
    }
    block(parser);
}

/// `{ … }` holding declarations and statements, parsed as a nested scope.
pub(super) fn block(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::Block);
    parser.bump(false);
    super::items_in(parser, Scope::Block);
    if !parser.eat(&RBRACE) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `}` to close the block");
    }
    parser.finish_node();
}

/// True when `function` or `async function` starts here (`async` [no line break]
/// `function`: after a line break `async` is a name).
pub(super) fn at_function(parser: &mut Parser<'_>) -> bool {
    parser.peek(false).kind == FUNCTION
        || (parser.at_word("async")
            && parser.peek_second().kind == FUNCTION
            && !parser.peek_second().has_newline_before)
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;
    use sjs_diag::{Code, Diagnostic, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn sexp(text: &str) -> String {
        let f = file(text);
        parse(&f).0.to_sexp(&f)
    }

    fn diagnostics(text: &str) -> Vec<(Code, u32, u32, String)> {
        parse(&file(text))
            .1
            .iter()
            .map(|d: &Diagnostic| {
                (
                    d.code,
                    d.span.start.line,
                    d.span.start.column,
                    d.message.clone(),
                )
            })
            .collect()
    }

    fn clean(text: &str) -> String {
        let found = diagnostics(text);
        assert_eq!(found, Vec::new(), "{text}");
        sexp(text)
    }

    /// Each text yields exactly one diagnostic: E000 on line 1 at `column`, whose
    /// message contains `expected`.
    fn check_e000(cases: &[(&str, u32, &str)]) {
        for (text, column, expected) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let message = &found[0].3;
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E000, 1, *column),
                "{text}: {message}"
            );
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    #[test]
    fn function_declarations_parse_in_every_form() {
        assert_eq!(
            clean("function f() {}"),
            "(Module (FunctionDecl function f (Params ( )) (Block { })) <eof>)"
        );
        assert_eq!(
            clean("async function f(a, b = 1, ...c) {\n  return a\n}"),
            "(Module (FunctionDecl async function f (Params ( (Param a) , (Param b (Initializer = 1)) , (RestParam ... c) )) (Block { (ReturnStmt return a) })) <eof>)"
        );
        assert_eq!(
            clean("function* g() { yield 1 }\nasync function* h() { yield await 1 }"),
            "(Module (FunctionDecl function * g (Params ( )) (Block { (ExprStmt (YieldExpr yield 1)) })) (FunctionDecl async function * h (Params ( )) (Block { (ExprStmt (YieldExpr yield (AwaitExpr await 1))) })) <eof>)"
        );
        assert_eq!(
            clean("function f({ a, b: [c] }, [d] = [], ...{ e }) {}"),
            "(Module (FunctionDecl function f (Params ( (Param (ObjectPattern { (PatternProperty a) , (PatternProperty b : (ArrayPattern [ (PatternElement c) ])) })) , (Param (ArrayPattern [ (PatternElement d) ]) (Initializer = (ArrayLit [ ]))) , (RestParam ... (ObjectPattern { (PatternProperty e) })) )) (Block { })) <eof>)"
        );
        assert_eq!(
            clean("function f() {\n  const a = 1\n  function g() {}\n  {\n    let b = 2\n  }\n}"),
            "(Module (FunctionDecl function f (Params ( )) (Block { (ConstDecl const a (Initializer = 1)) (FunctionDecl function g (Params ( )) (Block { })) (Block { (LetDecl let b (Initializer = 2)) }) })) <eof>)",
            "declarations and nested functions directly inside the body"
        );
        assert_eq!(
            clean("function f(a = (b) => b, c = async (d) => { return d }) {}"),
            "(Module (FunctionDecl function f (Params ( (Param a (Initializer = (ArrowFunction (Params ( (Param b) )) => b))) , (Param c (Initializer = (ArrowFunction async (Params ( (Param d) )) => (Block { (ReturnStmt return d) })))) )) (Block { })) <eof>)",
            "arrows as defaults"
        );
        assert_eq!(
            clean("export function f() {}\nexport async function g() {}\nf()"),
            "(Module (ExportDecl export (FunctionDecl function f (Params ( )) (Block { }))) (ExportDecl export (FunctionDecl async function g (Params ( )) (Block { }))) (ExprStmt (CallExpr f ( ))) <eof>)"
        );
    }

    #[test]
    fn arrow_functions_parse_in_expression_position() {
        assert_eq!(
            clean("const f = (a) => a"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( (Param a) )) => a))) <eof>)"
        );
        assert_eq!(
            clean("const f = async (a) => a + 1\nconst g = 2"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction async (Params ( (Param a) )) => (BinaryExpr a + 1)))) (ConstDecl const g (Initializer = 2)) <eof>)",
            "an expression body ends where the enclosing expression would"
        );
        assert_eq!(
            clean("const f = (a, { b }, ...c) => {}"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( (Param a) , (Param (ObjectPattern { (PatternProperty b) })) , (RestParam ... c) )) => (Block { })))) <eof>)",
            "`=> {{}}` is an empty block"
        );
        assert_eq!(
            clean("const f = () => ({})"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( )) => (ParenExpr ( (ObjectLit { }) ))))) <eof>)",
            "`=> ({{}})` is an object"
        );
        assert_eq!(
            clean("const r = f((a) => a, b, () => () => 1)"),
            "(Module (ConstDecl const r (Initializer = (CallExpr f ( (ArrowFunction (Params ( (Param a) )) => a) , b , (ArrowFunction (Params ( )) => (ArrowFunction (Params ( )) => 1)) )))) <eof>)",
            "arrows nest inside call arguments and inside arrows"
        );
        assert_eq!(
            clean("const f = (a) => {\n  const b = a\n  return (c) => b + c\n}"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( (Param a) )) => (Block { (ConstDecl const b (Initializer = a)) (ReturnStmt return (ArrowFunction (Params ( (Param c) )) => (BinaryExpr b + c))) })))) <eof>)"
        );
        assert_eq!(
            clean("const r = async(x)\nconst s = async(x) + 1"),
            "(Module (ConstDecl const r (Initializer = (CallExpr async ( x )))) (ConstDecl const s (Initializer = (BinaryExpr (CallExpr async ( x )) + 1))) <eof>)",
            "`async(x)` is a call to a binding named `async`"
        );
        assert_eq!(
            clean("const f = (a) => /x/v\nconst g = (a) => `${a}`"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( (Param a) )) => /x/v))) (ConstDecl const g (Initializer = (ArrowFunction (Params ( (Param a) )) => (TemplateLit `${ a }`)))) <eof>)",
            "a regex or template may start the body"
        );
        assert_eq!(
            clean("const r = c ? (a) => a : (b) => b\nconst s = (a) => a ? b : c\nconst t = [() => 1, 2]"),
            "(Module (ConstDecl const r (Initializer = (CondExpr c ? (ArrowFunction (Params ( (Param a) )) => a) : (ArrowFunction (Params ( (Param b) )) => b)))) (ConstDecl const s (Initializer = (ArrowFunction (Params ( (Param a) )) => (CondExpr a ? b : c)))) (ConstDecl const t (Initializer = (ArrayLit [ (ArrowFunction (Params ( )) => 1) , 2 ]))) <eof>)",
            "an arrow body ends at an enclosing conditional's `:` or at a `,`, but owns its own conditional"
        );
        assert_eq!(
            clean("const r = c ? (a) => a ? 1 : 2 : (b) => b"),
            "(Module (ConstDecl const r (Initializer = (CondExpr c ? (ArrowFunction (Params ( (Param a) )) => (CondExpr a ? 1 : 2)) : (ArrowFunction (Params ( (Param b) )) => b)))) <eof>)",
            "a body's own conditional closes before the enclosing `:` ends the body"
        );
        assert_eq!(
            sexp("x = () => 1, 2"),
            "(Module (ExprStmt (SequenceExpr (AssignExpr x = (ArrowFunction (Params ( )) => 1)) , 2)) <eof>)",
            "at statement level the `,` after the body is the comma operator (E000)"
        );
        assert_eq!(
            diagnostics("x = () => 1, 2").len(),
            1,
            "the comma operator, once"
        );
        assert_eq!(
            clean("for (const y of await ((a) => a) ? () => ({ b }) : await c) {}"),
            "(Module (ForOfStmt for ( (ConstDecl const y) of (CondExpr (AwaitExpr await (ParenExpr ( (ArrowFunction (Params ( (Param a) )) => a) ))) ? (ArrowFunction (Params ( )) => (ParenExpr ( (ObjectLit { (Property b) }) ))) : (AwaitExpr await c)) ) (Block { })) <eof>)",
            "where `await` after the body is the module's"
        );
        assert_eq!(
            clean("const f = (a = 1) => a\nconst g = ((a) => a)(1)"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( (Param a (Initializer = 1)) )) => a))) (ConstDecl const g (Initializer = (CallExpr (ParenExpr ( (ArrowFunction (Params ( (Param a) )) => a) )) ( 1 )))) <eof>)"
        );
    }

    #[test]
    fn unparenthesised_arrow_parameters_are_e000_with_the_fix() {
        let f = file("const f = a => a\nconst g = async b => b");
        let (tree, found) = parse(&f);
        let found: Vec<&Diagnostic> = found.iter().collect();
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            (found[0].code, found[0].span.start.column),
            (Code::E000, 11)
        );
        assert_eq!(found[0].current.as_deref(), Some("a"));
        assert_eq!(found[0].replacement.as_deref(), Some("(a)"));
        assert_eq!(
            (found[1].span.start.line, found[1].span.start.column),
            (2, 17)
        );
        assert_eq!(found[1].replacement.as_deref(), Some("(b)"));
        assert_eq!(
            tree.to_sexp(&f),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params a (Param)) => a))) (ConstDecl const g (Initializer = (ArrowFunction async (Params b (Param)) => b))) <eof>)",
            "parsed anyway"
        );
        assert_eq!(tree.print(&f), "const f = a => a\nconst g = async b => b");
    }

    #[test]
    fn function_expressions_are_e000_naming_the_replacement() {
        for text in [
            "const f = function () {}",
            "const f = function* () {}",
            "const f = async function () {}",
            "const f = function named() {}",
            "const f = () => function g() {}",
            "const f = [function () {}]",
            "x = function () {}",
            "f(async function () {})",
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(found[0].0, Code::E000, "{text}");
            let message = &found[0].3;
            assert!(
                message.contains("arrow function or a `function` declaration"),
                "{text}: {message}"
            );
        }
        assert_eq!(
            sexp("const f = function (a) { return a }"),
            "(Module (ConstDecl const f (Initializer = (FunctionExpr function (Params ( (Param a) )) (Block { (ReturnStmt return a) })))) <eof>)",
            "parsed anyway, as an expression node"
        );
        assert_eq!(
            sexp("const f = () => function g() {}"),
            "(Module (ConstDecl const f (Initializer = (ArrowFunction (Params ( )) => (FunctionExpr function g (Params ( )) (Block { }))))) <eof>)"
        );
    }

    #[test]
    fn parameter_and_body_errors_are_e000_once() {
        check_e000(&[
            ("function f(a, a) {}", 15, "parameter `a` is declared twice"),
            (
                "function f(a, { a }) {}",
                17,
                "parameter `a` is declared twice",
            ),
            (
                "function f([a], ...a) {}",
                20,
                "parameter `a` is declared twice",
            ),
            (
                "const f = (a, b, a) => a",
                18,
                "parameter `a` is declared twice",
            ),
            (
                "function f(...a, b) {}",
                16,
                "a rest parameter must be last",
            ),
            (
                "function f(...a = 1) {}",
                17,
                "a rest parameter takes no default",
            ),
            (
                "function f(...1) {}",
                15,
                "expected a binding name, `{` or `[`",
            ),
            (
                "const f = (a b) => a",
                14,
                "expected `,` or `)` in the parameter list",
            ),
            (
                "function f(a b) {}",
                14,
                "expected `,` or `)` in the parameter list",
            ),
            (
                "function f(1) {}",
                12,
                "expected a binding name, `{` or `[`",
            ),
            (
                "function f(a = ) {}",
                14,
                "expected an expression after `=`",
            ),
            (
                "function f {}",
                12,
                "expected `(` to open the parameter list",
            ),
            (
                "function f() 1",
                14,
                "expected `{` to open the function body",
            ),
            (
                "function () {}",
                10,
                "a `function` declaration needs a name",
            ),
            ("function f() {", 15, "expected `}` to close the block"),
            (
                "const f = (a) =>",
                15,
                "expected an expression or a block after `=>`",
            ),
        ]);
        assert_eq!(
            sexp("const f = (=> 1"),
            "(Module (ConstDecl const f (Initializer = (ParenExpr ( => 1))) <eof>)",
            "an unmatched `(` is not an arrow: the tokens stay with the enclosing expression"
        );
        assert_eq!(
            sexp("function f(a b) {}\nconst c = 1"),
            "(Module (FunctionDecl function f (Params ( (Param a))) b ) { } (ConstDecl const c (Initializer = 1)) <eof>)",
            "the declaration ends at the error and the rest is left to the items"
        );
    }

    #[test]
    fn yield_and_await_are_checked_against_the_enclosing_function() {
        assert_eq!(
            diagnostics("function* g() {\n  const f = () => yield 1\n}"),
            vec![(
                Code::E000,
                2,
                19,
                "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)"
                    .to_owned()
            )],
            "a yield inside an arrow inside a generator"
        );
        check_e000(&[
            (
                "yield 1",
                1,
                "`yield` is permitted only directly inside a generator",
            ),
            (
                "function f() { yield 1 }",
                16,
                "`yield` is permitted only directly inside a generator",
            ),
            (
                "function f() { await x }",
                16,
                "`await` is permitted only inside an `async` function or at the top level",
            ),
            (
                "const f = () => await x",
                17,
                "`await` is permitted only inside an `async` function",
            ),
            (
                "const f = (a = yield) => a",
                16,
                "`yield` is permitted only directly inside a generator",
            ),
        ]);
        for text in [
            "await x",
            "const a = await x",
            "async function f() { await x }",
            "const f = async () => await x",
            "function* g() { yield 1\n  yield* h()\n  const a = yield }",
            "async function* g() { yield await x }",
            "function* g() { const f = async () => await 1 }",
            "const o = { yield: 1, await: 2 }\nconst p = o.yield + o.await",
            "class C { yield() {} }",
            "class C {\n  static *g() {\n    yield 1\n  }\n  async f() {\n    await 1\n  }\n}",
            "class C extends D {\n  f() {\n    const g = async () => await 1\n  }\n}\nawait x",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
        assert_eq!(
            diagnostics("o.yield\no.await\nx = o.yield"),
            Vec::new(),
            "after `.`, `yield` and `await` are property names at statement level too"
        );
        assert_eq!(
            diagnostics("o.class\n{\n  yield 1\n}").len(),
            1,
            "`o.class` is a property name, not a class"
        );
    }

    #[test]
    fn declarations_only_at_statement_start_and_blocks_close_their_scope() {
        assert_eq!(
            clean("x = f\nfunction f() {}\na.function"),
            "(Module (ExprStmt (AssignExpr x = f)) (FunctionDecl function f (Params ( )) (Block { })) (ExprStmt (MemberExpr a . function)) <eof>)"
        );
        assert_eq!(
            clean("o.function(1)"),
            "(Module (ExprStmt (CallExpr (MemberExpr o . function) ( 1 ))) <eof>)",
            "after `.` the word is a property name: no function expression is read there"
        );
        for (text, column) in [("o.async (b) => b", 13), ("o.b => 1", 5)] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (found[0].1, found[0].2),
                (1, column),
                "nor an arrow after a property: the `=>` is out of place: {text}"
            );
        }
        assert_eq!(
            clean("function f() { const a = 1 }\nconst b = 2"),
            "(Module (FunctionDecl function f (Params ( )) (Block { (ConstDecl const a (Initializer = 1)) })) (ConstDecl const b (Initializer = 2)) <eof>)",
            "a `}}` on the same line ends the block and the initializer inside it"
        );
        assert_eq!(
            clean("if (x) {\n  function f() {}\n}"),
            "(Module (IfStmt if ( x ) (Block { (FunctionDecl function f (Params ( )) (Block { })) })) <eof>)",
            "directly inside a raw block too"
        );
        assert_eq!(
            diagnostics("class C {\n  g() {\n    const f = () => yield 1\n  }\n}"),
            vec![(
                Code::E000,
                3,
                21,
                "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)"
                    .to_owned()
            )],
            "an arrow inside a class method is still checked"
        );
        assert_eq!(
            diagnostics("class C {}\nfunction f() { yield 1 }").len(),
            1,
            "the class body's region ends at its `}}`"
        );
        assert_eq!(
            diagnostics("function f() {\n  class A {\n    static {\n      class B {\n        m() {}\n      }\n    }\n  }\n}\nawait x\nfunction g() { yield 1 }"),
            vec![(Code::E000, 11, 16, "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)".to_owned())],
            "nested class bodies leave no context behind"
        );
        let found = diagnostics("function f() {\n  export const a = 1\n}");
        assert_eq!(found.len(), 1);
        assert!(found[0].3.contains("top level"));
    }

    #[test]
    fn an_unmatched_parenthesis_scan_gives_up_without_scanning_the_whole_file() {
        let text = format!("const a = ({}", "b ".repeat(70_000));
        let f = file(&text);
        let (tree, found) = parse(&f);
        assert_eq!(tree.print(&f), text);
        let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, ["expected `)`"], "a parenthesis, not an arrow");
    }

    #[test]
    fn regexes_read_as_lookahead_leave_no_stale_report() {
        assert_eq!(
            clean("const r = f(/\\d/v)\nconst s = f((a) => a, /[\\(]/v)"),
            "(Module (ConstDecl const r (Initializer = (CallExpr f ( /\\d/v )))) (ConstDecl const s (Initializer = (CallExpr f ( (ArrowFunction (Params ( (Param a) )) => a) , /[\\(]/v )))) <eof>)",
            "the arrow scan read the regex as division first"
        );
        assert_eq!(
            clean("const t = `a${f(b, /x/v)[c]}d`\nconst u = f((a) => a, `${/y/v}`)"),
            "(Module (ConstDecl const t (Initializer = (TemplateLit `a${ (IndexExpr (CallExpr f ( b , /x/v )) [ c ]) }d`))) (ConstDecl const u (Initializer = (CallExpr f ( (ArrowFunction (Params ( (Param a) )) => a) , (TemplateLit `${ /y/v }`) )))) <eof>)",
            "a regex lexed again inside a substitution keeps the substitution open"
        );
        assert_eq!(
            clean("for (const x of /a/v) {}\nconst y = of / 2\nfor (const z of [/b/v]) {}"),
            "(Module (ForOfStmt for ( (ConstDecl const x) of /a/v ) (Block { })) (ConstDecl const y (Initializer = (BinaryExpr of / 2))) (ForOfStmt for ( (ConstDecl const z) of (ArrayLit [ /b/v ]) ) (Block { })) <eof>)",
            "a `/` after the contextual keyword `of` starts a regex; after a value named `of` it divides"
        );
        assert_eq!(
            clean("const a = await /x/v\nfunction* g() { yield /y/v }\nconst b = o.await / 2"),
            "(Module (ConstDecl const a (Initializer = (AwaitExpr await /x/v))) (FunctionDecl function * g (Params ( )) (Block { (ExprStmt (YieldExpr yield /y/v)) })) (ConstDecl const b (Initializer = (BinaryExpr (MemberExpr o . await) / 2))) <eof>)",
            "an operand follows `await` and `yield`; after `.` they are property names"
        );
        assert_eq!(
            clean("const a = { b: 1 } / 2\nx = { c: [1] } / 3\nif (y) {}\n/z/v\nfunction f() {\n  return { d: 1 } / 4\n}\n{}\n/w/v"),
            "(Module (ConstDecl const a (Initializer = (BinaryExpr (ObjectLit { (Property b : 1) }) / 2))) (ExprStmt (AssignExpr x = (BinaryExpr (ObjectLit { (Property c : (ArrayLit [ 1 ])) }) / 3))) (IfStmt if ( y ) (Block { })) (ExprStmt /z/v) (FunctionDecl function f (Params ( )) (Block { (ReturnStmt return (BinaryExpr (ObjectLit { (Property d : 1) }) / 4)) })) (Block { }) (ExprStmt /w/v) <eof>)",
            "a `}}` closing an object literal ends a value; one closing a block does not"
        );
        assert_eq!(
            clean("async function* g() {\n  yield {} / 2\n  const a = await {} / 3\n}"),
            "(Module (FunctionDecl async function * g (Params ( )) (Block { (ExprStmt (YieldExpr yield (BinaryExpr (ObjectLit { }) / 2))) (ConstDecl const a (Initializer = (BinaryExpr (AwaitExpr await (ObjectLit { })) / 3))) })) <eof>)",
            "an object literal after `yield` or `await` is a value too"
        );
        assert_eq!(
            sexp("export default function () {}"),
            "(Module (ExportDecl export default (FunctionExpr function (Params ( )) (Block { }))) <eof>)"
        );
    }

    #[test]
    fn functions_print_back_to_their_source() {
        let text =
            "async  function* g ( a ,b = 1,...c ) { // c\n  yield a\n}\nconst f = ( x ) =>\n  x\n";
        let f = file(text);
        let (tree, found) = parse(&f);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(tree.print(&f), text);
    }
}
