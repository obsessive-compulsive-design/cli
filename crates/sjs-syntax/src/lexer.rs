//! The lexer frame (SPEC.md §2): trivia, line terminators, comments, and a never-failing
//! loop in which anything unrecognised becomes an `Error` token carrying E000.

use sjs_diag::{Code, Diagnostic, Diagnostics, LineTerminator, Position, SourceFile, Span};

use crate::token::{Keyword, Token, TokenKind, TokenStream, Trivia, TriviaKind};

/// Lexes a whole file. Never fails: every byte is covered by exactly one trivia run or
/// token, so [`TokenStream::print`] reproduces the file; problems are diagnostics.
#[must_use]
pub fn lex(file: &SourceFile) -> (TokenStream, Diagnostics) {
    let mut lexer = Lexer {
        file,
        text: file.text(),
        pos: 0,
        diagnostics: Diagnostics::new(),
        after_dot: false,
    };
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            break;
        }
    }
    (TokenStream { tokens }, lexer.diagnostics)
}

struct Lexer<'a> {
    file: &'a SourceFile,
    text: &'a str,
    pos: usize,
    diagnostics: Diagnostics,
    /// True when the previous token's text ends with `.`: the next word is a property
    /// name, where a removed word is ordinary (`Map.delete`, SPEC.md §12).
    after_dot: bool,
}

/// ES2025 `WhiteSpace`: TAB, VT, FF, ZWNBSP and every `Zs` character.
fn is_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\u{000B}' | '\u{000C}' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

fn line_terminator(ch: char) -> Option<LineTerminator> {
    match ch {
        '\n' => Some(LineTerminator::Lf),
        '\r' => Some(LineTerminator::Cr),
        '\u{2028}' => Some(LineTerminator::LineSeparator),
        '\u{2029}' => Some(LineTerminator::ParagraphSeparator),
        _ => None,
    }
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.text[self.pos..].chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        // Both offsets are character boundaries the lexer itself produced; the fallback
        // only keeps this branch-free.
        self.file.span(start, end).unwrap_or(Span::new(
            Position::new(1, 1),
            Position::new(1, 1),
            start,
            end,
        ))
    }

    fn report(&mut self, code: Code, start: usize, end: usize, message: String) {
        let span = self.span(start, end);
        self.diagnostics
            .push(Diagnostic::new(code, self.file.path(), span, message));
    }

    fn next_token(&mut self) -> Token {
        let leading = self.trivia();
        let has_newline_before = leading.iter().any(Trivia::is_newline);
        let start = self.pos;
        let kind = match self.bump() {
            None => TokenKind::Eof,
            Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => self.word(start),
            Some('$') => self.error(start, "`$` is not permitted in identifiers"),
            Some('\\') if self.peek() == Some('u') => self.unicode_escape(start),
            Some('#') if self.peek() == Some('!') => self.hashbang(start),
            Some(ch) if ch.is_alphabetic() => self.error(
                start,
                &format!(
                    "{ch:?} is not permitted in identifiers: only ASCII letters, digits and `_`"
                ),
            ),
            Some(ch) => self.error(start, &format!("unexpected character {ch:?}")),
        };
        let span = self.span(start, self.pos);
        self.after_dot = kind != TokenKind::Eof && self.text[start..self.pos].ends_with('.');
        Token {
            kind,
            span,
            leading,
            has_newline_before,
        }
    }

    fn error(&mut self, start: usize, message: &str) -> TokenKind {
        self.report(Code::E000, start, self.pos, message.to_owned());
        TokenKind::Error
    }

    /// `[A-Za-z_][A-Za-z0-9_]*`: an identifier, a keyword, `null` or a boolean.
    fn word(&mut self, start: usize) -> TokenKind {
        while self
            .peek()
            .is_some_and(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            self.bump();
        }
        let word = &self.text[start..self.pos];
        match word {
            "null" => TokenKind::Null,
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => match Keyword::from_word(word) {
                None => TokenKind::Ident,
                Some(keyword) => {
                    if let Some(replacement) = keyword.removed_replacement() {
                        if !self.after_dot {
                            self.report(
                                Code::E000,
                                start,
                                self.pos,
                                format!("`{word}` is removed; use {replacement}"),
                            );
                        }
                    }
                    TokenKind::Keyword(keyword)
                }
            },
        }
    }

    /// `\uXXXX` or `\u{…}`: one error token for the whole escape.
    fn unicode_escape(&mut self, start: usize) -> TokenKind {
        self.bump();
        if self.peek() == Some('{') {
            while let Some(ch) = self.bump() {
                if ch == '}' {
                    break;
                }
            }
        } else {
            for _ in 0..4 {
                if self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.bump();
                }
            }
        }
        self.error(start, "Unicode escapes are not permitted in identifiers")
    }

    /// `#!…` to the end of the line: a server-only signal the subset rejects (SPEC.md §2).
    fn hashbang(&mut self, start: usize) -> TokenKind {
        while self.peek().is_some_and(|c| line_terminator(c).is_none()) {
            self.bump();
        }
        self.error(
            start,
            "`#!` is removed; a hashbang line belongs to a host-side launcher outside the subset",
        )
    }

    /// Every trivia run up to the next token or the end of the file.
    fn trivia(&mut self) -> Vec<Trivia> {
        let mut out = Vec::new();
        while let Some(ch) = self.peek() {
            let start = self.pos;
            let kind = if is_whitespace(ch) {
                while self.peek().is_some_and(is_whitespace) {
                    self.bump();
                }
                TriviaKind::Whitespace
            } else if let Some(terminator) = line_terminator(ch) {
                self.bump();
                if terminator == LineTerminator::Cr && self.peek() == Some('\n') {
                    self.bump();
                    TriviaKind::LineTerminator(LineTerminator::CrLf)
                } else {
                    TriviaKind::LineTerminator(terminator)
                }
            } else if ch == '/' && self.peek_second() == Some('/') {
                while self.peek().is_some_and(|c| line_terminator(c).is_none()) {
                    self.bump();
                }
                TriviaKind::LineComment
            } else if ch == '/' && self.peek_second() == Some('*') {
                self.block_comment(start)
            } else {
                break;
            };
            out.push(Trivia {
                kind,
                span: self.span(start, self.pos),
            });
        }
        out
    }

    /// `/* … */`; an unterminated comment runs to the end of the file and is E000 at its
    /// start (SPEC.md §2 comments are `//` and `/* */` only).
    fn block_comment(&mut self, start: usize) -> TriviaKind {
        self.bump();
        self.bump();
        let mut has_line_terminator = false;
        loop {
            match self.bump() {
                None => {
                    self.report(
                        Code::E000,
                        start,
                        self.pos,
                        "unterminated block comment".to_owned(),
                    );
                    break;
                }
                Some('*') if self.peek() == Some('/') => {
                    self.bump();
                    break;
                }
                Some(ch) => has_line_terminator |= line_terminator(ch).is_some(),
            }
        }
        TriviaKind::BlockComment {
            has_line_terminator,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::token::{TokenKind, TriviaKind};
    use sjs_diag::{Code, LineTerminator, Position, SourceFile};
    use sjs_testkit::prng::Rng;
    use sjs_testkit::program::{Options, ProgramGen};
    use sjs_testkit::prop::{check_with, Config, Gen, Text};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text)).0.tokens.iter().map(|t| t.kind).collect()
    }

    fn trivia_kinds(text: &str) -> Vec<TriviaKind> {
        lex(&file(text))
            .0
            .eof()
            .leading
            .iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn empty_input_is_only_eof() {
        let f = file("");
        let (stream, diagnostics) = lex(&f);
        assert_eq!(stream.tokens.len(), 1);
        let eof = stream.eof();
        assert_eq!(eof.kind, TokenKind::Eof);
        assert!(eof.leading.is_empty() && !eof.has_newline_before);
        assert_eq!((eof.span.byte_start, eof.span.byte_end), (0, 0));
        assert_eq!(eof.span.start, Position::new(1, 1));
        assert!(diagnostics.is_empty());
        assert_eq!(stream.print(&f), "");
    }

    #[test]
    fn input_that_is_only_trivia_attaches_it_all_to_eof() {
        let text = "  \t\n// note\r\n/* block */ \u{00A0}\u{3000}\u{FEFF}";
        let f = file(text);
        let (stream, diagnostics) = lex(&f);
        assert_eq!(kinds(text), [TokenKind::Eof]);
        assert_eq!(
            trivia_kinds(text),
            [
                TriviaKind::Whitespace,
                TriviaKind::LineTerminator(LineTerminator::Lf),
                TriviaKind::LineComment,
                TriviaKind::LineTerminator(LineTerminator::CrLf),
                TriviaKind::BlockComment {
                    has_line_terminator: false
                },
                TriviaKind::Whitespace,
            ]
        );
        assert!(stream.eof().has_newline_before);
        assert!(diagnostics.is_empty());
        assert_eq!(stream.print(&f), text);
        let spans: Vec<(usize, usize)> = stream
            .eof()
            .leading
            .iter()
            .map(|t| (t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            spans,
            [(0, 3), (3, 4), (4, 11), (11, 13), (13, 24), (24, 33)]
        );
    }

    #[test]
    fn every_line_terminator_is_one_trivia_run() {
        assert_eq!(
            trivia_kinds("\n\r\n\r\u{2028}\u{2029}"),
            [
                TriviaKind::LineTerminator(LineTerminator::Lf),
                TriviaKind::LineTerminator(LineTerminator::CrLf),
                TriviaKind::LineTerminator(LineTerminator::Cr),
                TriviaKind::LineTerminator(LineTerminator::LineSeparator),
                TriviaKind::LineTerminator(LineTerminator::ParagraphSeparator),
            ]
        );
        assert_eq!(
            trivia_kinds("\n\n"),
            [
                TriviaKind::LineTerminator(LineTerminator::Lf),
                TriviaKind::LineTerminator(LineTerminator::Lf)
            ],
            "terminators never merge"
        );
        assert_eq!(
            trivia_kinds("\r\r"),
            [
                TriviaKind::LineTerminator(LineTerminator::Cr),
                TriviaKind::LineTerminator(LineTerminator::Cr)
            ]
        );
    }

    #[test]
    fn has_newline_before_follows_the_asi_rule() {
        let case = |text: &str| lex(&file(text)).0.tokens[1].has_newline_before;
        assert!(!case("a b"), "whitespace is not a newline");
        assert!(!case("a /* same line */ b"));
        assert!(
            !case("a // comment b"),
            "a line comment without its terminator"
        );
        assert!(case("a\nb"));
        assert!(case("a\r\nb"));
        assert!(case("a\rb"));
        assert!(case("a\u{2028}b"));
        assert!(case("a\u{2029}b"));
        assert!(
            case("a /* multi\nline */ b"),
            "a block comment containing a terminator counts"
        );
        assert!(case("a /* multi\u{2029}line */ b"));
        assert!(case("a // comment\nb"));
        assert!(
            !lex(&file("a\nb")).0.tokens[0].has_newline_before,
            "the first token has nothing before it"
        );
        let (stream, _) = lex(&file("a /*\n*/ b"));
        assert_eq!(
            stream.tokens[1]
                .leading
                .iter()
                .map(|t| t.kind)
                .collect::<Vec<_>>(),
            [
                TriviaKind::Whitespace,
                TriviaKind::BlockComment {
                    has_line_terminator: true
                },
                TriviaKind::Whitespace
            ]
        );
    }

    #[test]
    fn comments_end_where_the_spec_says() {
        let (stream, diagnostics) = lex(&file("// to the end"));
        assert_eq!(stream.tokens.len(), 1);
        assert_eq!(stream.eof().leading[0].kind, TriviaKind::LineComment);
        assert_eq!(stream.eof().leading[0].span.byte_end, 13);
        assert!(diagnostics.is_empty());
        let (stream, _) = lex(&file("/* a */b"));
        assert_eq!(stream.tokens[0].leading[0].span.byte_end, 7);
        assert_eq!(stream.tokens[0].span.byte_start, 7);
        let (stream, _) = lex(&file("/**/x"));
        assert_eq!(
            stream.tokens[0].leading[0].span.byte_end, 4,
            "the shortest block comment"
        );
        let (stream, _) = lex(&file("/* * / */x"));
        assert_eq!(
            stream.tokens[0].leading[0].span.byte_end, 9,
            "a star and a slash apart do not close"
        );
    }

    #[test]
    fn an_unterminated_block_comment_is_e000_at_its_start_and_runs_to_the_end() {
        let f = file("a /* never\nclosed");
        let (stream, diagnostics) = lex(&f);
        let eof = stream.eof();
        assert_eq!(eof.leading.len(), 2);
        assert_eq!(
            eof.leading[1].kind,
            TriviaKind::BlockComment {
                has_line_terminator: true
            }
        );
        assert_eq!(
            (eof.leading[1].span.byte_start, eof.leading[1].span.byte_end),
            (2, 17)
        );
        assert!(eof.has_newline_before);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message == "unterminated block comment")
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, Code::E000);
        assert_eq!(errors[0].span.start, Position::new(1, 3));
        assert_eq!(
            (errors[0].span.byte_start, errors[0].span.byte_end),
            (2, 17)
        );
        assert_eq!(stream.print(&f), "a /* never\nclosed");
        let (_, diagnostics) = lex(&file("/*"));
        assert_eq!(diagnostics.len(), 1, "even the bare opener");
    }

    #[test]
    fn unrecognised_characters_become_error_tokens_with_e000_and_lexing_continues() {
        let f = file("#é\u{1F600}");
        let (stream, diagnostics) = lex(&f);
        assert_eq!(
            kinds("#é\u{1F600}"),
            [
                TokenKind::Error,
                TokenKind::Error,
                TokenKind::Error,
                TokenKind::Eof
            ]
        );
        let spans: Vec<(usize, usize, u32)> = stream
            .tokens
            .iter()
            .map(|t| (t.span.byte_start, t.span.byte_end, t.span.start.column))
            .collect();
        assert_eq!(spans, [(0, 1, 1), (1, 3, 2), (3, 7, 3), (7, 7, 4)]);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "unexpected character '#'",
                "'é' is not permitted in identifiers: only ASCII letters, digits and `_`",
                "unexpected character '\u{1F600}'"
            ]
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code == Code::E000 && d.path == "a.js"));
        assert_eq!(stream.print(&f), "#é\u{1F600}");
    }

    #[test]
    fn a_slash_alone_is_not_a_comment() {
        assert_eq!(kinds("/"), [TokenKind::Error, TokenKind::Eof]);
        assert_eq!(
            kinds("/ /"),
            [TokenKind::Error, TokenKind::Error, TokenKind::Eof]
        );
        assert_eq!(
            kinds("/*/"),
            [TokenKind::Eof],
            "`/*/` opens a comment that never closes"
        );
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
            assert_eq!(lex(&f).0.print(&f), text, "seed {seed}");
        }
    }

    /// Random strings over trivia, terminators, comment openers and closers, and plain
    /// characters: whatever comes in goes out unchanged, and the stream is well formed.
    #[test]
    fn random_text_round_trips_and_ends_with_eof() {
        let gen = Text {
            alphabet: &[
                'a', ' ', '\t', '\n', '\r', '\u{2028}', '/', '*', 'é', '\u{3000}',
            ],
            max_len: 24,
        };
        let result = check_with(&Config::new(400), &gen, &|text: &String| {
            let f = file(text);
            let (stream, _) = lex(&f);
            assert_eq!(stream.print(&f), *text, "round trip");
            assert_eq!(
                stream.tokens.last().map(|t| t.kind),
                Some(TokenKind::Eof),
                "no eof"
            );
            let covered: usize = stream
                .tokens
                .iter()
                .map(|t| {
                    t.leading
                        .iter()
                        .map(|tr| tr.span.byte_end - tr.span.byte_start)
                        .sum::<usize>()
                        + t.span.byte_end
                        - t.span.byte_start
                })
                .sum();
            assert_eq!(covered, text.len(), "every byte covered exactly once");
            Ok(())
        });
        assert_eq!(result, Ok(()));
    }
}

#[cfg(test)]
mod word_tests {
    use super::lex;
    use crate::token::{Keyword, TokenKind};
    use sjs_diag::{Code, Position, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text)).0.tokens.iter().map(|t| t.kind).collect()
    }

    fn messages(text: &str) -> Vec<String> {
        lex(&file(text))
            .1
            .iter()
            .map(|d| d.message.clone())
            .collect()
    }

    #[test]
    fn identifiers_are_ascii_letters_digits_and_underscores() {
        for word in [
            "a",
            "_",
            "_x",
            "abc",
            "a1",
            "A_1b",
            "camelCase",
            "UPPER_SNAKE",
            "x0",
        ] {
            assert_eq!(kinds(word), [TokenKind::Ident, TokenKind::Eof], "{word}");
            assert!(messages(word).is_empty(), "{word}");
        }
        let (stream, _) = lex(&file("ab cd"));
        let spans: Vec<(usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(spans, [(0, 2), (3, 5), (5, 5)]);
    }

    #[test]
    fn every_keyword_lexes_as_itself_and_nothing_else_does() {
        assert_eq!(Keyword::ALL.len(), 42);
        for (keyword, spelling) in Keyword::ALL {
            assert_eq!(
                kinds(spelling),
                [TokenKind::Keyword(*keyword), TokenKind::Eof],
                "{spelling}"
            );
            assert_eq!(keyword.as_str(), *spelling);
            assert_eq!(Keyword::from_word(spelling), Some(*keyword));
            let capitalised = format!("{}{}", spelling[..1].to_uppercase(), &spelling[1..]);
            assert_eq!(
                kinds(&capitalised),
                [TokenKind::Ident, TokenKind::Eof],
                "keywords are case-sensitive: {capitalised}"
            );
            let extended = format!("{spelling}x");
            assert_eq!(
                kinds(&extended),
                [TokenKind::Ident, TokenKind::Eof],
                "{extended}"
            );
            // Dropping the last letter of any keyword leaves a plain identifier.
            let prefix = &spelling[..spelling.len() - 1];
            assert_eq!(
                kinds(prefix),
                [TokenKind::Ident, TokenKind::Eof],
                "prefix {prefix}"
            );
        }
        assert_eq!(Keyword::from_word("notakeyword"), None);
        assert_eq!(
            kinds("le cons"),
            [TokenKind::Ident, TokenKind::Ident, TokenKind::Eof]
        );
        assert_eq!(
            kinds("instanceof"),
            [TokenKind::Keyword(Keyword::Instanceof), TokenKind::Eof],
            "`in` inside is not matched"
        );
        assert_eq!(kinds("input"), [TokenKind::Ident, TokenKind::Eof]);
    }

    #[test]
    fn contextual_words_are_identifiers() {
        for word in [
            "of",
            "get",
            "set",
            "static",
            "async",
            "await",
            "yield",
            "from",
            "as",
            "undefined",
            "constructor",
        ] {
            assert_eq!(kinds(word), [TokenKind::Ident, TokenKind::Eof], "{word}");
        }
    }

    #[test]
    fn null_and_booleans_are_literals() {
        assert_eq!(kinds("null"), [TokenKind::Null, TokenKind::Eof]);
        assert_eq!(kinds("true"), [TokenKind::Bool(true), TokenKind::Eof]);
        assert_eq!(kinds("false"), [TokenKind::Bool(false), TokenKind::Eof]);
        assert_eq!(kinds("nullish"), [TokenKind::Ident, TokenKind::Eof]);
        assert!(
            messages("null true false").is_empty(),
            "placement is the parser's job"
        );
    }

    #[test]
    fn removed_words_are_e000_naming_the_replacement() {
        let expected = [
            ("var", "`var` is removed; use `const` or `let`"),
            (
                "with",
                "`with` is removed; use nothing: there is no dynamic scope",
            ),
            ("void", "`void` is removed; use `undefined`"),
            (
                "delete",
                "`delete` is removed; use assigning `undefined`, or `Map.delete`",
            ),
            (
                "in",
                "`in` is removed; use a discriminant property or `Object.hasOwn`",
            ),
            ("arguments", "`arguments` is removed; use rest parameters"),
            (
                "eval",
                "`eval` is removed; use nothing: there is no code from strings",
            ),
            (
                "debugger",
                "`debugger` is removed; use nothing: it is a host-side signal",
            ),
        ];
        for (word, message) in expected {
            let (stream, diagnostics) = lex(&file(word));
            let keyword = Keyword::from_word(word).expect("a removed word is a keyword");
            assert_eq!(
                stream.tokens[0].kind,
                TokenKind::Keyword(keyword),
                "{word} is still a keyword token"
            );
            assert_eq!(messages(word), [message], "{word}");
            let d = diagnostics.iter().next().expect("one diagnostic");
            assert_eq!(
                (d.code, d.span.byte_start, d.span.byte_end),
                (Code::E000, 0, word.len())
            );
        }
        let removed: Vec<&str> = Keyword::ALL
            .iter()
            .filter(|(k, _)| k.removed_replacement().is_some())
            .map(|(_, s)| *s)
            .collect();
        assert_eq!(
            removed,
            [
                "var",
                "with",
                "void",
                "delete",
                "in",
                "arguments",
                "eval",
                "debugger"
            ]
        );
        assert_eq!(Keyword::Const.removed_replacement(), None);
        let (_, diagnostics) = lex(&file("var a in b"));
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn a_removed_word_after_a_dot_is_a_property_name() {
        for text in [
            "a.delete",
            "a?.delete",
            "a . delete",
            "a\n.delete",
            "a./* c */delete",
            "m.delete(k)",
        ] {
            let found = messages(text);
            assert!(
                found.iter().all(|m| !m.contains("removed")),
                "{text:?}: {found:?}"
            );
            assert!(
                kinds(text).contains(&TokenKind::Keyword(Keyword::Delete)),
                "{text:?}"
            );
        }
        let removals = |text: &str| -> Vec<String> {
            messages(text)
                .into_iter()
                .filter(|m| m.contains("removed"))
                .collect()
        };
        assert_eq!(
            removals("delete.a"),
            ["`delete` is removed; use assigning `undefined`, or `Map.delete`"]
        );
        assert_eq!(
            removals(".a var"),
            ["`var` is removed; use `const` or `let`"],
            "only the word right after the dot"
        );
        assert_eq!(
            removals("a.b in"),
            ["`in` is removed; use a discriminant property or `Object.hasOwn`"]
        );
    }

    #[test]
    fn dollar_signs_are_rejected_at_their_position() {
        let (stream, diagnostics) = lex(&file("$a a$ a$b"));
        assert_eq!(
            stream.tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [
                TokenKind::Error,
                TokenKind::Ident,
                TokenKind::Ident,
                TokenKind::Error,
                TokenKind::Ident,
                TokenKind::Error,
                TokenKind::Ident,
                TokenKind::Eof
            ]
        );
        let positions: Vec<(Position, &str)> = diagnostics
            .iter()
            .map(|d| (d.span.start, d.message.as_str()))
            .collect();
        assert_eq!(
            positions,
            [
                (Position::new(1, 1), "`$` is not permitted in identifiers"),
                (Position::new(1, 5), "`$` is not permitted in identifiers"),
                (Position::new(1, 8), "`$` is not permitted in identifiers"),
            ]
        );
    }

    #[test]
    fn unicode_escapes_are_one_error_token_each() {
        let (stream, diagnostics) = lex(&file("\\u0061 \\u{61}b \\uZZ"));
        let spans: Vec<(TokenKind, usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.kind, t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            spans,
            [
                (TokenKind::Error, 0, 6),
                (TokenKind::Error, 7, 13),
                (TokenKind::Ident, 13, 14),
                (TokenKind::Error, 15, 17),
                (TokenKind::Ident, 17, 19),
                (TokenKind::Eof, 19, 19),
            ]
        );
        assert!(diagnostics
            .iter()
            .take(3)
            .all(|d| d.message == "Unicode escapes are not permitted in identifiers"));
        assert_eq!(diagnostics.len(), 3);
        let (stream, _) = lex(&file("\\u{61"));
        assert_eq!(
            (stream.tokens[0].kind, stream.tokens[0].span.byte_end),
            (TokenKind::Error, 5),
            "an unclosed brace runs to the end"
        );
        assert_eq!(
            kinds("\\x"),
            [TokenKind::Error, TokenKind::Ident, TokenKind::Eof],
            "a backslash not starting an escape"
        );
    }

    #[test]
    fn non_ascii_letters_are_rejected_and_split_the_word() {
        let (stream, diagnostics) = lex(&file("café π"));
        assert_eq!(
            stream.tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            [
                TokenKind::Ident,
                TokenKind::Error,
                TokenKind::Error,
                TokenKind::Eof
            ]
        );
        assert_eq!(stream.tokens[0].span.byte_end, 3);
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>(),
            [
                "'é' is not permitted in identifiers: only ASCII letters, digits and `_`",
                "'π' is not permitted in identifiers: only ASCII letters, digits and `_`",
            ]
        );
        assert_eq!(
            messages("1"),
            ["unexpected character '1'"],
            "a digit alone is still unexpected until US-012"
        );
    }

    #[test]
    fn a_hashbang_is_rejected_anywhere_and_swallows_its_line() {
        let (stream, diagnostics) = lex(&file("#!/usr/bin/env node\nconst"));
        assert_eq!(
            stream
                .tokens
                .iter()
                .map(|t| (t.kind, t.span.byte_start, t.span.byte_end))
                .collect::<Vec<_>>(),
            [
                (TokenKind::Error, 0, 19),
                (TokenKind::Keyword(Keyword::Const), 20, 25),
                (TokenKind::Eof, 25, 25)
            ]
        );
        assert_eq!(
            diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
            ["`#!` is removed; a hashbang line belongs to a host-side launcher outside the subset"]
        );
        assert!(stream.tokens[1].has_newline_before);
        let (_, later) = lex(&file("a\n#! later"));
        assert_eq!(later.len(), 1, "anywhere, not only on the first line");
        assert_eq!(
            messages("#"),
            ["unexpected character '#'"],
            "a lone hash is not a hashbang"
        );
        assert_eq!(messages("#!"), ["`#!` is removed; a hashbang line belongs to a host-side launcher outside the subset"]);
    }
}
