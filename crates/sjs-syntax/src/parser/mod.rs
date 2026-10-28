//! The parser driver (SPEC.md §11.1): owns the lexer, pulls tokens with the
//! regex-allowed hint (US-015) and builds the [`SyntaxTree`]. A module is its import
//! declarations (US-021, `imports.rs`) followed by its items, among which export
//! declarations (US-022, `exports.rs`) are recognised; everything else stays a list of
//! tokens until the grammar stories add their productions.
//!
//! A `/` is lexed as a division until the grammar asks for an expression there; then
//! the lookahead is lexed again as a regex literal. Division lexing never reports
//! anything, so re-lexing leaves no stale diagnostic. The reverse never happens: once a
//! regex has been asked for, it stays.

mod bindings;
mod classes;
mod exports;
mod functions;
mod imports;

use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};

use crate::lexer::{regex_allowed_after, Checkpoint, Lexer};
use crate::token::{Keyword, Punct, TemplatePart, Token, TokenKind};
use crate::tree::{NodeId, NodeKind, SyntaxTree, TokenId, TreeBuilder};

/// Parses a whole file into its tree and diagnostics. Never fails.
#[must_use]
pub fn parse(file: &SourceFile) -> (SyntaxTree, Diagnostics) {
    let mut parser = Parser::new(file);
    parser.start_node(NodeKind::Module);
    imports::import_section(&mut parser);
    items_in(&mut parser, Scope::Module);
    parser.finish_node();
    parser.finish()
}

/// Where a list of items sits: the module's top level, or a block (a function body or,
/// in the statement story, any `{ … }`), which ends at its `}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Module,
    Block,
}

/// The function a token sits in, for `yield` and `await`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FnContext {
    Function {
        is_async: bool,
        is_generator: bool,
    },
    Arrow {
        is_async: bool,
    },
    /// A class member: a method, an accessor, a field initializer or a static block.
    /// `super.x` is permitted inside one; `super()` only in a derived constructor.
    Method {
        is_async: bool,
        is_generator: bool,
        derived_constructor: bool,
    },
}

/// The items of a scope: declarations (`const`, `let`, `function`), export
/// declarations, and otherwise a list of tokens until the grammar stories land. An
/// import declaration here is E000 (SPEC.md §2: imports come first) and an export
/// inside a block is E000 (exports sit at the top level); both are then parsed anyway
/// so the tree still records them. `import(` is a dynamic import, an expression, and
/// `a.import`, `a.export`, `{ export: 1 }` or a method `export() {}` are property
/// names: none of them is a declaration. In a block the loop stops before the `}` that
/// closes it.
pub(super) fn items_in(parser: &mut Parser<'_>, scope: Scope) {
    let mut previous = TokenKind::Eof;
    // After `await`, `yield` or the `of` of a `for` head an operand follows, so a `/`
    // there starts a regex (the expression and loop stories will say so from the
    // grammar; the token kinds alone call these identifiers).
    let mut previous_of = false;
    let mut depth: u32 = 0;
    let mut exported = exports::Exported::new();
    // For each open `{`, whether it began an object literal (so its `}` ends a value
    // and a `/` after it divides) rather than a block.
    let mut braces: Vec<bool> = Vec::new();
    loop {
        let regex_allowed = previous_of || regex_allowed_after(&previous);
        let kind = parser.peek(regex_allowed).kind.clone();
        let on_new_line = parser.peek(false).has_newline_before;
        let after_dot = matches!(previous, TokenKind::Punct(Punct::Dot | Punct::QuestionDot));
        match kind {
            TokenKind::Eof => {
                if scope == Scope::Module {
                    parser.bump(false);
                }
                return;
            }
            TokenKind::Punct(Punct::RBrace) if scope == Scope::Block && depth == 0 => return,
            TokenKind::Keyword(Keyword::Import)
                if !after_dot && parser.peek_second().kind != TokenKind::Punct(Punct::LParen) =>
            {
                let span = parser.peek(false).span;
                parser.error(
                    span,
                    "an import must precede every other module item (SPEC.md §2 `Module`)",
                );
                imports::import_decl(parser);
                previous = TokenKind::Eof;
            }
            TokenKind::Keyword(Keyword::Export)
                if !after_dot
                    && !matches!(
                        parser.peek_second().kind,
                        TokenKind::Punct(Punct::Colon | Punct::LParen)
                    ) =>
            {
                if depth > 0 || scope == Scope::Block {
                    let span = parser.peek(false).span;
                    parser.error(
                        span,
                        "an `export` belongs at the top level of the module, not inside a block (SPEC.md §2 `Module`)",
                    );
                }
                exports::export_decl(parser, &mut exported);
                previous = TokenKind::Eof;
            }
            TokenKind::Keyword(Keyword::Const | Keyword::Let)
                if statement_start(&previous, on_new_line) =>
            {
                bindings::declaration(parser);
                previous = TokenKind::Eof;
            }
            _ if statement_start(&previous, on_new_line) && functions::at_function(parser) => {
                functions::function(parser, false);
                previous = TokenKind::Eof;
            }
            TokenKind::Keyword(Keyword::Class)
                if statement_start(&previous, on_new_line) && classes::at_class(parser) =>
            {
                classes::class_decl(parser, false);
                previous = TokenKind::Eof;
            }
            TokenKind::Keyword(Keyword::Class) if !after_dot && classes::at_class(parser) => {
                parser.removed_class_expression();
                previous = TokenKind::Punct(Punct::RParen);
            }
            _ if !after_dot && parser.at_arrow() => {
                functions::arrow(parser, &[]);
                previous = TokenKind::Punct(Punct::RParen);
            }
            _ if !after_dot && functions::at_function(parser) => {
                parser.removed_function_expression();
                previous = TokenKind::Punct(Punct::RParen);
            }
            _ => {
                match kind {
                    TokenKind::Punct(Punct::LBrace) => {
                        depth += 1;
                        // `previous_of`: an operand follows the word before (`yield {}`).
                        braces.push(previous_of || opens_object_literal(&previous, on_new_line));
                    }
                    TokenKind::Punct(Punct::RBrace) => {
                        depth = depth.saturating_sub(1);
                        if braces.pop() == Some(true) {
                            previous_of = parser.operand_follows(after_dot, &previous);
                            parser.bump(regex_allowed);
                            previous = TokenKind::Punct(Punct::RParen);
                            continue;
                        }
                    }
                    TokenKind::Keyword(Keyword::Super) if !after_dot => parser.check_super(),
                    TokenKind::PrivateName(ref name) => parser.note_private_reference(name),
                    TokenKind::Ident if !after_dot => parser.check_yield_await(),
                    _ => {}
                }
                previous_of = parser.operand_follows(after_dot, &previous);
                parser.bump(regex_allowed);
                previous = kind;
            }
        }
    }
}

/// Whether an expression may end with `previous`, so that a line break after it ends
/// the statement: a value, a closing bracket, a postfix operator or `}`; not an
/// operator, `.`, `,`, `=>` or a keyword that takes an operand.
fn can_end_expression(previous: &TokenKind) -> bool {
    match previous {
        TokenKind::Punct(punct) => matches!(
            punct,
            Punct::RParen | Punct::RBracket | Punct::RBrace | Punct::PlusPlus | Punct::MinusMinus
        ),
        TokenKind::Keyword(keyword) => !matches!(
            keyword,
            Keyword::Typeof
                | Keyword::New
                | Keyword::Instanceof
                | Keyword::In
                | Keyword::Void
                | Keyword::Delete
        ),
        // A template head or middle never sits at bracket depth zero, so any template
        // token here is a whole or a tail: a value.
        _ => true,
    }
}

/// Whether `previous` can end the binding of a `for (… of …)` head: only there is a
/// value juxtaposed to the word `of`, so an `of` after one is the keyword and the `/`
/// after that starts a regex; `of / 2` with `=` before `of` is a division.
fn pattern_end(previous: &TokenKind) -> bool {
    matches!(
        previous,
        TokenKind::Ident | TokenKind::Punct(Punct::RBracket | Punct::RBrace)
    )
}

/// Whether a `{` after `previous` opens an object literal rather than a block: where an
/// operand is expected (after an operator, `=`, `(`, `,`, `:`, `return`, `throw`, …)
/// and not at the start of a statement.
fn opens_object_literal(previous: &TokenKind, on_new_line: bool) -> bool {
    !statement_start(previous, on_new_line)
        && (!can_end_expression(previous)
            || matches!(
                previous,
                TokenKind::Keyword(Keyword::Return | Keyword::Throw | Keyword::Case)
            ))
}

/// Whether a token after `previous` begins a statement: at the start, after `;`, `{` or
/// `}`, or on a new line. Until the statement grammar lands this is how a `const` or
/// `let` keyword is told from a property name (`a.const`) or a `for` head.
fn statement_start(previous: &TokenKind, on_new_line: bool) -> bool {
    on_new_line
        || matches!(
            previous,
            TokenKind::Eof | TokenKind::Punct(Punct::Semicolon | Punct::LBrace | Punct::RBrace)
        )
}

/// Up to two tokens of lookahead over the lexer, feeding a [`TreeBuilder`].
pub struct Parser<'a> {
    file: &'a SourceFile,
    lexer: Lexer<'a>,
    builder: TreeBuilder,
    lookahead: Vec<Token>,
    /// The lexer's state before each lookahead token, so a `/` can be lexed again.
    checkpoints: Vec<Checkpoint>,
    diagnostics: Diagnostics,
    /// The enclosing functions, innermost last; empty at the module's top level.
    contexts: Vec<FnContext>,
    /// The enclosing classes, innermost last.
    classes: Vec<classes::ClassScope>,
}

impl<'a> Parser<'a> {
    /// A parser at the start of `file`.
    #[must_use]
    pub fn new(file: &'a SourceFile) -> Self {
        Parser {
            file,
            lexer: Lexer::new(file),
            builder: TreeBuilder::new(),
            lookahead: Vec::new(),
            checkpoints: Vec::new(),
            diagnostics: Diagnostics::new(),
            contexts: Vec::new(),
            classes: Vec::new(),
        }
    }

    /// The next token without consuming it. With `regex_allowed`, a `/` or `/=` there
    /// is lexed again as a regex literal.
    pub fn peek(&mut self, regex_allowed: bool) -> &Token {
        self.fill(1);
        if regex_allowed
            && matches!(
                self.lookahead[0].kind,
                TokenKind::Punct(Punct::Slash | Punct::SlashEq)
            )
        {
            let token_start = self.lookahead[0].span.byte_start;
            let checkpoint = self.checkpoints[0].clone();
            self.lexer.restore(checkpoint, token_start);
            self.lookahead.clear();
            self.checkpoints.clear();
            self.checkpoints.push(self.lexer.checkpoint());
            let regex = self.lexer.next_token(true);
            self.lookahead.push(regex);
        }
        &self.lookahead[0]
    }

    /// The token after the next one, always lexed as division: it only ever decides
    /// between forms, never starts an expression.
    pub fn peek_second(&mut self) -> &Token {
        self.peek_nth(1)
    }

    /// The token `offset` places after the next one (`peek_nth(0)` is the next token),
    /// lexed as division like [`Parser::peek_second`].
    pub fn peek_nth(&mut self, offset: usize) -> &Token {
        self.fill(offset + 1);
        &self.lookahead[offset]
    }

    /// Consumes the next token into the innermost open node and returns it.
    pub fn bump(&mut self, regex_allowed: bool) -> &Token {
        self.peek(regex_allowed);
        // `peek` filled the lookahead, so the front exists.
        let token = self.lookahead.remove(0);
        self.checkpoints.remove(0);
        let id = self.builder.token(token);
        self.token(id)
    }

    fn fill(&mut self, count: usize) {
        while self.lookahead.len() < count {
            self.checkpoints.push(self.lexer.checkpoint());
            let token = self.lexer.next_token(false);
            self.lookahead.push(token);
        }
    }

    /// The source text of a token.
    #[must_use]
    pub fn text(&self, token: &Token) -> &'a str {
        self.file.slice(&token.span).unwrap_or("")
    }

    /// True when the next token is the identifier `word` (a contextual keyword).
    pub fn at_word(&mut self, word: &str) -> bool {
        let token = self.peek(false);
        let (is_ident, span) = (token.kind == TokenKind::Ident, token.span);
        is_ident && self.file.slice(&span) == Some(word)
    }

    /// Consumes the next token when it is `kind`.
    pub fn eat(&mut self, kind: &TokenKind) -> bool {
        if &self.peek(false).kind == kind {
            self.bump(false);
            true
        } else {
            false
        }
    }

    /// E000 at `span`.
    pub fn error(&mut self, span: Span, message: impl Into<String>) {
        self.report(Code::E000, span, message);
    }

    /// E000 at `span` carrying the text as written and the text to write instead
    /// (SPEC.md §12 replacements that are mechanical).
    pub fn error_with_fix(
        &mut self,
        span: Span,
        message: impl Into<String>,
        current: String,
        replacement: String,
    ) {
        let diagnostic = Diagnostic::new(Code::E000, self.file.path(), span, message)
            .with_fix(current, replacement);
        self.diagnostics.extend(diagnostic.ok());
    }

    /// Consumes the extent of an expression until the expression stories land: up to
    /// a token in `stops` at bracket depth zero, `;`, an unmatched closer or the end of
    /// the file, or a line break at depth zero after a token that can end an
    /// expression. `(`, `[`, `{` and template substitutions nest. Arrow functions and
    /// the removed `function` expression are parsed where they start (US-024), and
    /// `yield`/`await` are checked against the enclosing function. Returns how many
    /// tokens were consumed.
    pub fn raw_expression(&mut self, stops: &[TokenKind]) -> usize {
        let mut depth: u32 = 0;
        // `?` opens a conditional and `:` closes it; a `:` with none open belongs to an
        // enclosing conditional and ends this expression.
        let mut conditionals: u32 = 0;
        let mut previous = TokenKind::Punct(Punct::Eq);
        let mut previous_of = false;
        let mut consumed = 0;
        loop {
            let regex_allowed = previous_of || regex_allowed_after(&previous);
            let token = self.peek(regex_allowed);
            let kind = token.kind.clone();
            let on_new_line = token.has_newline_before;
            if kind == TokenKind::Eof || (depth == 0 && stops.contains(&kind)) {
                return consumed;
            }
            if depth == 0 && kind == TokenKind::Punct(Punct::Colon) {
                if conditionals == 0 {
                    return consumed;
                }
                conditionals -= 1;
            }
            if depth == 0 && kind == TokenKind::Punct(Punct::Question) {
                conditionals += 1;
            }
            if depth == 0
                && (matches!(
                    kind,
                    TokenKind::Punct(
                        Punct::Semicolon | Punct::RBrace | Punct::RParen | Punct::RBracket
                    )
                ) || (on_new_line && can_end_expression(&previous)))
            {
                return consumed;
            }
            let after_dot = matches!(previous, TokenKind::Punct(Punct::Dot | Punct::QuestionDot));
            if !after_dot && self.at_arrow() {
                functions::arrow(self, stops);
                consumed += 1;
                previous = TokenKind::Punct(Punct::RParen);
                continue;
            }
            if !after_dot && functions::at_function(self) {
                self.removed_function_expression();
                consumed += 1;
                previous = TokenKind::Punct(Punct::RParen);
                continue;
            }
            if !after_dot && classes::at_class(self) {
                self.removed_class_expression();
                consumed += 1;
                previous = TokenKind::Punct(Punct::RParen);
                continue;
            }
            match &kind {
                TokenKind::Punct(Punct::LParen | Punct::LBracket | Punct::LBrace) => depth += 1,
                TokenKind::Punct(Punct::RParen | Punct::RBracket | Punct::RBrace) => depth -= 1,
                TokenKind::Template(template) => match template.part {
                    TemplatePart::Head => depth += 1,
                    TemplatePart::Tail => depth -= 1,
                    TemplatePart::Middle | TemplatePart::Whole => {}
                },
                TokenKind::Keyword(Keyword::Super) if !after_dot => self.check_super(),
                TokenKind::PrivateName(ref name) => self.note_private_reference(name),
                TokenKind::Ident if !after_dot => self.check_yield_await(),
                _ => {}
            }
            previous_of = self.operand_follows(after_dot, &previous);
            self.bump(regex_allowed);
            consumed += 1;
            // Every `}` consumed here closes an object literal: a value, after which a
            // `/` divides (blocks belong to arrows and functions, parsed apart).
            previous = if kind == TokenKind::Punct(Punct::RBrace) {
                TokenKind::Punct(Punct::RParen)
            } else {
                kind
            };
        }
    }

    /// The removed `function` expression at the next token: E000, then parsed for
    /// recovery.
    fn removed_function_expression(&mut self) {
        let span = self.peek(false).span;
        self.error(
            span,
            "a `function` expression is removed: write an arrow function or a `function` declaration (SPEC.md §4.4)",
        );
        functions::function(self, true);
    }

    /// True when an arrow function starts at the next token: `(…) =>`, `async (…) =>`,
    /// `a =>` or `async a =>`.
    pub(super) fn at_arrow(&mut self) -> bool {
        match self.peek(false).kind.clone() {
            TokenKind::Punct(Punct::LParen) => functions::arrow_ahead(self, 0),
            TokenKind::Ident if self.at_word("async") => match self.peek_second().kind {
                TokenKind::Punct(Punct::LParen) => functions::arrow_ahead(self, 1),
                TokenKind::Ident => self.peek_nth(2).kind == TokenKind::Punct(Punct::Arrow),
                _ => self.peek_second().kind == TokenKind::Punct(Punct::Arrow),
            },
            TokenKind::Ident => self.peek_second().kind == TokenKind::Punct(Punct::Arrow),
            _ => false,
        }
    }

    /// True when the next token is a word an operand follows, so a `/` after it starts a
    /// regex: `await`, `yield`, or the `of` of a `for` head (an `of` juxtaposed to a
    /// binding; `of / 2` after `=` is a division). Not after `.`, where any word is a
    /// property name.
    fn operand_follows(&mut self, after_dot: bool, previous: &TokenKind) -> bool {
        !after_dot
            && (self.at_word("await")
                || self.at_word("yield")
                || (self.at_word("of") && pattern_end(previous)))
    }

    /// E000 for `yield` outside a generator or `await` inside a function that is not
    /// `async`, when the next token is that word used as a value: not a property key
    /// (`yield:`) and not a method name (`yield() {}`, which until the class story is
    /// told apart by the `(` alone).
    fn check_yield_await(&mut self) {
        if matches!(
            self.peek_second().kind,
            TokenKind::Punct(Punct::Colon | Punct::LParen)
        ) {
            return;
        }
        let span = self.peek(false).span;
        let text = self.file.slice(&span).unwrap_or("");
        match text {
            "yield" => {
                let ok = matches!(
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
                );
                if !ok {
                    self.error(
                        span,
                        "`yield` is permitted only directly inside a generator function (SPEC.md §4.4)",
                    );
                }
            }
            "await" => {
                let ok = matches!(
                    self.contexts.last(),
                    None | Some(
                        FnContext::Function { is_async: true, .. }
                            | FnContext::Arrow { is_async: true }
                            | FnContext::Method { is_async: true, .. }
                    )
                );
                if !ok {
                    self.error(
                        span,
                        "`await` is permitted only inside an `async` function or at the top level of a module (SPEC.md §5.1)",
                    );
                }
            }
            _ => {}
        }
    }

    /// A diagnostic at `span`.
    pub fn report(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(code, self.file.path(), span, message));
    }

    /// Opens a node.
    pub fn start_node(&mut self, kind: NodeKind) -> NodeId {
        self.builder.start_node(kind)
    }

    /// Closes the innermost open node.
    pub fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    /// The tree and every diagnostic, consuming the parser. A token peeked but never
    /// consumed is not part of the tree.
    #[must_use]
    pub fn finish(self) -> (SyntaxTree, Diagnostics) {
        let mut diagnostics = self.lexer.finish();
        diagnostics.extend(self.diagnostics.iter().cloned());
        (self.builder.finish(), diagnostics)
    }

    fn token(&self, id: TokenId) -> &Token {
        self.builder.token_ref(id)
    }
}

#[cfg(test)]
mod tests {
    use super::functions::{self, ARROW_SCAN_LIMIT};
    use super::{parse, Parser};
    use crate::lex;
    use crate::token::{Punct, Regex, TokenKind};
    use crate::tree::{Element, NodeKind, TokenRef};
    use sjs_diag::{Diagnostics, SourceFile};
    use sjs_testkit::conformance::discover;
    use sjs_testkit::prng::Rng;
    use sjs_testkit::program::{Options, ProgramGen};
    use sjs_testkit::prop::{check_with, Config, Gen, Text};
    use std::path::Path;

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    /// The tree prints back to the source and agrees with `lex` token for token; its
    /// root is a `Module` without a parent whose tokens are every token, `Eof` last.
    /// Returns the parser's diagnostics for the caller to compare with the lexer's.
    fn check(f: &SourceFile) -> (Diagnostics, Diagnostics) {
        let (tree, diagnostics) = parse(f);
        let (stream, lexed) = lex(f);
        assert_eq!(tree.print(f), f.text());
        // The first token that differs, if any, with the source around it; index 0
        // and an empty snippet otherwise, so the assertion below passes.
        let index = (0..tree.tokens().len().max(stream.tokens.len()))
            .find(|&i| tree.tokens().get(i) != stream.tokens.get(i))
            .unwrap_or(0);
        let at = tree.tokens().get(index).map_or(0, |t| t.span.byte_start);
        let snippet = &f.text()[at.saturating_sub(60)..(at + 60).min(f.len())];
        assert_eq!(
            tree.tokens().get(index),
            stream.tokens.get(index),
            "token {index} differs near {snippet:?}"
        );
        let root = tree.root();
        assert_eq!(root.kind(), NodeKind::Module);
        assert!(root.parent().is_none());
        assert_eq!(root.tokens().len(), stream.tokens.len());
        assert_eq!(root.span().byte_end, f.len());
        let last = root.children().last().expect("Eof");
        let eof = as_token(last).expect("the last child is the Eof token");
        assert_eq!(eof.token().kind, TokenKind::Eof);
        assert_eq!(eof.parent().id(), root.id());
        for lexed_diagnostic in lexed.iter() {
            assert!(
                diagnostics.iter().any(|d| d == lexed_diagnostic),
                "the parser keeps every lexer diagnostic: {lexed_diagnostic:?}"
            );
        }
        (diagnostics, lexed)
    }

    /// `check`, for text without imports: one node, every child a token, and no
    /// diagnostic beyond the lexer's.
    fn check_flat(f: &SourceFile) {
        let (diagnostics, lexed) = check(f);
        assert_eq!(diagnostics, lexed);
        let (tree, _) = parse(f);
        assert_eq!(tree.node_count(), 1);
        assert!(tree
            .root()
            .children()
            .all(|child| as_token(child).is_some()));
    }

    fn as_token(child: Element<'_>) -> Option<TokenRef<'_>> {
        match child {
            Element::Token(token) => Some(token),
            Element::Node(_) => None,
        }
    }

    #[test]
    fn the_test_helper_tells_nodes_from_tokens() {
        let f = file("a");
        let (tree, _) = parse(&f);
        assert!(as_token(Element::Node(tree.root())).is_none());
        assert!(as_token(tree.root().children().next().expect("a")).is_some());
    }

    #[test]
    fn nodes_opened_by_the_driver_close_where_the_grammar_says() {
        let f = file("a b");
        let mut parser = Parser::new(&f);
        parser.start_node(NodeKind::Module);
        let inner = parser.start_node(NodeKind::Module);
        parser.bump(false);
        parser.finish_node();
        parser.bump(false);
        parser.bump(false);
        parser.finish_node();
        let (tree, _) = parser.finish();
        assert_eq!(
            tree.node(inner).tokens().len(),
            1,
            "only `a` is inside the inner node"
        );
        assert_eq!(
            tree.root().tokens().len(),
            3,
            "`a`, `b` and Eof are inside the root"
        );
        assert_eq!(
            tree.root().children().count(),
            3,
            "the inner node, `b` and Eof"
        );
    }

    #[test]
    fn a_module_is_a_list_of_tokens_that_prints_back_to_the_source() {
        for text in [
            "",
            "a = 1",
            "  // only\n",
            "x = /a/v\n`${a}`",
            "\"\u{FEFF}\"",
            "@ $ \\u",
        ] {
            check_flat(&file(text));
        }
    }

    #[test]
    fn the_corpus_round_trips_byte_for_byte() {
        let gen = ProgramGen {
            options: Options {
                max_depth: 4,
                max_items: 6,
                doubles: true,
            },
        };
        for seed in 0..2000u64 {
            let text = gen.generate(&mut Rng::from_seed(seed)).to_string();
            let f = file(&text);
            let (tree, diagnostics) = parse(&f);
            assert_eq!(tree.print(&f), text, "seed {seed}");
            assert_eq!(tree.root().kind(), NodeKind::Module);
            assert_eq!(
                diagnostics.len(),
                0,
                "seed {seed}: a generated program parses clean: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn every_conformance_fixture_round_trips_byte_for_byte() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
        let fixtures = discover(&root).expect("fixtures are readable");
        assert_ne!(fixtures.len(), 0);
        let mut checked = 0;
        for fixture in &fixtures {
            for fixture_file in &fixture.files {
                if let Ok(f) =
                    SourceFile::new(fixture_file.relative.clone(), fixture_file.bytes.clone())
                {
                    check(&f);
                    checked += 1;
                }
            }
        }
        assert!(checked > 10, "{checked} valid fixture files");
    }

    #[test]
    fn random_text_round_trips() {
        let gen = Text {
            alphabet: &[
                'a', ' ', '\n', '/', '*', '`', '$', '{', '}', '"', '\\', '1', '.', 'é',
            ],
            max_len: 32,
        };
        check_with(&Config::new(400), &gen, &|text: &String| {
            check_flat(&file(text));
            Ok(())
        })
        .expect("every text round-trips");
    }

    #[test]
    fn a_slash_is_division_until_the_grammar_asks_for_a_regex() {
        let f = file("x = /a/v");
        let mut parser = Parser::new(&f);
        parser.start_node(NodeKind::Module);
        assert_eq!(parser.bump(false).kind, TokenKind::Ident);
        assert_eq!(parser.bump(false).kind, TokenKind::Punct(Punct::Eq));
        assert_eq!(
            parser.peek(false).kind,
            TokenKind::Punct(Punct::Slash),
            "lexed as division first"
        );
        assert_eq!(
            parser.peek(false).kind,
            TokenKind::Punct(Punct::Slash),
            "and stays so"
        );
        let regex = TokenKind::Regex(Regex {
            body: "a".into(),
            flags: "v".into(),
            groups: Box::default(),
        });
        assert_eq!(
            parser.peek(true).kind,
            regex,
            "lexed again as a regex when asked"
        );
        assert_eq!(
            parser.peek(false).kind,
            regex,
            "a regex, once asked for, stays"
        );
        let token = parser.bump(true);
        assert_eq!(token.kind, regex);
        assert_eq!((token.span.byte_start, token.span.byte_end), (4, 8));
        assert_eq!(
            token.leading.len(),
            1,
            "the trivia before it was lexed again too"
        );
        assert_eq!(parser.bump(false).kind, TokenKind::Eof);
        assert_eq!(
            parser.peek(true).kind,
            TokenKind::Eof,
            "Eof again and again"
        );
        parser.finish_node();
        let (tree, diagnostics) = parser.finish();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(tree.print(&f), "x = /a/v");
        assert_eq!(tree.tokens().len(), 4);
    }

    #[test]
    fn a_second_token_of_lookahead_is_dropped_when_the_first_is_lexed_again() {
        let f = file("/a/v 1");
        let mut parser = Parser::new(&f);
        assert_eq!(
            parser.peek_second().kind,
            TokenKind::Ident,
            "`/` `a` as division"
        );
        let regex = TokenKind::Regex(Regex {
            body: "a".into(),
            flags: "v".into(),
            groups: Box::default(),
        });
        assert_eq!(parser.peek(true).kind, regex);
        assert_eq!(
            parser.peek_second().kind,
            TokenKind::Number(crate::token::Number {
                value: 1.0,
                exceeds_safe_integer: false
            }),
            "`1` follows the regex"
        );
        let regex = parser.peek(false).clone();
        assert_eq!(parser.text(&regex), "/a/v");
        assert!(!parser.at_word("x"));
        assert!(!parser.eat(&TokenKind::Eof));
        parser.bump(true);
        parser.bump(false);
        assert!(parser.eat(&TokenKind::Eof));
        assert_eq!(
            parser.peek_second().kind,
            TokenKind::Eof,
            "Eof again and again"
        );
    }

    #[test]
    fn the_arrow_scan_stops_at_the_end_of_the_file_and_at_its_limit() {
        let f = file("(a");
        let mut parser = Parser::new(&f);
        assert!(!functions::arrow_ahead(&mut parser, 0));
        assert_eq!(
            parser.lookahead.len(),
            3,
            "`(`, `a`, Eof: the scan stopped at Eof"
        );
        let text = format!("({}", "b ".repeat(ARROW_SCAN_LIMIT));
        let f = file(&text);
        let mut parser = Parser::new(&f);
        assert!(!functions::arrow_ahead(&mut parser, 0));
        assert_eq!(
            parser.lookahead.len(),
            ARROW_SCAN_LIMIT + 1,
            "the scan read up to the limit and gave up"
        );
    }

    #[test]
    fn asking_for_a_regex_on_a_division_assignment_relexes_it_too() {
        let f = file("a /= 2/v");
        let mut parser = Parser::new(&f);
        parser.start_node(NodeKind::Module);
        parser.bump(false);
        assert_eq!(parser.peek(false).kind, TokenKind::Punct(Punct::SlashEq));
        let regex = TokenKind::Regex(Regex {
            body: "= 2".into(),
            flags: "v".into(),
            groups: Box::default(),
        });
        assert_eq!(
            parser.peek(true).kind,
            regex,
            "`/= 2/v` is a regex when asked"
        );
        assert_eq!(parser.bump(true).kind, regex);
        assert_eq!(parser.bump(false).kind, TokenKind::Eof);
        parser.finish_node();
        let (tree, diagnostics) = parser.finish();
        assert!(diagnostics.is_empty());
        assert_eq!(tree.print(&f), "a /= 2/v");
    }

    #[test]
    fn bumping_without_peeking_lexes_as_division() {
        let f = file("/ 2");
        let mut parser = Parser::new(&f);
        assert_eq!(parser.bump(false).kind, TokenKind::Punct(Punct::Slash));
        let (tree, diagnostics) = parser.finish();
        assert!(diagnostics.is_empty());
        assert_eq!(
            tree.tokens().len(),
            1,
            "no node was started, but the token is kept"
        );
        assert_eq!(tree.print(&f), "/");
    }
}
