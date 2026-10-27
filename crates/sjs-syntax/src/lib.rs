//! Lexer, parser, lossless syntax tree and exact printer (SPEC.md §2, §11.1).
//!
//! US-010 ships the token model and the lexer frame: every byte of a file ends up either
//! in a trivia run (whitespace, line terminators, comments) or in a token, and printing
//! the stream reproduces the file byte for byte. US-011 adds words (identifiers,
//! keywords, the removed words, `null`, booleans, and the rejections of `$`, Unicode
//! escapes, non-ASCII letters and `#!`), US-012 numeric literals with their values and
//! US-013 string literals cooked to their value (`'…'` is S001 with a fix) and US-014
//! template literals as head, middle, tail and whole pieces with a brace-depth stack so
//! substitutions nest. US-015 adds regex literals with their flags checked (E002 for a
//! set lacking `v`) behind a regex-allowed hint the parser passes to [`Lexer::next_token`]
//! (`lex` guesses it from the previous token). US-016 parses every regex body under
//! `v`-mode syntax ([`pattern`]), so a malformed pattern is E000 and the named groups
//! ride on the token. US-017 distinguishes `/** … */` doc comments from other block
//! comments and attaches them to the token on the next line (`Token::doc_comment`).
//! US-018 adds the remaining token kinds;
//! until then every other character is an `Error` token, so the lexer is not yet part of
//! the pipeline.

pub mod lexer;
mod number;
pub mod pattern;
mod regex;
mod string;
pub mod token;
mod unicode_property;

pub use lexer::{lex, regex_allowed_after, Lexer};
pub use token::{
    Keyword, Number, Punct, Regex, Template, TemplatePart, Token, TokenKind, TokenStream, Trivia,
    TriviaKind,
};
