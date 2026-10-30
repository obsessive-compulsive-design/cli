//! Canonical layout and S-code diffs (SPEC.md §9).
//!
//! [`doc`] is the document IR and its layout (US-040): every formatting rule maps a
//! syntax tree to [`Doc`]s, and [`print`] alone knows the width and the indent. [`expr`]
//! maps expressions (US-041): punctuation, strings, templates and the parentheses that
//! stay, the rest being S005. [`stmt`] maps statements, declarations and blank lines
//! and prints a whole file (US-042); [`whitespace`] reports what the text gets wrong
//! about indentation, line endings, trailing whitespace and the final newline (S007,
//! S008); [`lists`] reports a trailing comma at odds with a list's layout (S004) and an
//! arrow whose block body is one `return` (S011, US-043); [`chains`] lays out member
//! chains and reports one of three or more calls kept on a line (S010, US-044), the
//! breaking of binary expressions and ternaries living in [`expr`].
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

pub mod chains;
pub mod doc;
pub mod expr;
pub mod lists;
pub mod stmt;
pub mod whitespace;

pub use doc::{layout, print, Doc, INDENT, WIDTH};
pub use expr::Formatter;
