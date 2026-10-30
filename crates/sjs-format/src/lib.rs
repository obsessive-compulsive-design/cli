//! Canonical layout and S-code diffs (SPEC.md §9).
//!
//! [`doc`] is the document IR and its layout (US-040): every formatting rule maps a
//! syntax tree to [`Doc`]s, and [`print`] alone knows the width and the indent. [`expr`]
//! maps expressions (US-041): punctuation, strings, templates and the parentheses that
//! stay, the rest being S005. [`stmt`] maps statements, declarations and blank lines
//! and prints a whole file (US-042); [`whitespace`] reports what the text gets wrong
//! about indentation, line endings, trailing whitespace and the final newline (S007,
//! S008); [`lists`] reports a trailing comma at odds with a list's layout (S004) and an
//! arrow whose block body is one `return` (S011, US-043).
//!
//! ```
//! use sjs_format::{print, Doc};
//! let call = Doc::group(vec![
//!     Doc::text("f("),
//!     Doc::indent(vec![Doc::SoftLine, Doc::text("a"), Doc::text(","), Doc::Line, Doc::text("b")]),
//!     Doc::SoftLine,
//!     Doc::text(")"),
//! ]);
//! assert_eq!(print(&[call]), "f(a, b)");
//! ```

pub mod doc;
pub mod expr;
pub mod lists;
pub mod stmt;
pub mod whitespace;

pub use doc::{layout, print, Doc, INDENT, WIDTH};
pub use expr::Formatter;
