//! Import cycles (SPEC.md §5.1): every elementary cycle of the module graph is E203.

use crate::graph::{Module, ModuleGraph};
use sjs_diag::{Code, Diagnostic, Diagnostics};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

impl ModuleGraph {
    /// E203 for each elementary import cycle, once: on the import that closes it, the
    /// one that names the module whose path sorts first, with the cycle listed from
    /// that module. `import(…)` and `export … from` are imports like any other.
    #[must_use]
    pub fn import_cycles(&self) -> Diagnostics {
        let modules: Vec<&Module> = self.modules().collect();
        let index: BTreeMap<&Path, usize> = modules
            .iter()
            .enumerate()
            .map(|(at, module)| (module.path.as_path(), at))
            .collect();
        let target = |import: &crate::Import| {
            let path = import.target.as_deref()?;
            index.get(path).copied()
        };
        let edges: Vec<Vec<usize>> = modules
            .iter()
            .map(|module| {
                let targets: BTreeSet<usize> = module.imports.iter().filter_map(target).collect();
                targets.into_iter().collect()
            })
            .collect();
        let mut diagnostics = Diagnostics::new();
        for cycle in elementary_cycles(&edges) {
            let (first, last) = (cycle[0], cycle[cycle.len() - 1]);
            let names: Vec<&str> = cycle
                .iter()
                .chain([&first])
                .map(|&at| modules[at].name.as_str())
                .collect();
            let closing = modules[last]
                .imports
                .iter()
                .find(|import| target(import) == Some(first));
            diagnostics.extend(closing.map(|import| {
                Diagnostic::new(
                    Code::E203,
                    modules[last].name.clone(),
                    import.span,
                    format!(
                        "import cycle: {}; modules import one another in one direction only (SPEC.md §5.1)",
                        names.join(" → ")
                    ),
                )
            }));
        }
        diagnostics
    }
}

/// Every elementary cycle of the graph whose node `v` has the edges `edges[v]`, each
/// once, as its nodes in order from the least (Johnson's algorithm, without recursion:
/// time linear in the size of the graph for each cycle found).
fn elementary_cycles(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    loop {
        // Among the nodes from `from` on, the least that is on a cycle begins the
        // cycles it is the least node of; the nodes before it are done with.
        let component = components(edges, from);
        let inside = |v: usize, w: usize| w >= from && component[w] == component[v];
        let start = (from..edges.len()).find(|&v| edges[v].iter().any(|&w| inside(v, w)));
        let Some(start) = start else {
            return found;
        };
        cycles_from(edges, start, &|w| inside(start, w), &mut found);
        from = start + 1;
    }
}

/// The strongly connected component of each node from `from` on, in the graph without
/// the nodes before it: nodes of one component share a number (Tarjan's algorithm).
fn components(edges: &[Vec<usize>], from: usize) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let mut order = vec![NONE; edges.len()];
    let mut low = vec![0; edges.len()];
    let mut component = vec![NONE; edges.len()];
    let mut open = Vec::new();
    let mut counter = 0;
    for root in from..edges.len() {
        let mut work = Vec::new();
        if order[root] == NONE {
            work.push((root, 0));
            (order[root], low[root]) = (counter, counter);
            counter += 1;
            open.push(root);
        }
        while let Some((v, next)) = work.pop() {
            if let Some(&w) = edges[v].get(next) {
                work.push((v, next + 1));
                if w >= from && order[w] == NONE {
                    (order[w], low[w]) = (counter, counter);
                    counter += 1;
                    open.push(w);
                    work.push((w, 0));
                } else if w >= from && component[w] == NONE {
                    low[v] = low[v].min(order[w]);
                }
                continue;
            }
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == order[v] {
                while let Some(w) = open.pop() {
                    component[w] = v;
                    if w == v {
                        break;
                    }
                }
            }
        }
    }
    component
}

/// The elementary cycles through `start` among the nodes `allowed`, onto `found`. A
/// node that led to no cycle stays blocked until a node it waits on is on one.
fn cycles_from(
    edges: &[Vec<usize>],
    start: usize,
    allowed: &dyn Fn(usize) -> bool,
    found: &mut Vec<Vec<usize>>,
) {
    let mut blocked = vec![false; edges.len()];
    let mut waiting: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
    let mut path = vec![start];
    // Each node of the path with its next edge and whether a cycle went through it.
    let mut work = vec![(start, 0, false)];
    blocked[start] = true;
    while let Some((v, next, closed)) = work.pop() {
        if let Some(&w) = edges[v].get(next) {
            work.push((v, next + 1, closed || w == start));
            if w == start {
                found.push(path.clone());
            } else if allowed(w) && !blocked[w] {
                blocked[w] = true;
                path.push(w);
                work.push((w, 0, false));
            }
            continue;
        }
        path.pop();
        if closed {
            let mut free = vec![v];
            while let Some(u) = free.pop() {
                if blocked[u] {
                    blocked[u] = false;
                    free.append(&mut waiting[u]);
                }
            }
            if let Some(parent) = work.last_mut() {
                parent.2 = true;
            }
        } else {
            for &w in edges[v].iter().filter(|&&w| allowed(w)) {
                if !waiting[w].contains(&v) {
                    waiting[w].push(v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{components, elementary_cycles};
    use sjs_testkit::prng::Rng;

    fn sorted(mut cycles: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
        cycles.sort();
        cycles
    }

    /// Every simple path from each node back to it through greater nodes only.
    fn by_exhaustion(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
        fn extend(edges: &[Vec<usize>], path: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
            let last = path[path.len() - 1];
            for &w in &edges[last] {
                if w == path[0] {
                    found.push(path.clone());
                } else if w > path[0] && !path.contains(&w) {
                    path.push(w);
                    extend(edges, path, found);
                    path.pop();
                }
            }
        }
        let mut found = Vec::new();
        for start in 0..edges.len() {
            extend(edges, &mut vec![start], &mut found);
        }
        sorted(found)
    }

    #[test]
    fn small_graphs_have_the_cycles_they_have() {
        let none: Vec<Vec<usize>> = Vec::new();
        assert_eq!(elementary_cycles(&[]), none);
        assert_eq!(elementary_cycles(&[vec![]]), none);
        assert_eq!(elementary_cycles(&[vec![0]]), [[0]], "a self-import");
        assert_eq!(elementary_cycles(&[vec![1], vec![0]]), [[0, 1]]);
        assert_eq!(elementary_cycles(&[vec![1], vec![2], vec![1]]), [[1, 2]]);
        assert_eq!(
            sorted(elementary_cycles(&[vec![1], vec![2], vec![0, 1], vec![3]])),
            [vec![0, 1, 2], vec![1, 2], vec![3]],
            "two cycles that share the edge 1 → 2, and one apart"
        );
        // 2 leads nowhere while 1 is on the path, and to a cycle once it is not.
        let waits = [vec![1, 2], vec![2, 3], vec![1], vec![0]];
        assert_eq!(
            sorted(elementary_cycles(&waits)),
            [vec![0, 1, 3], vec![0, 2, 1, 3], vec![1, 2]]
        );
    }

    #[test]
    fn components_are_the_nodes_that_reach_one_another() {
        // An edge into a component already closed joins nothing to it.
        let found = components(&[vec![1, 2], vec![], vec![1]], 0);
        assert!(found[0] != found[1] && found[1] != found[2] && found[0] != found[2]);
        let found = components(&[vec![1], vec![2], vec![0, 3], vec![2]], 0);
        assert!(found.iter().all(|&c| c == found[0]), "{found:?}");
        let found = components(&[vec![1], vec![2], vec![0, 3], vec![2]], 1);
        assert_eq!(found[2], found[3]);
        assert_ne!(found[1], found[2], "without node 0, node 1 is on no cycle");
    }

    #[test]
    fn random_graphs_have_exactly_the_cycles_exhaustion_finds() {
        for seed in 0..2_000 {
            let mut rng = Rng::from_seed(seed);
            let nodes = rng.index(7) + 1;
            let density = rng.range(1, 6);
            let edges: Vec<Vec<usize>> = (0..nodes)
                .map(|_| (0..nodes).filter(|_| rng.chance(density, 8)).collect())
                .collect();
            assert_eq!(
                sorted(elementary_cycles(&edges)),
                by_exhaustion(&edges),
                "seed {seed}: {edges:?}"
            );
        }
    }

    #[test]
    fn a_wide_graph_without_cycles_and_a_long_cycle_take_no_time() {
        // Forty layers of four, each node importing the whole next layer: 4^40 paths.
        let layered: Vec<Vec<usize>> = (0..160)
            .map(|v| {
                let next = (v / 4 + 1) * 4;
                (next..next + 4).filter(|&w| w < 160).collect()
            })
            .collect();
        assert_eq!(elementary_cycles(&layered), Vec::<Vec<usize>>::new());
        let ring: Vec<Vec<usize>> = (0..100_000).map(|v| vec![(v + 1) % 100_000]).collect();
        let cycles = elementary_cycles(&ring);
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].iter().copied().eq(0..100_000));
    }
}
