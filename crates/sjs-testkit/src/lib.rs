//! std-only test tooling shared by every crate's tests (STORIES.md §0.2 item 11).
//!
//! - [`prng`]: a documented deterministic generator.
//! - [`prop`]: property testing with seeds and shrinking (`SJS_PROP_CASES`, `SJS_PROP_SEED`).
//! - [`snapshot`]: byte-exact snapshot files (`SJS_UPDATE_SNAPSHOTS=1` to write).
//! - [`fuzz`]: a time-budgeted fuzz loop with saved regressions (`SJS_FUZZ_SECONDS`).
//! - [`bench`]: median timing for `harness = false` benches.
//! - [`process`]: run a program and capture what it did.
//! - [`fs`]: a throwaway directory.
//! - [`conformance`]: the fixture format of STORIES.md §0.3.
//! - [`program`]: random Strict JS programs over the §2 grammar, with shrinking.

pub mod bench;
pub mod conformance;
pub mod fs;
pub mod fuzz;
pub mod prng;
pub mod process;
pub mod program;
pub mod prop;
pub mod snapshot;
