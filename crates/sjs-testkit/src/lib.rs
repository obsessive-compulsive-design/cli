//! std-only test tooling shared by every crate's tests (STORIES.md §0.2 item 11).
//!
//! US-008 adds `prng`, `prop`, `snapshot`, `fuzz` and `bench`. US-002 ships only the
//! [`process`] driver, which the workspace dependency-graph test needs to run `cargo tree`.

pub mod process;
