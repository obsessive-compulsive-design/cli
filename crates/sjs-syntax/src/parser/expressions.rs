//! Expressions (SPEC.md §2 `Expr` … `PrimaryExpr`, §4.1, §4.3, §12): precedence
//! climbing over assignment (`=` and the compound operators, `??=` included), the
//! conditional, `??`, `||`, `&&`, `|`, `^`, `&`, `===`/`!==`, the relational operators
//! with `instanceof`, the shifts, the additive and multiplicative operators and `**`
//! (right-associative), the unary operators `! - + ~ typeof await`, `++`/`--`, and
//! `yield` inside generators. Beneath them sit the member, call, `new`, `import(…)`,
//! template, array and object forms, parsed for their structure: their own rules are
//! US-029's and US-030's.
//!
//! Removed operators are E000 naming the replacement and parsed for recovery: `==` and
//! `!=` (fix: `===`, `!==`), `||=`, `&&=` and the comma operator; `in`, `void` and
//! `delete` are reported by the lexer at the word. `??` beside `&&` or `||` without
//! parentheses and a unary operand as the base of `**` (`-2 ** 2`) are E000. `i++`,
//! `i--`, `++i` and `--i` are S012 as a whole expression statement (fix: `i += 1`,
//! `i -= 1`) and E000 wherever else they are a value (§4.3). The left side of `=` is a
//! name, a property or an array or object pattern (the literal is reinterpreted); of a
//! compound assignment, a name or a property; a parenthesised name or property is
//! accepted and is S005.

use sjs_diag::{Code, Span};

use super::{classes, functions, FnContext, Parser};
use crate::token::{Keyword, Punct, TemplatePart, Token, TokenKind};
use crate::tree::{Child, NodeId, NodeKind};

const LPAREN: TokenKind = TokenKind::Punct(Punct::LParen);
const RPAREN: TokenKind = TokenKind::Punct(Punct::RParen);
const LBRACKET: TokenKind = TokenKind::Punct(Punct::LBracket);
const RBRACKET: TokenKind = TokenKind::Punct(Punct::RBracket);
const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);
const COLON: TokenKind = TokenKind::Punct(Punct::Colon);
const SEMICOLON: TokenKind = TokenKind::Punct(Punct::Semicolon);
const DOT: TokenKind = TokenKind::Punct(Punct::Dot);
const QUESTION_DOT: TokenKind = TokenKind::Punct(Punct::QuestionDot);
const QUESTION: TokenKind = TokenKind::Punct(Punct::Question);
const ELLIPSIS: TokenKind = TokenKind::Punct(Punct::Ellipsis);
const EQ: TokenKind = TokenKind::Punct(Punct::Eq);
const STAR: TokenKind = TokenKind::Punct(Punct::Star);
const STAR_STAR: TokenKind = TokenKind::Punct(Punct::StarStar);
const PLUS_PLUS: TokenKind = TokenKind::Punct(Punct::PlusPlus);
const MINUS_MINUS: TokenKind = TokenKind::Punct(Punct::MinusMinus);

const NOT_A_TARGET: &str =
    "a pattern assigns to names and properties: this is neither (SPEC.md §2 `AssignExpr`)";

/// What a parsed expression is, as far as the expression around it needs to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// Nothing: the next token cannot begin an expression. Nothing was consumed or
    /// reported; the caller names what it expected.
    Missing,
    /// An identifier.
    Name,
    /// `a.b` or `a[b]` with no `?.` in its chain: assignable.
    Member,
    /// `(name)` or `(a.b)`: assignable, inside redundant parentheses.
    ParenTarget,
    /// An array literal, which `=` can make an array pattern.
    Array(NodeId),
    /// An object literal, which `=` can make an object pattern.
    Object(NodeId),
    /// A prefix operator or `await` applied to an operand.
    Unary,
    /// `++` or `--`.
    Update,
    /// `a && b` or `a || b`.
    AndOr,
    /// `a ?? b`.
    Nullish,
    /// Anything else.
    Other,
}

/// A parsed expression: its shape and where it is.
#[derive(Debug, Clone, Copy)]
pub(super) struct Expr {
    shape: Shape,
    span: Span,
}

impl Expr {
    /// True when nothing was there.
    pub(super) fn is_missing(self) -> bool {
        self.shape == Shape::Missing
    }
}

/// An update at the start of an expression statement, decided when the statement
/// ends: S012 if it is the whole statement, E000 (a value) otherwise.
#[derive(Debug, Clone)]
pub(super) struct Update {
    span: Span,
    current: String,
    replacement: String,
}

/// How a binary operator combines with `??`, `&&` and `||`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    AndOr,
    Nullish,
    Other,
}

/// Assignment operators by what they accept on their left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assign {
    /// `=`: a name, a property or a pattern.
    Plain,
    /// `+=` and the other compound operators: a name or a property.
    Compound,
    /// `||=` and `&&=`, removed.
    Removed,
}

/// `Expr`: an assignment expression, then the removed comma operator, E000 once and
/// parsed for recovery.
pub(super) fn expression(p: &mut Parser<'_>) -> Expr {
    let marker = p.marker();
    let first = assignment(p);
    if first.is_missing() || p.peek(false).kind != COMMA {
        return first;
    }
    let comma = p.peek(false).span;
    p.error(
        comma,
        "the comma operator is removed: write separate statements (SPEC.md §12)",
    );
    p.start_node_at(marker, NodeKind::SequenceExpr);
    while p.eat(&COMMA) && !assignment(p).is_missing() {}
    p.finish_node();
    p.expr(Shape::Other, first.span)
}

/// `AssignExpr`.
pub(super) fn assignment(p: &mut Parser<'_>) -> Expr {
    assign(p, true)
}

/// An element or property value of an array or object literal: [`assignment`], but a
/// shorthand default (`{ a = 1 }`) inside it stays unsettled, since the enclosing
/// literal may yet become a pattern.
fn element(p: &mut Parser<'_>) -> Expr {
    assign(p, false)
}

fn assign(p: &mut Parser<'_>, settle: bool) -> Expr {
    if !p.enter() {
        p.leave();
        return too_deep(p);
    }
    let covers = p.covers.len();
    let result = assign_inner(p);
    if settle {
        for span in p.covers.split_off(covers) {
            p.error(
                span,
                "`{ a = 1 }` is a pattern's default, not an object property: write `{ a: 1 }` (SPEC.md §2 `ObjectLit`)",
            );
        }
    }
    p.leave();
    result
}

fn assign_inner(p: &mut Parser<'_>) -> Expr {
    let start = p.peek(true).span;
    if p.at_word("yield") {
        if !p.in_generator() {
            p.error(
                start,
                "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)",
            );
        }
        return yield_expr(p);
    }
    if p.at_arrow() {
        functions::arrow(p);
        return p.expr(Shape::Other, start);
    }
    let marker = p.marker();
    let covers = p.covers.len();
    let target = conditional(p);
    let op = p.peek(false).clone();
    let kind = match op.kind {
        TokenKind::Punct(punct) if !target.is_missing() => assignment_kind(punct),
        _ => None,
    };
    let Some(kind) = kind else {
        return target;
    };
    check_target(p, target, kind, &op, covers);
    p.start_node_at(marker, NodeKind::AssignExpr);
    p.bump(false);
    if assignment(p).is_missing() {
        let span = p.peek(false).span;
        let message = format!("expected an expression after `{}`", p.text(&op));
        p.error(span, message);
    }
    p.finish_node();
    p.expr(Shape::Other, start)
}

fn assignment_kind(punct: Punct) -> Option<Assign> {
    match punct {
        Punct::Eq => Some(Assign::Plain),
        Punct::PlusEq
        | Punct::MinusEq
        | Punct::StarEq
        | Punct::SlashEq
        | Punct::PercentEq
        | Punct::StarStarEq
        | Punct::QuestionQuestionEq
        | Punct::ShlEq
        | Punct::ShrEq
        | Punct::UShrEq
        | Punct::AmpEq
        | Punct::PipeEq
        | Punct::CaretEq => Some(Assign::Compound),
        Punct::PipePipeEq | Punct::AmpAmpEq => Some(Assign::Removed),
        _ => None,
    }
}

/// The left side of an assignment: what the operator accepts, S005 for parentheses
/// around a name or property, and a literal before `=` made a pattern.
fn check_target(p: &mut Parser<'_>, target: Expr, kind: Assign, op: &Token, covers: usize) {
    let operator = p.text(op);
    if kind == Assign::Removed {
        let message = format!(
            "`{operator}` is removed: `||` and `&&` take booleans only, so write `??=` for a default or an `if` with an explicit comparison (SPEC.md §12)"
        );
        p.error(op.span, message);
        return;
    }
    match target.shape {
        Shape::Name | Shape::Member => {}
        Shape::ParenTarget => {
            let current = p.file.slice(&target.span).unwrap_or("").to_owned();
            let inner = current
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .unwrap_or(&current)
                .trim()
                .to_owned();
            p.report_with_fix(
                Code::S005,
                target.span,
                "redundant parentheses around an assignment target",
                current,
                inner,
            );
        }
        Shape::Array(node) | Shape::Object(node) if kind == Assign::Plain => {
            // The literal is a pattern after all: its shorthand defaults are its own.
            p.covers.truncate(covers);
            to_pattern(p, node);
        }
        _ => {
            let message = if kind == Assign::Plain {
                "the left side of `=` is a name, a property or a pattern (SPEC.md §2 `AssignExpr`)"
                    .to_owned()
            } else {
                format!(
                    "the left side of `{operator}` is a name or a property (SPEC.md §2 `AssignExpr`)"
                )
            };
            p.error(target.span, message);
        }
    }
}

/// `test ? a : b`.
fn conditional(p: &mut Parser<'_>) -> Expr {
    let marker = p.marker();
    let test = binary(p, 0);
    if test.is_missing() || p.peek(false).kind != QUESTION {
        return test;
    }
    p.start_node_at(marker, NodeKind::CondExpr);
    p.bump(false);
    let consequent = assignment(p);
    let message = if consequent.is_missing() {
        Some("expected an expression after `?`")
    } else if !p.eat(&COLON) {
        Some("expected `:` in the conditional expression")
    } else {
        assignment(p)
            .is_missing()
            .then_some("expected an expression after `:`")
    };
    if let Some(message) = message {
        let span = p.peek(false).span;
        p.error(span, message);
    }
    p.finish_node();
    p.expr(Shape::Other, test.span)
}

/// A binary operator's precedence (higher binds tighter) and class.
fn binary_operator(kind: &TokenKind) -> Option<(u8, Class)> {
    let precedence = match kind {
        TokenKind::Punct(Punct::QuestionQuestion) => return Some((1, Class::Nullish)),
        TokenKind::Punct(Punct::PipePipe) => return Some((2, Class::AndOr)),
        TokenKind::Punct(Punct::AmpAmp) => return Some((3, Class::AndOr)),
        TokenKind::Punct(Punct::Pipe) => 4,
        TokenKind::Punct(Punct::Caret) => 5,
        TokenKind::Punct(Punct::Amp) => 6,
        TokenKind::Punct(Punct::EqEqEq | Punct::BangEqEq | Punct::EqEq | Punct::BangEq) => 7,
        TokenKind::Punct(Punct::Lt | Punct::Gt | Punct::LtEq | Punct::GtEq)
        | TokenKind::Keyword(Keyword::Instanceof | Keyword::In) => 8,
        TokenKind::Punct(Punct::Shl | Punct::Shr | Punct::UShr) => 9,
        TokenKind::Punct(Punct::Plus | Punct::Minus) => 10,
        TokenKind::Punct(Punct::Star | Punct::Slash | Punct::Percent) => 11,
        TokenKind::Punct(Punct::StarStar) => 12,
        _ => return None,
    };
    Some((precedence, Class::Other))
}

/// Operators binding at least as tightly as `min`, by precedence climbing: each is
/// left-associative except `**`.
fn binary(p: &mut Parser<'_>, min: u8) -> Expr {
    let marker = p.marker();
    let mut left = unary(p);
    if left.is_missing() {
        return left;
    }
    loop {
        let op = p.peek(false).clone();
        let Some((precedence, class)) = binary_operator(&op.kind) else {
            break;
        };
        if precedence < min {
            break;
        }
        if op.kind == STAR_STAR && left.shape == Shape::Unary {
            p.error(
                left.span,
                "a unary operator before the base of `**` is ambiguous: parenthesize the base, `(-x) ** 2`, or the power, `-(x ** 2)` (SPEC.md §2)",
            );
        }
        match op.kind {
            TokenKind::Punct(Punct::EqEq) => p.error_with_fix(
                op.span,
                "`==` is removed: it coerces its operands; write `===` (SPEC.md §12)",
                "==".to_owned(),
                "===".to_owned(),
            ),
            TokenKind::Punct(Punct::BangEq) => p.error_with_fix(
                op.span,
                "`!=` is removed: it coerces its operands; write `!==` (SPEC.md §12)",
                "!=".to_owned(),
                "!==".to_owned(),
            ),
            _ => {}
        }
        p.start_node_at(marker, NodeKind::BinaryExpr);
        p.bump(false);
        let next = if op.kind == STAR_STAR {
            precedence
        } else {
            precedence + 1
        };
        // The right operand's own `unary` checks the bound; this level only counts, so a
        // chain of `**` cannot recurse past it unseen.
        p.nesting += 1;
        let right = binary(p, next);
        p.nesting -= 1;
        if right.is_missing() {
            let span = p.peek(false).span;
            let message = format!("expected an expression after `{}`", p.text(&op));
            p.error(span, message);
        }
        let shape = match class {
            Class::AndOr => Shape::AndOr,
            Class::Nullish => Shape::Nullish,
            Class::Other => Shape::Other,
        };
        // `??` binds more loosely than `&&` and `||`, so an unparenthesised mix always
        // puts the `&&`/`||` beneath the `??`.
        let mixed =
            shape == Shape::Nullish && (left.shape == Shape::AndOr || right.shape == Shape::AndOr);
        if mixed {
            p.error(
                op.span,
                "`??` beside `&&` or `||` needs parentheses, as in `(a ?? b) || c` (SPEC.md §4.1)",
            );
        }
        p.finish_node();
        left = p.expr(shape, left.span);
    }
    left
}

/// A prefix operator, `++`/`--`, `await`, or a postfix expression.
fn unary(p: &mut Parser<'_>) -> Expr {
    if !p.enter() {
        p.leave();
        return too_deep(p);
    }
    let result = unary_inner(p);
    p.leave();
    result
}

fn unary_inner(p: &mut Parser<'_>) -> Expr {
    let token = p.peek(true).clone();
    match token.kind {
        TokenKind::Punct(Punct::Bang | Punct::Minus | Punct::Plus | Punct::Tilde)
        | TokenKind::Keyword(Keyword::Typeof | Keyword::Void | Keyword::Delete) => {
            // `void` and `delete` are removed; the lexer has reported the word.
            p.start_node(NodeKind::UnaryExpr);
            p.bump(false);
            operand(p, &token);
            p.finish_node();
            p.expr(Shape::Unary, token.span)
        }
        TokenKind::Punct(Punct::PlusPlus | Punct::MinusMinus) => {
            let index = p.marker().token_index();
            p.start_node(NodeKind::UpdateExpr);
            p.bump(false);
            let target = unary(p);
            if target.is_missing() {
                let span = p.peek(false).span;
                let message = format!("expected an expression after `{}`", p.text(&token));
                p.error(span, message);
            } else {
                let span = p.span_from(token.span);
                p.update(index, &token, target, span);
            }
            p.finish_node();
            p.expr(Shape::Update, token.span)
        }
        TokenKind::Ident if p.at_word("await") => {
            if !p.await_allowed() {
                p.error(
                    token.span,
                    "`await` is permitted only inside an `async` function or at the top level of a module (SPEC.md §5.1)",
                );
            }
            p.start_node(NodeKind::AwaitExpr);
            p.bump(false);
            operand(p, &token);
            p.finish_node();
            p.expr(Shape::Unary, token.span)
        }
        _ => postfix(p),
    }
}

/// The operand of the prefix `operator` just consumed.
fn operand(p: &mut Parser<'_>, operator: &Token) {
    if unary(p).is_missing() {
        let span = p.peek(false).span;
        let message = format!("expected an expression after `{}`", p.text(operator));
        p.error(span, message);
    }
}

/// A member, call or `new` chain, then a postfix `++`/`--` on the same line (a line
/// break before it ends the expression, as ASI's restricted production says).
fn postfix(p: &mut Parser<'_>) -> Expr {
    let marker = p.marker();
    let target = chain(p, true);
    let next = p.peek(false).clone();
    if target.is_missing()
        || !matches!(next.kind, PLUS_PLUS | MINUS_MINUS)
        || next.has_newline_before
    {
        return target;
    }
    p.start_node_at(marker, NodeKind::UpdateExpr);
    p.bump(false);
    let span = p.span_from(target.span);
    p.update(marker.token_index(), &next, target, span);
    p.finish_node();
    p.expr(Shape::Update, target.span)
}

/// A primary expression and its `.name`, `?.`, `[index]`, call and tagged-template
/// links. Without `calls` (a `new` callee) only `.name` and `[index]` follow.
fn chain(p: &mut Parser<'_>, calls: bool) -> Expr {
    let marker = p.marker();
    let start = primary(p);
    if start.is_missing() {
        return start;
    }
    let mut shape = start.shape;
    let mut optional = false;
    loop {
        let token = p.peek(false).clone();
        match token.kind {
            DOT => {
                p.start_node_at(marker, NodeKind::MemberExpr);
                p.bump(false);
                member_name(p);
            }
            LBRACKET => {
                p.start_node_at(marker, NodeKind::IndexExpr);
                p.bump(false);
                index(p);
            }
            QUESTION_DOT if calls => {
                optional = true;
                match p.peek_second().kind {
                    LBRACKET => {
                        p.start_node_at(marker, NodeKind::IndexExpr);
                        p.bump(false);
                        p.bump(false);
                        index(p);
                    }
                    LPAREN => {
                        p.start_node_at(marker, NodeKind::CallExpr);
                        p.bump(false);
                        arguments(p);
                    }
                    _ => {
                        p.start_node_at(marker, NodeKind::MemberExpr);
                        p.bump(false);
                        member_name(p);
                    }
                }
            }
            LPAREN if calls => {
                p.start_node_at(marker, NodeKind::CallExpr);
                arguments(p);
            }
            TokenKind::Template(ref template)
                if calls && matches!(template.part, TemplatePart::Whole | TemplatePart::Head) =>
            {
                p.start_node_at(marker, NodeKind::TaggedTemplate);
                if template.part == TemplatePart::Head {
                    template_literal(p);
                } else {
                    p.bump(false);
                }
            }
            _ => break,
        }
        p.finish_node();
        shape = match token.kind {
            DOT | LBRACKET if !optional => Shape::Member,
            _ => Shape::Other,
        };
    }
    p.expr(shape, start.span)
}

/// The name after `.` or `?.`: any identifier or keyword, or a private name.
fn member_name(p: &mut Parser<'_>) {
    let token = p.peek(false).clone();
    match token.kind {
        TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Bool(_) | TokenKind::Null => {
            p.bump(false);
        }
        TokenKind::PrivateName(ref name) => {
            p.note_private_reference(name);
            p.bump(false);
        }
        _ => p.error(token.span, "expected a property name"),
    }
}

/// `expr ]` after an index's `[`.
fn index(p: &mut Parser<'_>) {
    if expression(p).is_missing() {
        let span = p.peek(false).span;
        p.error(span, "expected an expression in `[ … ]`");
        close(p, &RBRACKET);
    } else if !p.eat(&RBRACKET) {
        let span = p.peek(false).span;
        p.error(span, "expected `]`");
        close(p, &RBRACKET);
    }
}

/// Recovery inside a bracketed form that has reported: through its `closer`, so what
/// is left inside it cannot cascade outside.
fn close(p: &mut Parser<'_>, closer: &TokenKind) {
    p.skip_until(closer);
    p.eat(closer);
}

/// `( arg, ...spread, )` at its `(`.
fn arguments(p: &mut Parser<'_>) {
    p.bump(false);
    loop {
        let token = p.peek(true).clone();
        if p.eat(&RPAREN) {
            return;
        }
        let argument = if token.kind == ELLIPSIS {
            spread(p, false)
        } else {
            assignment(p)
        };
        if argument.is_missing() {
            p.error(token.span, "expected an argument or `)`");
            close(p, &RPAREN);
            return;
        }
        if !p.eat(&COMMA) && p.peek(false).kind != RPAREN {
            let span = p.peek(false).span;
            p.error(span, "expected `,` or `)` in the argument list");
            close(p, &RPAREN);
            return;
        }
    }
}

/// `...expr` at its `...`; in a literal, the spread may yet be a pattern's rest.
fn spread(p: &mut Parser<'_>, in_literal: bool) -> Expr {
    p.start_node(NodeKind::SpreadElement);
    let dots = p.bump(false).span;
    let argument = if in_literal {
        element(p)
    } else {
        assignment(p)
    };
    if argument.is_missing() {
        let span = p.peek(false).span;
        p.error(span, "expected an expression after `...`");
    }
    p.finish_node();
    p.expr(Shape::Other, dots)
}

/// The innermost forms: literals, names, `this`, `super`, parentheses, arrays,
/// objects, templates, `new`, `import(…)`, and the removed `function` and `class`
/// expressions (E000, parsed).
fn primary(p: &mut Parser<'_>) -> Expr {
    let token = p.peek(true).clone();
    let shape = match token.kind {
        TokenKind::Number(_)
        | TokenKind::BigInt(_)
        | TokenKind::String(_)
        | TokenKind::Bool(_)
        | TokenKind::Null
        | TokenKind::Regex(_)
        | TokenKind::Keyword(Keyword::This) => {
            p.bump(false);
            Shape::Other
        }
        TokenKind::Template(ref template) if template.part == TemplatePart::Whole => {
            p.bump(false);
            Shape::Other
        }
        TokenKind::Template(ref template) if template.part == TemplatePart::Head => {
            template_literal(p);
            Shape::Other
        }
        TokenKind::Ident => return name(p, &token),
        TokenKind::Error => {
            // The lexer has reported the character; what is glued to it is the same
            // broken name.
            p.bump(false);
            p.absorb_glued();
            Shape::Other
        }
        TokenKind::PrivateName(ref name) => {
            p.note_private_reference(name);
            p.bump(false);
            Shape::Other
        }
        TokenKind::Keyword(Keyword::Super) => {
            p.check_super();
            p.bump(false);
            Shape::Other
        }
        TokenKind::Keyword(Keyword::Function) => {
            p.removed_function_expression();
            Shape::Other
        }
        TokenKind::Keyword(Keyword::Class) if classes::at_class(p) => {
            p.removed_class_expression();
            Shape::Other
        }
        TokenKind::Keyword(Keyword::New) => {
            new_expr(p);
            Shape::Other
        }
        TokenKind::Keyword(Keyword::Import) => {
            import_expr(p);
            Shape::Other
        }
        TokenKind::Keyword(Keyword::Arguments | Keyword::Eval) => {
            // Removed words the lexer has reported; they still name something.
            p.bump(false);
            Shape::Name
        }
        TokenKind::Keyword(
            Keyword::Enum
            | Keyword::Implements
            | Keyword::Interface
            | Keyword::Package
            | Keyword::Private
            | Keyword::Protected
            | Keyword::Public,
        ) => {
            let message = format!("`{}` is reserved and cannot be a name", p.text(&token));
            p.error(token.span, message);
            p.bump(false);
            Shape::Name
        }
        LPAREN => return paren(p),
        LBRACKET => return array(p),
        LBRACE => return object(p),
        _ => {
            return Expr {
                shape: Shape::Missing,
                span: token.span,
            }
        }
    };
    p.expr(shape, token.span)
}

/// An identifier at an operand: a name, or `async function` (removed), an arrow that
/// needs parentheses here, or a `yield` that does.
fn name(p: &mut Parser<'_>, token: &Token) -> Expr {
    if functions::at_function(p) {
        p.removed_function_expression();
        return p.expr(Shape::Other, token.span);
    }
    if p.at_arrow() {
        p.error(
            token.span,
            "an arrow function inside an operand needs parentheses: `(() => x)` (SPEC.md §2 `ArrowFn`)",
        );
        functions::arrow(p);
        return p.expr(Shape::Other, token.span);
    }
    if p.at_word("yield") {
        let message = if p.in_generator() {
            "a `yield` inside an operand needs parentheses: `(yield x)` (SPEC.md §2 `AssignExpr`)"
        } else {
            "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)"
        };
        p.error(token.span, message);
        return yield_expr(p);
    }
    p.bump(false);
    p.absorb_glued();
    p.expr(Shape::Name, token.span)
}

/// `yield`, `yield operand` or `yield* operand`, at `yield`. A line break after
/// `yield`, or a token that cannot begin an operand, leaves it without one.
fn yield_expr(p: &mut Parser<'_>) -> Expr {
    let start = p.peek(false).span;
    p.start_node(NodeKind::YieldExpr);
    p.bump(false);
    let next = p.peek(true).clone();
    let delegate = next.kind == STAR && !next.has_newline_before;
    if delegate {
        p.bump(false);
    }
    let ends = matches!(
        next.kind,
        RPAREN | RBRACKET | RBRACE | COMMA | SEMICOLON | COLON | TokenKind::Eof
    );
    if (delegate || (!next.has_newline_before && !ends)) && assignment(p).is_missing() {
        let span = p.peek(false).span;
        let message = if delegate {
            "expected an expression after `yield*`"
        } else {
            "expected an expression after `yield`"
        };
        p.error(span, message);
    }
    p.finish_node();
    p.expr(Shape::Other, start)
}

/// `( expr )`, or an arrow function where one needs parentheses.
fn paren(p: &mut Parser<'_>) -> Expr {
    let open = p.peek(false).span;
    if p.at_arrow() {
        p.error(
            open,
            "an arrow function inside an operand needs parentheses: `(() => x)` (SPEC.md §2 `ArrowFn`)",
        );
        functions::arrow(p);
        return p.expr(Shape::Other, open);
    }
    p.start_node(NodeKind::ParenExpr);
    p.bump(false);
    let inner = expression(p);
    if inner.is_missing() {
        let span = p.peek(false).span;
        p.error(span, "expected an expression in `( … )`");
        close(p, &RPAREN);
    } else if !p.eat(&RPAREN) {
        let span = p.peek(false).span;
        p.error(span, "expected `)`");
        close(p, &RPAREN);
    }
    p.finish_node();
    let shape = match inner.shape {
        Shape::Name | Shape::Member | Shape::ParenTarget => Shape::ParenTarget,
        _ => Shape::Other,
    };
    p.expr(shape, open)
}

/// `[ element, ...spread, ]`. A hole (`[a, , b]`) is left for US-030 to report.
fn array(p: &mut Parser<'_>) -> Expr {
    let node = p.start_node(NodeKind::ArrayLit);
    let open = p.bump(false).span;
    loop {
        let token = p.peek(true).clone();
        match token.kind {
            RBRACKET => {
                p.bump(false);
                break;
            }
            COMMA => {
                p.bump(false);
                continue;
            }
            _ => {}
        }
        let item = if token.kind == ELLIPSIS {
            spread(p, true)
        } else {
            element(p)
        };
        if item.is_missing() {
            p.error(token.span, "expected an element, `,` or `]`");
            close(p, &RBRACKET);
            break;
        }
        if !p.eat(&COMMA) && p.peek(false).kind != RBRACKET {
            let span = p.peek(false).span;
            p.error(span, "expected `,` or `]` in the array");
            close(p, &RBRACKET);
            break;
        }
    }
    p.finish_node();
    p.expr(Shape::Array(node), open)
}

/// `{ key: value, shorthand, ...spread, }`. Methods, accessors and the other
/// object-literal rules are US-030's.
fn object(p: &mut Parser<'_>) -> Expr {
    let node = p.start_node(NodeKind::ObjectLit);
    let open = p.bump(false).span;
    loop {
        let token = p.peek(false).clone();
        if token.kind == RBRACE {
            p.bump(false);
            break;
        }
        let complete = if token.kind == ELLIPSIS {
            spread(p, true);
            true
        } else {
            p.start_node(NodeKind::Property);
            let complete = property(p, &token);
            p.finish_node();
            complete
        };
        if !complete {
            close(p, &RBRACE);
            break;
        }
        if !p.eat(&COMMA) && p.peek(false).kind != RBRACE {
            let span = p.peek(false).span;
            p.error(span, "expected `,` or `}` in the object");
            close(p, &RBRACE);
            break;
        }
    }
    p.finish_node();
    p.expr(Shape::Object(node), open)
}

/// One property at its first token. False when it is malformed (reported).
fn property(p: &mut Parser<'_>, token: &Token) -> bool {
    match token.kind {
        TokenKind::Ident => {
            p.bump(false);
            if p.peek(false).kind == EQ {
                // `{ a = 1 }`: a pattern's default, settled by the enclosing literal.
                p.bump(false);
                if assignment(p).is_missing() {
                    let span = p.peek(false).span;
                    p.error(span, "expected an expression after `=`");
                    return false;
                }
                let span = p.span_from(token.span);
                p.covers.push(span);
                return true;
            }
            if p.peek(false).kind != COLON {
                return true;
            }
        }
        TokenKind::Keyword(_)
        | TokenKind::String(_)
        | TokenKind::Number(_)
        | TokenKind::BigInt(_)
        | TokenKind::Bool(_)
        | TokenKind::Null => {
            p.bump(false);
        }
        LBRACKET => {
            p.bump(false);
            if assignment(p).is_missing() {
                let span = p.peek(false).span;
                p.error(span, "expected an expression in `[ … ]`");
                return false;
            }
            if !p.eat(&RBRACKET) {
                let span = p.peek(false).span;
                p.error(span, "expected `]` after the computed key");
                return false;
            }
        }
        _ => {
            p.error(
                token.span,
                "expected a property: a name, a string, a number, `[key]` or `...spread`",
            );
            return false;
        }
    }
    if !p.eat(&COLON) {
        let span = p.peek(false).span;
        p.error(span, "expected `:` after the property key");
        return false;
    }
    if element(p).is_missing() {
        let span = p.peek(false).span;
        p.error(span, "expected an expression after `:`");
        return false;
    }
    true
}

/// A template with substitutions, at its head: `` `a${x}b${y}c` ``.
fn template_literal(p: &mut Parser<'_>) {
    p.start_node(NodeKind::TemplateLit);
    p.bump(false);
    loop {
        if expression(p).is_missing() {
            let span = p.peek(false).span;
            p.error(span, "expected an expression in `${ … }`");
        }
        let part = match p.peek(false).kind {
            TokenKind::Template(ref template) => Some(template.part),
            _ => None,
        };
        match part {
            Some(TemplatePart::Middle) => {
                p.bump(false);
            }
            Some(TemplatePart::Tail) => {
                p.bump(false);
                break;
            }
            _ => {
                let span = p.peek(false).span;
                p.error(span, "expected `}` to close the template substitution");
                break;
            }
        }
    }
    p.finish_node();
}

/// `new Callee(args)`, structurally (US-029 owns its rules), or the removed
/// `new.target`.
fn new_expr(p: &mut Parser<'_>) {
    let node = p.start_node(NodeKind::NewExpr);
    p.bump(false);
    if p.peek(false).kind == DOT {
        p.retag(node, NodeKind::MetaProperty);
        p.bump(false);
        member_name(p);
        p.finish_node();
        return;
    }
    let callee = if p.enter() {
        chain(p, false)
    } else {
        too_deep(p)
    };
    p.leave();
    if callee.is_missing() {
        let span = p.peek(false).span;
        p.error(span, "expected a class after `new`");
    } else if p.peek(false).kind == LPAREN {
        arguments(p);
    }
    p.finish_node();
}

/// `import(…)`, structurally (US-029 owns its rules), or the removed `import.meta`.
fn import_expr(p: &mut Parser<'_>) {
    let node = p.start_node(NodeKind::ImportCall);
    p.bump(false);
    match p.peek(false).kind {
        LPAREN => arguments(p),
        DOT => {
            p.retag(node, NodeKind::MetaProperty);
            p.bump(false);
            member_name(p);
        }
        _ => {
            let span = p.peek(false).span;
            p.error(span, "expected `(` after `import`");
        }
    }
    p.finish_node();
}

/// Past the nesting bound: the rest of the block is skipped (reported once).
fn too_deep(p: &mut Parser<'_>) -> Expr {
    p.skip_rest_of_block();
    Expr {
        shape: Shape::Other,
        span: p.peek(false).span,
    }
}

/// An expression statement: one expression, then a `;` or a line break. An update
/// that is the whole statement is S012 with the `+=`/`-=` replacement; one inside it
/// is a value (E000).
pub(super) fn expression_statement(p: &mut Parser<'_>) {
    p.start_node(NodeKind::ExprStmt);
    let start = p.marker().token_index();
    let outer = (p.statement_start.replace(start), p.statement_update.take());
    let first = p.peek(true).clone();
    let expr = expression(p);
    if expr.is_missing() {
        let message = format!("expected a statement, not `{}`", p.text(&first));
        p.error(first.span, message);
        p.bump(false);
    }
    if let Some(update) = p.statement_update.take() {
        if expr.shape == Shape::Update {
            p.report_with_fix(
                Code::S012,
                update.span,
                "an update as a statement: the canonical spelling is a compound assignment (SPEC.md §8.5)",
                update.current,
                update.replacement,
            );
        } else {
            p.value_update(&update);
        }
    }
    (p.statement_start, p.statement_update) = outer;
    p.terminate();
    p.finish_node();
}

/// Makes the array or object literal `node` the pattern on the left of `=`: it and its
/// nested literals, spreads and properties are retagged, and every target checked.
fn to_pattern(p: &mut Parser<'_>, node: NodeId) {
    let children = p.builder.children_of(node).to_vec();
    let is_array = p.builder.kind_of(node) == NodeKind::ArrayLit;
    p.retag(
        node,
        if is_array {
            NodeKind::ArrayPattern
        } else {
            NodeKind::ObjectPattern
        },
    );
    let items: Vec<Child> = children
        .into_iter()
        .filter(|child| match child {
            Child::Token(id) => !matches!(
                p.builder.token_ref(*id).kind,
                LBRACKET | RBRACKET | LBRACE | RBRACE | COMMA
            ),
            Child::Node(_) => true,
        })
        .collect();
    let count = items.len();
    for (at, item) in items.into_iter().enumerate() {
        let last = at + 1 == count;
        match item {
            Child::Node(inner) if p.builder.kind_of(inner) == NodeKind::Property => {
                pattern_property(p, inner);
            }
            Child::Node(inner) if p.builder.kind_of(inner) == NodeKind::SpreadElement => {
                rest(p, inner, last, is_array);
            }
            _ => target(p, item),
        }
    }
}

/// A property of an object pattern: shorthand (`a`, `a = 1`) names its target; `key:
/// target` holds one.
fn pattern_property(p: &mut Parser<'_>, node: NodeId) {
    p.retag(node, NodeKind::PatternProperty);
    let children = p.builder.children_of(node).to_vec();
    let colon = children.iter().position(
        |child| matches!(child, Child::Token(id) if p.builder.token_ref(*id).kind == COLON),
    );
    if let Some(value) = colon.and_then(|at| children.get(at + 1)) {
        target(p, *value);
    }
}

/// `...target`: last, and in an object pattern a name or a property.
fn rest(p: &mut Parser<'_>, node: NodeId, last: bool, in_array: bool) {
    p.retag(node, NodeKind::RestElement);
    let span = p.builder.span_of(node);
    if !last {
        p.error(span, "a rest element must be last");
        return;
    }
    let Some(argument) = p.builder.children_of(node).get(1).copied() else {
        return;
    };
    let nested = matches!(argument, Child::Node(inner)
        if matches!(p.builder.kind_of(inner), NodeKind::ArrayLit | NodeKind::ObjectLit));
    if nested && !in_array {
        p.error(
            span,
            "the rest of an object pattern is a name or a property",
        );
        return;
    }
    target(p, argument);
}

/// One target of a pattern: a name, a property, a nested pattern, or any of these with
/// a `= default`.
fn target(p: &mut Parser<'_>, child: Child) {
    match child {
        Child::Token(id) => {
            let token = p.builder.token_ref(id).clone();
            // An error token's character was reported by the lexer.
            if !matches!(token.kind, TokenKind::Ident | TokenKind::Error) {
                p.error(token.span, NOT_A_TARGET);
            }
        }
        Child::Node(node) => match p.builder.kind_of(node) {
            NodeKind::ArrayLit | NodeKind::ObjectLit => to_pattern(p, node),
            NodeKind::MemberExpr | NodeKind::IndexExpr if !p.optional(node) => {}
            NodeKind::ParenExpr if p.paren_target(node) => {}
            NodeKind::AssignExpr if p.is_default(node) => {
                let first = p.builder.children_of(node)[0];
                target(p, first);
            }
            _ => {
                let span = p.builder.span_of(node);
                p.error(span, NOT_A_TARGET);
            }
        },
    }
}

impl Parser<'_> {
    /// An expression of `shape` from `start` to the last token consumed.
    fn expr(&self, shape: Shape, start: Span) -> Expr {
        Expr {
            shape,
            span: self.span_from(start),
        }
    }

    /// From `start`, a token already consumed, to the end of the last token consumed.
    pub(super) fn span_from(&self, start: Span) -> Span {
        self.last.map_or(start, |last| {
            Span::new(start.start, last.end, start.byte_start, last.byte_end)
        })
    }

    /// Consumes the tokens glued to the last one without trivia between: the pieces of
    /// one name the lexer split at a character it reported (`$a`, `café`).
    fn absorb_glued(&mut self) {
        loop {
            let next = self.peek(false).clone();
            let glued = self
                .last
                .is_some_and(|last| last.byte_end == next.span.byte_start);
            if !glued
                || !matches!(
                    next.kind,
                    TokenKind::Ident | TokenKind::Error | TokenKind::Number(_)
                )
            {
                return;
            }
            self.bump(false);
        }
    }

    /// Whether the innermost function is a generator.
    pub(super) fn in_generator(&self) -> bool {
        matches!(
            self.contexts.last(),
            Some(
                FnContext::Function {
                    is_generator: true,
                    ..
                } | FnContext::Method {
                    is_generator: true,
                    ..
                }
            )
        )
    }

    /// `++`/`--` (`op`) on `target`, the whole spanning `span` from token number
    /// `index`: an operand that is not a name or a property is E000; at the start of
    /// an expression statement the verdict waits for the statement's end; elsewhere it
    /// is a value, E000.
    fn update(&mut self, index: usize, op: &Token, target: Expr, span: Span) {
        if !matches!(
            target.shape,
            Shape::Name | Shape::Member | Shape::ParenTarget
        ) {
            let message = format!("the operand of `{}` is a name or a property", self.text(op));
            self.error(target.span, message);
            return;
        }
        let current = self.file.slice(&span).unwrap_or("").to_owned();
        let operand = self.file.slice(&target.span).unwrap_or("");
        let assign = if op.kind == PLUS_PLUS { "+=" } else { "-=" };
        let update = Update {
            span,
            current,
            replacement: format!("{operand} {assign} 1"),
        };
        if self.statement_start == Some(index) {
            self.statement_update = Some(update);
        } else {
            self.value_update(&update);
        }
    }

    /// E000 for an update used as a value.
    fn value_update(&mut self, update: &Update) {
        let message = format!(
            "`{}` as a value is removed: write `{}` on its own line; the rewrite would change this expression's result (SPEC.md §4.3)",
            update.current, update.replacement
        );
        self.error(update.span, message);
    }

    /// Whether a member, index or call node has `?.` anywhere along its chain: in its
    /// own link or in the links of its object, down to the chain's first token.
    fn optional(&self, mut node: NodeId) -> bool {
        loop {
            let children = self.builder.children_of(node);
            let own = children.iter().any(|child| {
                matches!(child, Child::Token(id) if self.builder.token_ref(*id).kind == QUESTION_DOT)
            });
            if own {
                return true;
            }
            // The object of a link is its first child; a chain ends at a token there,
            // so `(a?.b).c` is a member of a parenthesised value, not of the chain.
            match children.first() {
                Some(Child::Node(inner)) => node = *inner,
                _ => return false,
            }
        }
    }

    /// Whether a parenthesised expression holds a name or a property (`(a)`, `(a.b)`).
    fn paren_target(&self, node: NodeId) -> bool {
        let inner = self.builder.children_of(node).get(1).copied();
        if let Some(Child::Node(inner)) = inner {
            return match self.builder.kind_of(inner) {
                NodeKind::MemberExpr | NodeKind::IndexExpr => !self.optional(inner),
                NodeKind::ParenExpr => self.paren_target(inner),
                _ => false,
            };
        }
        matches!(inner, Some(Child::Token(id)) if self.builder.token_ref(id).kind == TokenKind::Ident)
    }

    /// Whether an assignment node is `target = default` (not a compound operator).
    fn is_default(&self, node: NodeId) -> bool {
        matches!(
            self.builder.children_of(node).get(1),
            Some(Child::Token(id)) if self.builder.token_ref(*id).kind == EQ
        )
    }
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

    fn fixes(text: &str) -> Vec<(Code, Option<String>, Option<String>)> {
        parse(&file(text))
            .1
            .iter()
            .map(|d: &Diagnostic| (d.code, d.current.clone(), d.replacement.clone()))
            .collect()
    }

    fn clean(text: &str) -> String {
        let found = diagnostics(text);
        assert_eq!(found, Vec::new(), "{text}");
        sexp(text)
    }

    /// The tree of `x = text`'s right side.
    fn value(text: &str) -> String {
        let tree = clean(&format!("x = {text}"));
        let inner = tree
            .strip_prefix("(Module (ExprStmt (AssignExpr x = ")
            .and_then(|rest| rest.strip_suffix(")) <eof>)"))
            .unwrap_or(&tree);
        inner.to_owned()
    }

    /// Each text yields exactly one diagnostic: `code` at `line:column`, whose message
    /// contains `expected`.
    fn check_one(cases: &[(&str, Code, u32, u32, &str)]) {
        for (text, code, line, column, expected) in cases {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            let message = &found[0].3;
            assert_eq!(
                (found[0].0, found[0].1, found[0].2),
                (*code, *line, *column),
                "{text}: {message}"
            );
            assert!(message.contains(expected), "{text}: {message}");
        }
    }

    /// The binary operator levels from loosest to tightest, each with its operators.
    const LEVELS: [&[&str]; 11] = [
        &["||"],
        &["&&"],
        &["|"],
        &["^"],
        &["&"],
        &["===", "!=="],
        &["<", ">", "<=", ">=", "instanceof"],
        &["<<", ">>", ">>>"],
        &["+", "-"],
        &["*", "/", "%"],
        &["**"],
    ];

    #[test]
    fn every_adjacent_pair_of_precedence_levels_groups_the_tighter_first() {
        let mut cases = 0;
        for pair in LEVELS.windows(2) {
            for loose in pair[0] {
                for tight in pair[1] {
                    assert_eq!(
                        value(&format!("a {loose} b {tight} c")),
                        format!("(BinaryExpr a {loose} (BinaryExpr b {tight} c))"),
                        "{loose} then {tight}"
                    );
                    assert_eq!(
                        value(&format!("a {tight} b {loose} c")),
                        format!("(BinaryExpr (BinaryExpr a {tight} b) {loose} c)"),
                        "{tight} then {loose}"
                    );
                    cases += 2;
                }
            }
        }
        for level in LEVELS {
            for op in level {
                let grouped = if *op == "**" {
                    format!("(BinaryExpr a {op} (BinaryExpr b {op} c))")
                } else {
                    format!("(BinaryExpr (BinaryExpr a {op} b) {op} c)")
                };
                assert_eq!(value(&format!("a {op} b {op} c")), grouped, "{op} twice");
                cases += 1;
            }
        }
        assert!(cases >= 60, "{cases} cases");
    }

    #[test]
    fn the_levels_above_and_below_the_binary_operators_group_as_es_says() {
        for (text, expected) in [
            // A unary operand binds tighter than any binary operator except `**`'s
            // right side, and `await`/`typeof` nest.
            ("-a * b", "(BinaryExpr (UnaryExpr - a) * b)"),
            ("!a && b", "(BinaryExpr (UnaryExpr ! a) && b)"),
            ("a ** -b", "(BinaryExpr a ** (UnaryExpr - b))"),
            ("typeof typeof x", "(UnaryExpr typeof (UnaryExpr typeof x))"),
            ("await a + b", "(BinaryExpr (AwaitExpr await a) + b)"),
            ("~a.b", "(UnaryExpr ~ (MemberExpr a . b))"),
            // Members and calls bind tightest; `new A().b` is `(new A()).b`.
            ("new A().b", "(MemberExpr (NewExpr new A ( )) . b)"),
            ("new A.b(c)", "(NewExpr new (MemberExpr A . b) ( c ))"),
            (
                "a.b(c)[d]",
                "(IndexExpr (CallExpr (MemberExpr a . b) ( c )) [ d ])",
            ),
            // `??` below `||`, and the conditional below `??`, right-associative.
            ("a ?? b ?? c", "(BinaryExpr (BinaryExpr a ?? b) ?? c)"),
            ("a ?? b ? c : d", "(CondExpr (BinaryExpr a ?? b) ? c : d)"),
            ("a || b ? c : d", "(CondExpr (BinaryExpr a || b) ? c : d)"),
            (
                "a ? b : c ? d : e",
                "(CondExpr a ? b : (CondExpr c ? d : e))",
            ),
            (
                "a ? b ? c : d : e",
                "(CondExpr a ? (CondExpr b ? c : d) : e)",
            ),
            // Assignment is right-associative and lowest.
            ("a = b ? c : d", "(AssignExpr a = (CondExpr b ? c : d))"),
            (
                "(x ?? (y || z))",
                "(ParenExpr ( (BinaryExpr x ?? (ParenExpr ( (BinaryExpr y || z) ))) ))",
            ),
            (
                "(a ?? b) || c",
                "(BinaryExpr (ParenExpr ( (BinaryExpr a ?? b) )) || c)",
            ),
            (
                "(-1) ** 2",
                "(BinaryExpr (ParenExpr ( (UnaryExpr - 1) )) ** 2)",
            ),
        ] {
            assert_eq!(value(text), expected, "{text}");
        }
        assert_eq!(
            clean("a = b = c"),
            "(Module (ExprStmt (AssignExpr a = (AssignExpr b = c))) <eof>)"
        );
        assert_eq!(
            value("a < b < c"),
            "(BinaryExpr (BinaryExpr a < b) < c)",
            "relational operators are left-associative"
        );
    }

    #[test]
    fn every_assignment_operator_parses_and_the_removed_ones_are_e000() {
        for op in [
            "=", "+=", "-=", "*=", "/=", "%=", "**=", "??=", "<<=", ">>=", ">>>=", "&=", "|=", "^=",
        ] {
            assert_eq!(
                clean(&format!("a {op} b")),
                format!("(Module (ExprStmt (AssignExpr a {op} b)) <eof>)"),
                "{op}"
            );
            assert_eq!(
                clean(&format!("a.b {op} c")),
                format!("(Module (ExprStmt (AssignExpr (MemberExpr a . b) {op} c)) <eof>)"),
                "{op} on a property"
            );
        }
        check_one(&[
            ("a ||= b", Code::E000, 1, 3, "`||=` is removed"),
            ("a &&= b", Code::E000, 1, 3, "`&&=` is removed"),
        ]);
    }

    #[test]
    fn removed_operators_are_e000_naming_the_replacement() {
        check_one(&[
            ("x = a == b", Code::E000, 1, 7, "`==` is removed"),
            ("x = a != b", Code::E000, 1, 7, "`!=` is removed"),
            ("a, b", Code::E000, 1, 2, "the comma operator is removed"),
            (
                "x = (a, b)",
                Code::E000,
                1,
                7,
                "the comma operator is removed",
            ),
            (
                "x = a ?? b || c",
                Code::E000,
                1,
                7,
                "`??` beside `&&` or `||`",
            ),
            (
                "x = a || b ?? c",
                Code::E000,
                1,
                12,
                "`??` beside `&&` or `||`",
            ),
            (
                "x = a && b ?? c",
                Code::E000,
                1,
                12,
                "`??` beside `&&` or `||`",
            ),
            (
                "x = a ?? b && c",
                Code::E000,
                1,
                7,
                "`??` beside `&&` or `||`",
            ),
            ("x = -2 ** 2", Code::E000, 1, 5, "the base of `**`"),
            ("x = typeof a ** 2", Code::E000, 1, 5, "the base of `**`"),
            ("x = a in b", Code::E000, 1, 7, "`in` is removed"),
            ("x = void a", Code::E000, 1, 5, "`void` is removed"),
            ("delete a.b", Code::E000, 1, 1, "`delete` is removed"),
        ]);
        assert_eq!(
            fixes("x = a == b"),
            vec![(Code::E000, Some("==".to_owned()), Some("===".to_owned()))]
        );
        assert_eq!(
            fixes("x = a != b"),
            vec![(Code::E000, Some("!=".to_owned()), Some("!==".to_owned()))]
        );
        assert_eq!(
            sexp("a, b, c"),
            "(Module (ExprStmt (SequenceExpr a , b , c)) <eof>)",
            "the comma operator is parsed, reported once"
        );
        assert_eq!(
            value("a ** b ** c"),
            "(BinaryExpr a ** (BinaryExpr b ** c))"
        );
        for text in [
            "x = (a ?? b) || c",
            "x = a ?? (b || c)",
            "x = (-2) ** 2",
            "x = 2 ** -2",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
    }

    #[test]
    fn an_update_is_s012_as_a_statement_and_e000_as_a_value() {
        for (text, current, replacement) in [
            ("i++", "i++", "i += 1"),
            ("i--", "i--", "i -= 1"),
            ("++i", "++i", "i += 1"),
            ("--i", "--i", "i -= 1"),
            ("a.b++", "a.b++", "a.b += 1"),
            ("a[0]--;", "a[0]--", "a[0] -= 1"),
        ] {
            assert_eq!(
                fixes(text),
                vec![(
                    Code::S012,
                    Some(current.to_owned()),
                    Some(replacement.to_owned())
                )],
                "{text}"
            );
        }
        assert_eq!(
            sexp("i++\n--j"),
            "(Module (ExprStmt (UpdateExpr i ++)) (ExprStmt (UpdateExpr -- j)) <eof>)"
        );
        check_one(&[
            ("x = i++", Code::E000, 1, 5, "`i++` as a value is removed"),
            ("f(--i)", Code::E000, 1, 3, "`--i` as a value is removed"),
            ("i++ + 1", Code::E000, 1, 1, "`i++` as a value is removed"),
            ("x = -i++", Code::E000, 1, 6, "`i++` as a value is removed"),
            (
                "1++",
                Code::E000,
                1,
                1,
                "the operand of `++` is a name or a property",
            ),
            (
                "x = ++f()",
                Code::E000,
                1,
                7,
                "the operand of `++` is a name or a property",
            ),
            (
                "x = a?.b++",
                Code::E000,
                1,
                5,
                "the operand of `++` is a name or a property",
            ),
            (
                "x = ++",
                Code::E000,
                1,
                7,
                "expected an expression after `++`",
            ),
        ]);
        assert_eq!(
            sexp("a\n++b"),
            "(Module (ExprStmt a) (ExprStmt (UpdateExpr ++ b)) <eof>)",
            "a line break before a postfix `++` ends the statement before it"
        );
        assert_eq!(
            diagnostics("function f() {\n  i++\n}")
                .iter()
                .map(|d| d.0)
                .collect::<Vec<_>>(),
            vec![Code::S012],
            "inside a function too"
        );
    }

    #[test]
    fn assignment_targets_are_names_properties_and_patterns() {
        assert_eq!(
            clean("[a, b.c, d = 1, ...e] = f"),
            "(Module (ExprStmt (AssignExpr (ArrayPattern [ a , (MemberExpr b . c) , (AssignExpr d = 1) , (RestElement ... e) ]) = f)) <eof>)"
        );
        assert_eq!(
            clean("({ a, b: c.d, e = 1, f: [g], ...h } = o)"),
            "(Module (ExprStmt (ParenExpr ( (AssignExpr (ObjectPattern { (PatternProperty a) , (PatternProperty b : (MemberExpr c . d)) , (PatternProperty e = 1) , (PatternProperty f : (ArrayPattern [ g ])) , (RestElement ... h) }) = o) ))) <eof>)"
        );
        assert_eq!(
            clean("[[a], { b }, (c), (d.e)] = f"),
            "(Module (ExprStmt (AssignExpr (ArrayPattern [ (ArrayPattern [ a ]) , (ObjectPattern { (PatternProperty b) }) , (ParenExpr ( c )) , (ParenExpr ( (MemberExpr d . e) )) ]) = f)) <eof>)"
        );
        assert_eq!(clean("[...[a, b]] = c").matches("ArrayPattern").count(), 2);
        check_one(&[
            (
                "a + b = c",
                Code::E000,
                1,
                1,
                "the left side of `=` is a name, a property or a pattern",
            ),
            ("f() = 1", Code::E000, 1, 1, "the left side of `=`"),
            ("a?.b = 1", Code::E000, 1, 1, "the left side of `=`"),
            (
                "[a] += 1",
                Code::E000,
                1,
                1,
                "the left side of `+=` is a name or a property",
            ),
            ("([a]) = 1", Code::E000, 1, 1, "the left side of `=`"),
        ]);
    }

    #[test]
    fn every_target_inside_a_pattern_is_a_name_or_a_property() {
        check_one(&[
            (
                "[1] = a",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[f()] = a",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[a?.b] = c",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[(a + b)] = c",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[a += 1] = c",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "({ a: 1 } = b)",
                Code::E000,
                1,
                7,
                "a pattern assigns to names and properties",
            ),
            (
                "[...a, b] = c",
                Code::E000,
                1,
                2,
                "a rest element must be last",
            ),
            (
                "({ ...[a] } = b)",
                Code::E000,
                1,
                4,
                "the rest of an object pattern is a name or a property",
            ),
            (
                "x = { a = 1 }",
                Code::E000,
                1,
                7,
                "`{ a = 1 }` is a pattern's default",
            ),
            (
                "f({ a = 1 })",
                Code::E000,
                1,
                5,
                "`{ a = 1 }` is a pattern's default",
            ),
        ]);
        let found = fixes("(a) = 1");
        assert_eq!(
            found,
            vec![(Code::S005, Some("(a)".to_owned()), Some("a".to_owned()))],
            "accepted, with redundant parentheses"
        );
        assert_eq!(
            fixes("( a.b ) += 1"),
            vec![(
                Code::S005,
                Some("( a.b )".to_owned()),
                Some("a.b".to_owned())
            )]
        );
        assert_eq!(
            diagnostics("[{ a = 1 }] = b"),
            Vec::new(),
            "a nested shorthand default is the pattern's"
        );
        assert_eq!(
            diagnostics("[a = { b = 1 }] = c").len(),
            1,
            "but a default's own value is a value"
        );
    }

    #[test]
    fn yield_and_await_follow_their_function() {
        assert_eq!(
            clean("function* g() {\n  yield\n  yield 1\n  yield* h()\n  const a = yield b\n  f(yield, yield c)\n}"),
            "(Module (FunctionDecl function * g (Params ( )) (Block { (ExprStmt (YieldExpr yield)) (ExprStmt (YieldExpr yield 1)) (ExprStmt (YieldExpr yield * (CallExpr h ( )))) (ConstDecl const a (Initializer = (YieldExpr yield b))) (ExprStmt (CallExpr f ( (YieldExpr yield) , (YieldExpr yield c) ))) })) <eof>)"
        );
        assert_eq!(
            clean("async function f() {\n  await g()\n}\nawait h()"),
            "(Module (FunctionDecl async function f (Params ( )) (Block { (ExprStmt (AwaitExpr await (CallExpr g ( )))) })) (ExprStmt (AwaitExpr await (CallExpr h ( )))) <eof>)",
            "`await` at the module's top level is accepted"
        );
        check_one(&[
            (
                "function f() {\n  await g()\n}",
                Code::E000,
                2,
                3,
                "`await` is permitted only inside an `async` function",
            ),
            (
                "function f() {\n  yield 1\n}",
                Code::E000,
                2,
                3,
                "`yield` is permitted only directly inside a generator",
            ),
            (
                "x = a + yield",
                Code::E000,
                1,
                9,
                "`yield` is permitted only directly inside a generator",
            ),
            (
                "function* g() {\n  x = a + yield b\n}",
                Code::E000,
                2,
                11,
                "a `yield` inside an operand needs parentheses",
            ),
            (
                "function* g() {\n  yield a, b\n}",
                Code::E000,
                2,
                10,
                "the comma operator is removed",
            ),
            (
                "function* g() {\n  yield*\n}",
                Code::E000,
                3,
                1,
                "expected an expression after `yield*`",
            ),
            (
                "function* g() {\n  yield =\n}",
                Code::E000,
                2,
                9,
                "expected an expression after `yield`",
            ),
            (
                "x = await",
                Code::E000,
                1,
                10,
                "expected an expression after `await`",
            ),
        ]);
    }

    #[test]
    fn members_calls_new_templates_and_literals_parse_for_their_structure() {
        for (text, expected) in [
            ("a?.b?.[c]?.(d)", "(CallExpr (IndexExpr (MemberExpr a ?. b) ?. [ c ]) ?. ( d ))"),
            ("a.if.true.null", "(MemberExpr (MemberExpr (MemberExpr a . if) . true) . null)"),
            ("f(...a, b,)", "(CallExpr f ( (SpreadElement ... a) , b , ))"),
            ("new A", "(NewExpr new A)"),
            ("new new A()()", "(NewExpr new (NewExpr new A ( )) ( ))"),
            ("import(\"./a.js\")", "(ImportCall import ( \"./a.js\" ))"),
            ("import.meta", "(MetaProperty import . meta)"),
            ("new.target", "(MetaProperty new . target)"),
            ("tag`a${b}c${d}e`", "(TaggedTemplate tag (TemplateLit `a${ b }c${ d }e`))"),
            ("tag`a`", "(TaggedTemplate tag `a`)"),
            ("`a${`b${c}`}`", "(TemplateLit `a${ (TemplateLit `b${ c }`) }`)"),
            ("[1, , ...a,]", "(ArrayLit [ 1 , , (SpreadElement ... a) , ])"),
            ("[]", "(ArrayLit [ ])"),
            ("{}", "(ObjectLit { })"),
            ("{ a, b: 1, \"c\": 2, 3: 4, [d]: 5, if: 6, ...e, }", "(ObjectLit { (Property a) , (Property b : 1) , (Property \"c\" : 2) , (Property 3 : 4) , (Property [ d ] : 5) , (Property if : 6) , (SpreadElement ... e) , })"),
            ("/a/v.test(s)", "(CallExpr (MemberExpr /a/v . test) ( s ))"),
            ("this.#x", "(MemberExpr this . #x)"),
            ("a\n.b", "(MemberExpr a . b)"),
        ] {
            assert_eq!(value(text), expected, "{text}");
        }
        assert_eq!(
            sexp("class A {\n  #x\n  m() {\n    return this.#x\n  }\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassField #x) (ClassMethod m (Params ( )) (Block { (ReturnStmt return (MemberExpr this . #x)) })) })) <eof>)"
        );
        assert_eq!(
            value("a / b / c"),
            "(BinaryExpr (BinaryExpr a / b) / c)",
            "after an operand a `/` divides"
        );
        assert_eq!(value("{} / 2"), "(BinaryExpr (ObjectLit { }) / 2)");
    }

    #[test]
    fn malformed_expressions_are_e000_once() {
        check_one(&[
            (
                "x = a +",
                Code::E000,
                1,
                8,
                "expected an expression after `+`",
            ),
            (
                "x = !",
                Code::E000,
                1,
                6,
                "expected an expression after `!`",
            ),
            (
                "x = a ?",
                Code::E000,
                1,
                8,
                "expected an expression after `?`",
            ),
            (
                "x = a ? b",
                Code::E000,
                1,
                10,
                "expected `:` in the conditional",
            ),
            (
                "x = a ? b :",
                Code::E000,
                1,
                12,
                "expected an expression after `:`",
            ),
            ("x =", Code::E000, 1, 4, "expected an expression after `=`"),
            ("x = a.", Code::E000, 1, 7, "expected a property name"),
            (
                "x = a[]",
                Code::E000,
                1,
                7,
                "expected an expression in `[ … ]`",
            ),
            ("x = a[b", Code::E000, 1, 8, "expected `]`"),
            ("x = f(,)", Code::E000, 1, 7, "expected an argument or `)`"),
            (
                "x = f(a b)",
                Code::E000,
                1,
                9,
                "expected `,` or `)` in the argument list",
            ),
            (
                "x = f(...)",
                Code::E000,
                1,
                10,
                "expected an expression after `...`",
            ),
            (
                "x = ()",
                Code::E000,
                1,
                6,
                "expected an expression in `( … )`",
            ),
            ("x = (a", Code::E000, 1, 7, "expected `)`"),
            (
                "x = [a b]",
                Code::E000,
                1,
                8,
                "expected `,` or `]` in the array",
            ),
            (
                "x = [*]",
                Code::E000,
                1,
                6,
                "expected an element, `,` or `]`",
            ),
        ]);
    }

    #[test]
    fn malformed_literals_and_templates_are_e000_once() {
        check_one(&[
            (
                "x = { a b }",
                Code::E000,
                1,
                9,
                "expected `,` or `}` in the object",
            ),
            ("x = { * }", Code::E000, 1, 7, "expected a property"),
            (
                "x = { a = }",
                Code::E000,
                1,
                11,
                "expected an expression after `=`",
            ),
            (
                "x = { \"a\" }",
                Code::E000,
                1,
                11,
                "expected `:` after the property key",
            ),
            (
                "x = { a: }",
                Code::E000,
                1,
                10,
                "expected an expression after `:`",
            ),
            (
                "x = { [] : 1 }",
                Code::E000,
                1,
                8,
                "expected an expression in `[ … ]`",
            ),
            (
                "x = { [a : 1 }",
                Code::E000,
                1,
                10,
                "expected `]` after the computed key",
            ),
            (
                "x = `${}`",
                Code::E000,
                1,
                8,
                "expected an expression in `${ … }`",
            ),
            (
                "x = `${a b}`",
                Code::E000,
                1,
                10,
                "expected `}` to close the template substitution",
            ),
            ("x = new", Code::E000, 1, 8, "expected a class after `new`"),
            (
                "x = import",
                Code::E000,
                1,
                11,
                "expected `(` after `import`",
            ),
            ("x = enum", Code::E000, 1, 5, "`enum` is reserved"),
            (
                "x = a + () => b",
                Code::E000,
                1,
                9,
                "an arrow function inside an operand needs parentheses",
            ),
            (
                "x = !async () => b",
                Code::E000,
                1,
                6,
                "an arrow function inside an operand needs parentheses",
            ),
            ("x = a b", Code::E000, 1, 7, "expected `;` or a line break"),
            ("*a", Code::E000, 1, 1, "expected a statement, not `*`"),
        ]);
        for text in ["x = $a", "x = arguments", "x = café"] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "only the lexer's report: {text}: {found:?}");
        }
    }

    #[test]
    fn nested_patterns_and_private_names_take_every_path() {
        for text in [
            "[a.b.c, d[0].e, ((f)), (g.h)] = i",
            "({ ...o.x } = y)",
            "[...a.b] = c",
            "[(a).b] = c",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
        check_one(&[
            (
                "[a?.b.c] = d",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[(a?.b)] = c",
                Code::E000,
                1,
                2,
                "a pattern assigns to names and properties",
            ),
            (
                "[...] = a",
                Code::E000,
                1,
                5,
                "expected an expression after `...`",
            ),
        ]);
        let found = diagnostics("class A {\n  #x\n  m(o) {\n    return #x in o\n  }\n}");
        assert_eq!(found.len(), 1, "only the lexer's `in`: {found:?}");
        assert!(found[0].3.contains("`in` is removed"), "{found:?}");
    }

    #[test]
    fn deep_expressions_are_bounded_on_every_path() {
        for text in [
            format!("x = {}a{}", "(".repeat(300), ")".repeat(300)),
            format!("x = {}a{}", "[".repeat(300), "]".repeat(300)),
            format!("x = {}a", "a ** ".repeat(300)),
            format!("x = !({}a)", "a ** ".repeat(300)),
            format!("x = {}A", "new ".repeat(300)),
            format!("x = {}a", "() => ".repeat(300)),
            format!("x = {}a", "!".repeat(300)),
        ] {
            let found = diagnostics(&text);
            assert_eq!(found.len(), 1, "one report, then quiet: {found:?}");
            assert!(found[0].3.contains("nest deeper than"), "{found:?}");
            let f = file(&text);
            assert_eq!(parse(&f).0.print(&f), text);
        }
    }

    #[test]
    fn each_boundary_of_an_expression_is_where_es_puts_it() {
        assert_eq!(
            sexp("a, b c"),
            "(Module (ExprStmt (SequenceExpr a , b)) c <eof>)",
            "the sequence ends with its last comma's operand"
        );
        assert_eq!(
            value("new a?.b()"),
            "(CallExpr (MemberExpr (NewExpr new a) ?. b) ( ))",
            "a `new` callee stops at `?.`"
        );
        assert_eq!(
            diagnostics("[(a?.b).c] = d"),
            Vec::new(),
            "parentheses end an optional chain: `(a?.b).c` is a plain member"
        );
        let text = "x = a ** b\n".repeat(250);
        assert_eq!(
            diagnostics(&text),
            Vec::new(),
            "`**` gives its nesting back"
        );
        check_one(&[("a?.b.c = 1", Code::E000, 1, 1, "the left side of `=`")]);
        for (text, line, column) in [
            ("f(a) => b", 1, 6),
            ("x y => z", 1, 3),
            ("async\n(x) => x", 2, 5),
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (found[0].1, found[0].2),
                (line, column),
                "not an arrow: only `(…)`, a name or `async` on the same line begins one: {text}"
            );
        }
    }

    #[test]
    fn expressions_print_back_to_their_source() {
        let text = "x  =  a ?? ( b || c )  ? -d ** 2 : [ e , ...f ]\n( { g = 1 } = h )\ni ++\n";
        let f = file(text);
        let (tree, _) = parse(&f);
        assert_eq!(tree.print(&f), text);
    }
}
