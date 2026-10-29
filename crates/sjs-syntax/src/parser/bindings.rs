//! `const` and `let` declarations with their binding patterns (SPEC.md §2 `ConstDecl`,
//! `LetDecl`, §4.2, §12): a name, an object pattern `{ a, b: pattern = default,
//! ...rest }` or an array pattern `[ pattern = default, ...rest ]`, then `= Expr`. The
//! initializer and every default are expression extents until the expression stories
//! land ([`Parser::raw_expression`]).
//!
//! Early errors reported here: `let x` without an initializer (E000 with the fix
//! `let x = undefined`), a `const` without one, an elision in an array pattern (dense
//! arrays only), a rest element that is not last, a name bound twice in one pattern,
//! and `let`, `undefined`, `await` or `yield` as a binding name.

use sjs_diag::Span;

use super::Parser;
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

const COMMA: TokenKind = TokenKind::Punct(Punct::Comma);
const COLON: TokenKind = TokenKind::Punct(Punct::Colon);
const EQ: TokenKind = TokenKind::Punct(Punct::Eq);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const RBRACKET: TokenKind = TokenKind::Punct(Punct::RBracket);
const ELLIPSIS: TokenKind = TokenKind::Punct(Punct::Ellipsis);
const SEMICOLON: TokenKind = TokenKind::Punct(Punct::Semicolon);

/// A `const` or `let` declaration, at its keyword, with the `;` that may end it.
/// Returns the names it binds with their spans, in source order.
pub(super) fn declaration(parser: &mut Parser<'_>) -> Vec<(String, Span)> {
    let is_let = parser.peek(false).kind == TokenKind::Keyword(Keyword::Let);
    parser.start_node(if is_let {
        NodeKind::LetDecl
    } else {
        NodeKind::ConstDecl
    });
    let keyword = parser.bump(false).clone();
    let mut names = Vec::new();
    if !pattern(parser, &mut names) {
        parser.finish_node();
        return names;
    }
    if parser.peek(false).kind == EQ {
        parser.start_node(NodeKind::Initializer);
        let eq = parser.bump(false).span;
        if parser.raw_expression(&[COMMA]) == 0 {
            parser.error(eq, "expected an expression after `=`");
        }
        parser.finish_node();
        if parser.peek(false).kind == COMMA {
            let span = parser.peek(false).span;
            parser.error(
                span,
                "a declaration binds one pattern: write a second `const` or `let` for the next name",
            );
        }
    } else {
        let end = names.last().map_or(keyword.span, |(_, span)| *span);
        let span = Span::new(
            keyword.span.start,
            end.end,
            keyword.span.byte_start,
            end.byte_end,
        );
        let current = parser.file.slice(&span).unwrap_or("").to_owned();
        if is_let {
            parser.error_with_fix(
                span,
                "`let` without an initializer is removed: the implicit `undefined` is written out (SPEC.md §12)",
                current.clone(),
                format!("{current} = undefined"),
            );
        } else {
            parser.error(span, "a `const` declaration needs an initializer");
        }
    }
    parser.eat(&SEMICOLON);
    parser.finish_node();
    names
}

/// A binding pattern: a name, `{ … }` or `[ … ]`. False when nothing bindable is
/// there (reported).
pub(super) fn pattern(parser: &mut Parser<'_>, names: &mut Vec<(String, Span)>) -> bool {
    match parser.peek(false).kind.clone() {
        TokenKind::Ident => binding_name(parser, names),
        TokenKind::Punct(Punct::LBrace) => object_pattern(parser, names),
        TokenKind::Punct(Punct::LBracket) => array_pattern(parser, names),
        TokenKind::Keyword(Keyword::Let) => {
            let span = parser.peek(false).span;
            parser.error(span, "`let` cannot be a binding name");
            parser.bump(false);
            true
        }
        TokenKind::Error => {
            // The lexer has reported the character (`$`, `\u`, a non-ASCII letter);
            // the name it began still binds for recovery.
            parser.bump(false);
            if parser.peek(false).kind == TokenKind::Ident {
                binding_name(parser, names);
            }
            true
        }
        TokenKind::Keyword(keyword) if keyword.removed_replacement().is_some() => {
            // The lexer has reported the removed word; the name still binds for recovery.
            let token = parser.bump(false).clone();
            names.push((parser.text(&token).to_owned(), token.span));
            true
        }
        _ => {
            let span = parser.peek(false).span;
            parser.error(span, "expected a binding name, `{` or `[`");
            false
        }
    }
}

/// One identifier in binding position: not `undefined`, `await` or `yield`, and not a
/// name the same pattern already binds.
fn binding_name(parser: &mut Parser<'_>, names: &mut Vec<(String, Span)>) -> bool {
    let token = parser.bump(false).clone();
    let name = parser.text(&token).to_owned();
    match name.as_str() {
        "undefined" => parser.error(
            token.span,
            "`undefined` cannot be rebound: it is the one way to write the absent value (SPEC.md §12)",
        ),
        "await" | "yield" => {
            let message = format!("`{name}` is reserved in a module and cannot be a binding name");
            parser.error(token.span, message);
        }
        _ => {}
    }
    if names.iter().any(|(earlier, _)| *earlier == name) {
        let message = format!("`{name}` is bound twice in one pattern");
        parser.error(token.span, message);
    }
    names.push((name, token.span));
    true
}

/// `{ key, key: pattern, key = default, key: pattern = default, ...rest }`.
fn object_pattern(parser: &mut Parser<'_>, names: &mut Vec<(String, Span)>) -> bool {
    parser.start_node(NodeKind::ObjectPattern);
    parser.bump(false);
    loop {
        match parser.peek(false).kind.clone() {
            RBRACE => break,
            ELLIPSIS => {
                if !rest_element(parser, names, &RBRACE) {
                    parser.finish_node();
                    return false;
                }
                break;
            }
            TokenKind::Ident => {
                parser.start_node(NodeKind::PatternProperty);
                if parser.at_word("__proto__") {
                    let span = parser.peek(false).span;
                    parser.error(
                        span,
                        "`__proto__` is removed as a property key: prototypes are not manipulated (SPEC.md §12)",
                    );
                }
                if parser.peek_second().kind == COLON {
                    parser.bump(false);
                    parser.bump(false);
                    if !pattern(parser, names) {
                        parser.finish_node();
                        parser.finish_node();
                        return false;
                    }
                } else {
                    binding_name(parser, names);
                }
                default_value(parser, RBRACE);
                parser.finish_node();
            }
            _ => {
                let span = parser.peek(false).span;
                parser.error(
                    span,
                    "expected a property name, `...rest` or `}` in the object pattern",
                );
                parser.finish_node();
                return false;
            }
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RBRACE {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `}` in the object pattern");
            parser.finish_node();
            return false;
        }
    }
    // The loop only breaks at `}`.
    parser.bump(false);
    parser.finish_node();
    true
}

/// `[ pattern = default, ..., ...rest ]`; an elision is E000.
fn array_pattern(parser: &mut Parser<'_>, names: &mut Vec<(String, Span)>) -> bool {
    parser.start_node(NodeKind::ArrayPattern);
    parser.bump(false);
    loop {
        match parser.peek(false).kind.clone() {
            RBRACKET => break,
            COMMA => {
                let span = parser.peek(false).span;
                parser.error(
                    span,
                    "an elision in an array pattern is removed: arrays are dense, so name the element (SPEC.md §12)",
                );
                parser.finish_node();
                return false;
            }
            ELLIPSIS => {
                if !rest_element(parser, names, &RBRACKET) {
                    parser.finish_node();
                    return false;
                }
                break;
            }
            _ => {
                parser.start_node(NodeKind::PatternElement);
                if !pattern(parser, names) {
                    parser.finish_node();
                    parser.finish_node();
                    return false;
                }
                default_value(parser, RBRACKET);
                parser.finish_node();
            }
        }
        if !parser.eat(&COMMA) && parser.peek(false).kind != RBRACKET {
            let span = parser.peek(false).span;
            parser.error(span, "expected `,` or `]` in the array pattern");
            parser.finish_node();
            return false;
        }
    }
    // The loop only breaks at `]`.
    parser.bump(false);
    parser.finish_node();
    true
}

/// `...pattern`, which must be last and take no default.
fn rest_element(
    parser: &mut Parser<'_>,
    names: &mut Vec<(String, Span)>,
    close: &TokenKind,
) -> bool {
    parser.start_node(NodeKind::RestElement);
    parser.bump(false);
    if !pattern(parser, names) {
        parser.finish_node();
        return false;
    }
    parser.finish_node();
    if parser.peek(false).kind == *close {
        return true;
    }
    let span = parser.peek(false).span;
    let message = if parser.peek(false).kind == EQ {
        "a rest element takes no default"
    } else {
        "a rest element must be last in its pattern"
    };
    parser.error(span, message);
    false
}

/// An optional `= default`, whose expression runs to the next `,` or `close` at
/// bracket depth zero.
pub(super) fn default_value(parser: &mut Parser<'_>, close: TokenKind) {
    if parser.peek(false).kind != EQ {
        return;
    }
    parser.start_node(NodeKind::Initializer);
    let eq = parser.bump(false).span;
    if parser.raw_expression(&[COMMA, close]) == 0 {
        parser.error(eq, "expected an expression after `=`");
    }
    parser.finish_node();
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

    #[test]
    fn names_and_initializers_parse_into_declaration_nodes() {
        assert_eq!(
            clean("const a = 1"),
            "(Module (ConstDecl const a (Initializer = 1)) <eof>)"
        );
        assert_eq!(
            clean("let a = 1"),
            "(Module (LetDecl let a (Initializer = 1)) <eof>)"
        );
        assert_eq!(
            clean("const a = 1\nlet b = a + 2;\nconst c = f(\n  a,\n  b\n)"),
            "(Module (ConstDecl const a (Initializer = 1)) (LetDecl let b (Initializer = a + 2) ;) (ConstDecl const c (Initializer = f ( a , b ))) <eof>)",
            "a newline or `;` ends the initializer; brackets keep it open"
        );
        assert_eq!(
            clean("const a = 1 +\n  2\nconst b = a\n(c)"),
            "(Module (ConstDecl const a (Initializer = 1 + 2)) (ConstDecl const b (Initializer = a)) (ExprStmt ( c )) <eof>)",
            "a line ending in an operator continues; a line starting with `(` does not"
        );
        assert_eq!(
            clean("{ const a = 1 }"),
            "(Module (Block { (ConstDecl const a (Initializer = 1)) }) <eof>)",
            "inside a block, `}}` ends the initializer"
        );
        assert_eq!(
            clean("const t = `${a}` + `b`\nconst r = /x/v"),
            "(Module (ConstDecl const t (Initializer = `${ a }` + `b`)) (ConstDecl const r (Initializer = /x/v)) <eof>)",
            "templates and regexes inside initializers"
        );
        assert_eq!(
            clean("const o = { a: 1,\n  b: 2 }\nconst p = o.const"),
            "(Module (ConstDecl const o (Initializer = { a : 1 , b : 2 })) (ConstDecl const p (Initializer = o . const)) <eof>)",
            "`const` after `.` is a property name"
        );
        assert_eq!(
            clean("const a = typeof\n  x\nconst b = new\n  C()\nconst c = d instanceof\n  C"),
            "(Module (ConstDecl const a (Initializer = typeof x)) (ConstDecl const b (Initializer = new C ( ))) (ConstDecl const c (Initializer = d instanceof C)) <eof>)",
            "a keyword that takes an operand keeps the line open"
        );
        assert_eq!(
            clean("const a = this\nconst b = `${\n  a\n}`\nconst c = `x`\nconst d = 1"),
            "(Module (ConstDecl const a (Initializer = this)) (ConstDecl const b (Initializer = `${ a }`)) (ConstDecl const c (Initializer = `x`)) (ConstDecl const d (Initializer = 1)) <eof>)",
            "`this` and a template end a line; a substitution keeps it open"
        );
        assert_eq!(
            clean("for (const x of y) {}"),
            "(Module (ForOfStmt for ( (ConstDecl const x) of y ) (Block { })) <eof>)",
            "a `for` head is the loop story's"
        );
    }

    #[test]
    fn object_and_array_patterns_parse_with_defaults_nesting_and_rest() {
        assert_eq!(
            clean("const { a, b: c, d = 1, e: f = 2, ...g } = h"),
            "(Module (ConstDecl const (ObjectPattern { (PatternProperty a) , (PatternProperty b : c) , (PatternProperty d (Initializer = 1)) , (PatternProperty e : f (Initializer = 2)) , (RestElement ... g) }) (Initializer = h)) <eof>)"
        );
        assert_eq!(
            clean("const [a, b = a, [c], { d }, ...e] = f"),
            "(Module (ConstDecl const (ArrayPattern [ (PatternElement a) , (PatternElement b (Initializer = a)) , (PatternElement (ArrayPattern [ (PatternElement c) ])) , (PatternElement (ObjectPattern { (PatternProperty d) })) , (RestElement ... e) ]) (Initializer = f)) <eof>)",
            "a default may name an earlier element"
        );
        assert_eq!(
            clean("const { a: { b: [c] } } = d"),
            "(Module (ConstDecl const (ObjectPattern { (PatternProperty a : (ObjectPattern { (PatternProperty b : (ArrayPattern [ (PatternElement c) ])) })) }) (Initializer = d)) <eof>)"
        );
        assert_eq!(
            clean("const {} = x\nconst [] = y\nconst { a, } = z"),
            "(Module (ConstDecl const (ObjectPattern { }) (Initializer = x)) (ConstDecl const (ArrayPattern [ ]) (Initializer = y)) (ConstDecl const (ObjectPattern { (PatternProperty a) , }) (Initializer = z)) <eof>)",
            "empty patterns are accepted here (E116 later); a trailing comma too"
        );
        assert_eq!(
            clean("const { a = f(1, { b: 2 }), c = [3, 4] } = d"),
            "(Module (ConstDecl const (ObjectPattern { (PatternProperty a (Initializer = f ( 1 , { b : 2 } ))) , (PatternProperty c (Initializer = [ 3 , 4 ])) }) (Initializer = d)) <eof>)",
            "defaults run to the next `,` at bracket depth zero"
        );
        assert_eq!(
            clean("const [...[a, b]] = c"),
            "(Module (ConstDecl const (ArrayPattern [ (RestElement ... (ArrayPattern [ (PatternElement a) , (PatternElement b) ])) ]) (Initializer = c)) <eof>)",
            "a rest target may be a pattern"
        );
    }

    #[test]
    fn let_without_an_initializer_is_e000_with_the_undefined_fix() {
        let f = file("let x\nlet { a, b } = c");
        let (tree, found) = parse(&f);
        let found: Vec<&Diagnostic> = found.iter().collect();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, Code::E000);
        assert_eq!(
            (found[0].span.byte_start, found[0].span.byte_end),
            (0, 5),
            "spans `let x`"
        );
        assert_eq!(found[0].current.as_deref(), Some("let x"));
        assert_eq!(found[0].replacement.as_deref(), Some("let x = undefined"));
        assert!(found[0].message.contains("implicit `undefined`"));
        assert_eq!(
            tree.to_sexp(&f),
            "(Module (LetDecl let x) (LetDecl let (ObjectPattern { (PatternProperty a) , (PatternProperty b) }) (Initializer = c)) <eof>)"
        );
        let found = diagnostics("let { a, b }");
        assert_eq!(found.len(), 1);
        let f = file("let { a, b }");
        let (_, d) = parse(&f);
        assert_eq!(
            d.iter().next().map(|d| d.current.clone()),
            Some(Some("let { a, b".to_owned())),
            "the span ends at the last bound name"
        );
        let found = diagnostics("const x\nconst y = 1");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].0, found[0].1, found[0].2), (Code::E000, 1, 1));
        assert!(found[0].3.contains("needs an initializer"));
    }

    #[test]
    fn early_errors_in_patterns_are_e000_at_the_offending_token() {
        let cases = [
            (
                "const [a, , b] = y",
                11,
                "an elision in an array pattern is removed",
            ),
            (
                "const [, a] = y",
                8,
                "an elision in an array pattern is removed",
            ),
            ("const [...a, b] = y", 12, "a rest element must be last"),
            ("const { ...a, b } = y", 13, "a rest element must be last"),
            (
                "const [...a = 1] = y",
                13,
                "a rest element takes no default",
            ),
            ("const [...a,] = y", 12, "a rest element must be last"),
            (
                "const { a, a } = y",
                12,
                "`a` is bound twice in one pattern",
            ),
            (
                "const [a, { b: a }] = y",
                16,
                "`a` is bound twice in one pattern",
            ),
            (
                "const { a, ...a } = y",
                15,
                "`a` is bound twice in one pattern",
            ),
        ];
        check_e000(&cases);
        assert_eq!(
            sexp("const [a, , b] = y"),
            "(Module (ConstDecl const (ArrayPattern [ (PatternElement a) ,)) , b ] = y <eof>)",
            "the declaration ends at the error; the rest is left to the items"
        );
        assert_eq!(
            diagnostics("const { a, a, a } = y").len(),
            2,
            "each repeat is reported"
        );
        assert_eq!(
            sexp("let let = 1\nconst $a = 2"),
            "(Module (LetDecl let let (Initializer = 1)) (ConstDecl const $ a (Initializer = 2)) <eof>)",
            "the bad name is consumed and the declaration goes on"
        );
        assert_eq!(
            diagnostics("const eval = 1"),
            vec![(
                Code::E000,
                1,
                7,
                "`eval` is removed; use nothing: there is no code from strings".to_owned()
            )],
            "a removed word is the lexer's report alone"
        );
    }

    #[test]
    fn bad_binding_names_are_e000_at_the_name() {
        let cases = [
            ("let let = 1", 5, "`let` cannot be a binding name"),
            ("const $a = 1", 7, "`$` is not permitted in identifiers"),
            ("const é = 1", 7, "is not permitted in identifiers"),
            ("const let = 1", 7, "`let` cannot be a binding name"),
            ("let undefined = 1", 5, "`undefined` cannot be rebound"),
            (
                "const { undefined } = x",
                9,
                "`undefined` cannot be rebound",
            ),
            ("const await = 1", 7, "`await` is reserved in a module"),
            ("const yield = 1", 7, "`yield` is reserved in a module"),
            (
                "const { __proto__: p } = x",
                9,
                "`__proto__` is removed as a property key",
            ),
            ("const 1 = 2", 7, "expected a binding name, `{` or `[`"),
            ("const class = 2", 7, "expected a binding name, `{` or `[`"),
            (
                "const { 1 } = x",
                9,
                "expected a property name, `...rest` or `}`",
            ),
            (
                "const { a b } = x",
                11,
                "expected `,` or `}` in the object pattern",
            ),
            (
                "const { a: } = x",
                12,
                "expected a binding name, `{` or `[`",
            ),
            (
                "const [a b] = x",
                10,
                "expected `,` or `]` in the array pattern",
            ),
            ("const [1] = x", 8, "expected a binding name, `{` or `[`"),
            ("const a =", 9, "expected an expression after `=`"),
            ("const { a = } = x", 11, "expected an expression after `=`"),
            ("const [a = ] = x", 10, "expected an expression after `=`"),
            (
                "const [... ] = x",
                12,
                "expected a binding name, `{` or `[`",
            ),
            ("const a = 1, b = 2", 12, "a declaration binds one pattern"),
        ];
        check_e000(&cases);
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
    fn unterminated_patterns_and_initializers_at_the_end_of_the_file() {
        let found = diagnostics("const { a");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].3.contains("expected `,` or `}`"));
        let found = diagnostics("const [a");
        assert!(found[0].3.contains("expected `,` or `]`"));
        assert_eq!(
            sexp("const a ="),
            "(Module (ConstDecl const a (Initializer =)) <eof>)",
            "an empty initializer is reported and the node is kept"
        );
        assert_eq!(sexp("const"), "(Module (ConstDecl const) <eof>)");
    }

    #[test]
    fn declarations_start_statements_only() {
        assert_eq!(
            clean("a.const = 1\nx?.let\nf(const)"),
            "(Module (ExprStmt a . const = 1) (ExprStmt x ?. let) (ExprStmt f ( const )) <eof>)",
            "`const` after `.`, `?.` or `(` is not a declaration"
        );
        assert_eq!(
            clean("const a = 1; const b = 2"),
            "(Module (ConstDecl const a (Initializer = 1) ;) (ConstDecl const b (Initializer = 2)) <eof>)"
        );
        assert_eq!(
            clean("if (x) {\n  let y = 1\n}"),
            "(Module (IfStmt if ( x ) (Block { (LetDecl let y (Initializer = 1)) })) <eof>)"
        );
    }

    #[test]
    fn declarations_print_back_to_their_source() {
        let text = "const  { a ,\n  b: [ c = 1 ], ...d } = e // c\nlet f = `${a}`\n";
        let f = file(text);
        let (tree, found) = parse(&f);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(tree.print(&f), text);
    }
}
