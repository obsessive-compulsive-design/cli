//! Tokens and trivia: the lossless representation of a file (SPEC.md §11.1).
//!
//! Trivia (whitespace, line terminators, comments) is attached to the token that follows
//! it; the trailing run of a file is attached to the `Eof` token. Printing every token's
//! trivia then text, in order, reproduces the source exactly.

use sjs_diag::{LineTerminator, SourceFile, Span};

/// A run of source text that is not a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriviaKind {
    /// Spaces, tabs and the other ES2025 `WhiteSpace` characters.
    Whitespace,
    /// One line terminator (`\r\n` counts as one).
    LineTerminator(LineTerminator),
    /// `// …` up to, not including, the line terminator.
    LineComment,
    /// `/* … */`, which may span lines.
    BlockComment {
        /// True when the comment contains a line terminator, which makes it count as a
        /// newline for ASI purposes (ES2025 §12.4).
        has_line_terminator: bool,
    },
}

/// One trivia run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    /// What it is.
    pub kind: TriviaKind,
    /// Where it is.
    pub span: Span,
}

impl Trivia {
    /// True when this trivia ends a line as far as ASI is concerned.
    #[must_use]
    pub fn is_newline(&self) -> bool {
        matches!(
            self.kind,
            TriviaKind::LineTerminator(_)
                | TriviaKind::BlockComment {
                    has_line_terminator: true
                }
        )
    }
}

/// What a token is. US-010 knows only the two ends of the scale; US-011 to US-018 add
/// identifiers, literals, templates, regexes and punctuators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A character the lexer does not accept; reported as E000, lexing continues.
    Error,
    /// The end of the file; carries the trailing trivia.
    Eof,
}

/// A token with the trivia that precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What it is.
    pub kind: TokenKind,
    /// Its own text, trivia excluded (empty for `Eof`).
    pub span: Span,
    /// The trivia between the previous token and this one.
    pub leading: Vec<Trivia>,
    /// True when a line terminator, or a block comment containing one, lies between the
    /// previous token and this one (SPEC.md §2 E001 depends on it).
    pub has_newline_before: bool,
}

/// Every token of a file, `Eof` last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenStream {
    /// The tokens in source order.
    pub tokens: Vec<Token>,
}

impl TokenStream {
    /// The source text, reassembled from trivia and tokens.
    #[must_use]
    pub fn print(&self, file: &SourceFile) -> String {
        let mut out = String::with_capacity(file.len());
        for token in &self.tokens {
            for trivia in &token.leading {
                out.push_str(file.slice(&trivia.span).unwrap_or(""));
            }
            out.push_str(file.slice(&token.span).unwrap_or(""));
        }
        out
    }

    /// The `Eof` token, which every stream ends with.
    #[must_use]
    pub fn eof(&self) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        &self.tokens[last]
    }
}
