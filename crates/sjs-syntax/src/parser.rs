//! The parser driver (SPEC.md §11.1): owns the lexer, pulls one token at a time with the
//! regex-allowed hint (US-015) and builds the [`SyntaxTree`]. US-020 knows no grammar: a
//! module is a list of tokens. The grammar stories add the productions.
//!
//! A `/` is lexed as a division until the grammar asks for an expression there; then
//! the lookahead is lexed again as a regex literal. Division lexing never reports
//! anything, so re-lexing leaves no stale diagnostic. The reverse never happens: once a
//! regex has been asked for, it stays.

use sjs_diag::{Diagnostics, SourceFile};

use crate::lexer::{regex_allowed_after, Lexer};
use crate::token::{Punct, Token, TokenKind};
use crate::tree::{NodeId, NodeKind, SyntaxTree, TokenId, TreeBuilder};

/// Parses a whole file into its tree and diagnostics. Never fails.
#[must_use]
pub fn parse(file: &SourceFile) -> (SyntaxTree, Diagnostics) {
    let mut parser = Parser::new(file);
    parser.start_node(NodeKind::Module);
    let mut previous = TokenKind::Eof;
    loop {
        let regex_allowed = regex_allowed_after(&previous);
        let token = parser.bump(regex_allowed);
        if token.kind == TokenKind::Eof {
            break;
        }
        previous = token.kind.clone();
    }
    parser.finish_node();
    parser.finish()
}

/// One token of lookahead over the lexer, feeding a [`TreeBuilder`].
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    builder: TreeBuilder,
    lookahead: Option<Token>,
}

impl<'a> Parser<'a> {
    /// A parser at the start of `file`.
    #[must_use]
    pub fn new(file: &'a SourceFile) -> Self {
        Parser {
            lexer: Lexer::new(file),
            builder: TreeBuilder::new(),
            lookahead: None,
        }
    }

    /// The next token without consuming it. With `regex_allowed`, a `/` or `/=` there
    /// is lexed again as a regex literal.
    pub fn peek(&mut self, regex_allowed: bool) -> &Token {
        let token = self.next(regex_allowed);
        self.lookahead.insert(token)
    }

    /// Consumes the next token into the innermost open node and returns it.
    pub fn bump(&mut self, regex_allowed: bool) -> &Token {
        let token = self.next(regex_allowed);
        let id = self.builder.token(token);
        self.token(id)
    }

    /// The next token, taken out of the lookahead or lexed, re-lexed as a regex when
    /// asked and it is a `/` or `/=`.
    fn next(&mut self, regex_allowed: bool) -> Token {
        let token = self
            .lookahead
            .take()
            .unwrap_or_else(|| self.lexer.next_token(false));
        if !regex_allowed || !matches!(token.kind, TokenKind::Punct(Punct::Slash | Punct::SlashEq))
        {
            return token;
        }
        let start = token
            .leading
            .first()
            .map_or(token.span.byte_start, |trivia| trivia.span.byte_start);
        self.lexer.rewind(start);
        self.lexer.next_token(true)
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
        (self.builder.finish(), self.lexer.finish())
    }

    fn token(&self, id: TokenId) -> &Token {
        self.builder.token_ref(id)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Parser};
    use crate::lex;
    use crate::token::{Punct, Regex, TokenKind};
    use crate::tree::{Element, NodeKind, TokenRef};
    use sjs_diag::SourceFile;
    use sjs_testkit::conformance::discover;
    use sjs_testkit::prng::Rng;
    use sjs_testkit::program::{Options, ProgramGen};
    use sjs_testkit::prop::{check_with, Config, Gen, Text};
    use std::path::Path;

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    /// The tree is the token list under one `Module`, prints back to the source, and
    /// agrees with `lex` token for token and diagnostic for diagnostic.
    fn check(f: &SourceFile) {
        let (tree, diagnostics) = parse(f);
        let (stream, lexed) = lex(f);
        assert_eq!(tree.print(f), f.text());
        assert_eq!(tree.tokens(), &stream.tokens[..]);
        assert_eq!(diagnostics, lexed);
        let root = tree.root();
        assert_eq!(root.kind(), NodeKind::Module);
        assert!(root.parent().is_none());
        assert_eq!(tree.node_count(), 1);
        assert_eq!(root.tokens().len(), stream.tokens.len());
        assert_eq!(root.children().count(), stream.tokens.len());
        assert_eq!(root.span().byte_end, f.len());
        for (index, child) in root.children().enumerate() {
            let token = as_token(child).expect("a module is a list of tokens");
            assert_eq!(token.parent().id(), root.id());
            assert_eq!(token.token(), &stream.tokens[index]);
        }
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
            "const a = 1",
            "  // only\n",
            "x = /a/v\n`${a}`",
            "\"\u{FEFF}\"",
            "@ $ \\u",
        ] {
            check(&file(text));
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
            check(&file(&text));
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
            check(&file(text));
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
