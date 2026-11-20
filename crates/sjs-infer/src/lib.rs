//! Whole-program type inference (SPEC.md §3). US-083 begins it with the expressions
//! whose type is plain from how they are written: literals, templates, `??`, the
//! results of `JSON.parse` and of a JSON import. [`Typer::type_of`] gives the type of
//! an expression when it can be told yet; [`check_expressions`] reports `null` written
//! where a program may not write it (E100) and a template filled with a value that is
//! no text (E125).

mod expressions;

pub use expressions::{check_expressions, Facts, Typer};
