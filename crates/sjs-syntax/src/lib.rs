//! Lexer, parser, lossless syntax tree and exact printer (SPEC.md §2, §11.1).
//!
//! US-010 ships the token model and the lexer frame: every byte of a file ends up either
//! in a trivia run (whitespace, line terminators, comments) or in a token, and printing
//! the stream reproduces the file byte for byte. Only trivia is recognised so far; every
//! other character is an `Error` token, and US-011 to US-018 teach the lexer one token
//! kind each. The lexer is therefore not yet part of the pipeline.

pub mod lexer;
pub mod token;

pub use lexer::lex;
pub use token::{Token, TokenKind, TokenStream, Trivia, TriviaKind};
