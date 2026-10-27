//! Lexer, parser, lossless syntax tree and exact printer (SPEC.md §2, §11.1).
//!
//! US-010 ships the token model and the lexer frame: every byte of a file ends up either
//! in a trivia run (whitespace, line terminators, comments) or in a token, and printing
//! the stream reproduces the file byte for byte. US-011 adds words (identifiers,
//! keywords, the removed words, `null`, booleans, and the rejections of `$`, Unicode
//! escapes, non-ASCII letters and `#!`), US-012 numeric literals with their values and
//! US-013 string literals cooked to their value (`'…'` is S001 with a fix).
//! US-014 to US-018 add the remaining token kinds;
//! until then every other character is an `Error` token, so the lexer is not yet part of
//! the pipeline.

pub mod lexer;
mod number;
mod string;
pub mod token;

pub use lexer::lex;
pub use token::{Keyword, Number, Token, TokenKind, TokenStream, Trivia, TriviaKind};
