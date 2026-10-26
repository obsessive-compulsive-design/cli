//! std-only test tooling shared by every crate's tests (STORIES.md §0.2 item 11).
//!
//! US-008 adds `prng`, `prop`, `snapshot`, `fuzz` and `bench`. US-002 ships the
//! [`process`] driver, which the workspace dependency-graph test needs to run `cargo tree`;
//! US-006 adds [`fs::TempDir`] for tests that touch the file system.

pub mod fs;
pub mod process;
