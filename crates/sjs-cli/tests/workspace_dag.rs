//! STORIES.md US-002: no crate depends on a crate later in the pipeline (§0.5, §11).
//!
//! Reads `cargo tree` (text, no JSON) and checks every path-dependency edge against the
//! allowed table below. The table is the pipeline order; a story that needs a new edge
//! adds it here, in its own PR, where a reviewer sees it.

use sjs_testkit::process::run;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Cargo sets `CARGO` while compiling, so the path is baked in at build time.
const CARGO: &str = env!("CARGO");

/// Each crate and the crates it may depend on (normal edges). `sjs-testkit` may be a
/// dev-dependency of anyone and depends on nothing.
const ALLOWED: &[(&str, &[&str])] = &[
    ("sjs-testkit", &[]),
    ("sjs-diag", &[]),
    ("sjs-syntax", &["sjs-diag"]),
    ("sjs-format", &["sjs-syntax", "sjs-diag"]),
    ("sjs-bind", &["sjs-syntax", "sjs-diag"]),
    ("sjs-types", &["sjs-diag"]),
    (
        "sjs-infer",
        &["sjs-bind", "sjs-types", "sjs-syntax", "sjs-diag"],
    ),
    (
        "sjs-check",
        &[
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-lint",
        &[
            "sjs-check",
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-format",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-test",
        &[
            "sjs-check",
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-map",
        &[
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-docs",
        &[
            "sjs-map",
            "sjs-lint",
            "sjs-check",
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-format",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-lsp",
        &[
            "sjs-docs",
            "sjs-map",
            "sjs-test",
            "sjs-lint",
            "sjs-check",
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-format",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
    (
        "sjs-cli",
        &[
            "sjs-lsp",
            "sjs-docs",
            "sjs-map",
            "sjs-test",
            "sjs-lint",
            "sjs-check",
            "sjs-infer",
            "sjs-bind",
            "sjs-types",
            "sjs-format",
            "sjs-syntax",
            "sjs-diag",
        ],
    ),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Direct (parent, child) edges of the given kinds, from `cargo tree --prefix depth`.
fn edges(kinds: &str) -> BTreeSet<(String, String)> {
    let out = run(
        CARGO,
        &[
            "tree",
            "--workspace",
            "--offline",
            "-e",
            kinds,
            "--prefix",
            "depth",
            "--no-dedupe",
        ],
        Some(&workspace_root()),
    )
    .expect("cargo is runnable under cargo test");
    assert!(out.success(), "cargo tree failed: {}", out.stderr);

    let mut stack: Vec<String> = Vec::new();
    let mut edges = BTreeSet::new();
    for line in out.stdout.lines().filter(|line| !line.is_empty()) {
        let depth = line.chars().take_while(char::is_ascii_digit).count();
        let name = line[depth..]
            .split_whitespace()
            .next()
            .expect("a crate name follows the depth")
            .to_owned();
        let depth: usize = line[..depth].parse().expect("depth is a number");
        stack.truncate(depth);
        if let Some(parent) = stack.last() {
            edges.insert((parent.clone(), name.clone()));
        }
        stack.push(name);
    }
    edges
}

fn allowed(parent: &str) -> &'static [&'static str] {
    ALLOWED
        .iter()
        .find(|(name, _)| *name == parent)
        .map_or_else(
            || panic!("{parent} is not a crate listed in STORIES.md §0.5"),
            |(_, deps)| *deps,
        )
}

#[test]
fn every_crate_of_the_layout_exists_in_the_workspace() {
    let out = run(
        CARGO,
        &[
            "tree",
            "--workspace",
            "--offline",
            "--depth",
            "0",
            "--prefix",
            "none",
        ],
        Some(&workspace_root()),
    )
    .expect("cargo is runnable under cargo test");
    assert!(out.success(), "{}", out.stderr);
    let members: BTreeSet<&str> = out
        .stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    let expected: BTreeSet<&str> = ALLOWED.iter().map(|(name, _)| *name).collect();
    assert_eq!(members, expected);
}

#[test]
fn normal_dependencies_follow_the_pipeline_order() {
    let normal = edges("normal");
    for (parent, child) in &normal {
        assert!(
            allowed(parent).contains(&child.as_str()),
            "{parent} -> {child} is not an allowed pipeline edge (STORIES.md §0.5)"
        );
    }
    assert!(
        !normal.iter().any(|(_, child)| child == "sjs-testkit"),
        "sjs-testkit is a dev-dependency only"
    );
}

#[test]
fn dev_dependencies_only_add_edges_to_the_testkit() {
    let normal = edges("normal");
    let with_dev = edges("normal,dev");
    for edge in with_dev.difference(&normal) {
        assert_eq!(
            edge.1, "sjs-testkit",
            "dev edge {} -> {} is not to sjs-testkit",
            edge.0, edge.1
        );
        assert_ne!(edge.0, "sjs-testkit", "sjs-testkit depends on nothing");
    }
}
