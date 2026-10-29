//! Class declarations (SPEC.md §2 `ClassDecl` … `MemberKey`, §4.5): `class Name
//! [extends Name | ns.Name] { members }` with fields, `#private` members, methods
//! (`async`, `*`, `async *`), `get`/`set` accessors, `static` members and `static {}`
//! blocks. A member key is an identifier, a private name or exactly one of the four
//! `[Symbol.…]` keys of §2. The removed class expression is E000 and parsed for
//! recovery. ES2025 early errors reported here: a generator, async or accessor
//! `constructor`, two constructors, `#constructor`, a field named `constructor`, a
//! static member named `prototype`, a private name declared twice, a `#name` the class
//! (or an enclosing class) does not declare, `super()` outside a derived constructor,
//! `super.x` outside a class member and `super?.x`.

use std::collections::BTreeMap;

use sjs_diag::Span;

use super::{expressions, functions, FnContext, Parser};
use crate::token::{Keyword, Punct, TokenKind};
use crate::tree::NodeKind;

const LPAREN: TokenKind = TokenKind::Punct(Punct::LParen);
const LBRACE: TokenKind = TokenKind::Punct(Punct::LBrace);
const RBRACE: TokenKind = TokenKind::Punct(Punct::RBrace);
const LBRACKET: TokenKind = TokenKind::Punct(Punct::LBracket);
const RBRACKET: TokenKind = TokenKind::Punct(Punct::RBracket);
const DOT: TokenKind = TokenKind::Punct(Punct::Dot);
const EQ: TokenKind = TokenKind::Punct(Punct::Eq);
const SEMICOLON: TokenKind = TokenKind::Punct(Punct::Semicolon);
const STAR: TokenKind = TokenKind::Punct(Punct::Star);

/// What one class declares and references, for the checks at its end.
#[derive(Debug, Default)]
pub(super) struct ClassScope {
    /// `#name` → which of getter (1), setter (2) and anything else (4) declared it.
    declared: BTreeMap<String, u8>,
    /// Every `#name` used inside the body, with its span.
    references: Vec<(String, Span)>,
    derived: bool,
}

const GETTER: u8 = 1;
const SETTER: u8 = 2;
const OTHER: u8 = 4;

/// The four permitted computed keys.
const SYMBOL_KEYS: [&str; 4] = ["iterator", "asyncIterator", "dispose", "asyncDispose"];

/// A member's key as parsed.
struct Key {
    /// The identifier text, for `constructor` and `prototype` checks.
    ident: Option<String>,
    /// The private name without `#`.
    private: Option<String>,
    /// A `[Symbol.…]` key, which names a method or accessor, never a field.
    symbol: bool,
    span: Span,
}

/// True when the next token is `class` beginning a class: a name, `extends` or `{`
/// follows. A `class` keyword elsewhere (`const class = 1`) is the fault of whatever
/// reported it, not a class.
pub(super) fn at_class(parser: &mut Parser<'_>) -> bool {
    parser.peek(false).kind == TokenKind::Keyword(Keyword::Class)
        && matches!(
            parser.peek_second().kind,
            TokenKind::Ident | TokenKind::Keyword(Keyword::Extends) | LBRACE
        )
}

/// `class Name [extends …] { … }`, at `class`. As a declaration (`expression` false)
/// the name is required; as the removed expression form it is optional and the caller
/// has reported it. Returns the name and its span.
pub(super) fn class_decl(parser: &mut Parser<'_>, expression: bool) -> Option<(String, Span)> {
    parser.start_node(if expression {
        NodeKind::ClassExpr
    } else {
        NodeKind::ClassDecl
    });
    parser.bump(false);
    let name = if parser.peek(false).kind == TokenKind::Ident {
        let token = parser.bump(false).clone();
        Some((parser.text(&token).to_owned(), token.span))
    } else {
        if !expression {
            let span = parser.peek(false).span;
            parser.error(span, "a `class` declaration needs a name (SPEC.md §4.5)");
            if !matches!(
                parser.peek(false).kind,
                LBRACE | TokenKind::Keyword(Keyword::Extends)
            ) {
                // Nothing of a class follows (`class = 1`, a bare `class`): stop, and
                // the item skips the rest of the line.
                parser.finish_node();
                return None;
            }
        }
        None
    };
    let derived = heritage(parser);
    if parser.peek(false).kind != LBRACE {
        let span = parser.peek(false).span;
        parser.error(span, "expected `{` to open the class body");
        parser.finish_node();
        return name;
    }
    parser.classes.push(ClassScope {
        derived,
        ..ClassScope::default()
    });
    body(parser);
    let scope = parser.classes.pop().unwrap_or_default();
    for (reference, span) in scope.references {
        let declared = scope.declared.contains_key(&reference)
            || parser
                .classes
                .iter()
                .any(|outer| outer.declared.contains_key(&reference));
        if !declared {
            let message =
                format!("`#{reference}` is not declared in this class or an enclosing one");
            parser.error(span, message);
        }
    }
    parser.finish_node();
    name
}

/// `extends Name` or `extends ns.Name`; true when present.
fn heritage(parser: &mut Parser<'_>) -> bool {
    if parser.peek(false).kind != TokenKind::Keyword(Keyword::Extends) {
        return false;
    }
    parser.start_node(NodeKind::ClassHeritage);
    parser.bump(false);
    if parser.peek(false).kind != TokenKind::Ident {
        let span = parser.peek(false).span;
        parser.error(
            span,
            "`extends` names a class as `Name` or `ns.Name` (SPEC.md §2 `ClassDecl`)",
        );
        while !matches!(parser.peek(false).kind, LBRACE | RBRACE | TokenKind::Eof) {
            parser.bump(false);
        }
        parser.finish_node();
        return true;
    }
    parser.bump(false);
    if parser.eat(&DOT) {
        if parser.peek(false).kind == TokenKind::Ident {
            parser.bump(false);
        } else {
            let span = parser.peek(false).span;
            parser.error(span, "expected a name after `.` in `extends`");
        }
    }
    if parser.peek(false).kind == DOT {
        let span = parser.peek(false).span;
        parser.error(
            span,
            "`extends` names a class as `Name` or `ns.Name`: one `.` at most (SPEC.md §2 `ClassDecl`)",
        );
        while parser.eat(&DOT) {
            parser.eat(&TokenKind::Ident);
        }
    }
    parser.finish_node();
    true
}

/// `{ members }`.
fn body(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::ClassBody);
    parser.bump(false);
    let mut constructors = 0;
    loop {
        match parser.peek(false).kind.clone() {
            RBRACE => break,
            TokenKind::Eof => {
                let span = parser.peek(false).span;
                parser.error(span, "expected `}` to close the class body");
                parser.finish_node();
                return;
            }
            SEMICOLON => {
                parser.semicolon();
            }
            _ => member(parser, &mut constructors),
        }
    }
    parser.bump(false);
    parser.finish_node();
}

/// True when the token at lookahead `offset` is the identifier `word` used as a
/// modifier: a member key (or `*` when `allow_star`) follows it, so it is not itself
/// the key of a field or method (`static = 1`, `get() {}`).
fn modifier_at(parser: &mut Parser<'_>, offset: usize, word: &str, allow_star: bool) -> bool {
    let token = parser.peek_nth(offset).clone();
    if token.kind != TokenKind::Ident || parser.text(&token) != word {
        return false;
    }
    match parser.peek_nth(offset + 1).kind {
        TokenKind::Ident | TokenKind::PrivateName(_) | LBRACKET | TokenKind::Keyword(_) => true,
        STAR => allow_star,
        _ => false,
    }
}

/// One member: a static block, a field, a method or an accessor.
fn member(parser: &mut Parser<'_>, constructors: &mut u32) {
    if parser.at_word("static") && parser.peek_second().kind == LBRACE {
        static_block(parser);
        return;
    }
    let is_static = modifier_at(parser, 0, "static", true);
    let mut offset = usize::from(is_static);
    // `async` must share its line with the key: after a newline it is a field.
    let is_async = modifier_at(parser, offset, "async", true)
        && !parser.peek_nth(offset + 1).has_newline_before;
    offset += usize::from(is_async);
    let is_generator = parser.peek_nth(offset).kind == STAR;
    offset += usize::from(is_generator);
    let accessor = !is_async
        && !is_generator
        && (modifier_at(parser, offset, "get", false) || modifier_at(parser, offset, "set", false));
    let key_offset = offset + usize::from(accessor);
    let width = key_width(parser, key_offset);
    let is_method = parser.peek_nth(key_offset + width).kind == LPAREN;
    let kind = if accessor && is_method {
        NodeKind::ClassAccessor
    } else if is_method {
        NodeKind::ClassMethod
    } else {
        NodeKind::ClassField
    };
    parser.start_node(kind);
    if is_static {
        parser.bump(false);
    }
    if is_async {
        parser.bump(false);
    }
    if is_generator && kind == NodeKind::ClassField {
        let span = parser.peek(false).span;
        parser.error(span, "`*` marks a generator method, not a field");
    }
    let is_generator = is_generator && parser.eat(&STAR);
    let accessor_kind = if kind == NodeKind::ClassAccessor {
        let word = parser.bump(false).clone();
        Some(parser.text(&word).to_owned())
    } else {
        None
    };
    let Some(key) = member_key(parser) else {
        parser.finish_node();
        return;
    };
    match kind {
        NodeKind::ClassField => field(parser, is_static, &key),
        _ => method(
            parser,
            is_static,
            is_async,
            is_generator,
            accessor_kind.as_deref(),
            &key,
            constructors,
        ),
    }
    parser.finish_node();
}

/// How many tokens the key at lookahead `offset` spans: one, or the five of
/// `[Symbol.name]`.
fn key_width(parser: &mut Parser<'_>, offset: usize) -> usize {
    if parser.peek_nth(offset).kind == LBRACKET {
        (1..)
            .take(8)
            .find(|&i| parser.peek_nth(offset + i).kind == RBRACKET)
            .map_or(1, |i| i + 1)
    } else {
        1
    }
}

/// `static { … }`.
fn static_block(parser: &mut Parser<'_>) {
    parser.start_node(NodeKind::StaticBlock);
    parser.bump(false);
    parser.with_context(FnContext::Initializer, functions::block);
    parser.finish_node();
}

/// A member key: an identifier (including the contextual words), a private name or
/// `[Symbol.iterator]`-style key. `None` after reporting a token that is no key.
fn member_key(parser: &mut Parser<'_>) -> Option<Key> {
    let token = parser.peek(false).clone();
    match token.kind {
        TokenKind::Ident | TokenKind::Keyword(_) => {
            parser.bump(false);
            Some(Key {
                ident: Some(parser.text(&token).to_owned()),
                private: None,
                symbol: false,
                span: token.span,
            })
        }
        TokenKind::PrivateName(name) => {
            parser.bump(false);
            if &*name == "constructor" {
                parser.error(token.span, "`#constructor` is not a valid private name");
            }
            Some(Key {
                ident: None,
                private: Some(name.to_string()),
                symbol: false,
                span: token.span,
            })
        }
        LBRACKET => Some(symbol_key(parser)),
        // The lexer reported it (`# b`); nothing to add.
        TokenKind::Error => {
            parser.bump(false);
            None
        }
        _ => {
            let message = format!(
                "expected a class member: a field, a method, an accessor or `static {{ … }}`, not {:?}",
                parser.text(&token)
            );
            parser.error(token.span, message);
            skip_member(parser);
            None
        }
    }
}

/// Skips the rest of a malformed member: to a `;`, the `}` closing the class, the end
/// of input or the next line, with the braces of a method body balanced, so one
/// malformed member yields one diagnostic.
fn skip_member(parser: &mut Parser<'_>) {
    let mut depth: u32 = 0;
    loop {
        match parser.peek(false).kind {
            TokenKind::Eof => return,
            RBRACE if depth == 0 => return,
            RBRACE => depth -= 1,
            LBRACE => depth += 1,
            SEMICOLON if depth == 0 => {
                parser.bump(false);
                return;
            }
            _ => {}
        }
        parser.bump(false);
        if depth == 0 && parser.peek(false).has_newline_before {
            return;
        }
    }
}

/// `[Symbol.iterator]`, `[Symbol.asyncIterator]`, `[Symbol.dispose]` or
/// `[Symbol.asyncDispose]`; any other computed key is E000 and skipped to its `]`,
/// after which the member is parsed as usual.
fn symbol_key(parser: &mut Parser<'_>) -> Key {
    let open = parser.peek(false).span;
    let well_known = parser.peek_second_is_word("Symbol")
        && parser.peek_nth(2).kind == DOT
        && {
            let name = parser.peek_nth(3).clone();
            name.kind == TokenKind::Ident && SYMBOL_KEYS.contains(&parser.text(&name))
        }
        && parser.peek_nth(4).kind == RBRACKET;
    if !well_known {
        parser.error(
            open,
            "a computed key is removed: the only computed keys are `[Symbol.iterator]`, `[Symbol.asyncIterator]`, `[Symbol.dispose]` and `[Symbol.asyncDispose]` (SPEC.md §2 `MemberKey`)",
        );
        parser.bump(false);
        while !matches!(parser.peek(false).kind, RBRACKET | RBRACE | TokenKind::Eof) {
            parser.bump(false);
        }
        let close = parser.peek(false).span;
        parser.eat(&RBRACKET);
        return Key {
            ident: None,
            private: None,
            symbol: false,
            span: Span::new(open.start, close.start, open.byte_start, close.byte_start),
        };
    }
    parser.start_node(NodeKind::SymbolKey);
    for _ in 0..5 {
        parser.bump(false);
    }
    parser.finish_node();
    let close = parser.peek(false).span;
    Key {
        ident: None,
        private: None,
        symbol: true,
        span: Span::new(open.start, close.start, open.byte_start, close.byte_start),
    }
}

/// A field after its key: `[= expr]` and an optional `;`.
fn field(parser: &mut Parser<'_>, is_static: bool, key: &Key) {
    if key.symbol {
        parser.error(
            key.span,
            "a `[Symbol.…]` key names a method or an accessor, not a field (SPEC.md §2 `FieldDecl`)",
        );
    }
    if key.ident.as_deref() == Some("constructor") {
        parser.error(key.span, "a field cannot be named `constructor`");
    }
    if is_static && key.ident.as_deref() == Some("prototype") {
        parser.error(key.span, "a static member cannot be named `prototype`");
    }
    if let Some(private) = &key.private {
        declare_private(parser, private, OTHER, key.span);
    }
    if parser.peek(false).kind == EQ {
        parser.start_node(NodeKind::Initializer);
        let eq = parser.bump(false).span;
        let consumed = parser.with_context(FnContext::Initializer, expressions::assignment);
        if consumed.is_missing() {
            parser.error(eq, "expected an expression after `=`");
        }
        parser.finish_node();
    }
    if !parser.semicolon() {
        let next = parser.peek(false).clone();
        if next.kind != RBRACE && !next.has_newline_before {
            parser.error(next.span, "expected `;` or a newline after a field");
        }
    }
}

/// A method or accessor after its key: `(params) { … }`.
fn method(
    parser: &mut Parser<'_>,
    is_static: bool,
    is_async: bool,
    is_generator: bool,
    accessor: Option<&str>,
    key: &Key,
    constructors: &mut u32,
) {
    let is_constructor = !is_static && key.ident.as_deref() == Some("constructor");
    if is_constructor {
        if accessor.is_some() {
            parser.error(key.span, "`constructor` cannot be an accessor");
        } else if is_async || is_generator {
            parser.error(key.span, "`constructor` cannot be `async` or a generator");
        }
        *constructors += 1;
        if *constructors > 1 {
            parser.error(key.span, "a class has one `constructor`");
        }
    }
    if is_static && key.ident.as_deref() == Some("prototype") {
        parser.error(key.span, "a static member cannot be named `prototype`");
    }
    if let Some(private) = &key.private {
        let role = match accessor {
            Some("get") => GETTER,
            Some("set") => SETTER,
            _ => OTHER,
        };
        declare_private(parser, private, role, key.span);
    }
    let derived_constructor = is_constructor && parser.classes.last().is_some_and(|c| c.derived);
    let context = FnContext::Method {
        is_async,
        is_generator,
        derived_constructor,
    };
    parser.with_context(context, |parser| {
        let Some((count, has_rest)) = functions::params(parser) else {
            skip_member(parser);
            return;
        };
        match accessor {
            Some("get") if count != 0 => parser.error(key.span, "a getter takes no parameters"),
            Some("set") if count != 1 || has_rest => {
                parser.error(key.span, "a setter takes exactly one parameter");
            }
            _ => {}
        }
        if parser.peek(false).kind == LBRACE {
            functions::block(parser);
        } else {
            let span = parser.peek(false).span;
            parser.error(span, "expected `{` to open the method body");
            skip_member(parser);
        }
    });
}

/// Records `#name` as declared with `role`; a repeat is E000 unless it pairs a getter
/// with a setter.
fn declare_private(parser: &mut Parser<'_>, name: &str, role: u8, span: Span) {
    let clash = parser.classes.last_mut().is_some_and(|scope| {
        let roles = scope.declared.entry(name.to_owned()).or_insert(0);
        let clash = *roles & role != 0 || *roles & OTHER != 0 || (role == OTHER && *roles != 0);
        *roles |= role;
        clash
    });
    if clash {
        let message = format!("`#{name}` is declared twice in this class");
        parser.error(span, message);
    }
}

impl Parser<'_> {
    /// True when the token after the next one is the identifier `word`.
    fn peek_second_is_word(&mut self, word: &str) -> bool {
        let token = self.peek_second().clone();
        token.kind == TokenKind::Ident && self.text(&token) == word
    }

    /// Runs `parse` inside a class member's context: a method or accessor
    /// ([`FnContext::Method`]), or a field initializer or static block
    /// ([`FnContext::Initializer`]), where `super.x` is permitted.
    fn with_context<T>(&mut self, context: FnContext, parse: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.contexts.len();
        self.contexts.push(context);
        let result = parse(self);
        self.contexts.truncate(mark);
        result
    }

    /// The removed class expression at the next token: E000, then parsed for
    /// recovery.
    pub(super) fn removed_class_expression(&mut self) {
        let span = self.peek(false).span;
        self.error(
            span,
            "a `class` expression is removed: declare the class, so its name is its type (SPEC.md §4.5)",
        );
        class_decl(self, true);
    }

    /// Notes `#name`, the next token, as used inside the innermost class, for the check
    /// at its end.
    pub(super) fn note_private_reference(&mut self, name: &str) {
        let span = self.peek(false).span;
        if let Some(scope) = self.classes.last_mut() {
            scope.references.push((name.to_owned(), span));
        }
    }

    /// E000 for a `super` the grammar does not permit here: `super()` outside a derived
    /// constructor, `super.x` outside a class member, `super?.x`, or a bare `super`.
    /// Arrows do not bind `super`, so the nearest enclosing non-arrow context decides.
    pub(super) fn check_super(&mut self) {
        let span = self.peek(false).span;
        let member = self
            .contexts
            .iter()
            .rev()
            .find(|context| !matches!(context, FnContext::Arrow { .. }));
        let (in_member, in_derived_constructor) = match member {
            Some(FnContext::Method {
                derived_constructor,
                ..
            }) => (true, *derived_constructor),
            Some(FnContext::Initializer) => (true, false),
            _ => (false, false),
        };
        let message = match self.peek_second().kind {
            LPAREN if in_derived_constructor => return,
            LPAREN => "`super()` is permitted only in the constructor of a class with `extends` (SPEC.md §4.5)",
            TokenKind::Punct(Punct::QuestionDot) => {
                "`super?.x` is removed: `super` is never nullish (SPEC.md §2)"
            }
            DOT | LBRACKET if in_member => return,
            DOT | LBRACKET => "`super.x` is permitted only inside a class member (SPEC.md §2 `SuperProperty`)",
            _ => "`super` is used as `super()` or `super.x`",
        };
        self.error(span, message);
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
    fn every_member_form_parses() {
        assert_eq!(
            clean("class A {}"),
            "(Module (ClassDecl class A (ClassBody { })) <eof>)"
        );
        assert_eq!(
            clean("class A extends B {}\nclass C extends ns.D {}"),
            "(Module (ClassDecl class A (ClassHeritage extends B) (ClassBody { })) (ClassDecl class C (ClassHeritage extends ns . D) (ClassBody { })) <eof>)"
        );
        assert_eq!(
            clean("class A {\n  x\n  y = 1\n  #z = 2\n  static s = 3\n  static #t\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassField x) (ClassField y (Initializer = 1)) (ClassField #z (Initializer = 2)) (ClassField static s (Initializer = 3)) (ClassField static #t) })) <eof>)"
        );
        assert_eq!(
            clean("class A {\n  m() {}\n  async n(a) { await a }\n  *g() { yield 1 }\n  async *h() { yield await 1 }\n  static s() {}\n  static async *t() {}\n  #p() {}\n  static #q() {}\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassMethod m (Params ( )) (Block { })) (ClassMethod async n (Params ( (Param a) )) (Block { (ExprStmt (AwaitExpr await a)) })) (ClassMethod * g (Params ( )) (Block { (ExprStmt (YieldExpr yield 1)) })) (ClassMethod async * h (Params ( )) (Block { (ExprStmt (YieldExpr yield (AwaitExpr await 1))) })) (ClassMethod static s (Params ( )) (Block { })) (ClassMethod static async * t (Params ( )) (Block { })) (ClassMethod #p (Params ( )) (Block { })) (ClassMethod static #q (Params ( )) (Block { })) })) <eof>)"
        );
        assert_eq!(
            clean("class A {\n  get x() { return 1 }\n  set x(v) {}\n  static get y() {}\n  get #z() {}\n  set #z(v) {}\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassAccessor get x (Params ( )) (Block { (ReturnStmt return 1) })) (ClassAccessor set x (Params ( (Param v) )) (Block { })) (ClassAccessor static get y (Params ( )) (Block { })) (ClassAccessor get #z (Params ( )) (Block { })) (ClassAccessor set #z (Params ( (Param v) )) (Block { })) })) <eof>)"
        );
        assert_eq!(
            clean("class A {\n  static {\n    const a = 1\n  }\n  static {}\n}"),
            "(Module (ClassDecl class A (ClassBody { (StaticBlock static (Block { (ConstDecl const a (Initializer = 1)) })) (StaticBlock static (Block { })) })) <eof>)"
        );
        assert_eq!(
            clean("class A {\n  [Symbol.iterator]() {}\n  async *[Symbol.asyncIterator]() {}\n  static [Symbol.dispose]() {}\n  [Symbol.asyncDispose]() {}\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassMethod (SymbolKey [ Symbol . iterator ]) (Params ( )) (Block { })) (ClassMethod async * (SymbolKey [ Symbol . asyncIterator ]) (Params ( )) (Block { })) (ClassMethod static (SymbolKey [ Symbol . dispose ]) (Params ( )) (Block { })) (ClassMethod (SymbolKey [ Symbol . asyncDispose ]) (Params ( )) (Block { })) })) <eof>)"
        );
        assert_eq!(
            clean("class A extends B {\n  constructor(a) {\n    super(a)\n    this.#x = super.y\n  }\n  #x\n}"),
            "(Module (ClassDecl class A (ClassHeritage extends B) (ClassBody { (ClassMethod constructor (Params ( (Param a) )) (Block { (ExprStmt (CallExpr super ( a ))) (ExprStmt (AssignExpr (MemberExpr this . #x) = (MemberExpr super . y))) })) (ClassField #x) })) <eof>)"
        );
        assert_eq!(
            clean("export class A {}\nexport { A as B }"),
            "(Module (ExportDecl export (ClassDecl class A (ClassBody { }))) (ExportDecl export (NamedExports { (ExportSpecifier A as B) })) <eof>)"
        );
        assert_eq!(
            diagnostics("export class A {}\nexport { A }").len(),
            1,
            "the class name is exported"
        );
    }

    #[test]
    fn contextual_words_are_members_when_nothing_follows_them() {
        assert_eq!(
            sexp("class A {\n  static static = 1\n  static\n  get() {}\n  set() {}\n  async() {}\n  get = 2\n  static get = 3\n  static async = 4\n  constructor() {}\n  ;\n  ;\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassField static static (Initializer = 1)) (ClassMethod static get (Params ( )) (Block { })) (ClassMethod set (Params ( )) (Block { })) (ClassMethod async (Params ( )) (Block { })) (ClassField get (Initializer = 2)) (ClassField static get (Initializer = 3)) (ClassField static async (Initializer = 4)) (ClassMethod constructor (Params ( )) (Block { })) ; ; })) <eof>)"
        );
        assert_eq!(
            sexp("class A { ; }"),
            "(Module (ClassDecl class A (ClassBody { ; })) <eof>)"
        );
        let text =
            "class A {\n  async\n  m() {}\n  static\n  n() {}\n  get\n  x() {}\n  x = 1; y = 2\n}";
        assert_eq!(
            diagnostics(text)
                .iter()
                .map(|d| (d.0, d.1, d.2))
                .collect::<Vec<_>>(),
            [(Code::S002, 8, 8)],
            "the `;` between the fields is style"
        );
        assert_eq!(
            sexp(text),
            "(Module (ClassDecl class A (ClassBody { (ClassField async) (ClassMethod m (Params ( )) (Block { })) (ClassMethod static n (Params ( )) (Block { })) (ClassAccessor get x (Params ( )) (Block { })) (ClassField x (Initializer = 1) ;) (ClassField y (Initializer = 2)) })) <eof>)",
            "`async` ends at a newline; `static` and `get` do not"
        );
        assert_eq!(
            diagnostics("const class = 2"),
            vec![(
                Code::E000,
                1,
                7,
                "expected a binding name, `{` or `[`".to_owned()
            )],
            "a `class` that begins no class is left to whoever reported it"
        );
        assert_eq!(
            diagnostics("f(class)")
                .iter()
                .map(|d| d.3.as_str())
                .collect::<Vec<_>>(),
            ["expected an argument or `)`"],
            "in an operand it is no expression"
        );
        assert_eq!(
            clean("class A {\n  static async *[Symbol.iterator]() {}\n  if() {}\n  class = 1\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassMethod static async * (SymbolKey [ Symbol . iterator ]) (Params ( )) (Block { })) (ClassMethod if (Params ( )) (Block { })) (ClassField class (Initializer = 1)) })) <eof>)",
            "keywords are member names"
        );
    }

    #[test]
    fn early_errors_are_e000_once() {
        check_e000(&[
            (
                "const A = class {}",
                1,
                11,
                "a `class` expression is removed",
            ),
            ("f(class B {})", 1, 3, "a `class` expression is removed"),
            (
                "class A {\n  *constructor() {}\n}",
                2,
                4,
                "`constructor` cannot be `async` or a generator",
            ),
            (
                "class A {\n  async constructor() {}\n}",
                2,
                9,
                "`constructor` cannot be `async` or a generator",
            ),
            (
                "class A {\n  get constructor() {}\n}",
                2,
                7,
                "`constructor` cannot be an accessor",
            ),
            (
                "class A {\n  constructor() {}\n  constructor() {}\n}",
                3,
                3,
                "a class has one `constructor`",
            ),
            (
                "class A {\n  #constructor = 1\n}",
                2,
                3,
                "`#constructor` is not a valid private name",
            ),
            (
                "class A {\n  constructor = 1\n}",
                2,
                3,
                "a field cannot be named `constructor`",
            ),
            (
                "class A {\n  static prototype = 1\n}",
                2,
                10,
                "a static member cannot be named `prototype`",
            ),
            (
                "class A {\n  static prototype() {}\n}",
                2,
                10,
                "a static member cannot be named `prototype`",
            ),
            (
                "class A {\n  m() { return this.#x }\n}",
                2,
                21,
                "`#x` is not declared in this class",
            ),
            ("class A {\n  #x\n  #x\n}", 3, 3, "`#x` is declared twice"),
            (
                "class A {\n  #x\n  get #x() {}\n}",
                3,
                7,
                "`#x` is declared twice",
            ),
            (
                "class A {\n  get #x() {}\n  get #x() {}\n}",
                3,
                7,
                "`#x` is declared twice",
            ),
            (
                "class A {\n  set #x(v) {}\n  #x() {}\n}",
                3,
                3,
                "`#x` is declared twice",
            ),
        ]);
        assert_eq!(
            sexp("const A = class {}"),
            "(Module (ConstDecl const A (Initializer = (ClassExpr class (ClassBody { })))) <eof>)",
            "parsed for recovery"
        );
    }

    #[test]
    fn malformed_classes_are_e000_once() {
        check_e000(&[
            ("class A extends a.b.c {}", 1, 20, "one `.` at most"),
            ("class A extends 1 {}", 1, 17, "`extends` names a class"),
            ("class A extends a. {}", 1, 20, "expected a name after `.`"),
            (
                "class A {\n  [x]() {}\n}",
                2,
                3,
                "a computed key is removed",
            ),
            (
                "class A {\n  [Symbol.hasInstance]() {}\n}",
                2,
                3,
                "a computed key is removed",
            ),
            (
                "class A {\n  [Symbol.iterator] = 1\n}",
                2,
                3,
                "names a method or an accessor, not a field",
            ),
            (
                "class A {\n  get x(a) {}\n}",
                2,
                7,
                "a getter takes no parameters",
            ),
            (
                "class A {\n  set x() {}\n}",
                2,
                7,
                "a setter takes exactly one parameter",
            ),
            (
                "class A {\n  set x(...a) {}\n}",
                2,
                7,
                "a setter takes exactly one parameter",
            ),
            (
                "class A {\n  m() {}\n  1\n}",
                3,
                3,
                "expected a class member",
            ),
            (
                "class A {
  1 2 3; m() {}
}",
                2,
                3,
                "expected a class member",
            ),
            (
                "class A {
  4 5
  n() {}
}",
                2,
                3,
                "expected a class member",
            ),
        ]);
    }

    #[test]
    fn a_skipped_member_keeps_its_body_braces_balanced() {
        for text in [
            "class A {\n  m(1) { a; b }\n  n() {}\n}",
            "class A {\n  m(1) {\n    a\n  }\n  n() {}\n}",
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert!(
                sexp(text).contains("(ClassMethod n"),
                "{text}: n still parses"
            );
        }
    }

    #[test]
    fn a_member_skipped_at_the_end_of_the_file_leaves_one_report_for_the_class() {
        let found = diagnostics("class A {\n  m(1) {");
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].3.contains("expected a binding name"), "{found:?}");
        assert!(
            found[1].3.contains("expected `}` to close the class body"),
            "{found:?}"
        );
    }

    #[test]
    fn malformed_members_are_e000_once() {
        check_e000(&[
            (
                "class A {\n  x y\n}",
                2,
                5,
                "expected `;` or a newline after a field",
            ),
            (
                "class A {\n  get *x() {}\n}",
                2,
                7,
                "expected `;` or a newline after a field",
            ),
            (
                "class A {\n  *x = 1\n}",
                2,
                3,
                "`*` marks a generator method, not a field",
            ),
            (
                "class A extends 1 + 2 {\n  m() {}\n}",
                1,
                17,
                "`extends` names a class",
            ),
            (
                "class A {\n  [Foo.iterator]() {}\n}",
                2,
                3,
                "a computed key is removed",
            ),
            (
                "class A {\n  get x(...a) {}\n}",
                2,
                7,
                "a getter takes no parameters",
            ),
            (
                "class A {\n  x = this.#y\n}",
                2,
                12,
                "`#y` is not declared in this class",
            ),
            ("class A {\n  #a = 1\n  # b = 2\n}", 3, 3, "private name"),
            (
                "class A {\n  x =\n}",
                2,
                5,
                "expected an expression after `=`",
            ),
            (
                "class A {\n  m() 1\n}",
                2,
                7,
                "expected `{` to open the method body",
            ),
            ("class A { 1 }", 1, 11, "expected a class member"),
            (
                "class A { m() 1 }",
                1,
                15,
                "expected `{` to open the method body",
            ),
            ("class A {\n  m(1) {}\n}", 2, 5, "expected a binding name"),
            ("class A", 1, 8, "expected `{` to open the class body"),
            ("class {}", 1, 7, "a `class` declaration needs a name"),
            (
                "class A {\n  m() {}",
                2,
                9,
                "expected `}` to close the class body",
            ),
        ]);
        assert_eq!(
            sexp("class A {\n  [x + 1]() {}\n  m() {}\n}"),
            "(Module (ClassDecl class A (ClassBody { (ClassMethod [ x + 1 ] (Params ( )) (Block { })) (ClassMethod m (Params ( )) (Block { })) })) <eof>)",
            "a rejected computed key is skipped to its `]`"
        );
    }

    #[test]
    fn super_is_checked_against_the_member_it_sits_in() {
        check_e000(&[
            (
                "class A {\n  constructor() { super() }\n}",
                2,
                19,
                "`super()` is permitted only in the constructor of a class with `extends`",
            ),
            (
                "class A extends B {\n  m() { super() }\n}",
                2,
                9,
                "`super()` is permitted only in the constructor",
            ),
            (
                "class A extends B {\n  static { super() }\n}",
                2,
                12,
                "`super()` is permitted only in the constructor",
            ),
            (
                "class A extends B {\n  x = super()\n}",
                2,
                7,
                "`super()` is permitted only in the constructor",
            ),
            (
                "super.x",
                1,
                1,
                "`super.x` is permitted only inside a class member",
            ),
            (
                "function f() { super.x }",
                1,
                16,
                "`super.x` is permitted only inside a class member",
            ),
            (
                "class A extends B {\n  m() { super?.x }\n}",
                2,
                9,
                "`super?.x` is removed",
            ),
            (
                "class A extends B {\n  m() { super }\n}",
                2,
                9,
                "`super` is used as `super()` or `super.x`",
            ),
            (
                "class A extends B {\n  m() { const f = () => super() }\n}",
                2,
                25,
                "`super()` is permitted only in the constructor",
            ),
        ]);
        for text in [
            "class A extends B {\n  constructor() { super()\n    const f = () => super.x }\n  static m() { return super.x }\n  get g() { return super[\"y\"] }\n  x = super.z\n  static { super.w }\n  #p() { return super.q }\n}",
            "class A extends B {\n  constructor() {\n    const g = () => { super() }\n  }\n}",
        ] {
            assert_eq!(diagnostics(text), Vec::new(), "{text}");
        }
    }

    #[test]
    fn private_names_resolve_across_nested_classes() {
        assert_eq!(
            diagnostics("class A {\n  #x = 1\n  m() {\n    class B {\n      n() { return this.#x + this.#y }\n      #y\n    }\n    return this.#x\n  }\n}"),
            Vec::new(),
            "an inner class reaches the outer class's names"
        );
        let found = diagnostics(
            "class A {\n  m() {\n    class B {\n      #y\n    }\n    return this.#y\n  }\n}",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].1, found[0].2),
            (6, 17),
            "the outer class cannot reach the inner class's name"
        );
        assert_eq!(
            diagnostics("const o = { p: this.#x }"),
            Vec::new(),
            "outside a class the checker's E126 applies, not this one"
        );
        assert_eq!(
            diagnostics("o.super.x\nconst a = o.super.y"),
            Vec::new(),
            "`super` after a dot is a property name"
        );
        assert_eq!(
            sexp("class A { 1; m() {} }"),
            "(Module (ClassDecl class A (ClassBody { (ClassField 1 ;) (ClassMethod m (Params ( )) (Block { })) })) <eof>)",
            "a malformed member ends at `;`"
        );
    }

    #[test]
    fn classes_print_back_to_their_source() {
        let text = "class  A  extends  ns.B {\n  static  #x = 1 ;\n  async  *m ( a ) { // c\n    yield await a\n  }\n  get [Symbol.iterator] () {}\n}\n";
        let f = file(text);
        let (tree, found) = parse(&f);
        assert!(
            found.iter().all(|d| d.code == Code::S002),
            "only the `;` is reported: {found:?}"
        );
        assert_eq!(tree.print(&f), text);
    }

    #[test]
    fn every_semicolon_in_a_class_body_is_s002() {
        let found = diagnostics("class A {\n  x = 1;\n  ;\n  m() {}\n  ;\n}");
        assert_eq!(
            found.iter().map(|d| (d.0, d.1, d.2)).collect::<Vec<_>>(),
            [(Code::S002, 2, 8), (Code::S002, 3, 3), (Code::S002, 5, 3)]
        );
    }
}
