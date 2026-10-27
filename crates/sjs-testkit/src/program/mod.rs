//! Random Strict JS programs over the §2 grammar (STORIES.md US-009): a small syntax
//! tree, a renderer with exact precedence and layout, a generator that records which
//! productions it used, and statement-level shrinking.
//!
//! Programs are grammatically valid Strict JS (no removed production, no braceless body,
//! no ASI hazard, `yield`/`await`/`super`/`break` only where the grammar allows them) but
//! not necessarily well typed: they exist to prove parser round-trips and formatter
//! idempotence, not inference.

mod generate;
mod render;
#[cfg(test)]
mod tests;
mod tree;

pub use generate::{Options, Production, ProgramGen, RESERVED};
pub use tree::{
    Decl, Else, Expr, ForUpdate, Import, Item, Literal, Member, MemberKey, Param, Pattern, Program,
    Property, Stmt, TemplatePart,
};
