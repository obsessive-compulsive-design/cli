//! Lexer, parser, lossless syntax tree and exact printer (SPEC.md §2, §11.1).
//!
//! US-010 ships the token model and the lexer frame: every byte of a file ends up either
//! in a trivia run (whitespace, line terminators, comments) or in a token, and printing
//! the stream reproduces the file byte for byte. US-011 adds words (identifiers,
//! keywords, the removed words, `null`, booleans, and the rejections of `$`, Unicode
//! escapes, non-ASCII letters and `#!`). US-012 to US-018 add the remaining token kinds;
//! until then every other character is an `Error` token, so the lexer is not yet part of
//! the pipeline.

pub mod lexer;
pub mod token;

pub use lexer::lex;
pub use token::{Keyword, Token, TokenKind, TokenStream, Trivia, TriviaKind};
