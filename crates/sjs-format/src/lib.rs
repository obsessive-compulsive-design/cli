//! Canonical layout and S-code diffs (SPEC.md §9).
//!
//! [`doc`] is the document IR and its layout (US-040): every formatting rule maps a
//! syntax tree to [`Doc`]s, and [`print`] alone knows the width and the indent.
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

pub use doc::{layout, print, Doc, INDENT, WIDTH};
