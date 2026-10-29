//! Statements (SPEC.md §2 `Statement`, `Block`, `IfStmt`, `ForOfStmt`, `ForStmt`,
//! `ForUpdate`, `WhileStmt`, §4.3, §12): blocks, `if`/`else`/`else if`, `while`,
//! `for…of`, `for await…of`, the classic `for`, `return`, `break`, `continue`, `throw`
//! and expression statements. Every body is a block in braces; a braceless body is
//! E000 with the braced replacement and is parsed anyway. The classic `for` header is
//! exactly `let i = start; condition; update` with the update one of `i += k`,
//! `i -= k`, `i++`, `i--`, `++i`, `--i`.
//!
//! Removed statements are E000 naming their replacement and parsed for recovery:
//! `do…while`, `with`, labels and labelled `break`/`continue`; `var`, `for…in` and
//! `debugger` are reported by the lexer at their keyword, once. ES2025 early errors
//! reported here: `break` outside a loop or `switch`, `continue` outside a loop,
//! `return` outside a function, and `for await` outside an `async` function or the
//! module's top level. A bare `;` is an empty statement, reported S002.
//!
//! `switch` and `try` get their structure here so the declarations inside them parse;
//! their own rules (literal labels, braced clauses, one `default`, `try` needing a
//! handler) are US-027's. Expressions are extents until the expression stories land
//! ([`Parser::raw_expression`]).

use sjs_diag::{Code, Span};

use super::{bindings, exports, functions, FnContext, Jump, Parser, Scope};
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

const LPAREN: TokenKind = TokenKind::Punct(Punct::LParen);
const RPAREN: TokenKind = TokenKind::Punct(Punct::RParen);
const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const SEMICOLON: TokenKind = TokenKind::Punct(Punct::Semicolon);
const COLON: TokenKind = TokenKind::Punct(Punct::Colon);
const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);
const EQ: TokenKind = TokenKind::Punct(Punct::Eq);

/// One statement, at its first token. Past the nesting bound the rest of the
/// enclosing block is skipped instead (reported once).
pub(super) fn statement(parser: &mut Parser<'_>) {
    if parser.enter() {
        dispatch(parser);
    } else {
        parser.skip_rest_of_block();
    }
    parser.leave();
}

fn dispatch(parser: &mut Parser<'_>) {
    // A statement starts where an operand may: a `/` here begins a regex.
    let token = parser.peek(true).clone();
    match token.kind {
        LBRACE => functions::block(parser),
        SEMICOLON => empty(parser),
        TokenKind::Keyword(Keyword::If) => if_stmt(parser),
        TokenKind::Keyword(Keyword::While) => {
            parser.start_node(NodeKind::WhileStmt);
            parser.bump(false);
            if condition(parser, "while") {
                loop_body(parser);
            }
            parser.finish_node();
        }
        TokenKind::Keyword(Keyword::For) => for_stmt(parser),
        TokenKind::Keyword(Keyword::Do) => do_while(parser),
        TokenKind::Keyword(Keyword::With) => {
            parser.start_node(NodeKind::WithStmt);
            parser.error(
                token.span,
                "`with` is removed: there is no dynamic scope, so name the object (SPEC.md §12)",
            );
            parser.bump(false);
            if condition(parser, "with") {
                body(parser);
            }
            parser.finish_node();
        }
        TokenKind::Keyword(Keyword::Return) => return_stmt(parser),
        TokenKind::Keyword(Keyword::Break | Keyword::Continue) => jump(parser),
        TokenKind::Keyword(Keyword::Throw) => {
            parser.start_node(NodeKind::ThrowStmt);
            parser.bump(false);
            if matches!(parser.peek(true).kind, SEMICOLON | RBRACE | TokenKind::Eof) {
                parser.error(token.span, "expected an expression after `throw`");
            } else {
                parser.raw_expression(&[]);
            }
            parser.eat(&SEMICOLON);
            parser.finish_node();
        }
        TokenKind::Keyword(Keyword::Debugger) => {
            // The lexer has reported the removed word.
            parser.start_node(NodeKind::DebuggerStmt);
            parser.bump(false);
            parser.eat(&SEMICOLON);
            parser.finish_node();
        }
        TokenKind::Keyword(Keyword::Switch) => switch_stmt(parser),
        TokenKind::Keyword(Keyword::Try) => try_stmt(parser),
        TokenKind::Keyword(
            Keyword::Else | Keyword::Case | Keyword::Default | Keyword::Catch | Keyword::Finally,
        )
        | TokenKind::Punct(Punct::RParen | Punct::RBracket | Punct::Colon) => {
            let message = format!(
                "expected a statement, not `{}`: it continues a statement that is not there",
                parser.text(&token)
            );
            parser.error(token.span, message);
            parser.bump(false);
        }
        RBRACE | TokenKind::Eof => parser.error(token.span, "expected a statement"),
        TokenKind::Ident if parser.peek_second().kind == COLON => labelled(parser),
        _ => {
            parser.start_node(NodeKind::ExprStmt);
            parser.raw_expression(&[]);
            parser.eat(&SEMICOLON);
            parser.finish_node();
        }
    }
}

/// `;` alone: an empty statement, which is a semicolon and so S002.
fn empty(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::EmptyStmt);
    let span = parser.bump(false).span;
    parser.report_with_fix(
        Code::S002,
        span,
        "an empty statement is a semicolon: the canonical style omits them (SPEC.md §8.5)",
        ";".to_owned(),
        String::new(),
    );
    parser.finish_node();
}

/// `if (cond) block [else (block | if …)]`.
fn if_stmt(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::IfStmt);
    parser.bump(false);
    if !condition(parser, "if") {
        parser.finish_node();
        return;
    }
    body(parser);
    if parser.eat(&TokenKind::Keyword(Keyword::Else)) {
        if parser.peek(false).kind == TokenKind::Keyword(Keyword::If) {
            statement(parser);
        } else {
            body(parser);
        }
    }
    parser.finish_node();
}

/// `( expr )` after `keyword`. False when the parentheses are malformed (reported):
/// the statement then stops there and the item skips the rest of its line.
fn condition(parser: &mut Parser<'_>, keyword: &str) -> bool {
    if !parser.eat(&LPAREN) {
        let span = parser.peek(false).span;
        let message = format!("expected `(` after `{keyword}`");
        parser.error(span, message);
        return false;
    }
    if parser.raw_expression(&[]) == 0 {
        let span = parser.peek(false).span;
        let message = format!("expected a condition after `{keyword} (`");
        parser.error(span, message);
    }
    if !parser.eat(&RPAREN) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `)` after the condition");
        return false;
    }
    true
}

/// A statement body: a block. Anything else is E000 with the braced replacement and is
/// parsed as the one statement it is; a declaration there (`if (x) const y = 1`) is
/// named as such.
fn body(parser: &mut Parser<'_>) {
    let first = parser.peek(true).clone();
    match first.kind {
        LBRACE => {
            functions::block(parser);
            return;
        }
        RBRACE | TokenKind::Eof => {
            parser.error(first.span, "expected `{` to open the body (SPEC.md §4.3)");
            return;
        }
        _ => {}
    }
    let declaration = matches!(
        first.kind,
        TokenKind::Keyword(Keyword::Const | Keyword::Let | Keyword::Var | Keyword::Class)
    ) || functions::at_function(parser);
    super::item(parser, Scope::Block, &mut exports::Exported::new());
    // `item` consumed at least the first token, so a last token exists.
    let last = parser.last.unwrap_or(first.span);
    let span = Span::new(
        first.span.start,
        last.end,
        first.span.byte_start,
        last.byte_end,
    );
    let current = parser.file.slice(&span).unwrap_or("").to_owned();
    let message = if declaration {
        "a declaration cannot be a statement body: it would bind nothing usable, so put it in a block (SPEC.md §4.3)"
    } else {
        "every statement body is a block in braces (SPEC.md §4.3, §12)"
    };
    let replacement = format!("{{ {current} }}");
    parser.error_with_fix(span, message, current, replacement);
}

/// A loop's body, inside which `break` and `continue` have a target.
fn loop_body(parser: &mut Parser<'_>) {
    parser.with_jump(Jump::Loop, body);
}

/// `do block while (cond)`: removed, parsed for recovery.
fn do_while(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::DoWhileStmt);
    let span = parser.bump(false).span;
    parser.error(
        span,
        "`do…while` is removed: write a `while` loop (SPEC.md §12)",
    );
    loop_body(parser);
    if parser.eat(&TokenKind::Keyword(Keyword::While)) {
        condition(parser, "while");
    }
    parser.eat(&SEMICOLON);
    parser.finish_node();
}

/// `label: statement`: removed, parsed for recovery.
fn labelled(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::LabelledStmt);
    let span = parser.bump(false).span;
    parser.error(
        span,
        "labels are removed: restructure with functions (SPEC.md §12)",
    );
    parser.bump(false);
    super::item(parser, Scope::Block, &mut exports::Exported::new());
    parser.finish_node();
}

/// `return [expr]`, inside a function.
fn return_stmt(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::ReturnStmt);
    let span = parser.bump(false).span;
    let in_function = matches!(
        parser.contexts.last(),
        Some(FnContext::Function { .. } | FnContext::Arrow { .. } | FnContext::Method { .. })
    );
    if !in_function {
        parser.error(
            span,
            "`return` is permitted only inside a function: not at the top level of a module, in a static block or in a field initializer",
        );
    }
    let next = parser.peek(true);
    if !next.has_newline_before && !matches!(next.kind, SEMICOLON | RBRACE | TokenKind::Eof) {
        parser.raw_expression(&[]);
    }
    parser.eat(&SEMICOLON);
    parser.finish_node();
}

/// `break` or `continue`, which need an enclosing loop (or, for `break`, a `switch`) in
/// the same function. A label after either is removed.
fn jump(parser: &mut Parser<'_>) {
    let is_break = parser.peek(false).kind == TokenKind::Keyword(Keyword::Break);
    parser.start_node(if is_break {
        NodeKind::BreakStmt
    } else {
        NodeKind::ContinueStmt
    });
    let keyword = parser.bump(false).clone();
    let word = parser.text(&keyword);
    let next = parser.peek(false).clone();
    if next.kind == TokenKind::Ident && !next.has_newline_before {
        let message = format!(
            "a labelled `{word}` is removed: labels are, so restructure with functions (SPEC.md §12)"
        );
        parser.error(next.span, message);
        parser.bump(false);
    } else if is_break && !parser.can_break() {
        parser.error(
            keyword.span,
            "`break` is permitted only inside a loop or a `switch` of the same function",
        );
    } else if !is_break && !parser.can_continue() {
        parser.error(
            keyword.span,
            "`continue` is permitted only inside a loop of the same function",
        );
    }
    parser.eat(&SEMICOLON);
    parser.finish_node();
}

/// `for [await] ( head ) block`, where the head decides the node: `ForOfStmt`,
/// `ForStmt`, or the removed `ForInStmt`.
fn for_stmt(parser: &mut Parser<'_>) {
    let node = parser.start_node(NodeKind::ForStmt);
    let for_span = parser.bump(false).span;
    let is_await = parser.at_word("await");
    if is_await {
        let span = parser.bump(false).span;
        if !parser.await_allowed() {
            parser.error(
                span,
                "`for await` is permitted only inside an `async` function or at the top level of a module (SPEC.md §5.1)",
            );
        }
    }
    if !parser.eat(&LPAREN) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `(` after `for`");
        parser.finish_node();
        return;
    }
    let kind = head(parser, is_await, for_span);
    parser.retag(node, kind);
    if parser.eat(&RPAREN) {
        loop_body(parser);
    } else {
        let span = parser.peek(false).span;
        parser.error(span, "expected `)` to close the `for` header");
    }
    parser.finish_node();
}

/// The `for` header after its `(`, up to (not including) the `)`. Returns the node kind
/// it makes of the loop.
fn head(parser: &mut Parser<'_>, is_await: bool, for_span: Span) -> NodeKind {
    let keyword = parser.peek(false).clone();
    let decl = match keyword.kind {
        TokenKind::Keyword(Keyword::Const) => NodeKind::ConstDecl,
        TokenKind::Keyword(Keyword::Let) => NodeKind::LetDecl,
        // The lexer has reported `var`; the loop parses as if it were `let`.
        TokenKind::Keyword(Keyword::Var) => NodeKind::VarDecl,
        SEMICOLON => {
            parser.error(
                keyword.span,
                "`for (;;)` is removed: an infinite loop is `while (true)` (SPEC.md §4.3)",
            );
            skip_header(parser);
            return NodeKind::ForStmt;
        }
        _ => {
            parser.error(
                keyword.span,
                "a loop declares its binding in the header with `const` or `let` (SPEC.md §2 `ForOfStmt`, `ForStmt`)",
            );
            skip_header(parser);
            return NodeKind::ForStmt;
        }
    };
    parser.start_node(decl);
    parser.bump(false);
    let mut names = Vec::new();
    if !bindings::pattern(parser, &mut names) {
        parser.finish_node();
        skip_header(parser);
        return NodeKind::ForStmt;
    }
    let next = parser.peek(false).clone();
    if next.kind == TokenKind::Ident && parser.text(&next) == "of" {
        parser.finish_node();
        parser.bump(false);
        // An iterable follows `of`, so a `/` there begins a regex.
        parser.peek(true);
        if parser.raw_expression(&[]) == 0 {
            parser.error(next.span, "expected an iterable after `of`");
        }
        return NodeKind::ForOfStmt;
    }
    match next.kind {
        TokenKind::Keyword(Keyword::In) => {
            // The lexer has reported `in`, naming `for…of` over `Object.keys`.
            parser.finish_node();
            parser.bump(false);
            skip_header(parser);
            NodeKind::ForInStmt
        }
        EQ => {
            parser.start_node(NodeKind::Initializer);
            parser.bump(false);
            if parser.raw_expression(&[COMMA]) == 0 {
                parser.error(next.span, "expected an expression after `=`");
            }
            parser.finish_node();
            parser.finish_node();
            if decl == NodeKind::ConstDecl {
                parser.error(
                    keyword.span,
                    "the counter of a classic `for` changes, so it is declared with `let` (SPEC.md §2 `ForStmt`)",
                );
            }
            if is_await {
                parser.error(
                    for_span,
                    "`for await` iterates with `of`: `for await (const x of xs)` (SPEC.md §2 `ForOfStmt`)",
                );
            }
            classic(parser);
            NodeKind::ForStmt
        }
        _ => {
            parser.finish_node();
            parser.error(next.span, "expected `of` or `=` after the loop binding");
            skip_header(parser);
            NodeKind::ForStmt
        }
    }
}

/// The classic header after its initializer: `; condition; update`.
fn classic(parser: &mut Parser<'_>) {
    let next = parser.peek(false).span;
    if parser.peek(false).kind == COMMA {
        parser.error(
            next,
            "a classic `for` declares one counter: declare the others before the loop (SPEC.md §2 `ForStmt`)",
        );
        skip_header(parser);
        return;
    }
    if !parser.eat(&SEMICOLON) {
        parser.error(next, "expected `;` after the counter");
        skip_header(parser);
        return;
    }
    if parser.peek(true).kind == SEMICOLON {
        let span = parser.peek(false).span;
        parser.error(
            span,
            "the condition of a classic `for` is required: an infinite loop is `while (true)` (SPEC.md §4.3)",
        );
    } else {
        parser.raw_expression(&[]);
    }
    if !parser.eat(&SEMICOLON) {
        let span = parser.peek(false).span;
        parser.error(span, "expected `;` after the condition");
        skip_header(parser);
        return;
    }
    update(parser);
}

/// `i += k`, `i -= k`, `i++`, `i--`, `++i` or `--i`, then the header's `)`.
fn update(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::ForUpdate);
    let start = parser.peek(false).span;
    let first = parser.peek(false).kind.clone();
    let second = parser.peek_second().kind.clone();
    let ok = match (first, second) {
        (TokenKind::Ident, TokenKind::Punct(Punct::PlusEq | Punct::MinusEq)) => {
            parser.bump(false);
            parser.bump(false);
            parser.raw_expression(&[]) > 0
        }
        (TokenKind::Ident, TokenKind::Punct(Punct::PlusPlus | Punct::MinusMinus))
        | (TokenKind::Punct(Punct::PlusPlus | Punct::MinusMinus), TokenKind::Ident) => {
            parser.bump(false);
            parser.bump(false);
            true
        }
        _ => false,
    };
    if !ok || parser.peek(false).kind != RPAREN {
        parser.error(
            start,
            "the update of a classic `for` is `i += k`, `i -= k`, `i++`, `i--`, `++i` or `--i` (SPEC.md §2 `ForUpdate`)",
        );
        skip_header(parser);
    }
    parser.finish_node();
}

/// Skips the rest of a malformed `for` header, to its `)` (or a `}` or the end of the
/// file), so the header yields one diagnostic.
fn skip_header(parser: &mut Parser<'_>) {
    while !matches!(parser.peek(false).kind, RPAREN | RBRACE | TokenKind::Eof) {
        if parser.raw_expression(&[]) == 0 {
            parser.bump(false);
        }
    }
}

/// `switch (expr) { case …: … default: … }`, structurally: the clause rules are
/// US-027's.
fn switch_stmt(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::SwitchStmt);
    parser.bump(false);
    if !condition(parser, "switch") {
        parser.finish_node();
        return;
    }
    if parser.peek(false).kind == LBRACE {
        parser.bump(false);
        parser.with_jump(Jump::Switch, clauses);
    } else {
        let span = parser.peek(false).span;
        parser.error(span, "expected `{` to open the `switch` body");
    }
    parser.finish_node();
}

/// The clauses of a `switch` body after its `{`, through its `}`.
fn clauses(parser: &mut Parser<'_>) {
    let mut exported = exports::Exported::new();
    loop {
        let token = parser.peek(false).clone();
        match token.kind {
            RBRACE => {
                parser.bump(false);
                return;
            }
            TokenKind::Eof => {
                parser.error(token.span, "expected `}` to close the `switch` body");
                return;
            }
            TokenKind::Keyword(Keyword::Case | Keyword::Default) => {
                parser.bump(false);
                if token.kind == TokenKind::Keyword(Keyword::Case)
                    && parser.raw_expression(&[]) == 0
                {
                    let span = parser.peek(false).span;
                    parser.error(span, "expected a label after `case`");
                }
                if !parser.eat(&COLON) {
                    let span = parser.peek(false).span;
                    parser.error(span, "expected `:` after the clause label");
                }
            }
            _ => super::item(parser, Scope::Block, &mut exported),
        }
    }
}

/// `try block [catch [(e)] block] [finally block]`, structurally: the handler rules
/// are US-027's.
fn try_stmt(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::TryStmt);
    parser.bump(false);
    body(parser);
    if parser.eat(&TokenKind::Keyword(Keyword::Catch)) {
        if parser.eat(&LPAREN) {
            parser.raw_expression(&[]);
            if !parser.eat(&RPAREN) {
                let span = parser.peek(false).span;
                parser.error(span, "expected `)` after the `catch` binding");
                parser.finish_node();
                return;
            }
        }
        body(parser);
    }
    if parser.eat(&TokenKind::Keyword(Keyword::Finally)) {
        body(parser);
    }
    parser.finish_node();
}

/// `var pattern [= expr]`: the lexer has reported `var`; parsed for recovery.
pub(super) fn var_decl(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::VarDecl);
    parser.bump(false);
    let mut names = Vec::new();
    if bindings::pattern(parser, &mut names) && parser.peek(false).kind == EQ {
        parser.start_node(NodeKind::Initializer);
        parser.bump(false);
        parser.raw_expression(&[]);
        parser.finish_node();
    }
    parser.eat(&SEMICOLON);
    parser.finish_node();
}

#[cfg(test)]
mod tests {
    use crate::parser::{parse, MAX_NESTING};
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

    /// The fix of each diagnostic, as (current, replacement).
    fn fixes(text: &str) -> Vec<(Option<String>, Option<String>)> {
        parse(&file(text))
            .1
            .iter()
            .map(|d: &Diagnostic| (d.current.clone(), d.replacement.clone()))
            .collect()
    }

    fn clean(text: &str) -> String {
        let found = diagnostics(text);
        assert_eq!(found, Vec::new(), "{text}");
        sexp(text)
    }

    /// Each text yields exactly one diagnostic: E000 at `line:column`, whose message
    /// contains `expected`.
    fn check_e000(cases: &[(&str, u32, u32, &str)]) {
        for (text, line, column, expected) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let message = &found[0].3;
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (Code::E000, *line, *column),
                "{text}: {message}"
            );
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    #[test]
    fn every_statement_form_parses() {
        assert_eq!(
            clean("{\n  f()\n}\nif (a) {\n  b()\n} else if (c) {\n} else {\n}"),
            "(Module (Block { (ExprStmt f ( )) }) (IfStmt if ( a ) (Block { (ExprStmt b ( )) }) else (IfStmt if ( c ) (Block { }) else (Block { }))) <eof>)"
        );
        assert_eq!(
            clean("while (a) {\n  break\n}\nfor (const x of xs) {\n  continue\n}\nfor (let [k, v] of m) {}\nfor await (const y of ys) {}"),
            "(Module (WhileStmt while ( a ) (Block { (BreakStmt break) })) (ForOfStmt for ( (ConstDecl const x) of xs ) (Block { (ContinueStmt continue) })) (ForOfStmt for ( (LetDecl let (ArrayPattern [ (PatternElement k) , (PatternElement v) ])) of m ) (Block { })) (ForOfStmt for await ( (ConstDecl const y) of ys ) (Block { })) <eof>)"
        );
        assert_eq!(
            clean("for (let i = 0; i < n; i += 2) {}\nfor (let i = n; i > 0; i -= 1) {}\nfor (let i = 0; i < n; i++) {}\nfor (let i = 0; i < n; i--) {}\nfor (let i = 0; i < n; ++i) {}\nfor (let i = 0; i < n; --i) {}"),
            "(Module (ForStmt for ( (LetDecl let i (Initializer = 0)) ; i < n ; (ForUpdate i += 2) ) (Block { })) (ForStmt for ( (LetDecl let i (Initializer = n)) ; i > 0 ; (ForUpdate i -= 1) ) (Block { })) (ForStmt for ( (LetDecl let i (Initializer = 0)) ; i < n ; (ForUpdate i ++) ) (Block { })) (ForStmt for ( (LetDecl let i (Initializer = 0)) ; i < n ; (ForUpdate i --) ) (Block { })) (ForStmt for ( (LetDecl let i (Initializer = 0)) ; i < n ; (ForUpdate ++ i) ) (Block { })) (ForStmt for ( (LetDecl let i (Initializer = 0)) ; i < n ; (ForUpdate -- i) ) (Block { })) <eof>)"
        );
        assert_eq!(
            clean("function f(a) {\n  if (a) {\n    return\n  }\n  throw new Error(\"x\")\n  return a;\n}\nf(1);\n/a/v.test(\"a\")"),
            "(Module (FunctionDecl function f (Params ( (Param a) )) (Block { (IfStmt if ( a ) (Block { (ReturnStmt return) })) (ThrowStmt throw new Error ( \"x\" )) (ReturnStmt return a ;) })) (ExprStmt f ( 1 ) ;) (ExprStmt /a/v . test ( \"a\" )) <eof>)",
            "a statement may start with a regex"
        );
        assert_eq!(
            clean("switch (x) {\n  case 1:\n  case 2: {\n    break\n  }\n  default: {\n    f()\n  }\n}\ntry {\n} catch (e) {\n} finally {\n}\ntry {\n} catch {\n}"),
            "(Module (SwitchStmt switch ( x ) { case 1 : case 2 : (Block { (BreakStmt break) }) default : (Block { (ExprStmt f ( )) }) }) (TryStmt try (Block { }) catch ( e ) (Block { }) finally (Block { })) (TryStmt try (Block { }) catch (Block { })) <eof>)"
        );
        assert_eq!(
            clean("function f() {\n  return\n  1\n}"),
            "(Module (FunctionDecl function f (Params ( )) (Block { (ReturnStmt return) (ExprStmt 1) })) <eof>)",
            "a value on the next line is not returned"
        );
    }

    #[test]
    fn a_bare_semicolon_is_an_empty_statement_reported_s002() {
        let text = "{}\n;\nconst a = 1;";
        assert_eq!(
            sexp(text),
            "(Module (Block { }) (EmptyStmt ;) (ConstDecl const a (Initializer = 1) ;) <eof>)",
            "a `;` ending a statement belongs to it; after a block it stands alone"
        );
        assert_eq!(
            clean("f()\n;"),
            "(Module (ExprStmt f ( ) ;) <eof>)",
            "a `;` on the next line still ends the expression statement"
        );
        let found = diagnostics(text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (Code::S002, 2, 1));
        assert_eq!(
            fixes(text),
            vec![(Some(";".to_owned()), Some(String::new()))]
        );
    }

    #[test]
    fn every_braceless_body_is_e000_with_the_braced_replacement() {
        for (text, column, current) in [
            ("if (x) y()", 8, "y()"),
            ("if (x) {} else y();", 16, "y();"),
            ("for (const a of b) y()", 20, "y()"),
            ("for (let i = 0; i < n; i++) y()", 29, "y()"),
            ("while (x) y()", 11, "y()"),
            ("with (o) y()", 10, "y()"),
        ] {
            let found = diagnostics(text);
            let braced = found
                .iter()
                .filter(|d| d.3.contains("every statement body is a block"))
                .count();
            assert_eq!(braced, 1, "{text}: {found:?}");
            let fix = fixes(text);
            let expected = (Some(current.to_owned()), Some(format!("{{ {current} }}")));
            assert!(fix.contains(&expected), "{text}: {fix:?}");
            let at = found.iter().any(|d| d.1 == 1 && d.2 == column);
            assert!(at, "{text}: {found:?}");
        }
        assert_eq!(
            sexp("if (x) y()"),
            "(Module (IfStmt if ( x ) (ExprStmt y ( ))) <eof>)",
            "parsed anyway, as the statement it is"
        );
        assert_eq!(
            fixes("if (a)\n  if (b) c()"),
            vec![
                (
                    Some("if (b) c()".to_owned()),
                    Some("{ if (b) c() }".to_owned())
                ),
                (Some("c()".to_owned()), Some("{ c() }".to_owned())),
            ],
            "each braceless body is its own fault"
        );
    }

    #[test]
    fn a_declaration_as_a_body_is_e000_once() {
        for text in [
            "if (x) const y = 1",
            "if (x) let y = 1",
            "while (x) function f() {}",
            "if (x) class A {}",
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert!(
                found[0]
                    .3
                    .contains("a declaration cannot be a statement body"),
                "{text}: {found:?}"
            );
        }
        assert_eq!(
            fixes("if (x) const y = 1"),
            vec![(
                Some("const y = 1".to_owned()),
                Some("{ const y = 1 }".to_owned())
            )]
        );
    }

    #[test]
    fn every_malformed_for_header_is_e000_once_with_the_reason() {
        check_e000(&[
            ("for (;;) {}", 1, 6, "an infinite loop is `while (true)`"),
            (
                "for (const i = 0; i < n; i++) {}",
                1,
                6,
                "declared with `let`",
            ),
            (
                "for (let i = 0, j = 0; i < n; i++) {}",
                1,
                15,
                "declares one counter",
            ),
            (
                "for (let i = 0; i < n; i = i + 1) {}",
                1,
                24,
                "the update of a classic `for` is",
            ),
            (
                "for (let i = 0; i < n;) {}",
                1,
                23,
                "the update of a classic `for` is",
            ),
            (
                "for (let i = 0; i < n; i +=) {}",
                1,
                24,
                "the update of a classic `for` is",
            ),
            (
                "for (let i = 0; i < n; i++ + 1) {}",
                1,
                24,
                "the update of a classic `for` is",
            ),
            (
                "for (let i = 0; ; i++) {}",
                1,
                17,
                "the condition of a classic `for` is required",
            ),
            (
                "for (let i = 0 i < n) {}",
                1,
                21,
                "expected `;` after the counter",
            ),
            (
                "for (let i = 0; i < n) {}",
                1,
                22,
                "expected `;` after the condition",
            ),
            (
                "for (let i =; i < n; i++) {}",
                1,
                12,
                "expected an expression after `=`",
            ),
            (
                "for (x of xs) {}",
                1,
                6,
                "declares its binding in the header",
            ),
            ("for (const 1 of xs) {}", 1, 12, "expected a binding name"),
            ("for (const x xs) {}", 1, 14, "expected `of` or `=`"),
            (
                "for (const x of) {}",
                1,
                14,
                "expected an iterable after `of`",
            ),
            ("for x {}", 1, 5, "expected `(` after `for`"),
            ("for (var x of xs) {}", 1, 6, "use `const` or `let`"),
            (
                "for (const x of xs {}",
                1,
                22,
                "expected `)` to close the `for` header",
            ),
            (
                "for await (let i = 0; i < n; i++) {}",
                1,
                1,
                "`for await` iterates with `of`",
            ),
        ]);
        assert_eq!(
            sexp("for (;;) {}"),
            "(Module (ForStmt for ( ; ; ) (Block { })) <eof>)",
            "the body still parses"
        );
    }

    #[test]
    fn removed_statements_are_e000_once_naming_the_replacement() {
        check_e000(&[
            (
                "for (const k in o) {}",
                1,
                14,
                "`for…of` over `Object.keys`",
            ),
            ("do {\n} while (x)", 1, 1, "write a `while` loop"),
            ("do {} while (x);", 1, 1, "write a `while` loop"),
            ("a: while (x) {}", 1, 1, "labels are removed"),
            ("with (o) {}", 1, 1, "there is no dynamic scope"),
            ("debugger", 1, 1, "`debugger` is removed"),
            ("var x = 1", 1, 1, "use `const` or `let`"),
            ("var [a, b] = c;", 1, 1, "use `const` or `let`"),
            ("var x", 1, 1, "use `const` or `let`"),
            (
                "while (x) {\n  break a\n}",
                2,
                9,
                "a labelled `break` is removed",
            ),
            (
                "while (x) {\n  continue a\n}",
                2,
                12,
                "a labelled `continue` is removed",
            ),
        ]);
        assert_eq!(
            sexp("for (const k in o) {}\ndo {} while (x)\na: f()\nwith (o) {}\ndebugger;\nvar v = 1"),
            "(Module (ForInStmt for ( (ConstDecl const k) in o ) (Block { })) (DoWhileStmt do (Block { }) while ( x )) (LabelledStmt a : (ExprStmt f ( ))) (WithStmt with ( o ) (Block { })) (DebuggerStmt debugger ;) (VarDecl var v (Initializer = 1)) <eof>)",
            "each is parsed for recovery"
        );
        assert_eq!(
            sexp("while (x) {\n  break\n  a\n}"),
            "(Module (WhileStmt while ( x ) (Block { (BreakStmt break) (ExprStmt a) })) <eof>)",
            "a name on the next line is not a label"
        );
    }

    #[test]
    fn jumps_and_returns_need_their_target_in_the_same_function() {
        check_e000(&[
            (
                "break",
                1,
                1,
                "`break` is permitted only inside a loop or a `switch`",
            ),
            (
                "continue",
                1,
                1,
                "`continue` is permitted only inside a loop",
            ),
            (
                "switch (x) {\n  case 1: {\n    continue\n  }\n}",
                3,
                5,
                "`continue` is permitted only inside a loop",
            ),
            (
                "while (x) {\n  function f() {\n    break\n  }\n}",
                3,
                5,
                "`break` is permitted only inside a loop",
            ),
            (
                "while (x) {\n  const f = () => {\n    continue\n  }\n}",
                3,
                5,
                "`continue` is permitted only inside a loop",
            ),
            (
                "return 1",
                1,
                1,
                "`return` is permitted only inside a function",
            ),
            (
                "class A {\n  static {\n    return\n  }\n}",
                3,
                5,
                "`return` is permitted only inside a function",
            ),
        ]);
        for text in [
            "while (x) {\n  switch (y) {\n    case 1: {\n      continue\n    }\n  }\n}",
            "for (const a of b) {\n  if (a) {\n    break\n  }\n}",
            "switch (x) {\n  default: {\n    break\n  }\n}",
            "class A {\n  x = () => {\n    return 1\n  }\n  m() {\n    return 2\n  }\n}",
            "function f() {\n  while (x) {}\n  return\n}",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
    }

    #[test]
    fn for_await_needs_an_async_context_or_the_top_level() {
        check_e000(&[
            (
                "function f() {\n  for await (const x of y) {}\n}",
                2,
                7,
                "`for await` is permitted only inside an `async` function",
            ),
            (
                "class A {\n  static {\n    for await (const x of y) {}\n  }\n}",
                3,
                9,
                "`for await` is permitted only inside an `async` function",
            ),
        ]);
        for text in [
            "for await (const x of y) {}",
            "async function f() {\n  for await (const x of y) {}\n}",
            "const g = async () => {\n  for await (const x of y) {}\n}",
            "class A {\n  async m() {\n    for await (const x of y) {}\n  }\n}",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
    }

    #[test]
    fn else_if_chains_nest_and_the_nesting_is_bounded() {
        let mut chain = String::from("if (a) {\n}");
        for _ in 0..10 {
            chain.push_str(" else if (a) {\n}");
        }
        chain.push_str(" else {\n}");
        assert_eq!(diagnostics(&chain), Vec::new());
        assert_eq!(sexp(&chain).matches("(IfStmt").count(), 11);
        for text in [
            "{".repeat(100_000),
            "if (a) ".repeat(100_000),
            "function f() {".repeat(100_000),
            "if (a) {} else ".repeat(100_000),
        ] {
            let found = diagnostics(&text);
            let deep: Vec<_> = found
                .iter()
                .filter(|d| d.3.contains(&format!("deeper than {MAX_NESTING}")))
                .collect();
            assert_eq!(deep.len(), 1, "reported once");
            let f = file(&text);
            assert_eq!(parse(&f).0.print(&f), text);
        }
    }

    #[test]
    fn malformed_statements_are_e000_once() {
        check_e000(&[
            ("if x {}", 1, 4, "expected `(` after `if`"),
            ("if () {}", 1, 5, "expected a condition after `if (`"),
            ("while (x {}", 1, 12, "expected `)` after the condition"),
            ("if (x)", 1, 7, "expected `{` to open the body"),
            ("if (x) {} else", 1, 15, "expected `{` to open the body"),
            ("throw", 1, 1, "expected an expression after `throw`"),
            (
                "function f() {\n  throw\n}",
                2,
                3,
                "expected an expression after `throw`",
            ),
            ("else {}", 1, 1, "expected a statement, not `else`"),
            ("do {}", 1, 1, "write a `while` loop"),
            ("f())", 1, 4, "expected a statement, not `)`"),
            ("}", 1, 1, "unmatched `}`"),
            (
                "switch (x)",
                1,
                11,
                "expected `{` to open the `switch` body",
            ),
            (
                "switch (x) {",
                1,
                13,
                "expected `}` to close the `switch` body",
            ),
            (
                "switch (x) {\n  case: {}\n}",
                2,
                7,
                "expected a label after `case`",
            ),
            (
                "switch (x) {\n  case 1 }",
                2,
                10,
                "expected `:` after the clause label",
            ),
            ("switch x {}", 1, 8, "expected `(` after `switch`"),
            (
                "try {} catch (e {}",
                1,
                19,
                "expected `)` after the `catch` binding",
            ),
        ]);
        let found = diagnostics("with o {}");
        assert_eq!(found.len(), 2, "`with` and its missing `(`: {found:?}");
        assert!(
            found[1].3.contains("expected `(` after `with`"),
            "{found:?}"
        );
        let found = diagnostics("a:");
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[1].3.contains("expected a statement"), "{found:?}");
    }

    #[test]
    fn after_a_syntax_error_the_rest_of_the_line_is_skipped() {
        for (text, expected) in [
            (
                "{\n  const { 1 } = x\n  f()\n}",
                "(Module (Block { (ConstDecl const (ObjectPattern {)) 1 } = x (ExprStmt f ( )) }) <eof>)",
            ),
            (
                "{ const [a, , b] = y }",
                "(Module (Block { (ConstDecl const (ArrayPattern [ (PatternElement a) ,)) , b ] = y }) <eof>)",
            ),
            (
                "{ const [a, , b] = y }}",
                "(Module (Block { (ConstDecl const (ArrayPattern [ (PatternElement a) ,)) , b ] = y }) } <eof>)",
            ),
            (
                "const [a, , b] = y; f()",
                "(Module (ConstDecl const (ArrayPattern [ (PatternElement a) ,)) , b ] = y ; (ExprStmt f ( )) <eof>)",
            ),
        ] {
            assert_eq!(sexp(text), expected, "{text}");
        }
        assert_eq!(
            diagnostics("{ const [a, , b] = y }}").len(),
            2,
            "and the stray `}}`"
        );
    }

    #[test]
    fn the_line_skip_balances_brackets_and_leaves_the_blocks_own_brace() {
        for text in [
            // The object's `}` is consumed; the block's, alone at the line's end, is not.
            "{ const [a, , b] = { c: 1 } }",
            "{\n  const [a, , b] = { c: 1 }\n  f()\n}",
            // A `)` closes its `(`, so the block's `}` is still at depth zero.
            "{ const [a, , b] = f(1) }",
            // A `;` inside brackets does not end the skip.
            "const [a, , b] = (c; d) e\nf()",
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let f = file(text);
            assert_eq!(parse(&f).0.print(&f), text);
        }
        assert_eq!(
            sexp("{\n  const [a, , b] = { c: 1 }\n  f()\n}"),
            "(Module (Block { (ConstDecl const (ArrayPattern [ (PatternElement a) ,)) , b ] = { c : 1 } (ExprStmt f ( )) }) <eof>)"
        );
    }

    #[test]
    fn the_nesting_skip_balances_braces_to_the_enclosing_block() {
        let depth = usize::try_from(MAX_NESTING).unwrap_or(usize::MAX);
        let text = format!(
            "{}{{ x }} y\n{}\nz",
            "{\n".repeat(depth),
            "}\n".repeat(depth)
        );
        let found = diagnostics(&text);
        assert_eq!(found.len(), 1, "only the depth is reported: {found:?}");
        let tree = sexp(&text);
        assert_eq!(
            tree.matches("(ExprStmt").count(),
            1,
            "only `z`, after the blocks"
        );
        assert!(
            tree.ends_with("(ExprStmt z) <eof>)"),
            "z follows the blocks"
        );
    }

    #[test]
    fn a_var_whose_pattern_fails_takes_no_initializer() {
        assert_eq!(sexp("var = 1"), "(Module (VarDecl var) = 1 <eof>)");
        assert_eq!(
            diagnostics("var = 1").len(),
            2,
            "the lexer's `var` and the missing name"
        );
    }

    #[test]
    fn a_class_keyword_that_begins_no_class_is_e000_once() {
        for text in ["class\n(f)", "class = 1"] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert!(found[0].3.contains("needs a name"), "{text}: {found:?}");
        }
        assert_eq!(
            sexp("class\n(f)"),
            "(Module (ClassDecl class) (ExprStmt ( f )) <eof>)"
        );
    }

    #[test]
    fn statements_print_back_to_their_source() {
        let text =
            "if  (a)  {\n  b() ; // c\n}  else  if (d) {}\nfor  ( let i = 0 ;i<n; i ++ ) { }\n;\n";
        let f = file(text);
        let (tree, _) = parse(&f);
        assert_eq!(tree.print(&f), text);
    }
}
