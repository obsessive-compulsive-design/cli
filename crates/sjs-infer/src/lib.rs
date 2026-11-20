//! Whole-program type inference (SPEC.md §3). US-083 begins it with the expressions
//! whose type is plain from how they are written: literals, templates, `??`, the
//! results of `JSON.parse` and of a JSON import. [`Typer::type_of`] gives the type of
//! an expression when it can be told yet; [`check_expressions`] reports `null` written
//! where a program may not write it (E100) and a template filled with a value that is
//! no text (E125). US-084 adds [`infer_bindings`]: the type of each `const` and `let`,
//! through destructuring too. US-085 completes object literals (spreads, computed keys,
//! E000, E134 and E138 for what they may not hold) and adds [`infer_program`]: the
//! modules in order, so that an import has the type of what it is bound to and a
//! namespace the shape of its module's exports. US-086 types property reads and judges
//! them and property writes (E137, E121, E129, E138). US-087 follows what the program mutates
//! and what fills an empty collection: a `const` array literal nothing mutates is a
//! tuple, an empty `[]`, `new Map()` or `new Set()` is typed by what is put into it and
//! is E110 when nothing is, and assigning `length` is E133. US-088 types the operators
//! of SPEC.md §4.1 and judges their operands (E138, E104, E105, E120).

mod bindings;
mod expressions;
mod members;
mod mutation;
mod operators;
mod program;

pub use bindings::infer_bindings;
pub use expressions::{check_expressions, Facts, Typer};
pub use program::{infer_program, ModuleTypes};
