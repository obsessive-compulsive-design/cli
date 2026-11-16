//! Module graph, scopes and the ambient table (SPEC.md §4, §5).
//!
//! US-060 ships the module graph: [`ModuleGraph::load`] reads the transitive closure of
//! the root's imports through a [`Host`], parses each file once and keys it by its
//! canonical path, so the program is a closed world for every whole-program pass.
//! US-061 adds [`ModuleGraph::import_cycles`]: E203 for every elementary import cycle.
//! US-062 adds [`ModuleGraph::unreachable`]: E207 for every `.js` file beneath `./src`
//! that is no module of the graph. US-063 adds [`bind`]: the scopes, declarations and
//! references of one file, with E000 for a name declared twice and E135 for a use in
//! the temporal dead zone.

mod cycles;
mod graph;
mod host;
mod json;
mod scopes;
mod unreachable;

pub use graph::{Content, Import, LoadError, Module, ModuleGraph};
pub use host::{Disk, Host};
pub use json::is_json;
pub use scopes::{bind, Bindings, DeclKind, Declaration, Reference, Scope, ScopeKind};
