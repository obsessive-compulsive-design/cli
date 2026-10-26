//! Diagnostic model shared by every pass (SPEC.md §8, §13.1).
//!
//! US-003 ships the [`Code`] table, [`Severity`], [`Position`], [`Span`], [`Diagnostic`]
//! and the sorting, deduplicating [`Diagnostics`] collector. US-004 adds [`SourceFile`],
//! the line index every position comes from. US-135 and US-136 add text rendering and
//! the JSON reports.

mod code;
mod diagnostic;
mod source;

pub use code::{Code, ParseCodeError, Severity};
pub use diagnostic::{Diagnostic, Diagnostics, FixError, Position, Related, Span};
pub use source::{Line, LineTerminator, SourceFile};
