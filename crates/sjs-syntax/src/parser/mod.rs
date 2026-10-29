//! The parser driver (SPEC.md §11.1): owns the lexer, pulls tokens with the
//! regex-allowed hint (US-015) and builds the [`SyntaxTree`]. A module is its import
//! declarations (US-021, `imports.rs`) followed by its items: export declarations
//! (US-022, `exports.rs`), `const`/`let` (US-023), functions (US-024), classes (US-025)
//! and statements (US-026, `statements.rs`), whose expressions are parsed by
//! precedence climbing (US-028, `expressions.rs`).
//!
//! A `/` is lexed as a division until the grammar asks for an expression there; then
//! the lookahead is lexed again as a regex literal. Division lexing never reports
//! anything, so re-lexing leaves no stale diagnostic. The reverse never happens: once a
//! regex has been asked for, it stays.

mod bindings;
mod classes;
mod exports;
mod expressions;
mod functions;
mod imports;
mod statements;

use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};

use crate::lexer::{Checkpoint, Lexer};
use crate::token::{Keyword, Punct, Token, TokenKind};
use crate::tree::{Marker, NodeId, NodeKind, SyntaxTree, TokenId, TreeBuilder};

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
    /// A class method or accessor. `super.x` is permitted inside one; `super()` only
    /// in a derived constructor.
    Method {
        is_async: bool,
        is_generator: bool,
        derived_constructor: bool,
    },
    /// A static block or a field initializer: `super.x` is permitted, and `return`,
    /// `yield`, `await` and `super()` are not.
    Initializer,
}

/// What a `break` or `continue` can leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Jump {
    Loop,
    Switch,
}

/// How deep statements and blocks nest before the rest of a block is skipped: the
/// parser recurses per level, so a bound keeps a hostile file from overflowing the
/// stack (each block counts twice, as a statement and as a list of items).
pub const MAX_NESTING: u32 = 200;

/// The items of a scope: declarations and statements. In a block the loop stops before
/// the `}` that closes it; at the top level a stray `}` is E000.
pub(super) fn items_in(parser: &mut Parser<'_>, scope: Scope) {
    if !parser.enter() {
        parser.skip_rest_of_block();
    }
    let mut exported = exports::Exported::new();
    loop {
        let token = parser.peek(false).clone();
        match token.kind {
            TokenKind::Eof => {
                if scope == Scope::Module {
                    parser.bump(false);
                }
                break;
            }
            TokenKind::Punct(Punct::RBrace) if scope == Scope::Block => break,
            TokenKind::Punct(Punct::RBrace) => {
                parser.error(token.span, "unmatched `}`");
                parser.bump(false);
            }
            _ => item(parser, scope, &mut exported),
        }
    }
    parser.leave();
}

/// One item at its first token: a declaration or a statement. An import declaration
/// here is E000 (SPEC.md §2: imports come first) and an export inside a block is E000
/// (exports sit at the top level); both are then parsed anyway so the tree still
/// records them. `import(…)` and `import.meta` are expressions.
///
/// A grammar that reports a syntax error stops at the offending token; the rest of
/// that line is then skipped, so one fault is one diagnostic.
pub(super) fn item(parser: &mut Parser<'_>, scope: Scope, exported: &mut exports::Exported) {
    let reported = parser.diagnostics.len();
    let outer = std::mem::replace(&mut parser.item_mark, reported);
    dispatch_item(parser, scope, exported);
    parser.recover(reported, scope);
    parser.item_mark = outer;
}

fn dispatch_item(parser: &mut Parser<'_>, scope: Scope, exported: &mut exports::Exported) {
    let token = parser.peek(false).clone();
    match token.kind {
        TokenKind::Keyword(Keyword::Import)
            if !matches!(
                parser.peek_second().kind,
                TokenKind::Punct(Punct::LParen | Punct::Dot)
            ) =>
        {
            parser.error(
                token.span,
                "an import must precede every other module item (SPEC.md §2 `Module`)",
            );
            imports::import_decl(parser);
        }
        TokenKind::Keyword(Keyword::Export) => {
            if scope == Scope::Block {
                parser.error(
                    token.span,
                    "an `export` belongs at the top level of the module, not inside a block (SPEC.md §2 `Module`)",
                );
            }
            exports::export_decl(parser, exported);
        }
        TokenKind::Keyword(Keyword::Const | Keyword::Let) => {
            bindings::declaration(parser);
        }
        TokenKind::Keyword(Keyword::Var) => statements::var_decl(parser),
        _ if functions::at_function(parser) => {
            functions::function(parser, false);
        }
        TokenKind::Keyword(Keyword::Class) => {
            classes::class_decl(parser, false);
        }
        _ => statements::statement(parser),
    }
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
    /// The enclosing loops and `switch`es, innermost last, each with the depth of
    /// `contexts` it sits at, so a function boundary hides the ones outside it.
    jumps: Vec<(usize, Jump)>,
    /// How many statements and item lists enclose the next token.
    nesting: u32,
    /// Whether [`MAX_NESTING`] has been reported.
    too_deep: bool,
    /// The span of the last token consumed.
    last: Option<Span>,
    /// The diagnostics count when the current item began: a statement that has
    /// reported something does not also report a missing `;` or line break.
    item_mark: usize,
    /// The index of the current expression statement's first token, so an update that
    /// turns out to be the whole statement is S012 rather than E000.
    statement_start: Option<usize>,
    /// The update found at `statement_start`, judged when the statement ends.
    statement_update: Option<expressions::Update>,
    /// Shorthand defaults (`{ a = 1 }`) not yet settled as a pattern's or an error.
    covers: Vec<Span>,
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
            jumps: Vec::new(),
            nesting: 0,
            too_deep: false,
            last: None,
            item_mark: 0,
            statement_start: None,
            statement_update: None,
            covers: Vec::new(),
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
        self.last = Some(token.span);
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
        self.report_with_fix(Code::E000, span, message, current, replacement);
    }

    /// A diagnostic at `span` carrying the text as written and the text to write
    /// instead.
    pub fn report_with_fix(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
        current: String,
        replacement: String,
    ) {
        if self.too_deep {
            return;
        }
        let diagnostic =
            Diagnostic::new(code, self.file.path(), span, message).with_fix(current, replacement);
        self.diagnostics.extend(diagnostic.ok());
    }

    /// Changes the kind of an open node, once the tokens after its start have decided
    /// it (a `for` loop's header).
    pub(super) fn retag(&mut self, node: NodeId, kind: NodeKind) {
        self.builder.retag(node, kind);
    }

    /// Enters one level of statement nesting. False past [`MAX_NESTING`], reported at
    /// the next token the first time; the caller then skips instead of recursing.
    /// Every call is paired with [`Parser::leave`].
    pub(super) fn enter(&mut self) -> bool {
        self.nesting += 1;
        if self.nesting <= MAX_NESTING {
            return true;
        }
        if !self.too_deep {
            let span = self.peek(false).span;
            self.error(
                span,
                format!(
                    "statements and expressions nest deeper than {MAX_NESTING} levels: the rest of this block is skipped"
                ),
            );
            self.too_deep = true;
        }
        false
    }

    /// Leaves the level [`Parser::enter`] entered.
    pub(super) fn leave(&mut self) {
        self.nesting -= 1;
    }

    /// Consumes tokens up to the `}` that closes the enclosing block (not consuming it)
    /// or the end of the file, without recursing.
    pub(super) fn skip_rest_of_block(&mut self) {
        let mut depth: u32 = 0;
        loop {
            match self.peek(false).kind {
                TokenKind::Eof => return,
                TokenKind::Punct(Punct::RBrace) if depth == 0 => return,
                TokenKind::Punct(Punct::LBrace) => depth += 1,
                TokenKind::Punct(Punct::RBrace) => depth -= 1,
                _ => {}
            }
            self.bump(false);
        }
    }

    /// After an item: when it reported anything beyond the first `reported`
    /// diagnostics and did not end at a `;` or `}`, it stopped at the offending token,
    /// and the rest of its line is skipped.
    pub(super) fn recover(&mut self, reported: usize, scope: Scope) {
        let ended = matches!(
            self.last,
            Some(span) if matches!(self.file.slice(&span), Some(";" | "}"))
        );
        if self.diagnostics.len() > reported && !ended {
            self.skip_rest_of_line(scope);
        }
    }

    /// Consumes the rest of the line after a syntax error: tokens on the same line,
    /// brackets balanced, through a `;` at depth zero. In a block, a `}` that nothing
    /// follows on its line is taken to close the block and is left for it.
    pub(super) fn skip_rest_of_line(&mut self, scope: Scope) {
        let mut depth: u32 = 0;
        loop {
            let next = self.peek(false).clone();
            if next.has_newline_before || next.kind == TokenKind::Eof {
                return;
            }
            match next.kind {
                TokenKind::Punct(Punct::LParen | Punct::LBracket | Punct::LBrace) => depth += 1,
                TokenKind::Punct(Punct::RParen | Punct::RBracket) => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Punct(Punct::RBrace) if depth > 0 => depth -= 1,
                TokenKind::Punct(Punct::RBrace) if scope == Scope::Block => {
                    let after = self.peek_second();
                    if after.has_newline_before
                        || matches!(after.kind, TokenKind::Eof | TokenKind::Punct(Punct::RBrace))
                    {
                        return;
                    }
                }
                TokenKind::Punct(Punct::Semicolon) if depth == 0 => {
                    self.bump(false);
                    return;
                }
                _ => {}
            }
            self.bump(false);
        }
    }

    /// Runs `parse` with `jump` as the innermost target of `break` (and, for a loop,
    /// `continue`).
    pub(super) fn with_jump(&mut self, jump: Jump, parse: impl FnOnce(&mut Self)) {
        self.jumps.push((self.contexts.len(), jump));
        parse(self);
        self.jumps.pop();
    }

    /// Whether a `break` here has a loop or `switch` to leave in the same function.
    pub(super) fn can_break(&self) -> bool {
        self.jumps
            .last()
            .is_some_and(|(depth, _)| *depth == self.contexts.len())
    }

    /// Whether a `continue` here has a loop to continue in the same function.
    pub(super) fn can_continue(&self) -> bool {
        self.jumps
            .iter()
            .rev()
            .take_while(|(depth, _)| *depth == self.contexts.len())
            .any(|(_, jump)| *jump == Jump::Loop)
    }

    /// Whether `await` is permitted here: at the module's top level or directly inside
    /// an `async` function, arrow or method.
    pub(super) fn await_allowed(&self) -> bool {
        matches!(
            self.contexts.last(),
            None | Some(
                FnContext::Function { is_async: true, .. }
                    | FnContext::Arrow { is_async: true }
                    | FnContext::Method { is_async: true, .. }
            )
        )
    }

    /// The removed `function` expression at the next token: E000, then parsed for
    /// recovery.
    pub(super) fn removed_function_expression(&mut self) {
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
            // `async` [no line break] `(`: after a line break `async` is a name.
            TokenKind::Ident if self.at_word("async") && !self.peek_second().has_newline_before => {
                match self.peek_second().kind {
                    TokenKind::Punct(Punct::LParen) => functions::arrow_ahead(self, 1),
                    TokenKind::Ident => self.peek_nth(2).kind == TokenKind::Punct(Punct::Arrow),
                    _ => self.peek_second().kind == TokenKind::Punct(Punct::Arrow),
                }
            }
            TokenKind::Ident => self.peek_second().kind == TokenKind::Punct(Punct::Arrow),
            _ => false,
        }
    }

    /// A diagnostic at `span`. Past the nesting bound nothing more is reported: the
    /// file is hostile, and its skipped remainder would only cascade.
    pub fn report(&mut self, code: Code, span: Span, message: impl Into<String>) {
        if self.too_deep {
            return;
        }
        self.diagnostics
            .push(Diagnostic::new(code, self.file.path(), span, message));
    }

    /// The place before the next child of the innermost open node.
    pub(super) fn marker(&self) -> Marker {
        self.builder.marker()
    }

    /// Opens a node adopting what the innermost open node gained since `marker`.
    pub(super) fn start_node_at(&mut self, marker: Marker, kind: NodeKind) -> NodeId {
        self.builder.start_node_at(marker, kind)
    }

    /// The end of a simple statement: a `;`, or before the next token a line break, a
    /// `}` or the end of the file (where ASI would end it). Anything else on the line
    /// is E000, unless the statement has already reported something, or it is a
    /// character the lexer reported, which is skipped with the rest of its line.
    pub(super) fn terminate(&mut self) {
        if self.eat(&TokenKind::Punct(Punct::Semicolon)) {
            return;
        }
        let next = self.peek(false).clone();
        if next.has_newline_before
            || matches!(next.kind, TokenKind::Punct(Punct::RBrace) | TokenKind::Eof)
            || self.diagnostics.len() > self.item_mark
        {
            return;
        }
        if next.kind == TokenKind::Error {
            self.skip_rest_of_line(Scope::Block);
            return;
        }
        self.error(
            next.span,
            "expected `;` or a line break after the statement",
        );
    }

    /// Consumes tokens, brackets balanced, up to (not including) `stop` at depth zero,
    /// an unmatched closer or the end of the file: recovery inside a malformed
    /// construct.
    pub(super) fn skip_until(&mut self, stop: &TokenKind) {
        let mut depth: u32 = 0;
        loop {
            let kind = self.peek(false).kind.clone();
            match kind {
                TokenKind::Eof => return,
                _ if depth == 0 && &kind == stop => return,
                TokenKind::Punct(Punct::LParen | Punct::LBracket | Punct::LBrace) => depth += 1,
                TokenKind::Punct(Punct::RParen | Punct::RBracket | Punct::RBrace) => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            self.bump(false);
        }
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
        let snippet = around(f.text(), at);
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

    /// Up to 60 bytes either side of `at`, widened to character boundaries.
    fn around(text: &str, at: usize) -> &str {
        let lo = (0..=at.saturating_sub(60))
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        let hi = ((at + 60).min(text.len())..=text.len())
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(text.len());
        &text[lo..hi]
    }

    /// For any text: the tree prints back to it, under a `Module` root whose last
    /// child is the `Eof` token. (Agreement with `lex` is not asserted: a `/` after a
    /// block's `}` is a regex to the parser and a division to the lexer's guess.)
    fn check_round_trip(f: &SourceFile) {
        let (tree, _) = parse(f);
        assert_eq!(tree.print(f), f.text());
        let root = tree.root();
        assert_eq!(root.kind(), NodeKind::Module);
        let last = root.children().last().expect("Eof");
        let eof = as_token(last).expect("the last child is the Eof token");
        assert_eq!(eof.token().kind, TokenKind::Eof);
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
        assert!(as_token(tree.root().children().last().expect("Eof")).is_some());
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
    fn simple_modules_print_back_to_the_source_with_only_the_lexers_diagnostics() {
        for text in [
            "",
            "a = 1",
            "  // only\n",
            "x = /a/v\ny = `${a}`",
            "\"\u{FEFF}\"",
            "@ $ \\u",
        ] {
            let (diagnostics, lexed) = check(&file(text));
            assert_eq!(diagnostics, lexed, "{text:?}");
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
            check_round_trip(&file(text));
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
