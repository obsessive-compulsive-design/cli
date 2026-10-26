## Story

- **ID:** US-___
- **Spec sections:** §
- **Decisions relied on** (the `Decision:` markers in the story, plus any new judgement call this PR makes; a new one must be added to the story in this PR):
  -

## How I tried to break it

<!-- List the edge cases you considered and, for each, whether it has a test. -->

-

## Definition of done (STORIES.md §0.2)

- [ ] Every acceptance bullet has a test that fails without the change.
- [ ] Every new diagnostic has a trigger, a near-miss and a boundary fixture under `tests/conformance/`.
- [ ] `cargo llvm-cov` reports 100 % lines and regions for the workspace, with no exclusions.
- [ ] `cargo mutants` on the changed files reports zero surviving mutants, or each survivor is listed below with why it is equivalent code.
- [ ] `cargo clippy` (pedantic, `unwrap`/`expect` denied outside tests) and `cargo fmt --check` are clean.
- [ ] Property tests, fuzz targets and snapshots exist where §0.2 requires them.
- [ ] No `unwrap()`, `expect()` or `panic!` in non-test code.

## Reviewer checklist (STORIES.md §0.4)

- [ ] Every spec sentence the story cites maps to a test; any that does not is listed here.
- [ ] Each diagnostic's near-miss fixture exercises the *reason* the rule exists (§12 rationale), not just a syntactic variant.
- [ ] Positions (line and column) are asserted, not just codes.
- [ ] Behaviour on an empty input, a one-token input and a 100-column line is tested.
- [ ] No code path depends on file order, hash-map iteration order or wall-clock time.
- [ ] Every judgement call is listed as a `Decision:` in the story (and considered for `SPEC.md`).

## Surviving mutants (if any)

<!-- mutant → why it is equivalent code -->
