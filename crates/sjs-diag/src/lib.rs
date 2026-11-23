//! Diagnostic model shared by every pass (SPEC.md §8, §13.1).
//!
//! US-003 ships the [`Code`] table, [`Severity`], [`Position`], [`Span`], [`Diagnostic`]
//! and the sorting, deduplicating [`Diagnostics`] collector. US-004 adds [`SourceFile`],
//! the line index every position comes from. US-135 and US-136 add text rendering and
//! the JSON reports. US-100 adds [`Json`], written and read in-house, and
//! [`TypesDocument`], the form of `types.json`.

mod code;
mod diagnostic;
mod json;
mod source;
mod types_json;

pub use code::{Code, ParseCodeError, Severity};
pub use diagnostic::{Diagnostic, Diagnostics, FixError, Position, Related, Span};
pub use json::Json;
pub use source::{Line, LineTerminator, SourceFile};
pub use types_json::{ExportedType, ModuleExports, TypesDocument, TYPES_VERSION};
