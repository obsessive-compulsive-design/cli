//! The lexer frame (SPEC.md §2): trivia, line terminators, comments, and a never-failing
//! loop in which anything unrecognised becomes an `Error` token carrying E000.

use sjs_diag::{Code, Diagnostic, Diagnostics, LineTerminator, Position, SourceFile, Span};

use crate::number;
use crate::pattern;
use crate::regex::{self, Flags, ScanKind};
use crate::string::{self, TemplateEnd};
use crate::token::{
    Keyword, Punct, Regex, Template, TemplatePart, Token, TokenKind, TokenStream, Trivia,
    TriviaKind,
};

/// Lexes a whole file. Never fails: every byte is covered by exactly one trivia run or
/// token, so [`TokenStream::print`] reproduces the file; problems are diagnostics.
/// Whether a `/` starts a regex is guessed from the previous token with
/// [`regex_allowed_after`]; a parser knows better and drives a [`Lexer`] itself.
#[must_use]
pub fn lex(file: &SourceFile) -> (TokenStream, Diagnostics) {
    let mut lexer = Lexer::new(file);
    let mut tokens: Vec<Token> = Vec::new();
    loop {
        let regex_allowed = tokens.last().is_none_or(|t| regex_allowed_after(&t.kind));
        let token = lexer.next_token(regex_allowed);
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            break;
        }
    }
    (TokenStream { tokens }, lexer.finish())
}

/// Whether a `/` right after a token of kind `previous` starts a regex literal rather
/// than a division, judged from that token alone: a regex can only start where an
/// expression starts, so it never follows a value, `)` or `]`. A `}` is taken to end a
/// block (the parser knows when it ends an object literal instead), and an `Error`
/// token is taken not to be a value.
#[must_use]
pub fn regex_allowed_after(previous: &TokenKind) -> bool {
    match previous {
        TokenKind::Ident
        | TokenKind::PrivateName(_)
        | TokenKind::Null
        | TokenKind::Bool(_)
        | TokenKind::Number(_)
        | TokenKind::BigInt(_)
        | TokenKind::String(_)
        | TokenKind::Regex(_)
        | TokenKind::Keyword(Keyword::This | Keyword::Super)
        | TokenKind::Punct(Punct::RParen | Punct::RBracket) => false,
        TokenKind::Template(template) => {
            !matches!(template.part, TemplatePart::Whole | TemplatePart::Tail)
        }
        TokenKind::Keyword(_) | TokenKind::Punct(_) | TokenKind::Error | TokenKind::Eof => true,
    }
}

/// An incremental lexer: the parser (US-020) pulls one token at a time and says whether
/// a `/` at that point may start a regex literal.
pub struct Lexer<'a> {
    file: &'a SourceFile,
    text: &'a str,
    pos: usize,
    diagnostics: Diagnostics,
    /// True when the previous token's text ends with `.`: the next word is a property
    /// name, where a removed word is ordinary (`Map.delete`, SPEC.md §12).
    after_dot: bool,
    /// The open template substitutions, innermost last: a `}` closes the innermost one
    /// only when no `{` inside it is still open.
    substitutions: Vec<Substitution>,
}

/// A `${` whose `}` has not been seen yet.
struct Substitution {
    /// Byte offset of the `${`.
    opened_at: usize,
    /// `{` seen inside it and not yet closed.
    depth: u32,
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

impl<'a> Lexer<'a> {
    /// A lexer at the start of `file`.
    #[must_use]
    pub fn new(file: &'a SourceFile) -> Self {
        Lexer {
            file,
            text: file.text(),
            pos: 0,
            diagnostics: Diagnostics::new(),
            after_dot: false,
            substitutions: Vec::new(),
        }
    }

    /// The diagnostics so far, consuming the lexer.
    #[must_use]
    pub fn finish(self) -> Diagnostics {
        self.diagnostics
    }

    /// Moves back to `offset` so the parser can lex a `/` token again as a regex. The
    /// offset is one the lexer handed out as a trivia or token start, so it is a
    /// character boundary; only the position moves, since a `/` token changes nothing
    /// else and reports nothing.
    pub(crate) fn rewind(&mut self, offset: usize) {
        self.pos = offset;
    }

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

    /// The next token, with its leading trivia; `Eof` at the end, again and again. With
    /// `regex_allowed`, a `/` here starts a regex literal; otherwise it is `/` or `/=`.
    pub fn next_token(&mut self, regex_allowed: bool) -> Token {
        let leading = self.trivia();
        let has_newline_before = leading.iter().any(Trivia::is_newline);
        let start = self.pos;
        let kind = match self.bump() {
            None => self.eof(start),
            Some('`') => self.template(TemplateEnd::Backtick),
            Some('{') => {
                if let Some(open) = self.substitutions.last_mut() {
                    open.depth += 1;
                }
                TokenKind::Punct(Punct::LBrace)
            }
            Some('}') => self.close_brace(),
            Some('/') if regex_allowed => self.regex(start),
            Some('/') if self.peek() == Some('=') => {
                self.bump();
                TokenKind::Punct(Punct::SlashEq)
            }
            Some('/') => TokenKind::Punct(Punct::Slash),
            Some(ch) if ch == '_' || ch.is_ascii_alphabetic() => self.word(start),
            Some(ch) if ch.is_ascii_digit() => self.number(start),
            Some('.') if self.peek().is_some_and(|c| c.is_ascii_digit()) => self.number(start),
            Some('"' | '\'') => self.string(start),
            Some('$') => self.error(start, "`$` is not permitted in identifiers"),
            Some('\\') if self.peek() == Some('u') => self.unicode_escape(start),
            Some('#') if self.peek() == Some('!') => self.hashbang(start),
            Some('#') => self.private_name(start),
            Some('@') => self.error(
                start,
                "decorators are removed; use explicit wrapping functions (SPEC.md §12)",
            ),
            Some(ch) if ch.is_alphabetic() => self.error(
                start,
                &format!(
                    "{ch:?} is not permitted in identifiers: only ASCII letters, digits and `_`"
                ),
            ),
            Some(ch) => self.punct(start, ch),
        };
        let span = self.span(start, self.pos);
        // `1.` is a number, not a member access.
        self.after_dot = !matches!(kind, TokenKind::Eof | TokenKind::Number(_))
            && self.text[start..self.pos].ends_with('.');
        Token {
            kind,
            span,
            leading,
            has_newline_before,
        }
    }

    /// E000 covering the one character at `at` (empty at the end of the file).
    fn report_at(&mut self, at: usize, message: String) {
        let after = at + self.text[at..].chars().next().map_or(0, char::len_utf8);
        self.report(Code::E000, at, after, message);
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
                    // `with` has one permitted use, the import-attributes keyword, so the
                    // parser reports `WithStatement`; the other removed words have none.
                    if let Some(replacement) = keyword.removed_replacement() {
                        if !self.after_dot && keyword != Keyword::With {
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

    /// The longest punctuator at `start` (`ch` is its first character, already consumed);
    /// `?.` directly before a decimal digit is `?`, so `a?.5:1` is a ternary.
    fn punct(&mut self, start: usize, ch: char) -> TokenKind {
        let rest = &self.text[start..];
        let Some(punct) = Punct::longest_match(rest) else {
            return self.error(start, &format!("unexpected character {ch:?}"));
        };
        let punct =
            if punct == Punct::QuestionDot && rest[2..].starts_with(|c: char| c.is_ascii_digit()) {
                Punct::Question
            } else {
                punct
            };
        self.pos = start + punct.as_str().len();
        TokenKind::Punct(punct)
    }

    /// `#name`: the `#` must be followed directly by an identifier.
    fn private_name(&mut self, start: usize) -> TokenKind {
        if !self
            .peek()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        {
            return self.error(
                start,
                "`#` begins a private name and must be followed directly by its identifier",
            );
        }
        while self
            .peek()
            .is_some_and(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            self.bump();
        }
        TokenKind::PrivateName(self.text[start + 1..self.pos].into())
    }

    /// A numeric literal (`number.rs`). A rejected literal is one error token covering
    /// the whole run, with the diagnostic at the exact offending character.
    fn number(&mut self, start: usize) -> TokenKind {
        let (end, result) = number::scan(self.text, start);
        self.pos = end;
        match result {
            Ok(kind) => kind,
            Err(rejection) => {
                while self
                    .peek()
                    .is_some_and(|c| matches!(c, '_' | '$' | '\\') || c.is_alphanumeric())
                {
                    self.bump();
                }
                let at = rejection.at;
                let after = at + self.text[at..].chars().next().map_or(0, char::len_utf8);
                self.report(Code::E000, at, after, rejection.message);
                TokenKind::Error
            }
        }
    }

    /// The end of the file: every `${` still open is E000 here.
    fn eof(&mut self, start: usize) -> TokenKind {
        for open in std::mem::take(&mut self.substitutions) {
            let at = self
                .file
                .position(open.opened_at)
                .unwrap_or(Position::new(1, 1));
            self.report(
                Code::E000,
                start,
                start,
                format!(
                    "unterminated template literal: the `${{` at {}:{} is never closed by `}}`",
                    at.line, at.column
                ),
            );
        }
        TokenKind::Eof
    }

    /// `}`: closes the innermost substitution when no `{` inside it is open, resuming the
    /// template; otherwise it is a brace.
    fn close_brace(&mut self) -> TokenKind {
        if let Some(open) = self.substitutions.last_mut() {
            if open.depth == 0 {
                self.substitutions.pop();
                return self.template(TemplateEnd::Substitution);
            }
            open.depth -= 1;
        }
        TokenKind::Punct(Punct::RBrace)
    }

    /// A template piece opened by `` ` `` (`opener` is `Backtick`) or by the `}` of a
    /// substitution (`opener` is `Substitution`), already consumed. Bad escapes make it
    /// an error token with an E000 per escape; a piece the file ends inside is E000 at
    /// the end of the file.
    fn template(&mut self, opener: TemplateEnd) -> TokenKind {
        let piece = string::scan_template(self.text, self.pos);
        self.pos = piece.end;
        let malformed = !piece.problems.is_empty();
        for problem in piece.problems {
            self.report(Code::E000, problem.start, problem.end, problem.message);
        }
        let part = match (opener, piece.ending) {
            (_, TemplateEnd::Eof) => {
                self.report(
                    Code::E000,
                    piece.end,
                    piece.end,
                    "unterminated template literal: expected `` ` `` before the end of the file"
                        .to_owned(),
                );
                return TokenKind::Error;
            }
            (TemplateEnd::Backtick, TemplateEnd::Backtick) => TemplatePart::Whole,
            (TemplateEnd::Backtick, TemplateEnd::Substitution) => TemplatePart::Head,
            (_, TemplateEnd::Backtick) => TemplatePart::Tail,
            (_, TemplateEnd::Substitution) => TemplatePart::Middle,
        };
        if piece.ending == TemplateEnd::Substitution {
            self.substitutions.push(Substitution {
                opened_at: piece.end - 2,
                depth: 0,
            });
        }
        if malformed {
            return TokenKind::Error;
        }
        TokenKind::Template(Template {
            part,
            cooked: piece.cooked.into_boxed_str(),
            raw: piece.raw.into_boxed_str(),
        })
    }

    /// A regex literal (`regex.rs`), whose opening `/` is already consumed. A line
    /// terminator or the end of the file before the closing `/` is E000 there; a flag
    /// that is not one of `d g i m s v y`, a repeated flag or `u` alongside `v` is E000
    /// at that flag; a well-formed flag set lacking `v` is E002 on the literal, which
    /// still lexes as a regex (SPEC.md §2).
    fn regex(&mut self, start: usize) -> TokenKind {
        let scan = regex::scan(self.text, start);
        self.pos = scan.end;
        match scan.kind {
            ScanKind::Unterminated => {
                self.report(
                    Code::E000,
                    scan.end,
                    scan.end,
                    "unterminated regex literal: expected `/` before the end of the line"
                        .to_owned(),
                );
                TokenKind::Error
            }
            ScanKind::Closed {
                body,
                flags,
                flags_at,
            } => {
                let mut malformed = false;
                let groups = match pattern::parse(&body) {
                    Ok(parsed) => parsed.groups,
                    Err(error) => {
                        malformed = true;
                        self.report_at(start + 1 + error.offset, error.message);
                        Vec::new()
                    }
                };
                match regex::check_flags(&flags) {
                    Flags::Ok => {}
                    Flags::Syntax { offset, message } => {
                        malformed = true;
                        self.report_at(flags_at + offset, message);
                    }
                    Flags::MissingV(message) => {
                        self.report(Code::E002, start, scan.end, message);
                    }
                }
                if malformed {
                    return TokenKind::Error;
                }
                TokenKind::Regex(Regex {
                    body: body.into_boxed_str(),
                    flags: flags.into_boxed_str(),
                    groups: groups.into_iter().map(String::into_boxed_str).collect(),
                })
            }
        }
    }

    /// A string literal (`string.rs`). Bad escapes make it one error token with an E000
    /// per escape; an unterminated one is E000 at the line end and the terminator starts
    /// the next token's trivia. A `'…'` literal is S001 with its double-quoted spelling.
    fn string(&mut self, start: usize) -> TokenKind {
        let literal = string::scan(self.text, start);
        self.pos = literal.end;
        let malformed = !literal.problems.is_empty();
        for problem in literal.problems {
            self.report(Code::E000, problem.start, problem.end, problem.message);
        }
        if !literal.terminated {
            let quote = &self.text[start..=start];
            self.report(
                Code::E000,
                literal.end,
                literal.end,
                format!(
                    "unterminated string literal: expected `{quote}` before the end of the line"
                ),
            );
            return TokenKind::Error;
        }
        if malformed {
            return TokenKind::Error;
        }
        if self.text.as_bytes()[start] == b'\'' {
            let raw = &self.text[start..literal.end];
            let span = self.span(start, literal.end);
            let finding = Diagnostic::new(
                Code::S001,
                self.file.path(),
                span,
                "single-quoted string: strings are written with double quotes",
            )
            .with_fix(raw, string::double_quoted(raw));
            self.diagnostics.extend(finding.ok());
        }
        TokenKind::String(literal.cooked.into_boxed_str())
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

    /// `/* … */` or `/** … */`; an unterminated comment runs to the end of the file and
    /// is E000 at its start (SPEC.md §2 comments are `//` and `/* */` only). `<!--` and
    /// `-->` are never comments: a module has no HTML-like comments, so they lex as
    /// operators for the parser to reject.
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
        // Starts with `/**` and is longer than `/**/`: `/**/` is an empty block comment,
        // `/***/` a doc comment whose body is `*`.
        let text = &self.text[start..self.pos];
        if text.starts_with("/**") && text.len() > 4 {
            TriviaKind::DocComment {
                has_line_terminator,
            }
        } else {
            TriviaKind::BlockComment {
                has_line_terminator,
            }
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
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
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
        let f = file("\\é\u{1F600}");
        let (stream, diagnostics) = lex(&f);
        assert_eq!(
            kinds("\\é\u{1F600}"),
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
                "unexpected character '\\\\'",
                "'é' is not permitted in identifiers: only ASCII letters, digits and `_`",
                "unexpected character '\u{1F600}'"
            ]
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.code == Code::E000 && d.path == "a.js"));
        assert_eq!(stream.print(&f), "\\é\u{1F600}");
    }

    #[test]
    fn a_slash_alone_is_not_a_comment() {
        assert_eq!(
            kinds("/"),
            [TokenKind::Error, TokenKind::Eof],
            "an unterminated regex"
        );
        assert_eq!(
            kinds("/ /"),
            [
                TokenKind::Regex(crate::token::Regex {
                    body: " ".into(),
                    flags: "".into(),
                    groups: Box::default(),
                }),
                TokenKind::Eof
            ],
            "a regex of one space, lacking its flags"
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
                stream.tokens.last().map(|t| t.kind.clone()),
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
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
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
        assert_eq!(
            kinds("with"),
            [TokenKind::Keyword(Keyword::With), TokenKind::Eof],
            "`with` is a keyword token"
        );
        assert!(
            messages("with").is_empty(),
            "but the lexer does not report it: it is the import-attributes keyword, and the parser reports a `with` statement"
        );
        assert_eq!(
            Keyword::With.removed_replacement(),
            Some("nothing: there is no dynamic scope"),
            "the parser's message still names the replacement"
        );
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
            assert_eq!(found, Vec::<String>::new(), "{text:?}");
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
            stream
                .tokens
                .iter()
                .map(|t| t.kind.clone())
                .collect::<Vec<_>>(),
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
            .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
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
            (
                stream.tokens[0].kind.clone(),
                stream.tokens[0].span.byte_end
            ),
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
            stream
                .tokens
                .iter()
                .map(|t| t.kind.clone())
                .collect::<Vec<_>>(),
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
            Vec::<String>::new(),
            "a digit starts a number"
        );
    }

    #[test]
    fn a_hashbang_is_rejected_anywhere_and_swallows_its_line() {
        let (stream, diagnostics) = lex(&file("#!/usr/bin/env node\nconst"));
        assert_eq!(
            stream
                .tokens
                .iter()
                .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
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
            ["`#` begins a private name and must be followed directly by its identifier"],
            "a lone hash is not a hashbang"
        );
        assert_eq!(messages("#!"), ["`#!` is removed; a hashbang line belongs to a host-side launcher outside the subset"]);
    }
}

#[cfg(test)]
mod number_tests {
    use super::lex;
    use crate::token::{Number, TokenKind};
    use sjs_diag::{Code, SourceFile};
    use sjs_testkit::prop::{check_with, Config, U64};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    /// The one token of `text`, which must lex cleanly to a number.
    fn number(text: &str) -> Number {
        let (stream, diagnostics) = lex(&file(text));
        assert!(diagnostics.is_empty(), "{text}: {diagnostics:?}");
        assert_eq!(stream.tokens.len(), 2, "{text} is one token");
        as_number(&stream.tokens[0].kind).expect("a number")
    }

    fn as_number(kind: &TokenKind) -> Result<Number, String> {
        match kind {
            TokenKind::Number(number) => Ok(*number),
            other => Err(format!("{other:?} is not a number")),
        }
    }

    fn as_bigint(kind: &TokenKind) -> Result<String, String> {
        match kind {
            TokenKind::BigInt(digits) => Ok(digits.to_string()),
            other => Err(format!("{other:?} is not a bigint")),
        }
    }

    #[test]
    fn the_helpers_name_a_wrong_kind() {
        assert_eq!(
            as_number(&TokenKind::Eof),
            Err("Eof is not a number".to_owned())
        );
        assert_eq!(
            as_bigint(&TokenKind::Null),
            Err("Null is not a bigint".to_owned())
        );
    }

    fn value(text: &str) -> f64 {
        number(text).value
    }

    fn bigint(text: &str) -> String {
        let (stream, diagnostics) = lex(&file(text));
        assert!(diagnostics.is_empty(), "{text}: {diagnostics:?}");
        assert_eq!(stream.tokens.len(), 2, "{text} is one token");
        as_bigint(&stream.tokens[0].kind).expect("a bigint")
    }

    /// The single E000 of `text` as (column, message), with the error token's extent.
    fn rejection(text: &str) -> (u32, String, usize) {
        let (stream, diagnostics) = lex(&file(text));
        assert_eq!(stream.tokens[0].kind, TokenKind::Error, "{text}");
        assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
        let d = diagnostics.iter().next().expect("one diagnostic");
        assert_eq!(d.code, Code::E000);
        (
            d.span.start.column,
            d.message.clone(),
            stream.tokens[0].span.byte_end,
        )
    }

    #[test]
    fn decimal_literals_have_their_double_value() {
        let cases: [(&str, f64); 16] = [
            ("0", 0.0),
            ("1", 1.0),
            ("42", 42.0),
            ("1.5", 1.5),
            (".5", 0.5),
            ("1.", 1.0),
            ("0.25", 0.25),
            ("1e3", 1000.0),
            ("1E-3", 0.001),
            ("1.e3", 1000.0),
            ("1.5e+2", 150.0),
            ("0e1", 0.0),
            ("1_000_000", 1_000_000.0),
            ("1_0.0_1e1_0", 100_100_000_000.0),
            ("0.1", 0.1),
            ("123456789012345678901234567890", 1.234_567_890_123_456_8e29),
        ];
        for (text, expected) in cases {
            assert_eq!(value(text).to_bits(), expected.to_bits(), "{text}");
        }
        assert_eq!(
            value("1e400"),
            f64::INFINITY,
            "overflow is infinity, not an error"
        );
        assert_eq!(value("1e-400"), 0.0, "underflow is zero");
    }

    #[test]
    fn radix_literals_are_exact_then_rounded_once() {
        let cases: [(&str, f64); 10] = [
            ("0x1F", 31.0),
            ("0XfF", 255.0),
            ("0x0", 0.0),
            ("0o17", 15.0),
            ("0O7", 7.0),
            ("0b101", 5.0),
            ("0B1", 1.0),
            ("0xFF_FF", 65535.0),
            ("0b1_0", 2.0),
            ("0x1_0000_0000_0000_0000", 18_446_744_073_709_551_616.0),
        ];
        for (text, expected) in cases {
            assert_eq!(value(text).to_bits(), expected.to_bits(), "{text}");
        }
        assert_eq!(
            value("0x20000000000001"),
            9_007_199_254_740_992.0,
            "2^53 + 1 rounds to even"
        );
        assert_eq!(
            value("0x20000000000003"),
            9_007_199_254_740_996.0,
            "2^53 + 3 rounds up"
        );
    }

    #[test]
    fn bigint_literals_carry_their_decimal_digits() {
        let cases = [
            ("0n", "0"),
            ("1n", "1"),
            ("1_000n", "1000"),
            ("0x10n", "16"),
            ("0o10n", "8"),
            ("0b11n", "3"),
            ("0x0n", "0"),
            ("0b0_0n", "0"),
            ("9007199254740993n", "9007199254740993"),
            ("12345678901234567890123n", "12345678901234567890123"),
            ("0xFFFF_FFFF_FFFF_FFFF_FFFFn", "1208925819614629174706175"),
        ];
        for (text, digits) in cases {
            assert_eq!(bigint(text), digits, "{text}");
        }
    }

    #[test]
    fn integers_beyond_the_safe_range_are_flagged() {
        for text in [
            "0",
            "1",
            "1.5",
            "1e15",
            "9007199254740991",
            "0x1FFFFFFFFFFFFF",
            "1e-400",
        ] {
            assert!(!number(text).exceeds_safe_integer, "{text}");
        }
        for text in [
            "9007199254740992",
            "9007199254740993",
            "0x20000000000000",
            "1e16",
            "1e400",
        ] {
            assert!(number(text).exceeds_safe_integer, "{text}");
        }
        assert_ne!(number("9007199254740991"), number("9007199254740992"));
        assert_eq!(number("1e3"), number("1000"));
        assert_ne!(number("0"), number("1"));
    }

    #[test]
    fn leading_zeros_are_rejected_at_the_first_column() {
        assert_eq!(
            rejection("017"),
            (
                1,
                "`017` is a legacy octal literal; write `0o17`".to_owned(),
                3
            )
        );
        assert_eq!(
            rejection("0017"),
            (
                1,
                "`0017` is a legacy octal literal; write `0o17`".to_owned(),
                4
            )
        );
        assert_eq!(
            rejection("08"),
            (1, "`08` has a leading zero; write `8`".to_owned(), 2)
        );
        assert_eq!(
            rejection("09"),
            (1, "`09` has a leading zero; write `9`".to_owned(), 2)
        );
        assert_eq!(
            rejection("0189"),
            (1, "`0189` has a leading zero; write `189`".to_owned(), 4)
        );
        assert_eq!(
            rejection("00"),
            (1, "`00` has a leading zero; write `0`".to_owned(), 2)
        );
        assert_eq!(
            rejection("01n"),
            (
                1,
                "`01` is a legacy octal literal; write `0o1`".to_owned(),
                3
            )
        );
        assert_eq!(
            rejection("0_1"),
            (
                2,
                "a numeric separator `_` must sit between two digits".to_owned(),
                3
            )
        );
    }

    #[test]
    fn malformed_literals_are_e000_at_the_exact_column() {
        let separator = "a numeric separator `_` must sit between two digits";
        let run_in = "a numeric literal must not run into an identifier or another digit";
        let cases: [(&str, u32, &str); 20] = [
            ("1__0", 3, separator),
            ("1_", 2, separator),
            ("0x_1", 3, separator),
            ("1_n", 2, separator),
            ("1_.5", 2, separator),
            ("1._5", 3, separator),
            ("1e_3", 3, separator),
            ("1_e3", 2, separator),
            (
                "1.5n",
                4,
                "a bigint literal must be an integer: no fraction and no exponent",
            ),
            (
                "1e3n",
                4,
                "a bigint literal must be an integer: no fraction and no exponent",
            ),
            (
                ".5n",
                3,
                "a bigint literal must be an integer: no fraction and no exponent",
            ),
            ("0b2", 3, "expected a binary digit after `0b`"),
            ("0o8", 3, "expected a octal digit after `0o`"),
            ("0xG", 3, "expected a hexadecimal digit after `0x`"),
            ("0X", 3, "expected a hexadecimal digit after `0X`"),
            ("3in", 2, run_in),
            ("0b12", 4, run_in),
            ("1é", 2, run_in),
            ("1$", 2, run_in),
            ("1e", 3, "expected an exponent digit"),
        ];
        for (text, column, message) in cases {
            let (at, found, _) = rejection(text);
            assert_eq!((at, found.as_str()), (column, message), "{text}");
        }
        assert_eq!(rejection("1e+").0, 4);
        assert_eq!(rejection("1\\u0061").0, 2);
    }

    #[test]
    fn a_rejected_literal_is_one_error_token_and_lexing_continues() {
        let (stream, diagnostics) = lex(&file("3in 4 1__0 x 1é"));
        let tokens: Vec<(TokenKind, usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            tokens,
            [
                (TokenKind::Error, 0, 3),
                (TokenKind::Number(number("4")), 4, 5),
                (TokenKind::Error, 6, 10),
                (TokenKind::Ident, 11, 12),
                (TokenKind::Error, 13, 16),
                (TokenKind::Eof, 16, 16),
            ]
        );
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(stream.print(&file("3in 4 1__0 x 1é")), "3in 4 1__0 x 1é");
        let (stream, _) = lex(&file("0x"));
        assert_eq!(
            (
                stream.tokens[0].span.byte_start,
                stream.tokens[0].span.byte_end
            ),
            (0, 2)
        );
    }

    #[test]
    fn a_number_ends_where_the_grammar_says() {
        assert_eq!(
            kinds("1.5.3"),
            [
                TokenKind::Number(number("1.5")),
                TokenKind::Number(number(".3")),
                TokenKind::Eof
            ],
            "`1.5.3` is `1.5` then `.3`"
        );
        assert_eq!(
            kinds("1..5"),
            [
                TokenKind::Number(number("1.")),
                TokenKind::Number(number(".5")),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("_1"),
            [TokenKind::Ident, TokenKind::Eof],
            "`_1` is an identifier"
        );
        assert_eq!(
            kinds("..5")[0],
            TokenKind::Punct(crate::token::Punct::Dot),
            "a lone `.` is not a number"
        );
        assert_eq!(
            rejection("1.in").0,
            3,
            "`1.in` runs a number into a word, as in ES2025"
        );
        let messages: Vec<String> = lex(&file("1. in"))
            .1
            .iter()
            .map(|d| d.message.clone())
            .collect();
        assert_eq!(
            messages,
            ["`in` is removed; use a discriminant property or `Object.hasOwn`"],
            "`1.` is a number, not a member access, so the removed word is reported"
        );
    }

    #[test]
    fn random_integers_round_trip_through_every_radix() {
        let config = Config::from_vars(None, None);
        let gen = U64 {
            lo: 0,
            hi: u64::MAX,
        };
        check_with(&config, &gen, &|n: &u64| {
            let n = *n;
            let decimal = format!("{n}");
            let safe = n <= 9_007_199_254_740_991;
            // The rounded double is what the lexer must reproduce.
            #[allow(clippy::cast_precision_loss)]
            let expected = n as f64;
            for text in [
                decimal.clone(),
                format!("0x{n:x}"),
                format!("0X{n:X}"),
                format!("0o{n:o}"),
                format!("0b{n:b}"),
            ] {
                let got = number(&text);
                assert_eq!(got.value.to_bits(), expected.to_bits(), "{text}: value");
                assert_ne!(got.exceeds_safe_integer, safe, "{text}: safe flag");
                assert_eq!(
                    bigint(&format!("{text}n")),
                    decimal,
                    "{text}n: bigint digits"
                );
            }
            Ok(())
        })
        .expect("every integer lexes to its value");
    }
}

#[cfg(test)]
mod string_tests {
    use super::lex;
    use crate::token::TokenKind;
    use sjs_diag::{Code, Diagnostic, Position, SourceFile};
    use sjs_testkit::prop::{check_with, Config, Text};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    fn diagnostics(text: &str) -> Vec<Diagnostic> {
        lex(&file(text)).1.iter().cloned().collect()
    }

    fn string(cooked: &str) -> TokenKind {
        TokenKind::String(cooked.into())
    }

    #[test]
    fn double_quoted_strings_are_one_token_with_their_cooked_value() {
        let text = r#""a\n\t\"\\A\u{1F600}\x41" "" "é\u{2028}x""#;
        let (stream, diagnostics) = lex(&file(text));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let tokens: Vec<(TokenKind, usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            tokens,
            [
                (string("a\n\t\"\\A\u{1F600}A"), 0, 25),
                (string(""), 26, 28),
                (string("é\u{2028}x"), 29, 42),
                (TokenKind::Eof, 42, 42),
            ]
        );
        assert_eq!(stream.print(&file(text)), text, "the raw text is kept");
        assert_eq!(kinds(r#""\0""#), [string("\0"), TokenKind::Eof]);
        assert_eq!(
            kinds("\"a\\\nb\""),
            [string("ab"), TokenKind::Eof],
            "a continuation"
        );
        assert!(
            !lex(&file("\"a\\\nb\" c")).0.tokens[1].has_newline_before,
            "a continuation inside a string is not a line break between tokens"
        );
    }

    #[test]
    fn single_quoted_strings_are_s001_with_the_double_quoted_replacement() {
        let cases = [
            ("'a'", "\"a\"", "a"),
            (r#"'say "hi"'"#, r#""say \"hi\"""#, "say \"hi\""),
            (r"'it\'s'", r#""it's""#, "it's"),
            (r#"'a\'b"c'"#, r#""a'b\"c""#, "a'b\"c"),
            ("''", "\"\"", ""),
            (r"'\n'", r#""\n""#, "\n"),
        ];
        for (raw, replacement, cooked) in cases {
            let text = format!("x {raw}");
            let (stream, found) = lex(&file(&text));
            assert_eq!(stream.tokens[1].kind, string(cooked), "{raw}");
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{raw}: {found:?}");
            let d = found[0];
            assert_eq!(d.code, Code::S001, "{raw}");
            assert_eq!(
                (d.span.byte_start, d.span.byte_end, d.span.start, d.span.end),
                (
                    2,
                    text.len(),
                    Position::new(1, 3),
                    Position::new(1, u32::try_from(text.len()).expect("small") + 1)
                ),
                "{raw}: the finding spans the whole literal"
            );
            assert_eq!(
                (d.current.as_deref(), d.replacement.as_deref()),
                (Some(raw), Some(replacement)),
                "{raw}"
            );
            assert_eq!(
                d.message,
                "single-quoted string: strings are written with double quotes"
            );
        }
        assert_eq!(diagnostics("'a' 'b'").len(), 2, "one finding per literal");
    }

    #[test]
    fn bad_escapes_make_an_error_token_with_an_e000_per_escape() {
        for text in [
            r#""\1""#,
            r#""\01""#,
            r#""\8""#,
            r#""\9""#,
            r#""\u{110000}""#,
            r#""\x4""#,
        ] {
            let (stream, found) = lex(&file(text));
            assert_eq!(stream.tokens[0].kind, TokenKind::Error, "{text}");
            assert_eq!(
                (stream.tokens[0].span.byte_end, stream.tokens.len()),
                (text.len(), 2),
                "{text}: the whole literal is one token"
            );
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(
                (found[0].code, found[0].span.start),
                (Code::E000, Position::new(1, 2)),
                "{text}"
            );
        }
        let found = diagnostics(r#""\1\8""#);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found
                .iter()
                .map(|d| d.span.start.column)
                .collect::<Vec<_>>(),
            [2, 4]
        );
        let found = diagnostics(r"'\1'");
        assert_eq!(
            found.len(),
            1,
            "a malformed single-quoted literal is E000 only"
        );
        assert_eq!(found[0].code, Code::E000);
        assert_eq!(kinds(r"'\1'"), [TokenKind::Error, TokenKind::Eof]);
    }

    #[test]
    fn an_unterminated_string_is_e000_at_the_line_end_and_lexing_resumes() {
        let text = "\"abc\nconst";
        let (stream, found) = lex(&file(text));
        let tokens: Vec<(TokenKind, usize, usize, bool)> = stream
            .tokens
            .iter()
            .map(|t| {
                (
                    t.kind.clone(),
                    t.span.byte_start,
                    t.span.byte_end,
                    t.has_newline_before,
                )
            })
            .collect();
        assert_eq!(
            tokens,
            [
                (TokenKind::Error, 0, 4, false),
                (
                    TokenKind::Keyword(crate::token::Keyword::Const),
                    5,
                    10,
                    true
                ),
                (TokenKind::Eof, 10, 10, false),
            ]
        );
        let found: Vec<&Diagnostic> = found.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (
                found[0].code,
                found[0].span.start,
                found[0].span.end,
                found[0].message.as_str()
            ),
            (
                Code::E000,
                Position::new(1, 5),
                Position::new(1, 5),
                "unterminated string literal: expected `\"` before the end of the line"
            )
        );
        assert_eq!(stream.print(&file(text)), text);
        for (text, column) in [
            ("\"abc", 5),
            ("'abc", 5),
            ("\"abc\r\nx", 5),
            ("x \"", 4),
            ("\"a\\", 4),
        ] {
            let found = diagnostics(text);
            assert_eq!(found.len(), 1, "{text:?}");
            assert_eq!(found[0].span.start.column, column, "{text:?}");
        }
        assert_eq!(
            diagnostics("'abc")[0].message,
            "unterminated string literal: expected `'` before the end of the line"
        );
        let found = diagnostics("\"\\1\n");
        assert_eq!(
            found.len(),
            2,
            "a bad escape and the missing quote are both reported"
        );
    }

    #[test]
    fn random_quote_free_text_cooks_to_itself() {
        let gen = Text {
            alphabet: &[
                'a', ' ', '\t', '\u{2028}', '\u{2029}', 'é', '😀', '/', '*', '\'',
            ],
            max_len: 24,
        };
        check_with(&Config::new(200), &gen, &|body: &String| {
            let text = format!("\"{body}\"");
            assert_eq!(kinds(&text), [string(body), TokenKind::Eof], "{text:?}");
            assert!(diagnostics(&text).is_empty(), "{text:?}");
            Ok(())
        })
        .expect("every quote-free body is its own value");
    }
}

#[cfg(test)]
mod template_tests {
    use super::lex;
    use crate::token::{Punct, Template, TemplatePart, TokenKind};
    use sjs_diag::{Code, Diagnostic, Position, SourceFile};
    use sjs_testkit::prop::{check_with, Config, Text};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    fn diagnostics(text: &str) -> Vec<Diagnostic> {
        lex(&file(text)).1.iter().cloned().collect()
    }

    fn piece(part: TemplatePart, cooked: &str, raw: &str) -> TokenKind {
        TokenKind::Template(Template {
            part,
            cooked: cooked.into(),
            raw: raw.into(),
        })
    }

    fn template(part: TemplatePart, text: &str) -> TokenKind {
        piece(part, text, text)
    }

    use Punct::{LBrace, RBrace};
    use TemplatePart::{Head, Middle, Tail, Whole};

    #[test]
    fn a_template_without_substitutions_is_one_token_with_both_values() {
        let text = "`a\\n\\u{41}` ``";
        let (stream, found) = lex(&file(text));
        assert!(found.is_empty(), "{found:?}");
        let tokens: Vec<(TokenKind, usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            tokens,
            [
                (piece(Whole, "a\nA", "a\\n\\u{41}"), 0, 11),
                (template(Whole, ""), 12, 14),
                (TokenKind::Eof, 14, 14),
            ]
        );
        assert_eq!(stream.print(&file(text)), text);
    }

    #[test]
    fn substitutions_split_a_template_into_head_middle_and_tail() {
        let text = "`a${b}c${d}e`";
        let (stream, found) = lex(&file(text));
        assert!(found.is_empty(), "{found:?}");
        let tokens: Vec<(TokenKind, usize, usize)> = stream
            .tokens
            .iter()
            .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
            .collect();
        assert_eq!(
            tokens,
            [
                (template(Head, "a"), 0, 4),
                (TokenKind::Ident, 4, 5),
                (template(Middle, "c"), 5, 9),
                (TokenKind::Ident, 9, 10),
                (template(Tail, "e"), 10, 13),
                (TokenKind::Eof, 13, 13),
            ],
            "each piece owns its delimiters"
        );
        assert_eq!(
            kinds("`${a}`"),
            [
                template(Head, ""),
                TokenKind::Ident,
                template(Tail, ""),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("`${a}${b}`"),
            [
                template(Head, ""),
                TokenKind::Ident,
                template(Middle, ""),
                TokenKind::Ident,
                template(Tail, ""),
                TokenKind::Eof
            ],
            "adjacent substitutions"
        );
        assert_eq!(
            kinds("`${}`"),
            [template(Head, ""), template(Tail, ""), TokenKind::Eof],
            "an empty substitution is the parser's problem"
        );
    }

    #[test]
    fn every_nesting_case_round_trips() {
        let cases: [(&str, Vec<TokenKind>); 7] = [
            (
                "`${ {a:1}.a }`",
                vec![
                    template(Head, ""),
                    TokenKind::Punct(LBrace),
                    TokenKind::Ident,
                    TokenKind::Punct(Punct::Colon),
                    TokenKind::Number(crate::token::Number {
                        value: 1.0,
                        exceeds_safe_integer: false,
                    }),
                    TokenKind::Punct(RBrace),
                    TokenKind::Punct(Punct::Dot),
                    TokenKind::Ident,
                    template(Tail, ""),
                    TokenKind::Eof,
                ],
            ),
            (
                "`a${`b${c}d`}e`",
                vec![
                    template(Head, "a"),
                    template(Head, "b"),
                    TokenKind::Ident,
                    template(Tail, "d"),
                    template(Tail, "e"),
                    TokenKind::Eof,
                ],
            ),
            (
                "`${\"}\"}`",
                vec![
                    template(Head, ""),
                    TokenKind::String("}".into()),
                    template(Tail, ""),
                    TokenKind::Eof,
                ],
            ),
            (
                "`${ \"`\" }`",
                vec![
                    template(Head, ""),
                    TokenKind::String("`".into()),
                    template(Tail, ""),
                    TokenKind::Eof,
                ],
            ),
            (
                "`${ { x: { y: `z` } } }`",
                vec![
                    template(Head, ""),
                    TokenKind::Punct(LBrace),
                    TokenKind::Ident,
                    TokenKind::Punct(Punct::Colon),
                    TokenKind::Punct(LBrace),
                    TokenKind::Ident,
                    TokenKind::Punct(Punct::Colon),
                    template(Whole, "z"),
                    TokenKind::Punct(RBrace),
                    TokenKind::Punct(RBrace),
                    template(Tail, ""),
                    TokenKind::Eof,
                ],
            ),
            (
                "`*/ /* ${a} // `",
                vec![
                    template(Head, "*/ /* "),
                    TokenKind::Ident,
                    template(Tail, " // "),
                    TokenKind::Eof,
                ],
            ),
            (
                "{ `${a}` }",
                vec![
                    TokenKind::Punct(LBrace),
                    template(Head, ""),
                    TokenKind::Ident,
                    template(Tail, ""),
                    TokenKind::Punct(RBrace),
                    TokenKind::Eof,
                ],
            ),
        ];
        for (text, expected) in cases {
            let f = file(text);
            let (stream, found) = lex(&f);
            let kinds: Vec<TokenKind> = stream.tokens.iter().map(|t| t.kind.clone()).collect();
            assert_eq!(kinds, expected, "{text}");
            assert_eq!(stream.print(&f), text, "{text}");
            assert!(found.is_empty(), "{text}: {found:?}");
        }
    }

    #[test]
    fn braces_outside_templates_are_plain_punctuators() {
        assert_eq!(
            kinds("} { } }"),
            [
                TokenKind::Punct(RBrace),
                TokenKind::Punct(LBrace),
                TokenKind::Punct(RBrace),
                TokenKind::Punct(RBrace),
                TokenKind::Eof
            ]
        );
        assert_eq!(diagnostics("} { } }").len(), 0);
        assert_eq!(
            kinds("} `${a}`"),
            [
                TokenKind::Punct(RBrace),
                template(Head, ""),
                TokenKind::Ident,
                template(Tail, ""),
                TokenKind::Eof
            ],
            "a stray `}}` does not confuse a later template"
        );
        assert_eq!(Punct::LBrace.as_str(), "{");
        assert_eq!(Punct::RBrace.as_str(), "}");
    }

    #[test]
    fn an_unterminated_template_is_e000_at_the_end_of_the_file() {
        for (text, kinds_expected, columns) in [
            ("`abc", vec![TokenKind::Error, TokenKind::Eof], vec![5]),
            (
                "`a${b}c",
                vec![
                    template(Head, "a"),
                    TokenKind::Ident,
                    TokenKind::Error,
                    TokenKind::Eof,
                ],
                vec![8],
            ),
            (
                "`a${b",
                vec![template(Head, "a"), TokenKind::Ident, TokenKind::Eof],
                vec![6],
            ),
            ("`${", vec![template(Head, ""), TokenKind::Eof], vec![4]),
            (
                "`${`${",
                vec![template(Head, ""), template(Head, ""), TokenKind::Eof],
                vec![7, 7],
            ),
            (
                "`${ {",
                vec![template(Head, ""), TokenKind::Punct(LBrace), TokenKind::Eof],
                vec![6],
            ),
        ] {
            assert_eq!(kinds(text), kinds_expected, "{text}");
            let found = diagnostics(text);
            assert_eq!(
                found
                    .iter()
                    .map(|d| d.span.start.column)
                    .collect::<Vec<_>>(),
                columns,
                "{text}: {found:?}"
            );
            assert!(found.iter().all(|d| d.code == Code::E000), "{text}");
        }
        assert_eq!(
            diagnostics("`abc")[0].message,
            "unterminated template literal: expected `` ` `` before the end of the file"
        );
        let found = diagnostics("x\n`a${b");
        assert_eq!(
            (found[0].span.start, found[0].message.as_str()),
            (
                Position::new(2, 6),
                "unterminated template literal: the `${` at 2:3 is never closed by `}`"
            )
        );
        let found = diagnostics("`${`${");
        assert!(
            found[0].message.contains("at 1:2 ") && found[1].message.contains("at 1:5 "),
            "{found:?}"
        );
    }

    #[test]
    fn an_invalid_escape_in_a_template_is_e000() {
        for text in ["`\\u{`", "`\\1`", "`\\x`", "`\\8`", "`\\u{110000}`"] {
            let (stream, found) = lex(&file(text));
            assert_eq!(stream.tokens[0].kind, TokenKind::Error, "{text}");
            assert_eq!(
                stream.tokens[0].span.byte_end,
                text.len(),
                "{text}: the whole piece"
            );
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(
                (found[0].code, found[0].span.start),
                (Code::E000, Position::new(1, 2)),
                "{text}"
            );
        }
        assert_eq!(
            kinds("`\\1${a}b${c}\\8`"),
            [
                TokenKind::Error,
                TokenKind::Ident,
                template(Middle, "b"),
                TokenKind::Ident,
                TokenKind::Error,
                TokenKind::Eof
            ],
            "a malformed piece still opens and closes its substitution"
        );
        assert_eq!(diagnostics("`\\1${a}b${c}\\8`").len(), 2);
    }

    #[test]
    fn a_multi_line_template_keeps_its_terminators_in_the_source_and_normalises_its_values() {
        let text = "`a\r\nb\rc\nd` e";
        let f = file(text);
        let (stream, found) = lex(&f);
        assert!(found.is_empty());
        assert_eq!(stream.tokens[0].kind, template(Whole, "a\nb\nc\nd"));
        assert_eq!(
            stream.print(&f),
            text,
            "the span keeps CR and CRLF as written"
        );
        assert_eq!(f.slice(&stream.tokens[0].span), Some("`a\r\nb\rc\nd`"));
        assert_eq!(stream.tokens[0].span.end, Position::new(4, 3));
        assert!(
            !stream.tokens[1].has_newline_before,
            "line terminators inside a template are not line breaks between tokens"
        );
        assert_eq!(
            kinds("`a\\\nb`"),
            [piece(Whole, "ab", "a\\\nb"), TokenKind::Eof],
            "a continuation is raw but not cooked"
        );
    }

    #[test]
    fn random_template_soup_round_trips_and_ends_with_eof() {
        let gen = Text {
            alphabet: &['`', '$', '{', '}', '\\', '\n', '\r', '"', 'a', ' ', 'u'],
            max_len: 24,
        };
        check_with(&Config::new(400), &gen, &|text: &String| {
            let f = file(text);
            let (stream, _) = lex(&f);
            assert_eq!(stream.print(&f), *text, "{text:?}");
            assert_eq!(stream.eof().kind, TokenKind::Eof, "{text:?}");
            let last = stream.tokens.len() - 1;
            assert!(
                stream.tokens[..last]
                    .iter()
                    .all(|t| t.kind != TokenKind::Eof),
                "{text:?}"
            );
            Ok(())
        })
        .expect("every soup round-trips");
    }
}

#[cfg(test)]
mod regex_tests {
    use super::{lex, regex_allowed_after, Lexer};
    use crate::token::{Keyword, Number, Punct, Regex, Template, TemplatePart, TokenKind};
    use sjs_diag::{Code, Diagnostic, Position, SourceFile};
    use sjs_testkit::prop::{check_with, Config, Text};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    fn diagnostics(text: &str) -> Vec<Diagnostic> {
        lex(&file(text)).1.iter().cloned().collect()
    }

    fn regex(body: &str, flags: &str) -> TokenKind {
        TokenKind::Regex(Regex {
            body: body.into(),
            flags: flags.into(),
            groups: Box::default(),
        })
    }

    fn number(value: f64) -> TokenKind {
        TokenKind::Number(Number {
            value,
            exceeds_safe_integer: false,
        })
    }

    use Punct::{Slash, SlashEq};

    #[test]
    fn a_regex_with_v_is_one_token_with_body_and_flags() {
        for (text, body, flags) in [
            ("/a/v", "a", "v"),
            ("/[\\/]/v", "[\\/]", "v"),
            ("/\\//v", "\\/", "v"),
            ("/=/v", "=", "v"),
            ("/a/dgimsyv", "a", "dgimsyv"),
            ("/a/vgi", "a", "vgi"),
            (
                "/a*b+(c|d)[^e]{2,}\\d\\p{L}$/v",
                "a*b+(c|d)[^e]{2,}\\d\\p{L}$",
                "v",
            ),
        ] {
            let f = file(text);
            let (stream, found) = lex(&f);
            assert!(found.is_empty(), "{text}: {found:?}");
            let tokens: Vec<(TokenKind, usize, usize)> = stream
                .tokens
                .iter()
                .map(|t| (t.kind.clone(), t.span.byte_start, t.span.byte_end))
                .collect();
            assert_eq!(
                tokens,
                [
                    (regex(body, flags), 0, text.len()),
                    (TokenKind::Eof, text.len(), text.len())
                ],
                "{text}"
            );
            assert_eq!(stream.print(&f), text, "{text}");
        }
        let r = Regex {
            body: "a".into(),
            flags: "vgi".into(),
            groups: Box::default(),
        };
        assert!(r.has_flag('g') && r.has_flag('v') && !r.has_flag('y'));
        let (stream, found) = lex(&file("/[/]/v"));
        assert_eq!(
            (
                stream.tokens[0].kind.clone(),
                stream.tokens[0].span.byte_end
            ),
            (TokenKind::Error, 6),
            "a bare `/` in a class does not end the literal, but v-mode syntax rejects it"
        );
        let found: Vec<&Diagnostic> = found.iter().collect();
        assert_eq!(
            (
                found.len(),
                found[0].span.start.column,
                found[0].message.as_str()
            ),
            (1, 3, "`/` must be escaped as `\\/` inside a class")
        );
    }

    #[test]
    fn a_well_formed_flag_set_without_v_is_e002_and_still_a_regex() {
        for (text, message) in [
            (
                "/a/",
                "regex literal without flags: write the flags, including `v`",
            ),
            (
                "/a/u",
                "regex flag `u` is the legacy Unicode mode: write `v` with `v` instead",
            ),
            ("/a/gi", "regex flags `gi` lack `v`: write `giv`"),
        ] {
            let (stream, found) = lex(&file(text));
            let flags = &text[3..];
            assert_eq!(stream.tokens[0].kind, regex("a", flags), "{text}");
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(
                (
                    found[0].code,
                    found[0].span.byte_start,
                    found[0].span.byte_end,
                    found[0].message.as_str()
                ),
                (Code::E002, 0, text.len(), message),
                "{text}: E002 spans the whole literal"
            );
        }
        assert_eq!(
            kinds("/a/u"),
            [regex("a", "u"), TokenKind::Eof],
            "the flags stay as written"
        );
    }

    #[test]
    fn a_bad_flag_is_e000_at_that_flag_and_the_literal_is_an_error_token() {
        for (text, column) in [
            ("/a/vu", 5),
            ("/a/uv", 5),
            ("/a/vv", 5),
            ("/a/vx", 5),
            ("/a/xv", 4),
            ("/a/v$", 5),
            ("/a/v1", 5),
            ("/a/vé", 5),
            ("/a/gvg", 6),
        ] {
            let (stream, found) = lex(&file(text));
            assert_eq!(
                (
                    stream.tokens[0].kind.clone(),
                    stream.tokens[0].span.byte_end
                ),
                (TokenKind::Error, text.len()),
                "{text}: one error token including the flags"
            );
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(
                (found[0].code, found[0].span.start, found[0].span.end),
                (
                    Code::E000,
                    Position::new(1, column),
                    Position::new(1, column + 1)
                ),
                "{text}"
            );
        }
        assert_eq!(
            diagnostics("/a/vx")[0].message,
            "`x` is not a regex flag: the flags are `d`, `g`, `i`, `m`, `s`, `v` and `y`"
        );
    }

    #[test]
    fn a_line_terminator_or_the_file_end_inside_a_regex_is_e000_there() {
        for (text, column, next) in [
            ("/a", 3, TokenKind::Eof),
            ("/a\nb", 3, TokenKind::Ident),
            ("/a\r\nb", 3, TokenKind::Ident),
            ("/[a\u{2028}]/v", 4, TokenKind::Punct(Punct::RBracket)),
            ("/a\\", 4, TokenKind::Eof),
            ("/a\\\nb", 4, TokenKind::Ident),
            ("/", 2, TokenKind::Eof),
        ] {
            let (stream, found) = lex(&file(text));
            assert_eq!(stream.tokens[0].kind, TokenKind::Error, "{text:?}");
            assert_eq!(stream.tokens[1].kind, next, "{text:?}: lexing resumes");
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found[0].code, Code::E000, "{text:?}");
            assert_eq!(found[0].span.start, Position::new(1, column), "{text:?}");
            assert_eq!(
                found[0].message,
                "unterminated regex literal: expected `/` before the end of the line"
            );
            assert_eq!(stream.print(&file(text)), text, "{text:?}");
        }
        assert!(
            lex(&file("/a\nb")).0.tokens[1].has_newline_before,
            "the terminator belongs to the next token"
        );
    }

    #[test]
    fn a_slash_after_a_value_is_division() {
        assert_eq!(
            kinds("a / b / c"),
            [
                TokenKind::Ident,
                TokenKind::Punct(Slash),
                TokenKind::Ident,
                TokenKind::Punct(Slash),
                TokenKind::Ident,
                TokenKind::Eof
            ],
            "two divisions"
        );
        assert_eq!(
            kinds("a /= 2"),
            [
                TokenKind::Ident,
                TokenKind::Punct(SlashEq),
                number(2.0),
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("/a/v / 2"),
            [
                regex("a", "v"),
                TokenKind::Punct(Slash),
                number(2.0),
                TokenKind::Eof
            ],
            "a regex is a value"
        );
        for prefix in [
            "1", "1n", "\"s\"", "`t`", "`${a}`", "null", "true", "this", "super",
        ] {
            let text = format!("{prefix} / 2");
            let found = kinds(&text);
            assert_eq!(
                found[found.len() - 3..],
                [TokenKind::Punct(Slash), number(2.0), TokenKind::Eof],
                "{text}"
            );
            assert!(diagnostics(&text).is_empty(), "{text}");
        }
        for prefix in [
            "return", "typeof", "case", "=", ",", "(", "{", "}", "`${", "",
        ] {
            let text = format!("{prefix} /a/v");
            let found = kinds(&text);
            assert!(found.contains(&regex("a", "v")), "{text}: {found:?}");
        }
        assert_eq!(Punct::Slash.as_str(), "/");
        assert_eq!(Punct::SlashEq.as_str(), "/=");
    }

    #[test]
    fn the_previous_token_decides_the_guess() {
        let regexes = [
            (TokenKind::Ident, false),
            (TokenKind::Null, false),
            (TokenKind::Bool(true), false),
            (number(1.0), false),
            (TokenKind::BigInt("1".into()), false),
            (TokenKind::String("s".into()), false),
            (regex("a", "v"), false),
            (TokenKind::Keyword(Keyword::This), false),
            (TokenKind::Keyword(Keyword::Super), false),
            (TokenKind::Keyword(Keyword::Return), true),
            (TokenKind::Keyword(Keyword::In), true),
            (TokenKind::Punct(Punct::LBrace), true),
            (TokenKind::Punct(Punct::RBrace), true),
            (TokenKind::Punct(Slash), true),
            (TokenKind::Error, true),
            (TokenKind::Eof, true),
        ];
        for (kind, expected) in regexes {
            assert_eq!(regex_allowed_after(&kind), expected, "{kind:?}");
        }
        for (part, expected) in [
            (TemplatePart::Whole, false),
            (TemplatePart::Tail, false),
            (TemplatePart::Head, true),
            (TemplatePart::Middle, true),
        ] {
            let kind = TokenKind::Template(Template {
                part,
                cooked: "".into(),
                raw: "".into(),
            });
            assert_eq!(regex_allowed_after(&kind), expected, "{part:?}");
        }
    }

    #[test]
    fn the_parser_hint_overrides_the_guess_after_a_closing_brace() {
        let f = file("} /a/v");
        let mut block = Lexer::new(&f);
        assert_eq!(block.next_token(true).kind, TokenKind::Punct(Punct::RBrace));
        assert_eq!(
            block.next_token(true).kind,
            regex("a", "v"),
            "after a block: a regex"
        );
        assert_eq!(block.next_token(true).kind, TokenKind::Eof);
        assert_eq!(
            block.next_token(true).kind,
            TokenKind::Eof,
            "Eof again and again"
        );
        assert!(block.finish().is_empty());

        let mut object = Lexer::new(&f);
        assert_eq!(
            object.next_token(true).kind,
            TokenKind::Punct(Punct::RBrace)
        );
        assert_eq!(
            object.next_token(false).kind,
            TokenKind::Punct(Slash),
            "after an object literal: division"
        );
        let token = object.next_token(false);
        assert_eq!(
            (token.kind, token.span.byte_start),
            (TokenKind::Ident, 3),
            "`a` follows"
        );
        assert_eq!(object.next_token(false).kind, TokenKind::Punct(Slash));
        assert_eq!(object.next_token(false).kind, TokenKind::Ident);
        assert_eq!(object.next_token(false).kind, TokenKind::Eof);
        assert!(object.finish().is_empty());

        let f = file("/a");
        let mut lexer = Lexer::new(&f);
        assert_eq!(lexer.next_token(true).kind, TokenKind::Error);
        assert_eq!(lexer.finish().len(), 1, "finish hands over the diagnostics");
    }

    #[test]
    fn a_regex_can_be_followed_directly_by_a_member_access() {
        let (stream, found) = lex(&file("/a/v.test(s)"));
        assert_eq!(stream.tokens[0].kind, regex("a", "v"));
        assert_eq!(
            (
                stream.tokens[1].span.byte_start,
                stream.tokens[2].kind.clone(),
                stream.tokens[2].span.byte_start
            ),
            (4, TokenKind::Ident, 5),
            "`.` then `test` follow"
        );
        assert_eq!(found.len(), 0, "{found:?}");
    }

    #[test]
    fn the_token_carries_the_named_groups_in_source_order() {
        let (stream, found) = lex(&file("/(?<y>a)(?<z>b)(c)|(?<y>d)/v"));
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            stream.tokens[0].kind,
            TokenKind::Regex(Regex {
                body: "(?<y>a)(?<z>b)(c)|(?<y>d)".into(),
                flags: "v".into(),
                groups: ["y", "z", "y"].map(Into::into).into(),
            })
        );
    }

    #[test]
    fn a_malformed_pattern_is_e000_at_the_offending_column_inside_the_literal() {
        for (text, column, message) in [
            ("/\\k<x>/v", 2, "`\\k<x>` names no group in the pattern"),
            (
                "/(?<y>a)\\k<x>/v",
                9,
                "`\\k<x>` names no group in the pattern",
            ),
            ("/a{2,1}/v", 3, "quantifier `{2,1}` is out of order"),
            (
                "/[a-]/v",
                4,
                "`-` must be escaped as `\\-` unless it forms a range",
            ),
            ("/x(/v", 3, "unterminated group: expected `)`"),
        ] {
            let (stream, found) = lex(&file(text));
            assert_eq!(
                (
                    stream.tokens[0].kind.clone(),
                    stream.tokens[0].span.byte_end
                ),
                (TokenKind::Error, text.len()),
                "{text}: the whole literal is one error token"
            );
            let found: Vec<&Diagnostic> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text}: {found:?}");
            assert_eq!(
                (
                    found[0].code,
                    found[0].span.start,
                    found[0].message.as_str()
                ),
                (Code::E000, Position::new(1, column), message),
                "{text}"
            );
        }
        let found = diagnostics("/\\k<x>/");
        assert_eq!(
            found.iter().map(|d| d.code).collect::<Vec<_>>(),
            [Code::E002, Code::E000],
            "a bad pattern and missing flags are both reported (ordered by position)"
        );
        assert_eq!(
            diagnostics("/\\k<x>/vx").len(),
            2,
            "a bad pattern and a bad flag are both reported"
        );
        assert_eq!(kinds("/\\k<x>/vx"), [TokenKind::Error, TokenKind::Eof]);
    }

    #[test]
    fn random_bodies_without_slashes_lex_back_to_themselves() {
        let gen = Text {
            alphabet: &['a', ' ', '[', ']', '*', '+', '(', ')', '=', 'é', '.', '\\'],
            max_len: 16,
        };
        check_with(&Config::new(300), &gen, &|body: &String| {
            // `//` and `/*` open comments; a trailing backslash escapes the closing slash.
            if body.is_empty() || body.starts_with('*') || body.ends_with('\\') {
                return Ok(());
            }
            let text = format!("/{body}/v");
            let f = file(&text);
            let (stream, found) = lex(&f);
            assert_eq!(stream.print(&f), text, "{text:?}");
            let mut in_class = false;
            let mut chars = body.chars();
            while let Some(ch) = chars.next() {
                match ch {
                    '\\' => {
                        chars.next();
                    }
                    '[' => in_class = true,
                    ']' => in_class = false,
                    _ => {}
                }
            }
            if in_class {
                assert_eq!(
                    stream.tokens[0].kind,
                    TokenKind::Error,
                    "{text:?}: unclosed class"
                );
            } else if crate::pattern::parse(body).is_ok() {
                assert_eq!(stream.tokens[0].kind, regex(body, "v"), "{text:?}");
                assert!(found.is_empty(), "{text:?}");
            } else {
                assert_eq!(
                    stream.tokens[0].kind,
                    TokenKind::Error,
                    "{text:?}: bad pattern"
                );
                assert_eq!(found.len(), 1, "{text:?}");
            }
            Ok(())
        })
        .expect("every body round-trips");
    }
}

#[cfg(test)]
mod comment_tests {
    use super::lex;
    use crate::token::{Token, TokenKind, TriviaKind};
    use sjs_diag::{Code, Position, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn first_trivia(text: &str) -> Vec<TriviaKind> {
        lex(&file(text)).0.tokens[0]
            .leading
            .iter()
            .map(|t| t.kind)
            .collect()
    }

    const BLOCK: TriviaKind = TriviaKind::BlockComment {
        has_line_terminator: false,
    };
    const DOC: TriviaKind = TriviaKind::DocComment {
        has_line_terminator: false,
    };

    #[test]
    fn the_three_comment_kinds_are_distinct_trivia() {
        assert_eq!(
            first_trivia("// a\nx"),
            [
                TriviaKind::LineComment,
                TriviaKind::LineTerminator(sjs_diag::LineTerminator::Lf)
            ]
        );
        assert_eq!(first_trivia("/* a */x"), [BLOCK]);
        assert_eq!(first_trivia("/** a */x"), [DOC]);
        assert_eq!(
            first_trivia("/**/x"),
            [BLOCK],
            "`/**/` is an empty block comment"
        );
        assert_eq!(
            first_trivia("/***/x"),
            [DOC],
            "`/***/` is a doc comment whose body is `*`"
        );
        assert_eq!(first_trivia("/****/x"), [DOC]);
        assert_eq!(first_trivia("/**a*/x"), [DOC]);
        assert_eq!(first_trivia("/* */x"), [BLOCK]);
        assert_eq!(
            first_trivia("/*/x"),
            [BLOCK],
            "`/*/` opens a comment that never closes, so it all belongs to Eof"
        );
        assert_eq!(
            first_trivia("/**\n*/x"),
            [TriviaKind::DocComment {
                has_line_terminator: true
            }]
        );
        let (stream, _) = lex(&file("/**\n * a\n */\nconst"));
        assert!(stream.tokens[0].has_newline_before);
        assert!(
            stream.tokens[0].leading[0].is_newline(),
            "a multi-line doc comment is a newline for ASI"
        );
        let text = "/** d */ /* b */ // l\nx /**/";
        let f = file(text);
        assert_eq!(lex(&f).0.print(&f), text);
    }

    #[test]
    fn an_unterminated_doc_comment_is_e000_at_its_start() {
        let multi_line = TriviaKind::DocComment {
            has_line_terminator: true,
        };
        for (text, kind) in [
            ("/** never", DOC),
            ("/**", BLOCK),
            ("/***", BLOCK),
            ("/****", DOC),
            ("x /** a\n b", multi_line),
        ] {
            let (stream, found) = lex(&file(text));
            let found: Vec<_> = found.iter().collect();
            assert_eq!(found.len(), 1, "{text:?}");
            assert_eq!(found[0].code, Code::E000, "{text:?}");
            assert_eq!(found[0].message, "unterminated block comment", "{text:?}");
            let at = text.find("/**").expect("has an opener");
            assert_eq!(found[0].span.byte_start, at, "{text:?}");
            let last = stream.eof().leading.last().expect("trailing trivia").kind;
            assert_eq!(last, kind, "{text:?}");
        }
    }

    #[test]
    fn html_like_comments_are_tokens_not_trivia() {
        for text in ["<!-- a\nb", "a <!-- b", "-->\nb", "a\n--> b", "x\n  --> y"] {
            let f = file(text);
            let (stream, _) = lex(&f);
            assert!(
                stream
                    .tokens
                    .iter()
                    .all(|t| t.leading.iter().all(|tr| matches!(
                        tr.kind,
                        TriviaKind::Whitespace | TriviaKind::LineTerminator(_)
                    ))),
                "{text:?}: no comment trivia"
            );
            let last = stream.tokens.len() - 2;
            assert_eq!(
                stream.tokens[last].kind,
                TokenKind::Ident,
                "{text:?}: the text after is still code"
            );
            assert_eq!(stream.print(&f), text);
        }
    }

    fn doc_of(text: &str, index: usize) -> Option<String> {
        let f = file(text);
        let (stream, _) = lex(&f);
        let token: &Token = &stream.tokens[index];
        token
            .doc_comment()
            .map(|t| f.slice(&t.span).unwrap_or("").to_owned())
    }

    #[test]
    fn a_doc_comment_attaches_to_the_token_on_the_next_line() {
        assert_eq!(doc_of("/** a */\nconst", 0), Some("/** a */".to_owned()));
        assert_eq!(
            doc_of("/** a */   \n   const", 0),
            Some("/** a */".to_owned()),
            "trailing and leading whitespace"
        );
        assert_eq!(
            doc_of("/**\n * a\n */\r\nexport", 0),
            Some("/**\n * a\n */".to_owned()),
            "multi-line, CRLF"
        );
        assert_eq!(
            doc_of("x\n\n/** a */\nconst", 1),
            Some("/** a */".to_owned()),
            "after other code"
        );
        assert_eq!(
            doc_of("/* b */\n/** a */\nconst", 0),
            Some("/** a */".to_owned()),
            "an earlier comment does not matter"
        );
        assert_eq!(
            doc_of("/** a */\n", 0),
            Some("/** a */".to_owned()),
            "at the end of the file it attaches to Eof; the parser has no declaration for it"
        );
    }

    #[test]
    fn a_blank_line_another_comment_or_the_same_line_breaks_the_attachment() {
        assert_eq!(doc_of("/** a */\n\nconst", 0), None, "a blank line");
        assert_eq!(
            doc_of("/** a */\n  \nconst", 0),
            None,
            "a blank line with whitespace"
        );
        assert_eq!(
            doc_of("/** a */\n// c\nconst", 0),
            None,
            "a line comment between"
        );
        assert_eq!(
            doc_of("/** a */\n/* c */\nconst", 0),
            None,
            "a block comment between"
        );
        assert_eq!(
            doc_of("/** a */\n/** c */\nconst", 0),
            Some("/** c */".to_owned()),
            "only the nearest doc comment"
        );
        assert_eq!(doc_of("/** a */ const", 0), None, "the same line");
        assert_eq!(
            doc_of("/** a */ /* c */\nconst", 0),
            None,
            "a comment after it on its line"
        );
        assert_eq!(doc_of("/* a */\nconst", 0), None, "a plain block comment");
        assert_eq!(
            doc_of("/**/\nconst", 0),
            None,
            "`/**/` is not a doc comment"
        );
        assert_eq!(doc_of("// a\nconst", 0), None, "a line comment");
        assert_eq!(doc_of("const", 0), None, "nothing before");
        assert_eq!(doc_of("\nconst", 0), None, "only a terminator before");
        assert_eq!(
            doc_of("x /** a */\nconst", 1),
            Some("/** a */".to_owned()),
            "a doc comment ending the previous line attaches even after code"
        );
    }

    #[test]
    fn comments_inside_strings_and_at_the_end_of_the_file() {
        let (stream, found) = lex(&file("\"/** */\" '/* a */' `// x`"));
        assert_eq!(stream.tokens[0].kind, TokenKind::String("/** */".into()));
        assert!(stream
            .tokens
            .iter()
            .all(|t| t.leading.iter().all(|tr| tr.kind == TriviaKind::Whitespace)));
        assert_eq!(found.len(), 1, "only the S001");
        let (stream, found) = lex(&file("const a = 1 // tail"));
        assert_eq!(found.len(), 0, "{found:?}");
        assert_eq!(
            stream
                .eof()
                .leading
                .iter()
                .map(|t| t.kind)
                .collect::<Vec<_>>(),
            [TriviaKind::Whitespace, TriviaKind::LineComment],
            "a comment at the end of the file without a newline belongs to Eof"
        );
        let (stream, _) = lex(&file("x /** d */"));
        assert_eq!(stream.eof().leading.last().map(|t| t.kind), Some(DOC));
        assert_eq!(stream.eof().span.start, Position::new(1, 11));
    }
}

#[cfg(test)]
mod punct_tests {
    use super::lex;
    use crate::token::{Number, Punct, TokenKind};
    use sjs_diag::{Code, Diagnostic, Position, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(&file(text))
            .0
            .tokens
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    fn diagnostics(text: &str) -> Vec<Diagnostic> {
        lex(&file(text)).1.iter().cloned().collect()
    }

    fn punct(p: Punct) -> TokenKind {
        TokenKind::Punct(p)
    }

    fn number(value: f64) -> TokenKind {
        TokenKind::Number(Number {
            value,
            exceeds_safe_integer: false,
        })
    }

    #[test]
    fn every_punctuator_lexes_alone_and_between_identifiers() {
        assert_eq!(Punct::ALL.len(), 57);
        for (p, spelling) in Punct::ALL {
            assert_eq!(p.as_str(), *spelling);
            assert_eq!(Punct::longest_match(spelling), Some(*p), "{spelling}");
            let expected_alone = if matches!(p, Punct::Slash | Punct::SlashEq) {
                TokenKind::Error
            } else {
                punct(*p)
            };
            let found = kinds(spelling);
            assert_eq!(
                found[0], expected_alone,
                "{spelling} alone (a leading `/` opens a regex)"
            );
            let text = format!("a {spelling} b");
            assert_eq!(
                kinds(&text),
                [
                    TokenKind::Ident,
                    punct(*p),
                    TokenKind::Ident,
                    TokenKind::Eof
                ],
                "{text}"
            );
            assert!(
                diagnostics(&text).is_empty(),
                "{text}: punctuators are never diagnostics"
            );
        }
        assert_eq!(Punct::longest_match("x"), None);
        assert_eq!(Punct::longest_match(""), None);
        assert_eq!(kinds("a @ b")[1], TokenKind::Error);
    }

    #[test]
    fn the_longest_punctuator_wins() {
        let cases: [(&str, &[Punct]); 14] = [
            (">>>=", &[Punct::UShrEq]),
            (">>>", &[Punct::UShr]),
            (">>=", &[Punct::ShrEq]),
            (">>", &[Punct::Shr]),
            (">=", &[Punct::GtEq]),
            ("**=", &[Punct::StarStarEq]),
            ("??=", &[Punct::QuestionQuestionEq]),
            ("...", &[Punct::Ellipsis]),
            ("=>", &[Punct::Arrow]),
            ("===", &[Punct::EqEqEq]),
            ("!==", &[Punct::BangEqEq]),
            ("....", &[Punct::Ellipsis, Punct::Dot]),
            ("=>=", &[Punct::Arrow, Punct::Eq]),
            (">>>>", &[Punct::UShr, Punct::Gt]),
        ];
        for (text, expected) in cases {
            let text = format!("a {text} b");
            let found = kinds(&text);
            let mut expected_kinds = vec![TokenKind::Ident];
            expected_kinds.extend(expected.iter().map(|p| punct(*p)));
            expected_kinds.extend([TokenKind::Ident, TokenKind::Eof]);
            assert_eq!(found, expected_kinds, "{text}");
        }
        assert_eq!(
            kinds("a ?. b"),
            [
                TokenKind::Ident,
                punct(Punct::QuestionDot),
                TokenKind::Ident,
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("a?.5:1"),
            [
                TokenKind::Ident,
                punct(Punct::Question),
                number(0.5),
                punct(Punct::Colon),
                number(1.0),
                TokenKind::Eof
            ],
            "`?.` before a digit is `?` then `.5`: a ternary"
        );
        assert_eq!(
            kinds("a?.b"),
            [
                TokenKind::Ident,
                punct(Punct::QuestionDot),
                TokenKind::Ident,
                TokenKind::Eof
            ]
        );
        assert_eq!(
            kinds("a?.[0]")[1..3],
            [punct(Punct::QuestionDot), punct(Punct::LBracket)]
        );
    }

    #[test]
    fn removed_punctuators_lex_without_a_diagnostic_for_the_parser_to_report() {
        for (text, p) in [
            ("==", Punct::EqEq),
            ("!=", Punct::BangEq),
            ("||=", Punct::PipePipeEq),
            ("&&=", Punct::AmpAmpEq),
            ("++", Punct::PlusPlus),
            ("--", Punct::MinusMinus),
        ] {
            assert!(p.is_removed(), "{text}");
            let text = format!("a {text} b");
            assert_eq!(kinds(&text)[1], punct(p), "{text}");
            assert!(diagnostics(&text).is_empty(), "{text}");
        }
        let removed = Punct::ALL.iter().filter(|(p, _)| p.is_removed()).count();
        assert_eq!(removed, 6);
        assert!(!Punct::EqEqEq.is_removed());
        assert!(!Punct::Plus.is_removed());
        assert_eq!(
            kinds("<!-- a -->"),
            [
                punct(Punct::Lt),
                punct(Punct::Bang),
                punct(Punct::MinusMinus),
                TokenKind::Ident,
                punct(Punct::MinusMinus),
                punct(Punct::Gt),
                TokenKind::Eof
            ],
            "HTML-like comments are operators in a module"
        );
    }

    #[test]
    fn a_private_name_is_hash_directly_followed_by_an_identifier() {
        assert_eq!(
            kinds("this.#a"),
            [
                TokenKind::Keyword(crate::token::Keyword::This),
                punct(Punct::Dot),
                TokenKind::PrivateName("a".into()),
                TokenKind::Eof
            ]
        );
        assert_eq!(kinds("#_x1")[0], TokenKind::PrivateName("_x1".into()));
        assert_eq!(
            kinds("#class")[0],
            TokenKind::PrivateName("class".into()),
            "any identifier name"
        );
        let (stream, _) = lex(&file("#ab cd"));
        assert_eq!(
            (
                stream.tokens[0].span.byte_start,
                stream.tokens[0].span.byte_end
            ),
            (0, 3)
        );
        for (text, column) in [("# a", 1), ("#", 1), ("#1", 1), ("#$a", 1), ("x #", 3)] {
            let found = diagnostics(text);
            assert_eq!(found[0].code, Code::E000, "{text}");
            assert_eq!(found[0].span.start, Position::new(1, column), "{text}");
            assert_eq!(
                found[0].message,
                "`#` begins a private name and must be followed directly by its identifier"
            );
        }
        assert_eq!(
            kinds("# a"),
            [TokenKind::Error, TokenKind::Ident, TokenKind::Eof]
        );
        assert_eq!(
            kinds("#!x")[0],
            TokenKind::Error,
            "a hashbang is still a hashbang"
        );
    }

    #[test]
    fn a_decorator_is_e000_naming_the_replacement() {
        let found = diagnostics("@dec\nclass A {}");
        assert_eq!(found.len(), 1);
        assert_eq!(
            (
                found[0].code,
                found[0].span.start,
                found[0].span.end,
                found[0].message.as_str()
            ),
            (
                Code::E000,
                Position::new(1, 1),
                Position::new(1, 2),
                "decorators are removed; use explicit wrapping functions (SPEC.md §12)"
            )
        );
        assert_eq!(kinds("@dec")[..2], [TokenKind::Error, TokenKind::Ident]);
    }

    #[test]
    fn a_slash_after_a_closing_paren_or_bracket_is_division() {
        assert_eq!(kinds("(a) / 2")[3], punct(Punct::Slash), "after `)`");
        assert_eq!(kinds("a[0] / 2")[4], punct(Punct::Slash), "after `]`");
        assert_eq!(
            kinds("a.#b / 2")[3],
            punct(Punct::Slash),
            "after a private name"
        );
        let regex = TokenKind::Regex(crate::token::Regex {
            body: "a".into(),
            flags: "v".into(),
            groups: Box::default(),
        });
        assert_eq!(kinds("x = /a/v")[2], regex, "after `=`");
        assert_eq!(kinds("f(/a/v)")[2], regex, "after `(`");
        assert_eq!(kinds("[/a/v]")[1], regex, "after `[`");
        assert_eq!(kinds("a, /a/v")[2], regex, "after `,`");
    }

    #[test]
    fn a_whole_program_lexes_without_error_tokens() {
        let text = "export const f = (a, b, ...rest) => {\n  const c = a ** b >>> 1\n  const d = c?.x ?? rest[0]\n  let e = 1\n  e **= 2\n  e >>>= 1\n  e ??= 3\n  return { c, d, e, ...rest }\n}\n";
        let f = file(text);
        let (stream, found) = lex(&f);
        assert!(found.is_empty(), "{found:?}");
        assert!(stream.tokens.iter().all(|t| t.kind != TokenKind::Error));
        assert_eq!(stream.print(&f), text);
    }
}

#[cfg(test)]
mod limit_tests {
    use super::lex;
    use crate::number::MAX_RADIX_DIGITS;
    use crate::token::{Number, TokenKind};
    use sjs_diag::{Code, SourceFile};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn bigint_digits(kind: &TokenKind) -> Result<String, String> {
        match kind {
            TokenKind::BigInt(digits) => Ok(digits.to_string()),
            other => Err(format!("{other:?}")),
        }
    }

    #[test]
    fn radix_literals_past_the_digit_cap_are_infinite_or_rejected_without_conversion() {
        let digits = "f".repeat(MAX_RADIX_DIGITS);
        let (stream, found) = lex(&file(&format!("0x{digits}")));
        assert!(found.is_empty());
        assert_eq!(
            stream.tokens[0].kind,
            TokenKind::Number(Number {
                value: f64::INFINITY,
                exceeds_safe_integer: true
            }),
            "at the cap the exact conversion still runs; the value is beyond any double"
        );
        let over = "f".repeat(MAX_RADIX_DIGITS + 1);
        let (stream, found) = lex(&file(&format!("0x{over}")));
        assert!(found.is_empty());
        assert_eq!(
            stream.tokens[0].kind,
            TokenKind::Number(Number {
                value: f64::INFINITY,
                exceeds_safe_integer: true
            })
        );
        let at_cap = format!("0b1{}n", "0".repeat(MAX_RADIX_DIGITS - 1));
        let (stream, found) = lex(&file(&at_cap));
        assert!(found.is_empty(), "exactly the cap converts");
        let digits = bigint_digits(&stream.tokens[0].kind).expect("a bigint");
        assert_eq!(digits.len(), 1233, "2^4095 has 1233 decimal digits");
        assert!(digits.starts_with("5221"));
        assert_eq!(bigint_digits(&TokenKind::Eof), Err("Eof".to_owned()));
        let zeros = "0".repeat(10_000);
        let (stream, found) = lex(&file(&format!("0x{zeros}1n")));
        assert!(found.is_empty(), "leading zeros do not count");
        assert_eq!(stream.tokens[0].kind, TokenKind::BigInt("1".into()));
        let ones = "1".repeat(MAX_RADIX_DIGITS + 1);
        let (stream, found) = lex(&file(&format!("0b{ones}n")));
        assert_eq!(stream.tokens[0].kind, TokenKind::Error);
        let found: Vec<_> = found.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (
                found[0].code,
                found[0].span.byte_start,
                found[0].message.as_str()
            ),
            (
                Code::E000,
                0,
                "a bigint literal in a radix may have at most 4096 significant digits"
            )
        );
        let text = format!("0x{over}n");
        let (stream, _) = lex(&file(&text));
        assert_eq!(
            stream.tokens[0].span.byte_end,
            text.len(),
            "one error token"
        );
    }
}
