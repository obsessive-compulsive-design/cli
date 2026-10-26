# Strict JS compiler: implementation stories

This file breaks `SPEC.md` into user stories small enough for a human to review in one
sitting. Each story is one pull request. Stories are ordered so that every one builds on
merged work only; nothing is stubbed "for a later story" except where a story says so.

Read §0 once. Then work the stories in the order of §1. Each story cites the spec section
it implements (`§n`). Where the spec is silent, the story states the choice it makes under
a **Decision:** marker, always erring toward rejection (§1 "subtract only"), so a reviewer
can see every judgement call and move it into `SPEC.md` if it deserves to be a rule.

---

## 0. Working agreements

### 0.1 Size and shape of a story

- One story is one PR. Target under **400 changed lines** of hand-written code and tests,
  excluding generated fixtures and snapshot files. A story that cannot fit is split before
  work starts, not during review.
- Sizes: **S** ≤ 200 lines, **M** ≤ 400 lines, **L** ≤ 600 lines (L is allowed only where
  marked, and the reviewer may still ask for a split).
- A story touches one crate wherever possible; cross-crate changes are listed in scope.
- Nothing lands behind a feature flag. If a story is not finished, it is not merged.

### 0.2 Definition of done (every story)

1. Every acceptance bullet is demonstrated by at least one test that fails without the
   change.
2. Every diagnostic the story introduces has, in `tests/conformance/`:
   - at least one fixture that **triggers** it at the stated line and column,
   - at least one **near-miss** fixture that looks like a trigger but must not fire (the
     edge the rule is meant to respect),
   - one fixture at each **boundary** the spec names (first/last member, exactly at a
     limit, exactly one over).
3. `cargo llvm-cov --fail-under-lines 100 --fail-under-regions 100` passes for the whole
   workspace. There is no exclusion list; unreachable code is deleted, not excluded.
   (Region coverage is the LLVM proxy for branch coverage.)
4. `cargo mutants` on the files the PR changed reports **zero surviving mutants**, or the
   PR description lists each survivor with the reason it is equivalent code. Coverage
   proves lines executed; mutation testing is how this plan approximates "complete
   functional and edge-case coverage".
5. `cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic` and
   `cargo fmt --check` are clean.
6. Property tests (`sjs-testkit::prop`, US-008) run with at least 256 cases locally
   (`SJS_PROP_CASES`) and 4 096 in CI for any story touching the lexer, parser,
   formatter, type normalizer or assignability.
7. Fuzz targets (tests named `fuzz_*` built on `sjs-testkit::fuzz`) exist for the lexer,
   parser and formatter from the story that creates each; CI runs each for 60 seconds
   (`SJS_FUZZ_SECONDS`) on every PR.
8. Snapshot tests (`sjs-testkit::snapshot`) cover every human-readable output
   (diagnostic text, unified diffs, TAP, help text, generated HTML and Markdown). A
   changed snapshot is reviewed as code.
9. The PR description contains the story ID, the spec sections, every **Decision** the
   story makes, and a "how I tried to break it" paragraph listing the edge cases considered
   and whether each has a test.
10. No `unwrap()`/`expect()`/`panic!` in non-test code; every error path is a typed error
    that the CLI maps to exit code 70 (§13.1), and the mapping is tested.
11. **Zero dependencies, all safe Rust.** No crate outside this workspace appears in any
    Cargo manifest section (`dependencies`, `dev-dependencies`, `build-dependencies`) or
    in `Cargo.lock`; everything the tests need (property testing, snapshots, fuzzing,
    process driving, benchmark timing, JSON) is written in `sjs-testkit` or the product
    crates on top of `std`. Every manifest forbids `unsafe_code`. `scripts/audit.sh`
    enforces both in CI. External *tools* that never enter a manifest (cargo-llvm-cov,
    cargo-mutants, Node/Deno/Bun for the runner) are allowed.

### 0.3 Test taxonomy and where tests live

| Kind | Tool | Location | Purpose |
|---|---|---|---|
| Unit | `#[test]` | beside the code | one function, one behaviour |
| Conformance fixture | custom harness (US-007) | `tests/conformance/<section>/<code-or-topic>/*.js` | the spec's own examples and every rule's trigger/near-miss/boundary programs |
| Property | `sjs-testkit::prop` | `tests/prop_*.rs` per crate | invariants over generated programs (round-trip, idempotence, normalization) |
| Fuzz | `sjs-testkit::fuzz` | `tests/fuzz_*.rs` per crate (regressions under `tests/fuzz-regressions/`) | no panics, no hangs, on arbitrary bytes |
| Snapshot | `sjs-testkit::snapshot` | `tests/snapshots/` | exact text output |
| Integration (CLI) | `sjs-testkit::process` (std `Command`) | `crates/sjs-cli/tests/` | argv in; exit code, stdout and files out |
| Runtime | `sjs test` on `runtime/` | `runtime/*.test.js` | the Strict JS matcher module's own tests, also run under Jest, Vitest and Bun in CI (§11.3) |
| Benchmark | `sjs-testkit::bench` (`harness = false`) | `benches/` | §11.3 throughput and incremental latency |

A conformance fixture declares its expectations inline so a reviewer sees rule and example
together:

```js
// expect: E103 non-exhaustive @7:3
// expect: S001 @9:14  "'a'" -> "\"a\""
```

A fixture with no `expect:` line must produce no diagnostics. The harness fails on any
unexpected diagnostic, any expected one that is missing, and any position mismatch.

### 0.4 Review checklist (copied into the PR template)

- Does every spec sentence the story cites map to a test? List any that does not.
- For each diagnostic: is there a near-miss fixture, and does it exercise the *reason* the
  rule exists (the §12 rationale), not just a syntactic variant?
- Are all positions (line, column) asserted, not just codes?
- Is behaviour on an empty input, a one-token input, and a 100-column line tested?
- Does any code path depend on file order, hash-map iteration order or wall-clock time?
- Does the PR make a judgement call the story does not list as a **Decision**? If so, add
  it to the story (and consider moving it to `SPEC.md`).

### 0.5 Target repository layout

The current `src/` (an `ocd-cli` binary with `core/`, `commands/`, `utils/` stubs) is
replaced in US-002 by a Cargo workspace. One crate per pass keeps PRs small and makes §11's
"each pass is a pure function of the previous" a property the module system enforces.

```
Cargo.toml                 workspace; binary target is `sjs`
crates/
  sjs-diag/                diagnostic model, positions, rendering, report/types JSON
  sjs-syntax/              lexer, parser, lossless tree, exact printer         (Epics 1–2)
  sjs-format/              canonical layout, S-code diffs                       (Epic 3)
  sjs-bind/                module graph, scopes, ambient table                  (Epic 4)
  sjs-types/               type representation, normalization, assignability, printer
  sjs-infer/               inference                                            (Epic 5)
  sjs-check/               CFG, narrowing, exhaustiveness, checks               (Epic 6)
  sjs-lint/                limits, naming, structure, doc comments              (Epic 7)
  sjs-test/                runner hosts, reporters, coverage                    (Epic 8)
  sjs-map/ sjs-docs/       side tools                                           (Epic 9)
  sjs-lsp/                 language server                                      (Epic 10)
  sjs-cli/                 argv, dispatch, exit codes, `help`
  sjs-testkit/             std-only test tooling: PRNG, property runner, snapshots, fuzz loop,
                           process driver, bench timer (US-008)
ambient/                   data files: the five-runtime feature table, IDL-derived signatures
runtime/                   the Strict JS matcher module, the per-runtime host shims, their tests
tests/conformance/         fixtures (0.3)
scripts/                   audit.sh (zero-dependency and no-unsafe gate), gates.sh (all gates locally)
benches/
SPEC.md STORIES.md
```

---

## 1. Order of work

| Phase | Milestone (§11.2) | Stories |
|---|---|---|
| Foundations | M0 | US-001 … US-009 |
| Lexer | M0 | US-010 … US-019 |
| Parser | M0 | US-020 … US-033 |
| Formatter | M0 | US-040 … US-050 |
| Binder | M1 | US-060 … US-070 |
| Types and inference | M1 | US-080 … US-100 |
| Checker | M2 | US-110 … US-124 |
| Linter, reporter, build | M3 | US-130 … US-138 |
| map, docs, help | M3 | US-150 … US-156 |
| Test runner | M4 | US-140 … US-149 |
| Performance, incremental, LSP, adoption | M5 | US-160 … US-165 |

Dependencies are listed per story as **After**. Stories with the same **After** can be
developed in parallel by different people; their PRs are still merged one at a time.

The test runner (M4) is listed after map/docs because it depends on the checker's
test-file typing (US-124) and the reporter (US-136). US-140 (the matcher module, which is
plain Strict JS) can start as soon as US-002 lands and is a good first contribution for
someone who will write Strict JS rather than Rust.

---

## 2. Stories

Story format: **ID Title** · Spec · After · Size · Milestone, then the user story, scope,
acceptance criteria ("Done when") and the tests the reviewer must find in the PR. "Edge"
bullets are the cases most likely to be missed; each one needs its own test.

---

### Epic 0 · Foundations

#### US-001 Toolchain pin, CI and quality gates
§0.2 · After: nothing · Size: S · M0 · **Status: complete** (2026-10-26)

> Delivered: `rust-toolchain.toml` pinned to 1.99.1; `.github/workflows/ci.yml` with the
> fmt, clippy (two passes), audit, test, coverage, mutants and fuzz jobs on every push to
> `main` and every PR with no path filters; `scripts/audit.sh` and `scripts/gates.sh`;
> `.github/PULL_REQUEST_TEMPLATE.md` carrying §0.2 and §0.4; `unsafe_code = "forbid"` in
> the manifest and no dependency sections at all; the existing stubs brought to 100 % line
> and region coverage (unit tests for the dispatcher, `tests/cli.rs` running the binary
> through `std::process::Command`) so the gate is green before US-002.
> Verified locally with cargo-llvm-cov 0.9.1 and cargo-mutants 27.1.0: an untested line
> fails the coverage job, an `unwrap()` in non-test code fails clippy pass 1, and the diff
> has zero surviving mutants. The throwaway-branch CI runs happen when this lands as a PR;
> the run links go in that PR's description.

As a reviewer I want every gate in §0.2 enforced by CI before any compiler code exists, so
that no later story can lower the bar.

**Scope.** `rust-toolchain.toml` pinned to the latest stable (the machine currently has
1.77.2, which predates several dev-dependencies; the exact pin is the reviewer's call);
CI workflow running fmt, clippy pedantic as errors, the zero-dependency and no-unsafe
audit (`scripts/audit.sh`), tests, `cargo llvm-cov` with 100 % line and region
thresholds, `cargo mutants` on changed files, the `fuzz_*` tests with
`SJS_FUZZ_SECONDS=60` (matching nothing until US-019), `SJS_PROP_CASES=4096`; PR
template containing §0.4.

**Done when**
- A PR adding an untested line fails CI with the coverage message.
- A PR adding `unwrap()` outside tests fails clippy (`clippy::unwrap_used` and
  `clippy::expect_used` denied for non-test code).
- A PR adding any `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` entry, or
  an `unsafe` block, fails the audit job.
- CI is green on the empty workspace.

**Tests.** The workflow is exercised by a throwaway branch that violates each gate once
(run links in the PR). Edge: a PR that changes only `*.md` still runs the gates.

#### US-002 Workspace skeleton and `sjs` binary
§11, §13 · After: US-001 · Size: M · M0 · **Status: complete** (2026-10-26)

> Delivered: a virtual workspace (`Cargo.toml` has no root package; the **Decision** below
> is refined to "the root is a virtual workspace and the `sjs` binary is `sjs-cli`'s
> `[[bin]]`") with fourteen `crates/sjs-*` members inheriting version, edition and the
> `unsafe_code = "forbid"` lint through `lints.workspace = true`; every placeholder crate
> exposes one `*Placeholder` type naming the story that replaces it; `sjs-cli` is a library
> (`run(args, stdout, stderr) -> u8`, every failure mapped to an exit code so `main` is one
> expression) plus the `sjs` binary; `sjs-testkit` ships only `process::run` (the rest is
> US-008); `crates/sjs-cli/tests/workspace_dag.rs` reads `cargo tree` text and checks every
> edge against the §0.5 pipeline table; the old `src/`, `tests/` and the `Cargo.lock`
> ignore are gone. Verified locally: fmt, both clippy passes, audit, tests, 100 % lines and
> regions, zero surviving mutants on the diff.

As a maintainer I want the crate layout of §0.5 in place with an empty pipeline, so that
every later story has an obvious home.

**Scope.** Replace `src/` with the workspace; `sjs-cli` builds the `sjs` binary; every
other crate exists with a `lib.rs` exposing one placeholder type that the first story in
its epic replaces. **Decision:** the root package is renamed from `ocd-cli` to `sjs`;
members are `sjs-*`. `Cargo.lock` is committed (binary project; removed from
`.gitignore`).

**Done when**
- `cargo build --workspace` produces `target/debug/sjs`.
- `sjs` with no arguments prints a one-line command list and exits 0 (placeholder text;
  US-156 replaces it with the §13.8 spec-derived help).
- No crate depends on a crate later in the pipeline (a test runs `cargo tree --workspace`
  through `sjs-testkit::process` and asserts the path-dependency DAG matches §0.5; no
  JSON parsing is needed).
- Every member manifest has `lints.workspace = true` and the root forbids `unsafe_code`;
  `scripts/audit.sh` passes.

**Tests.** Integration test for the no-argument run; the DAG test. Edge: a non-UTF-8
argv element exits 2, not 70.

#### US-003 Diagnostic model
§8, §13.1 · After: US-002 · Size: M · M0 · **Status: complete** (2026-10-26)

> Delivered in `sjs-diag`: a `codes!` table generating `Code` (all 86 codes: E000–E002,
> E100–E145, E150–E153, E160–E163, E201–E207, E301–E304, E401–E403, S001–S012, S020,
> S021, S030) with `as_str`, `name`, `section`, `severity` (from the letter), `is_hand_edit`
> (exactly S020/S021/S030), `Display` and exact `FromStr`; `Severity`; `Position`, `Span`
> (line/column plus byte offsets, §13.1), `Related`; `Diagnostic` with `with_fix`
> (rejecting a no-op replacement) and `with_related`; `Diagnostics` as a `BTreeSet` whose
> derived order is path, span, code, message, current, replacement, related, giving the
> (path, line, column, code) report order and dedup of identical findings; `has_errors`,
> `error_count`, `style_count`. **Decisions:** E110 is named `empty-untyped` (§3.2's
> newer wording wins over the §8.1 table's `empty-array-untyped`); E120 is
> `self-assign-or-compare` and E133 `removed-global-or-member` (one kebab name per code);
> S-code names are short rule names (`single-quoted-string`, `semicolon`, …). The
> permutation property test is hand-rolled until US-008. Also fixed here: the member
> manifests from US-002 had `lints.workspace = true` under `[package]`, which cargo
> ignores; they now carry a real `[lints]` table and `scripts/audit.sh` checks the table
> structurally and fails on any "unused manifest key" warning.

As every later pass I want one `Diagnostic` type carrying code, name, severity, primary
span, message, optional current text and replacement, and related spans, so rendering and
JSON are written once.

**Scope.** `sjs-diag`: a `Code` enum generated from one table listing every code in §8.1,
§8.2, §8.3, §8.4, §8.5, §5 and §13.7 (E000–E002, E100–E145, E150–E153, E160–E163,
E201–E207, E301–E304, E401–E403, S001–S012, S020, S021, S030) with name, severity and
spec section; `Diagnostic` and `Span`; a `Diagnostics` collector that sorts
deterministically (path, line, column, code) and deduplicates identical entries; a
`has_errors()` predicate (style codes never set it) the CLI maps to exit codes.

**Done when**
- Every code round-trips through its string form with the right name and severity.
- Sorting is total and stable; same position, different codes orders by code.
- S020, S021 and S030 carry a `hand_edit` flag that the reporter and `format --write`
  honour (§8.5).

**Tests.** Table-driven over every code; property test that sorting any permutation
yields one order. Edge: identical diagnostics from two passes collapse to one; a
replacement equal to the current text is rejected at construction.

#### US-004 Source text and positions
§13.1 (positions) · After: US-003 · Size: S · M0 · **Status: complete** (2026-10-26)

> Delivered in `sjs-diag::source`: `SourceFile::new(path, bytes)` validates UTF-8 (an
> invalid sequence is `E000` at the first bad byte with its line/column computed from the
> valid prefix) and rejects a BOM as `E000` at 1:1; a line index over `\n`, `\r\n`, `\r`,
> U+2028 and U+2029 with each line's terminator kept (`Line`, `LineTerminator`);
> `position(offset)` and `offset(position)` as inverses over every character boundary
> (terminator characters have columns of their own; the offset after a terminator is
> column 1 of the next line; a file ending in a terminator has a final empty line);
> `span(start, end)`, `slice(&span)`, `line(n)`, `line_count`, `len`, `is_empty`,
> `text`, `path`. Every accessor returns `None` on bad input; nothing panics.
> **Decisions:** errors are returned as `Box<Diagnostic>` (clippy's large-error lint);
> the random-string round-trip test is hand-rolled until US-008.

As the lexer and reporter I want a `SourceFile` with a line index converting byte offsets
to 1-based line/column in Unicode scalar values and back, so every position agrees.

**Scope.** UTF-8 validation (an invalid byte is `E000` at its offset, never a crash);
line index over `\n`, `\r\n`, `\r`, U+2028, U+2029 (ES line terminators), remembering
each line's terminator (S008 needs it); offset ⇄ position; slicing by span.

**Done when**
- Positions match an independent naive scan in the test.
- **Decision:** a byte-order mark is `E000` at 1:1 (§2 says UTF-8; a BOM would be copied
  into `dist/` byte for byte and is not part of the subset).

**Tests.** Property test: `pos(offset(p)) == p` for every valid position of random
strings. Edge: empty file; no final newline; a line of 100 multi-byte characters (column
100, not byte 300); a lone `\r`; U+2028 inside a string literal still counts as a line
terminator for positions.

#### US-005 CLI dispatch, usage errors and exit codes
§13, §13.1 · After: US-003 · Size: M · M0

As a CI author I want `sjs <command>` to resolve the eight commands, reject anything else
with exit 2, and honour `--help` on every command, so scripts can rely on exit codes
before any pass exists.

**Scope.** Command enum; `--help` synonym (§13.8); unknown command or flag → 2 with the
command's help; the exit mapping of §13.1 as one function (0/1 for build, lint, map,
docs; file counts capped at 255 for format and test; 0 for help and clean `lsp`
shutdown); internal error → 70 with the error on stderr. Commands other than `help`
return "not implemented" with exit 70 until their story lands (the only permitted stub,
removed by each command's story).

**Done when**
- Every exit code in §13.1 is produced by a test (0, 1, 2, 70, 255 cap).
- `sjs lint --help`, `sjs help lint` and `sjs --help lint` print the same text.
- Command names are case-sensitive (`sjs Lint` is a usage error; the current prototype
  lower-cases, which the spec does not permit).

**Tests.** `sjs-testkit::process` per case; snapshot of usage text. Edge: `--` ends flag parsing so
an entry literally named `--weird.js` is accepted; a flag given twice is a usage error;
`--out=` with an empty value is a usage error; `--target` (removed by §13.1) is an
unknown flag.

#### US-006 Common flags and root resolution
§5.3, §13.1 · After: US-005 · Size: M · M0

As a programmer I want the common flags (`--out`, `--json`, `--quiet`) and the root
(`./src/index.js` by default, or the entries named) parsed and validated once, so every
command behaves identically.

**Scope.** Flag parsing into an `Options` struct; per-command `--out` default (`dist/`,
`map/`, `docs/`, `coverage/`); root existence check; test-file discovery per §5.3 (every
`*.test.js` beneath `./src` with default entries; with explicit entries, every
`*.test.js` in the directory of each module in the graph, which needs the graph and is
therefore completed in US-060) in sorted order.

**Done when**
- `sjs lint` with no entries and no `./src/index.js` is a usage error naming the path.
- Explicit entries given twice, or via a symlink and its target, are counted once.
- Discovery ignores nothing (the spec has no ignore rule); tested with a
  `node_modules`-named directory under `src/`.

**Tests.** Integration tests with temp directories. Edge: a `.test.js` file inside a
directory whose name ends in `.test.js`; an entry outside `./src` (allowed; E207
scanning in US-062 still only looks beneath `./src`).

#### US-007 Conformance fixture harness
§0.3, §11.3 · After: US-003 · Size: M · M0

As a reviewer I want fixtures to state their expected diagnostics inline and a harness
that runs every fixture through whatever passes exist, so later stories add programs,
not test code.

**Scope.** Parser for `// expect:` lines (code, optional name, `@line:col`, optional
`"current" -> "replacement"`); discovery of `tests/conformance/**/*.js`; exact-position
comparison; multi-file fixtures as a directory with an `entry.js`; no `--update` mode
(expectations are written by hand); an audit test driven by a checked-in table of code →
story that reports codes without fixtures as "pending: US-xxx" until the story is marked
merged, and fails afterwards.

**Done when**
- A fixture with a wrong line number fails with expected vs actual.
- A fixture directory can declare which file is the root and which are test files.

**Tests.** The harness tests itself under `tests/conformance/_harness/`. Edge: an
`expect:` past the end of the file fails loudly; two `expect:` lines for the same
position and code require the diagnostic to fire exactly twice.

#### US-008 `sjs-testkit`: zero-dependency test tooling
§0.2 items 6–8 and 11 · After: US-003 · Size: M · M0

As every later story I want property testing, snapshots, fuzzing, process driving and
benchmark timing available from one workspace crate written on `std` alone, so the test
kinds §0.3 requires exist without a single external crate.

**Scope.** `prng`: a documented 64-bit generator (SplitMix64 seeding xoshiro256**) with
fixed test vectors. `prop`: `check(gen, property)` running `SJS_PROP_CASES` cases
(default 256), printing the failing seed and replaying it from `SJS_PROP_SEED`, shrinking
through a generator-supplied `shrink` iterator to a minimal counter-example. `snapshot`:
`assert(name, text)` against `tests/snapshots/<name>.snap`, writing when
`SJS_UPDATE_SNAPSHOTS=1` and otherwise failing with a unified diff (the testkit has its
own small line differ; `sjs-format` must not depend on a test crate, so US-048 writes the
product one separately). `fuzz`: `run(target)` for `SJS_FUZZ_SECONDS` (default 1) feeding
random bytes and mutations (bit flips, splices, dictionary tokens, previous crashes from
`tests/fuzz-regressions/`), saving any panicking input there. `process`: run a binary with
args and stdin, capturing status, stdout, stderr. `bench`: `time(name, iterations, f)`
reporting the median on `std::time::Instant`.

**Done when**
- Each facility has unit tests; shrinking reaches the minimal counter-example on a known
  failing property; the snapshot diff output is itself snapshotted.
- The crate has no dependency sections and forbids `unsafe_code`; `scripts/audit.sh`
  passes.

**Tests.** Edge: PRNG output matches fixed vectors on every platform; `SJS_PROP_SEED`
replay reproduces a failure exactly; a snapshot file with CRLF is compared byte-exactly;
the fuzz loop respects its time budget within 10 %; `process` on a missing binary is a
typed error, not a panic.

#### US-009 Program generator for the §2 grammar
§11.2 M0 · After: US-008 · Size: M · M0

As the parser and formatter stories I want a generator (on `sjs-testkit::prop`) producing
random Strict JS programs covering every production in §2, so round-trip and idempotence
are proven on 10 000 programs rather than hand-picked ones.

**Scope.** Generators for every production with weights that reach deep nesting and all
precedences; shrinking to minimal failing programs; a 10 000-program corpus with a fixed
seed committed as a tarball so failures reproduce across machines.

**Done when**
- Every production in §2 appears in at least one generated program (a coverage counter in
  the generator is asserted).
- Generated programs contain no removed production and no braceless body.

**Tests.** The production-coverage test; determinism test. Edge: generated identifiers
never collide with keywords or §6 globals; generated regex literals always include `v`;
generated numeric literals stay within `int32` unless doubles are requested.

---

### Epic 1 · Lexer

#### US-010 Token model and lossless token stream
§2, §11.1 · After: US-004, US-009 · Size: M · M0

As the formatter I want tokens carrying exact source text, leading trivia and a "preceded
by line terminator" flag, so that `print(lex(x)) == x` for any input.

**Scope.** `Token { kind, span, has_newline_before }`; trivia stored on the following
token (the last run attaches to EOF); a `Lexer` that never fails (unexpected bytes become
an `Error` token with `E000` and lexing continues).

**Done when**
- Concatenating trivia and token text reproduces the input byte for byte for the corpus
  and every fixture.
- `has_newline_before` is set after `\n`, `\r\n`, `\r`, U+2028, U+2029, or a block
  comment containing a line terminator (ES2025 rule).

**Tests.** Property round-trip on the corpus. Edge: empty input yields only EOF; input
that is only trivia; a block comment with a newline between two tokens sets the flag; one
without does not.

#### US-011 Identifiers, keywords, removed words, hashbang
§2 · After: US-010 · Size: S · M0

As a programmer I want `$` and Unicode escapes rejected in identifiers, the eight removed
words reported, and a hashbang line reported.

**Done when**
- `[A-Za-z_][A-Za-z0-9_]*` is `Ident`; `$a`, `a$`, `a` and a non-ASCII letter each
  produce `E000` at the offending character and lexing continues.
- `var with void delete in arguments eval debugger` produce `E000` naming the §12
  replacement; `null` lexes as a literal (placement is the parser's job).
- Contextual words (`of get set static async await yield from as`) lex as `Ident`.
- `#!` anywhere produces `E000` naming "a host-side launcher outside the subset" (§12).

**Tests.** Table over every keyword and contextual word. Edge: `in` inside `instanceof`
is not matched; `_` alone is an identifier; prefixes of keywords (`le`, `cons`); a
removed word after `.` (`a.delete`) lexes as a keyword token and the parser accepts it as
a property name, because `Map.prototype.delete` is kept (§12).

#### US-012 Numeric literals
§2 · After: US-010 · Size: S · M0

**Done when**
- Decimal (`1`, `1.5`, `.5`, `1.`, `1e3`, `1E-3`), `0x`, `0o`, `0b`, `_` separators and
  the `n` suffix tokenize with their value (`f64`, or arbitrary precision for bigint).
- `017`, `08`, `09` are `E000` (legacy octal and strict-mode leading zero).

**Tests.** Edge: `1__0`, `_1`, `1_`, `0x_1`, `1_n`, `1.5n`, `1e3n`, `0b2`, `0o8`, `0xG`
each `E000` at the exact column; `1_000_000` equals `1000000`; `9007199254740993`
lexes with a "not safely representable" flag for the type system; `3in` is `E000`
(numeric literal followed by an identifier start).

#### US-013 String literals
§2, S001 · After: US-010 · Size: S · M0

**Done when**
- `"a\n\t\"\\A\u{1F600}\x41"` yields the cooked value; raw text is kept.
- `'a'` lexes with `S001` and replacement `"a"`; `'say "hi"'` → `"say \"hi\""`;
  `'it\'s'` → `"it's"`.
- `"\1"`, `"\01"`, `"\8"`, `"\9"`, `"\u{110000}"` are `E000`; an unterminated string is
  `E000` at the line end and lexing resumes on the next line.

**Tests.** Edge: a line continuation is kept raw and removed cooked; U+2028/U+2029 inside
a string are permitted; `"\0"` is permitted when not followed by a digit; a string
containing both quote kinds gets the minimal-escape replacement.

#### US-014 Template literals
§2, §4.1 · After: US-013 · Size: M · M0

**Scope.** Template token modes with a heap-allocated brace-depth stack so
`${ {a:1}.a }` and nested templates lex; raw and cooked values; positions for US-029's
tag check.

**Done when**
- Every nesting case in the test list round-trips.
- An invalid escape in a template (`` `\u{` ``) is `E000` (permitted by ES only in tagged
  templates, and the only tag is `String.raw`, so it is an error here regardless).

**Tests.** Edge: unterminated template at EOF; `}` inside a string inside a substitution;
a template containing `*/`; a multi-line template keeps its original line terminators
(§9.3 never reflows it).

#### US-015 Regex literal tokenization and flags
§2, E002 · After: US-010 · Size: M · M0

**Scope.** Slash disambiguation driven by a parser hint (US-020 wires it); flag parsing.

**Done when**
- `/a/v` is a regex token with body `a` and flags `{v}`; `/a/` and `/a/u` are `E002`
  naming the missing `v`; `/a/vu`, `/a/vv`, `/a/vx` are `E000`.
- `d g i m s y` are accepted in any order alongside `v`.

**Tests.** Edge: `a / b / c` is two divisions; `x = /=/v`; `/[/]/v`; `/\//v`; a regex
containing a line terminator is `E000`; `/a/v.test(s)` continues with `.test`; `}` then
`/` after a block is a regex and after an object literal is division (both tested).

#### US-016 Regex pattern parser (v-mode)
§2, §6.1 RegExp · After: US-015 · Size: L · M0

As the type system I want every regex body parsed under `v`-mode syntax so named groups
are known and a malformed pattern is a compile error rather than a runtime `SyntaxError`.

**Scope.** ES2025 pattern grammar with `v`: disjunction, quantifiers (lazy, `{n,m}`),
assertions and lookaround, atoms, escapes (`\d \w \s \b \B \p{…} \P{…} \k<name> \1`),
named groups, modifier groups `(?i:…)`, class-set syntax (`[[a-z]--[aeiou]]`,
`[\p{L}&&\p{Lu}]`, `\q{abc|def}`), `v`-mode reserved punctuators. No matching.

**Done when**
- Every valid pattern in the table parses; every invalid one is `E000` at the offending
  offset inside the literal.
- Named groups are returned in source order; duplicates rejected unless in different
  alternatives (ES2025).

**Tests.** ≥ 150 patterns from test262's regex syntax cases. Edge: `\k<x>` without a
group `x`; back-reference to a later group; a literal `{` in `v`-mode; `[a-]`;
`(?<a>x)|(?<a>y)` permitted; unbracketed nested set operators; unknown `\p{}` property.

#### US-017 Comments and doc comments
§2, §9.5, §8.4 (E162 attachment) · After: US-010 · Size: S · M0

**Done when**
- `//`, `/* */`, `/** */` are distinct trivia kinds; `/**/` is an empty block comment;
  `/***/` is a doc comment with body `*` (the rule is "starts with `/**` and is longer
  than `/**/`").
- An unterminated block comment is `E000` at its start.
- `<!--` and `-->` are not comments in modules; they lex as operators and the parser
  rejects them.
- Attachment: a doc comment attaches to the declaration starting on the next non-blank
  line; a blank line between breaks it (§2, §8.4).

**Tests.** Edge: doc comment, blank line, declaration (not attached); doc comment, other
comment, declaration (not attached); `/** */` inside a string; a comment at EOF without a
trailing newline.

#### US-018 Punctuators
§2 · After: US-010 · Size: S · M0

**Done when**
- Every ES2025 punctuator lexes with longest match, including `?.`, `??`, `??=`, `**`,
  `**=`, `...`, `=>`, `>>>=`, `#`, and the removed `==`, `!=`, `||=`, `&&=`, `++`, `--`
  (tokenized so the parser can report them with the right message).
- `?.` followed by a decimal digit lexes as `?` then `.5`.

**Tests.** Table over every punctuator; longest-match pairs. Edge: `a?.5:1` is a ternary;
`#a` is a private name and `# a` is an error; `@` is `E000` naming decorators.

#### US-019 Lexer fuzzing and error recovery
§0.2 · After: US-011 … US-018 · Size: S · M0

**Done when**
- `fuzz_lex` with `SJS_FUZZ_SECONDS=600` locally finds nothing; CI runs it for 60 s per PR.
- Any input lexes in time linear in its length (10 MB of `/`; 10 MB of unterminated
  strings) under a fixed bound.

**Tests.** The fuzz target plus a regression file of every crash found. Edge: template
substitution depth 10 000 does not overflow the stack.

---

### Epic 2 · Parser

#### US-020 Syntax tree, spans, trivia attachment and exact printer
§11.1, §2 · After: US-010, US-009 · Size: M · M0

As the formatter and every pass I want an arena-allocated, immutable, lossless tree with a
printer reproducing the source exactly, so "the AST is never rewritten" (§11) is a
type-level guarantee.

**Scope.** `SyntaxNode`/`SyntaxToken` in an arena with parent links; every token keeps
its trivia; `print(tree) == source`; a `Parser` driver owning the lexer and passing the
regex-allowed hint (US-015); typed accessors added by later stories. No grammar beyond "a
module is a list of tokens".

**Done when**
- Exact round-trip on the corpus and every fixture.
- The tree has no `&mut` API after construction (a doctest that must fail to compile).

**Tests.** Property round-trip. Edge: zero-length nodes have valid spans; a node's span is
the union of its tokens' spans excluding leading trivia; parent of root is `None`.

#### US-021 Import declarations
§2, §5.1 · After: US-020 · Size: M · M0

As a programmer I want `import { a, b as c } from "./x.js"`, `import * as ns from
"../y.js"` and `import cfg from "./c.json" with { type: "json" }` parsed, with every other
import form rejected and the specifier string rules enforced.

**Done when**
- The two `.js` forms and the one JSON default form parse; trailing comma permitted in
  the list.
- `import x from "./x.js"` is `E000` (§5.1 "a default import of a `.js` module");
  `import "./side.js"`, `import x, { y }`, `import { default as x }` are `E000`.
- String rules checked here: must start with `./` or `../`; `.js` with a `with` clause
  or `.json` without one is `E205`; `.json` with any attribute other than exactly
  `{ type: "json" }` is `E000`; bare, absolute, URL, `node:`, `.mjs`, `.ts` or no
  extension is `E201`. File existence is US-060.
- Imports precede every other module item (§2 `Module`); an import after a statement is
  `E000`.

**Tests.** Edge: `"./a.js/"`, `"./"`, `"././a.js"` (accepted; normalization is US-060),
`".//a.js"` (E201), a specifier containing `\`, `with { type: "JSON" }` (E000), duplicate
attribute keys, `import {} from "./x.js"` (E000: the grammar requires one specifier),
`import { k } from "./c.json" with { type: "json" }` (E000: JSON has no named exports).

#### US-022 Export declarations
§2, §5.1, §8.4 · After: US-021 · Size: S · M0

**Done when**
- `export const/let/function/class`, `export { a, b as c }` and `export { a } from
  "./x.js"` parse; `export default …` is `E202` for every default form; `export * from`
  and `export * as ns from` are `E161`.
- `export let` is accepted syntactically (reassignment is the binder's concern).

**Tests.** Edge: exporting one name twice is `E000` (ES early error); `export { x as
default }` is `E202`; `export {}` is `E000`; an export inside a block is `E000`.

#### US-023 `const`, `let` and binding patterns
§2, §4.2 · After: US-020 · Size: M · M0

**Done when**
- Object and array destructuring with defaults and rest parse; `let x` without an
  initializer is `E000` with replacement `let x = undefined` (§12).
- **Decision:** an elision in an array *pattern* (`const [a, , b] = y`) is `E000`, by the
  same "dense arrays" rationale as array literals.

**Tests.** Edge: `let let = 1` and `let undefined = 1` are `E000`; a rest element not
last; a default referencing an earlier element (`[a, b = a]`, valid); `const {} = x`
(accepted syntactically; E116 later); duplicate names within one pattern (`E000`).

#### US-024 Function declarations and arrow functions
§2, §4.4 · After: US-023 · Size: M · M0

**Done when**
- `function`, `async function`, `function*`, `async function*` declarations parse only at
  module top level or directly inside a `Block`.
- Arrows: `(a) => a`, `async (a) => a`, `(a, { b }, ...c) => {}`; `a => a` and `async a
  => a` are `E000` with the parenthesised replacement.
- `const f = function () {}` and `function* () {}` expressions are `E000` naming "arrow
  function or `function` declaration".
- Defaults, destructuring, rest in parameters; duplicate parameter names are `E000`.

**Tests.** Edge: `() => {}` is an empty block, `() => ({})` an object; `(a, b) => a, b` is
`E000` (comma); `async(x)` is a call to a binding named `async`; a `yield` inside an
arrow inside a generator is `E000`; a `function` declaration as the body of an arrow
expression position is `E000`.

#### US-025 Class declarations
§2, §4.5 · After: US-024 · Size: M · M0

**Done when**
- Fields, `#private` members, methods (`async`, `*`, `async *`), accessors, `static`
  members, `static {}` blocks, `extends Ident` and `extends ns.Ident` parse.
- `MemberKey` accepts identifiers, private names and exactly the four `[Symbol.…]` keys
  of §2; any other computed key is `E000`.
- `const A = class {}` is `E000`; a generator, async or accessor `constructor`, two
  constructors, `#constructor`, a field named `constructor`, a static field named
  `prototype`, and an undeclared `#x` are each `E000` (ES early errors).

**Tests.** Edge: `static static = 1`; `get` and `set` as method names; a body of only
`;`; `super()` outside a derived constructor is `E000`; `super.x` in a static method is
accepted; `extends a.b.c` is `E000` (grammar allows one dot).

#### US-026 Statements and blocks
§2, §4.3 · After: US-024 · Size: M · M0

**Done when**
- `Block`, `if`/`else`/`else if`, `while`, `for…of`, `for await…of`, the classic `for`,
  `return`, `break`, `continue`, `throw` and expression statements parse.
- Every body is a `Block`: `if (x) y()`, `else y()`, `for (…) y()`, `while (x) y()` are
  `E000` with the braced replacement (§4.3, §12).
- The `for` header is exactly `LetDecl ; Expr ; ForUpdate` with `ForUpdate` one of
  `i += e`, `i -= e`, `i++`, `i--`, `++i`, `--i`; `for (;;)`, `for (const …)`, `for (let
  i = 0, j = 0; …)`, `for (let i = 0; i < n; i = i + 1)` are each `E000` with the reason.
- `for…in`, `do…while`, labels, `with`, `debugger`, `var`, labelled `break`/`continue`
  are `E000` naming the §12 replacement.

**Tests.** Edge: a bare `;` parses as an empty statement and is reported S002 (it is a
semicolon); `for (const [k, v] of m)`; `for await` outside an async context is `E000`
(module top level counts as async); `else if` chains of depth 10; a declaration directly
as a body (`if (x) const y = 1`) is `E000`.

#### US-027 `switch` and `try`
§2, §4.3 · After: US-026 · Size: S · M0

**Done when**
- `case` labels are literals (number, string, boolean, bigint, and a unary-minus number);
  **Decision:** `case undefined:` is `E000` because `undefined` is an identifier, not a
  `Literal`; narrow before the switch.
- Each clause body is a `Block` (braces required); statements without braces are `E000`
  with the braced replacement.
- `default` at most once, anywhere; `try` without `catch` or `finally` is `E000`;
  `catch {}` and `catch (e)` both parse; a pattern in `catch (…)` is `E000`.

**Tests.** Edge: duplicate literal labels are `E000`; `case 1n:`; `case -1:`; an empty
`switch (x) {}`; a clause block that is empty `{}`.

#### US-028 Expression grammar and operators
§2, §4.1, §4.3 · After: US-023 · Size: L · M0

**Scope.** Precedence climbing for assignment (incl. `??=` and compound ops), ternary,
`??`, `||`, `&&`, `|`, `^`, `&`, `===`/`!==`, relational incl. `instanceof`, shift,
additive, multiplicative, exponent (right-assoc, unary restriction), unary (`! - + ~
typeof await`), postfix (US-029); `yield` inside generators.

**Done when**
- `==`, `!=`, `in`, `void`, `delete`, comma, `||=`, `&&=` are `E000` naming the
  replacement; `a ?? b || c` is `E000`; `(a ?? b) || c` is fine; `-2 ** 2` is `E000`.
- `i++`/`i--`/`++i`/`--i` as an expression statement parse and are reported S012 with
  replacement `i += 1` / `i -= 1`; used as a value (`x = i++`, `f(--i)`) they are `E000`
  (§4.3).
- Assignment targets: identifier, member, pattern; `a + b = c` is `E000`; `(a) = 1` is
  accepted and is S005.

**Tests.** A precedence table test over every adjacent-precedence pair (≥ 60 cases).
Edge: `a < b < c` left-assoc; `typeof typeof x`; `await` in a non-async function is
`E000`, at top level accepted; `yield a, b` is `E000`; `new A().b`; `a ? b : c ? d : e`
right-assoc; `x ?? (y || z)`.

#### US-029 Member, call, optional chaining, `new`, `import()`, `this`, `super`, templates
§2 · After: US-028 · Size: M · M0

**Done when**
- `a.b`, `a?.b`, `a[b]`, `a?.[b]`, `a()`, `a?.()` chain in any mix with optional-chain
  short-circuit groups represented in the tree.
- `new A()` and `new ns.A()` parse; `new A` (no parens), `new a.b.c()`, `new (f())()` are
  `E000`.
- `import("./x.js")` with a string literal parses under US-021's string rules;
  `import(x)` and template specifiers are `E000`.
- `super.x`/`super()` only inside class members; `super?.x` is `E000`.
- `` String.raw`a${b}` `` parses; any other tag is `E000` (§2).
- Keywords after `.` are property names (`a.delete(k)`), per US-011.

**Tests.** Edge: `a?.b.c` short-circuits the whole chain (tree shape); `a?.[0]?.()`;
`new.target` and `import.meta` are `E000` naming §12/§5.1; `this` at top level parses
(E126 is the checker's); `f(a,)`; `f(...xs)`; an optional call on a template tag is
`E000`.

#### US-030 Object and array literals
§2, §4.6 · After: US-028 · Size: S · M0

**Done when**
- `{ a, b: 1, "c-d": 2, [k]: 3, ...o }` and `[1, ...xs]` parse with trailing commas.
- `{ get a() {} }`, `{ a() {} }`, `{ __proto__: p }`, `{ "__proto__": p }` are `E000`
  naming "classes" / "prototype manipulation"; `[1, , 2]` and `[,]` are `E000` ("dense
  arrays").
- `E134` for a key spelled twice among non-computed keys (`{ a: 1, a: 2 }`, `{ 1: "a",
  "1": "b" }`); computed-key duplicates are decided in US-085 once the key's literal
  type is known.

**Tests.** Edge: `{ a: 1, ...o, a: 2 }` is not E134 (spread between; later wins); `{
async: 1 }`, `{ get: 1 }`; a trailing comma after a spread; `{}` and `[]`; `{
["__proto__"]: p }` is accepted by ES (does not set the prototype) and accepted here,
with a test documenting why.

#### US-031 ASI hazards and semicolons
§2, §9.2, S002 · After: US-029 · Size: M · M0

As a programmer I want every place where ASI would change the meaning of a no-semicolon
file reported as E001, and every semicolon reported as S002, so the file parses the same
with or without ASI and the layout is canonical.

**Done when**
- A line starting with `(`, `[`, `` ` ``, `+`, `-`, `/` whose previous non-trivia line
  ends an expression is `E001` at the line's first column, with no replacement, **whether
  or not a `;` precedes it** (§2).
- A line terminator between `return`/`throw`/`yield` and its operand is `E001`.
  **Decision:** `break`/`continue` followed by a newline and an identifier, and a
  postfix `++`/`--` split from its operand, are the same hazard and are also E001.
- Every `;` is `S002` with replacement `""`; a bare `;` statement likewise.

**Tests.** Every hazard token; every non-hazard line start (`{`, identifier, keyword, `.`
which is a member continuation by ASI rules and stays). Edge: previous line ends in `{`,
`;` or a class `}` (no hazard); a hazardous line after a comment-only line looks past the
comment; `x\n/re/v.test(y)` is E001; the first line is never E001; `return\n+1` is E001
by both rules and reported once.

#### US-032 Parser error recovery and fuzzing
§0.2 · After: US-031 · Size: M · M0

**Done when**
- A fixture with five independent errors reports exactly five; error nodes still
  round-trip.
- `fuzz_parse` with `SJS_FUZZ_SECONDS=600` clean; 1 000 corpus programs each mutated by deleting
  one random token parse without panic and round-trip.
- Nesting depth 100 000 of `(` reports `E000` and returns (no stack overflow).

**Tests.** Edge: unclosed `{` at EOF; `)` with no opener; a file of only `}`; an error
inside a template substitution recovers at the template end.

#### US-033 Grammar conformance sweep
§2, §11.2 M0, §12 · After: US-032 · Size: M · M0

**Done when**
- `tests/conformance/02-grammar/pass/` has a fixture per production in §2 and `fail/` has
  one per item in "Removed productions" and per syntactic row of §12, each asserting
  `E000` (or the named code) at the exact position with the replacement in the message.
- The 10 000-program round-trip is a CI test with the fixed seed.

**Tests.** Edge: each `fail/` fixture contains exactly one error.

---

### Epic 3 · Formatter

#### US-040 Pretty-printer core
§9, §9.1 · After: US-020 · Size: M · M0

As the formatter I want a document IR (text, line, group, indent, if-break) and a layout
algorithm with a hard 100-column width and 2-space indent, so every later rule is a
tree-to-IR mapping and width logic exists once.

**Done when**
- Groups break outermost-first; a group that fits stays flat; a single token wider than
  100 columns sits on its own line without forcing its parent to break further.
- Width counts Unicode scalar values (consistent with §13.1 columns).

**Tests.** Unit tests per IR node; property test that no line exceeds 100 columns except
one holding a single over-long token. Edge: width exactly 100 fits, 101 breaks; nested
groups where only the inner breaks; empty document; indentation inside a broken group
inside a flat parent.

#### US-041 Expression formatting and punctuation
§9.2, S001, S005 · After: US-040, US-030 · Size: M · M0

**Done when**
- Spaces around binary operators and after `,` `:`; none inside `()` `[]`; `{ a }` for
  single-line objects and destructuring; `{}` empty.
- Strings are emitted double-quoted with minimal escapes by the same function that
  produces S001's replacement (lexer and formatter cannot disagree).
- **Decision (S005 minimality):** parentheses are kept only where ES2025 precedence
  requires them, around arrow parameters, around `??` beside `&&`/`||`, around an
  object-literal arrow body, and around an `await`/`typeof`/unary operand that is itself
  a binary expression; every other pair is S005.
- Template literals are copied verbatim (§9.3).

**Tests.** Snapshot per operator family; a parenthesization table over every pair of
operators checking keep/drop and parse-equivalence (US-050). Edge: `(a ?? b) || c` and
`(a || b) ?? c` keep; `((a))` → `a`; `(-1) ** 2` keeps; `(await p).x` keeps; `(() =>
1)()` keeps; `typeof (a + b)` keeps; `-(-a)` keeps; a string that is only `"` has
replacement `"\""`.

#### US-042 Statements, declarations and blank lines
§9.1, §9.4, S007, S008 · After: US-041, US-027 · Size: M · M0

**Done when**
- One blank line between top-level declarations; at most one consecutive blank line; none
  at the start or end of a block; a `function` declaration has one blank line above its
  preceding comment.
- Every statement form from US-026/US-027 has a snapshot.
- `\r\n`, `\r`, trailing whitespace, missing or doubled final newline → S008; tabs and
  non-multiple-of-two indentation → S007; `--write` output always ends in exactly one
  `\n`.

**Tests.** The blank-line matrix (0–3 blank lines in each of: between top-level items,
after `{`, before `}`, between statements). Edge: a file that is only comments; no
trailing newline plus a trailing comment; an empty clause block; blank lines inside a
multi-line template untouched.

#### US-043 Lists and breaking
§9.3, S004, S011 · After: US-041 · Size: M · M0

**Done when**
- Objects, arrays, arguments, parameters and import lists are single-line when they fit
  and one-element-per-line otherwise; trailing commas only in the multi-line form (S004
  both directions).
- An object literal with a newline between `{` and its first key stays multi-line.
- Arrow bodies: expression stays expression; object-literal body is `() => ({ … })`; a
  block body that is a single `return` is S011 with the expression-body replacement.

**Tests.** Snapshots at widths 99/100/101 per list kind. Edge: a list whose only element
is over-long; a broken child forces the parent; a comment between `{` and the first key
keeps multi-line; an empty object across two lines collapses to `{}`; a rest parameter
never gets a trailing comma (ES forbids it).

#### US-044 Chains, binary breaking and ternaries
§9.3, S010 · After: US-043 · Size: M · M0

**Done when**
- A chain of three or more calls breaks before each `.`, one call per line, regardless of
  width (S010 when not); two calls stay on one line if they fit.
- Binary expressions break after the operator with indented continuation; ternaries break
  before `?` and `:` with both branches indented.
- `?.` segments count as links; `a[0].b().c()` counts `.b()` and `.c()`.

**Tests.** Edge: a chain starting with `new`; a chain interrupted by `await`; nested
ternaries; a binary whose operands are broken chains; `a.b.c.d` (property accesses, not
calls) stays on one line.

#### US-045 Import layout and ordering
§9.1, S003 · After: US-043, US-021 · Size: S · M0

**Done when**
- **Decision (order):** imports sort by depth (number of `../` segments descending, then
  `./`), then by path bytes; specifiers sort by imported-name bytes; the JSON `with`
  clause does not affect order.
- One S003 per import block with the whole block as replacement; comments attached to an
  import move with it.

**Tests.** Edge: `./a.js` vs `./a/b.js` vs `../a.js`; two imports from one path (kept as
two: merging is semantic, not formatting); `{ b as a, a as b }` sorts by imported name;
a JSON import among `.js` ones; a namespace import among named ones.

#### US-046 Comment placement
§9.5, S006 · After: US-042 · Size: M · M0

**Done when**
- `//x` → `// x`; `a = 1 // x` → `a = 1  // x`; a comment between two statements stays
  between them; a comment inside an argument list stays inside it when the list breaks.
- Doc comment prose is reflowed to 100 columns preserving paragraphs, fenced blocks
  (`@example`) verbatim and tag lines unbroken.
- **Decision:** a `//` comment whose text begins with `/` or `*` (rulers, ASCII art) is
  left untouched.

**Tests.** US-009 gains a comment-insertion mode; snapshots with comments in every
syntactic position. Edge: an over-width comment is S006 with no replacement; a block
comment mid-expression; a comment after the last statement without newline; a doc
comment line that is only `*`.

#### US-047 Class member ordering
§9.4, S009 · After: US-042, US-025 · Size: S · M0

**Done when**
- Order: static fields, static methods, instance fields (public then `#private`),
  constructor, accessors, methods (public then `#private`); source order within a group;
  comments move with their member. **Decision:** `static {}` blocks keep their position
  relative to static methods.
- One S009 per class with the whole-body replacement.

**Tests.** Edge: a getter/setter pair stays adjacent; a static private method; a class
already in order yields nothing; a one-member class.

#### US-048 S-code reporter
§8.5, §9.6, §13.4 · After: US-041 … US-047, US-031 · Size: M · M0

As a programmer running `sjs lint` I want each formatting deviation reported as the right
S-code with file, line, column, current text and replacement, and as a unified diff under
`sjs format`.

**Scope.** Token-granularity diff of the exact-printed source against the formatted
output; each formatting rule tags its IR with its code so hunks classify; S006/S007/S008
for pure whitespace hunks; S002 from US-031.

**Done when**
- Every code S001–S012 is produced by a fixture with exact before/after text; an
  unclassified hunk is a test failure.
- The unified diff applies cleanly with `patch -p0` to reproduce `--write` output.

**Tests.** Fixture per code; the apply test on the corpus. Edge: a clean file produces no
findings and no diff; only a missing final newline produces one S008; a hunk mixing two
codes is split into two findings.

#### US-049 `sjs format` command
§13.4, §13.1 · After: US-048, US-006 · Size: S · M0

**Done when**
- Exit code equals changed-file count capped at 255; `--check` is the default spelled
  out; `--write` exits 0 after writing each file atomically (temp file + rename) with its
  mode preserved.
- **Decision:** `--write` never touches a file with any `E000`/`E001` (it cannot format
  what it cannot parse safely); the file is reported, skipped and counted.
- S020/S021/S030 are never applied (the test is added now and stays).

**Tests.** Integration on temp trees. Edge: a read-only file under `--write` is detected
before any write and exits 70 having written nothing; a file given twice; a file outside
the graph is never touched.

#### US-050 Idempotence and equivalence proof
§11.2 M0, §9 · After: US-049 · Size: S · M0

**Done when**
- `fmt(fmt(x)) == fmt(x)` and `strip(parse(fmt(x))) == strip(parse(x))` (strip removes
  trivia and redundant parentheses) hold on the 10 000 corpus (with comments) and all
  fixtures, as a CI test.
- A `fuzz_format` test asserts no panic and, when the input had no `E000`, both
  properties.

**Tests.** As described. Edge: the corpus is regenerated with the comment mode and the
seed recorded; any shrunk counter-example is committed as a fixture.

---

### Epic 4 · Binder

#### US-060 Module graph loader
§5.1, §5.3 · After: US-021, US-006 · Size: M · M1

As every whole-program pass I want the transitive closure of the root's imports loaded,
parsed and keyed by canonical path, so the program is a closed world.

**Scope.** Resolve specifiers against the importing file's directory; canonicalise (`.`,
`..`, symlinks) so one file has one identity; `E205` for a missing, unreadable or
directory target; load JSON modules (malformed JSON is `E205`); include `import()`
literal targets; deterministic module order by canonical path; complete the explicit-
entry test discovery of §5.3 (every `*.test.js` in the directory of each graph module).

**Done when**
- **Decision:** on a case-insensitive filesystem a specifier whose case differs from the
  directory entry is `E205`, so builds stay portable to Linux.
- The same file reached by two spellings is loaded once.

**Tests.** Temp-directory tests. Edge: a self-importing root terminates (E203 in US-061);
10 000 modules load in linear time; a JSON import of an empty file; a file reachable only
via `import()`; test discovery with an entry outside `./src`.

#### US-061 Import cycles
§5.1 · After: US-060 · Size: S · M1

**Done when**
- Each elementary cycle is reported once as `E203` at the import that closes it, listing
  the cycle from the lexicographically smallest path.
- **Decision:** dynamic `import()` edges participate (§5.1 "the graph stays static").

**Tests.** Edge: self-import; two-module cycle; two overlapping cycles sharing an edge
(both reported, no duplicate); a cycle through `export { a } from`; a large DAG with no
false positive.

#### US-062 Unreachable modules
§5.3 · After: US-060 · Size: S · M1

As a programmer I want every `.js` file beneath `./src` that the root does not reach
reported as `E207`, so what is on disk and what is the program never differ.

**Done when**
- `E207` at 1:1 of each unreachable non-test `.js` file; `*.test.js` files are exempt;
  JSON files are exempt (the rule names `.js`).
- **Decision:** the scan root is `./src` when it exists, regardless of where explicit
  entries live; with no `./src` nothing is scanned.

**Tests.** Edge: a file reachable only from a test file (still E207: tests are roots only
for `sjs test`, and §5.3 says the *root's* imports; tested and recorded as a
**Decision** that `sjs test` does not change E207); a file reachable only via
`import()` (reachable); a dotfile `.hidden.js` (scanned: no ignore rule).

#### US-063 Scopes, declarations, hoisting and TDZ
§4.2, §4.4, §8.1 E135 · After: US-023 … US-027 · Size: M · M1

**Scope.** Module, function, arrow, block, `for`, `for…of`, `switch`, `catch` and class
scopes (class name binding, private names); function hoisting to its block; parameter
scope distinct from body scope; references resolved to declarations, to ambient globals
(US-068) or collected as unresolved; duplicate declarations in one scope are `E000`; TDZ
reads are `E135`.

**Done when**
- `E135` fires for a `const` read before its declaration, including through a function
  declared above and called before it, and not when the call is after.
- Every binding kind in the grammar is represented with a reference map.

**Tests.** Edge: a default referencing a later parameter (E135); a class referenced in
its own static initializer (fine) and in its own `extends` (E135); each `case` block is
its own scope; a function declared in a block is invisible outside it.

#### US-064 Block shadowing
§4.2 · After: US-063 · Size: S · M1

**Done when**
- `const x = 1; { const x = 2 }` is `E107`; a nested function or arrow may shadow; a
  `for (const x of …)` inside a function whose parameter is `x` is `E107`.

**Tests.** Edge: shadowing an import; `catch (e)` inside a function already binding `e`
(E107); a class method shadowing a module binding (permitted); a global like `fetch`
declared as a `const` at module level is not shadowing a binding (US-068 handles it).

#### US-065 Unused bindings
§8.1 E115, §8.3 E153 · After: US-063 · Size: S · M1

**Done when**
- A binding never read is `E115` (imports, variables, parameters, catch bindings,
  functions, classes, destructured elements); `_x` never read is fine; `_x` or `_` that
  is read is `E153`; exported bindings count as read; a write is not a read; a read in
  the binding's own initializer does not count.
- A parameter before a used parameter can be unused (`(a, b) => b` reports `a`).

**Tests.** Edge: an unused rest element; an unused catch binding (use `catch {}`); a class
used only in `instanceof` (read); a function used only as a value (read).

#### US-066 Reassignment analysis
§4.2 E106, §8.1 E123, E138 · After: US-063 · Size: S · M1

**Done when**
- `let x = 1` never reassigned is `E106` with replacement `const`; assignment in a nested
  function counts.
- `for (let x of xs) { x = 1 }` and `for (let i = 0; …; i += 1) { i += 1 }` are `E123`;
  the header update is not; `for (const x of xs) { x = 1 }` is `E138`.
- Assignment to a `const`, import, `function` or `class` binding is `E138` (§8.1).

**Tests.** Edge: compound assignment and `??=` count as reassignment; `[a, b] = [b, a]`
counts for both; a `let` reassigned only in dead code still counts here (E117 is the
checker's); a loop variable modified in a nested arrow inside the body is `E123`.

#### US-067 Export and import resolution
§5.1 · After: US-060, US-063 · Size: S · M1

**Done when**
- `import { a }` binds to `a`'s declaration through `export { a } from` chains; an
  unexported name is `E206`; `ns.b` and `ns["b"]` for an unexported `b` are `E206`;
  `ns[x]` with a non-literal is `E137` (a namespace has literal keys only, §4.6).
- A JSON default import binds to a value of type `unknown` (typing in US-083);
  `import()` resolves to a namespace.

**Tests.** Edge: re-export with rename; a chain of depth 5; `import { a } …; export { a
as b }`; exporting a `let`.

#### US-068 Ambient globals table and E204/E133
§5.2, §6, §10.1 · After: US-063 · Size: L · M1

As a programmer I want exactly the §6 intersection visible as globals, every member outside
it reported, and the test globals visible only in `.test.js` files.

**Scope.** `ambient/globals.toml`: each global, its kept members, its removed members and
per-runtime presence for browser, Node 24+, Deno 2+, Bun and workerd, transcribed from §6
with the ES2025/WinterCG source cited per row; the binder resolves references against the
*intersection* (there is no target selection); a global or member absent on any one
runtime is `E204` naming the runtime; a member in the removed column is `E133` with the
§12 replacement; `describe`/`it`/`expect` and friends resolve only in `*.test.js`.

**Done when**
- Fixtures per name are generated from the table so table and fixtures cannot drift.
- `Temporal` is `E204` (§6.1) until the table marks it present everywhere.
- Reassigning an ambient global is accepted only in a `.test.js` file (type constraint in
  US-124) and is `E138` elsewhere.

**Tests.** Edge: `Uint8Array.fromBase64` per the table; `navigator.userAgent` (E133) vs
`navigator.hardwareConcurrency`; `console.trace` (E133: not listed); `globalThis.x`
(E133); `undefined`, `NaN`, `Infinity` resolve; `Symbol()` is E133 while
`Symbol.iterator` is fine; `encodeURI` E133 while `encodeURIComponent` is fine.

#### US-069 `this`, `super`, defaults and E126
§4.4, §4.5 · After: US-063, US-025 · Size: S · M1

**Done when**
- `this` at module level, in a `function` declaration, or in an arrow outside a class is
  `E126`; in an arrow inside a method it is fine; in a `function` declared inside a
  method it is `E126`.
- **Decision:** a parameter default referencing `this` is `E126` (one code for every
  misplaced `this`).
- `this` textually before `super()` in a derived constructor is `E000` here; US-110
  refines to all paths.

**Tests.** Edge: `this` in a static block and in an arrow inside a field initializer
(fine); a nested class inside a method restores `this`.

#### US-070 Module purity and evaluation order
§5.1, §7 · After: US-063, US-067 · Size: S · M1

**Done when**
- A module is pure when its top-level items are only imports, exports and declarations
  whose initializers are literals, arrows, classes without static blocks, or
  object/array literals of such; any top-level call, `await`, assignment, `new`, or
  initializer containing a call makes it impure, recording the first impure span.
  **Decision:** `Object.freeze(<literal>)` is a call and therefore impure.
- Evaluation order is ES module order (post-order DFS from the root in import order),
  stable across runs.

**Tests.** Edge: a class with a `static {}` block is impure; dynamic `import()` does not
affect order; a JSON module is pure.

---

### Epic 5 · Types and inference

#### US-080 Type representation and normalization
§3.1, §3.2, §11.1 · After: US-003 · Size: M · M1

As the inference and checker passes I want interned, immutable types with unions kept as
normalized sets, so equality is pointer equality and every later rule is written once.

**Scope.** `sjs-types`: `number`, `int32`, `bigint`, `string`, `boolean`, `undefined`,
`null`, `unknown`, `never`, literal types, object shapes (ordered key → type with a
"literal retained" flag per key), dictionaries (§4.6), arrays (element, mutated flag),
tuples, functions (params with optional/rest, return, type params), class refs (nominal
id), type variables, `Promise<T>`, generator/iterator types, builtin generics, union; a
normalizer that flattens, dedupes, absorbs a literal into its present primitive, absorbs
`int32` into `number`, orders members canonically and collapses singletons.

**Done when**
- `norm(a | b) == norm(b | a)`; `norm("a" | string) == string`; `norm(int32 | number) ==
  number`; `norm(never | T) == T`; `norm(T | T) == T`; `norm(unknown | T) == unknown`.
- Interning gives one id per structurally equal type (property test).

**Tests.** Property tests over a type generator added here. Edge: a union of 1 000
literals; key order does not change a shape's identity; tuple vs array differ; optional
param vs `T | undefined` param differ; `null` never absorbs into `undefined`.

#### US-081 Type printer
§13.7 notation, §10.6 · After: US-080 · Size: S · M1

**Done when**
- The two §13.7 examples print exactly; unions in canonical order with ` | `; literals as
  JS literals; classes by name; generics `<T>(x: T): T`; optional `x?: T`; rest `...xs:
  T[]`; tuples `[number, string]`; arrays `T[]`; functions in unions parenthesised;
  dictionaries `{ [key: string]: T }`.
- Printing is total and stable.

**Tests.** Snapshot per constructor; property test that printing is injective on the
interned set. Edge: a self-referential class prints by name; an empty `{}`; a key that is
not an identifier prints quoted.

#### US-082 Assignability
§3.3 · After: US-080 · Size: M · M1

**Done when**
- Structural for objects (width), arrays (invariant when mutated anywhere, covariant
  otherwise), tuples (same length, elementwise), functions (contravariant params incl.
  optional/rest, covariant return); nominal for classes; `int32 <: number`; literal <:
  primitive; a union source needs every member assignable; a union target accepts any
  member; `unknown` accepts everything and is assignable only to `unknown`; `never` to
  everything; `null` and `undefined` distinct.
- E101 is not decided here (needs call-site context, US-120).

**Tests.** A pairwise matrix over ~40 representative types reviewed in the PR; property
tests for reflexivity and transitivity. Edge: method parameter contravariance; an array
mutated in one module is invariant program-wide; a tuple is assignable to an array of
its element union only when the array is unmutated; fewer params assignable to more.

#### US-083 Literal and primitive expression typing
§3.1, §4.1, E100, E125 · After: US-080, US-029 · Size: S · M1

**Done when**
- Integer literals in [−2³¹, 2³¹) are `int32` literals (including `1e3`, `1.0`,
  `-2147483648` with the folded unary minus); other numbers `number`; `1n` `bigint`;
  **Decision:** `-0` is `number`.
- `null` anywhere but as the right operand of `===`/`!==` is `E100` (`return null`,
  `const x = null`, `f(null)`, `[null]`, `{ a: null }`, `null === x`).
- Template substitutions must be `string | number | bigint | boolean` (or a union of
  those), or an object whose `toString()` the checker sees returns `string`; otherwise
  `E125`; the result is `string`.
- A JSON default import and `JSON.parse` results are `unknown`.

**Tests.** Edge: `` `${x ?? "d"}` `` with `x: string | undefined`; a class with
`toString(): string` in a template (fine); an array in a template (E125); `0x7fffffff`
is int32; `2147483648` is number.

#### US-084 Binding types and destructuring
§3.2, §12 (`let x = undefined`) · After: US-083, US-063 · Size: M · M1

**Done when**
- `const x = 1` is literal `1`; `let x = 1` is `int32`, `number` after a non-int32
  assignment; `let x = undefined; x = "a"` is `undefined | string`.
- Destructuring from shapes, tuples, arrays (`T | undefined` elements), with defaults
  unioned and rest producing the remaining shape or an array.

**Tests.** Edge: `const { a = 1 } = { a: "x" }` is `string` (the default is dead, no
error here); destructuring a class instance's public fields; a `let` assigned `x + 1` in
a loop stays `number`.

#### US-085 Object literal typing
§3.2, §4.6 · After: US-084 · Size: M · M1

**Done when**
- `{ kind: "circle", r: 1 }` retains literal types for its keys; properties widen when
  the object is assigned to a `let` or mutated.
- `{ ...a, ...b, c }` merges structurally, later wins; spreading a union spreads each
  member.
- A computed key must be an inferred literal (`E000` otherwise, §4.6); a computed literal
  key equal to another key is `E134` (completing US-030).
- A namespace import has the shape of its module's exports.

**Tests.** Edge: spread of `undefined` (accepted, contributes nothing); spread of a class
instance copies public fields; **Decision:** `{ ...[1] }` is `E138`; shorthand of an
undefined binding; nested literal retention.

#### US-086 Property reads and writes
§3.2, §4.6, E137, E129, E121 · After: US-085 · Size: M · M1

**Done when**
- `o.k` is `E137` unless every member has `k`; on a union the result is the union of the
  members' `k`; `o?.k` on a nullish-including receiver yields `T | undefined`, on a
  non-nullish receiver is `E121` (§8.1).
- `o[k]` with `k` an inferred literal or a union of literals every member of which is a
  key is the union of those value types; `Object.keys(o)` / `Object.entries(o)` on a
  shape yield that literal union (and `[K, V]` tuples); any other computed read is
  `E137`.
- `o.k = v` requires `k` present and `v` assignable (`E138`); `o.newKey = v` is `E129`.
- **Decision:** reading a class method as a value (`o.m` without a call) is `E138`
  "method used as value" (there is no `.bind`, §12).

**Tests.** Edge: `a?.b.c` typing through the short-circuit group; `.length` is `int32`;
writes through a union receiver require the key on every member; `o[k]` where `k` is
`"a" | "z"` and `z` is not a key (E137 naming `z`).

#### US-087 Arrays, tuples and empty collections
§3.2, §4.6, E110, §3.4 · After: US-084 · Size: M · M1

**Done when**
- `[1, "a"]` is `(int32 | string)[]`; a `const` `[1, 2]` never mutated and never passed
  to a mutating callee is tuple `[1, 2]`, else `int32[]`.
- An empty `[]`, `new Map()` or `new Set()` is typed from `push`/`unshift`/index
  assignment, `set`, or `add` within the binding's scope (function body for a local,
  class body for a field, module for a top-level `const`); none is `E110
  empty-untyped` at the literal.
- Mutating methods (`push pop shift unshift splice sort reverse fill copyWithin`, index
  assignment, `set`/`add`/`delete`/`clear`) mark the value mutated; passing it to a
  parameter mutated in the callee marks it mutated (whole-program fixpoint).
- `arr[i]` without proof is `T | undefined` (proofs in US-115); `.length` is `int32`;
  `arr.length = 0` is `E133`.

**Tests.** Edge: a `push` in a nested arrow inside the same function (counts: it is in
the function body's scope); a field `#items = []` filled in a method (counts: class
body); `[[]]` (E110 on the inner); `xs.push()` with no arguments does not type; an array
mutated only in a test file is mutated program-wide.

#### US-088 Operators
§4.1, E104, E105, E120, E138 · After: US-083 · Size: M · M1

**Done when**
- Every row of §4.1 has accept fixtures per permitted operand kind and reject fixtures
  per mismatch (`E138`, or E104 for bitwise on non-`int32` numbers, E105 for disjoint
  `===`); `%` on `int32×int32` is `int32`; `>>>` on `int32×int32` is `number` and on
  `bigint` is `E138`; `+ - * / **` on `int32` operands are `number`.
- `&&`/`||`/`!` boolean only; `??` needs a nullish-including left side and yields the
  union minus nullish; `?:` needs a boolean condition; `typeof x` is the literal union
  §3.1 allows for `x`; `instanceof` needs a class on the right.
- `x === x`, `x = x`, `a.b = a.b` are `E120`.

**Tests.** The operator × operand-kind matrix as generated fixtures. Edge: `"a" + 1`,
`1 + 1n`, `-"1"` are E138; `x === undefined` with `x: number` is E105; `typeof x ===
"symbol"` is E105 (no symbol values exist); `a === b` with `1 | 2` and `2 | 3` overlaps;
`(a + b) & 1` with int32 `a`, `b` is E104 (the sum is `number`).

#### US-089 Function signatures from bodies and call sites
§3.2, §4.4, E111, E136, E139 · After: US-088, US-067 · Size: L · M1

**Scope.** Program-wide call-site collection per function (declaration, arrow, method,
constructor) before bodies are checked; parameter type = union of argument types at all
sites plus the default's type; optional when some site omits it; rest as arrays; return
type = union of `return` expressions or `undefined`; `E111` for mixed returns; recursion
solved by fixpoint with a terminating widening; `E139` for calling a non-function or
`new` on a non-class; `E136` for argument counts outside `[required, declared + rest]`.

**Done when**
- The §13.7 `greet` and `parse` examples infer the printed signatures.
- A function called with `string` and `number` from two modules has a `string | number`
  parameter; a body use the union cannot take reports `E138` with both call sites as
  related spans.

**Tests.** Edge: an uncalled, unexported function is still inferred; a call through a
union of two functions; a callback invoked by a builtin is a call site typed by the
builtin's signature; a function that only throws returns `never`; a recursive return
type; 10 000 call sites resolve in linear time.

#### US-090 Generalization and instantiation
§3.2, §4.4, E112 · After: US-089 · Size: M · M1

**Done when**
- A parameter is opaque when every use is: passed through, returned, stored (field,
  array element, `Map`/`Set` key or value), or compared with `===`/`!==` (§3.2); any
  property read, operator, call or narrowing constrains it.
- `export const id = (x) => x` is `<T>(x: T): T`; a local uncalled `(x) => x` is `E112`;
  `(xs) => xs[0]` is `<T>(xs: T[]): T | undefined`; two instantiations stay distinct; a
  generic bound to a `let` is monomorphic.

**Tests.** Edge: `compose(f, g)`; a generic storing into a `Map` (opaque); `<` use pins
to `number | bigint | string` then to call sites; passing to a non-generic function
constrains by its parameter type.

#### US-091 Contextual typing of callbacks
§3.2 "bidirectional" · After: US-090 · Size: M · M1

**Done when**
- Arrows passed as arguments, stored in typed slots, or returned where a function type is
  expected take parameter types from the expectation; otherwise US-089 applies.
- A use the expected type cannot support is `E137`/`E138` at the use with the
  expectation's origin as a related span.

**Tests.** Edge: an expected union of function types; `xs.map(id)` with a generic
callback; fewer params than expected (fine) or more (E136); nested callbacks.

#### US-092 Classes
§4.5, E114, E129, E138 · After: US-090, US-069 · Size: L · M1

**Done when**
- Fields typed from initializers or constructor assignments; fields without initializer
  must be assigned in the constructor (straight-line here, all-paths in US-110):
  `E114`.
- `this.x = v` for undeclared `x` is `E129`; accessors: setter must accept the getter's
  type (E138); a getter without setter is read-only (E138 on write).
- `extends`: base must be a class; an override must be assignable to the base method
  (contravariant params, covariant return) else `E138` (§8.1); `super(...)` is a call
  site of the base constructor; static members live on the class value (`A.x`
  undeclared is `E137`).
- Generic classes (`class Box { constructor(v) { this.v = v } }` → `Box<T>`).

**Tests.** Edge: a field explicitly `= undefined` later assigned `T` is `T | undefined`;
a field assigned only in a method (E114); `this.#m()`; a class used as a value has the
constructor's function type; a class with `toString(): string` in a template.

#### US-093 Iteration protocol typing
§4.7, §3.1 · After: US-092 · Size: M · M1

**Done when**
- Arrays yield `T`; strings `string` (code points); `Map` `[K, V]`; `Set` `T`;
  generators the union of `yield`s (with `yield*` delegation); a class with
  `*[Symbol.iterator]()` is iterable with that element type; a non-iterable in `for…of`
  is `E138`; `for await` accepts async and sync iterables.
- Iterator helpers and `Iterator.from` are typed on every iterator type.

**Tests.** Edge: `for (const [k, v] of map)`; spreading a string gives `string[]`; a
generator yielding nothing has element `never`; an async generator's `return` value;
`Array.from(iterable)`.

#### US-094 Async typing
§4.4, E113, E118, E119, E127, E128 · After: US-089 · Size: M · M1

**Done when**
- `async` functions return `Promise<R>`; `await p` is `R`; `await` of a non-promise (or a
  union with a non-promise member) is `E119`; top-level `await` is accepted.
- **Decision (E113 scope):** reported at the expression whose `Promise` value is
  discarded: an expression statement, an unused array element, an argument to a parameter
  the callee never awaits, or a `for…of` body expression; `Promise.all/allSettled/any/
  race` arguments and returns count as use.
- **Decision (E118 scope):** an `async` function or arrow whose body contains no `await`
  (an async generator with `yield` but no `await` included); top-level `await` never
  triggers it.
- `new Promise(…)` is `E127`; `.then`/`.catch`/`.finally` on a `Promise` is `E128`;
  `Promise.all` is tuple-preserving; `withResolvers` and `try` typed.

**Tests.** Edge: `await` of `Promise<Promise<T>>` is `T`; a promise stored then awaited
later is not E113; a promise returned from an arrow body is used; a user class with a
`then` method is not E128 and `await` unwraps it.

#### US-095 Built-ins: `Object` and `Array`
§6.1 · After: US-093 · Size: M · M1

**Done when**
- `Object.keys/values/entries/fromEntries/hasOwn/groupBy/freeze/isFrozen/assign` typed
  (`fromEntries` and `groupBy` produce dictionaries, §4.6; `hasOwn` narrows in US-121;
  `freeze` returns the frozen type, US-119); every ES2025 `Array.prototype` method
  typed incl. `at`, `findLast`, `toSorted`, `toReversed`, `toSpliced`, `with`, `flat`
  (depth-typed to a fixed bound), `flatMap`.
- Removed `Object` members are `E133`; `sort()`/`toSorted()` without a comparator on a
  non-`string[]` is `E133` (§8.1).

**Tests.** Generated from the ambient table. Edge: `arr.indexOf(x)` with `x` outside the
element type (E138); `arr.at(-1)` is `T | undefined`; `Array.isArray` narrows (US-111);
`Object.assign` with an excess key (E101, US-120).

#### US-096 Built-ins: `String`, `Number`, `Math`, `BigInt`, `JSON`, `structuredClone`
§6.1 · After: US-095 · Size: M · M1

**Done when**
- All kept members typed; `Number.parseInt` without a radix, global
  `parseInt/parseFloat/isNaN/isFinite`, `substr`, `charCodeAt`, `fromCharCode`, HTML
  wrappers, `localeCompare` without an `Intl.Collator`, and `JSON.stringify` with a
  replacer are `E133`; `JSON.parse` is `unknown`.
- **Decision:** `structuredClone` of a class instance or function is `E138` (the clone
  loses its prototype); of a plain shape, array, `Map`, `Set`, `Date` it is identity.
- **Decision:** `BigInt(x)` as a call is `E133` (only `asIntN`, `asUintN`,
  `toString(radix)` are listed); write `1n` literals.

**Tests.** Edge: `Math.trunc(x)` is `int32` only when provable (US-116);
`Number.isInteger(x)` does not narrow to `int32`.

#### US-097 Built-ins: `Map`, `Set`, `WeakMap`, typed arrays
§6.1 · After: US-087, US-093 · Size: M · M1

**Done when**
- `Map<K, V>` and `Set<T>` inferred from constructor arguments and from `set`/`add`
  within the binding's scope (US-087); `get` is `V | undefined`; ES2025 set methods and
  `Map.groupBy` typed; `WeakMap` keys must be object types; `WeakSet`, `WeakRef`,
  `FinalizationRegistry` are `E133`.
- Typed arrays, `ArrayBuffer`, `DataView`, and the base64/hex members per the table.
  **Decision:** typed-array indexing is `T | undefined` with the same proofs as arrays.

**Tests.** Edge: a `Map` keyed by a literal union; `map.get(k) ?? d`; iterating a
`WeakMap` is E137.

#### US-098 Built-ins: `RegExp`, `Error`, `Date`, `Intl`, `Temporal`, `Symbol`
§6.1, §2 · After: US-016, US-093 · Size: M · M1

**Done when**
- A regex literal's type carries flags and named groups; `exec` returns a match type
  `| null` with `groups` typed `string | undefined` per group; `test` boolean;
  `matchAll`/`replaceAll` flag checks are US-117; `new RegExp`, `lastIndex` assignment,
  `compile`, legacy statics are `E133`.
- `Error` family: `message`, `cause`, `name`, `stack`, `new Error(msg, { cause })`,
  `AggregateError`; `Error.captureStackTrace` E133.
- `Date`: kept forms only; `new Date()` with no argument, setters, local getters E133;
  a non-ISO-8601 string literal argument is `E133`; a non-literal string is accepted and
  documented as the one unchecked runtime parse.
- `Intl` typed from a data file; `Temporal` E204 per the table; `Symbol` members are the
  four protocol keys only.

**Tests.** Edge: a named group inside an alternative; `d` flag exposes `indices`;
`str.replace(re, (m, g1) => …)` callback typing.

#### US-099 Platform globals from IDL
§6.2 · After: US-068, US-094 · Size: L · M1

**Scope.** `ambient/platform/*.toml` hand-transcribed signatures for every §6.2 global
with the IDL source cited; `null` kept where IDL is nullable; callbacks typed; `any` as
`unknown`. **Decision:** `setTimeout`/`setInterval` return an opaque handle type accepted
only by `clearTimeout`/`clearInterval` (runtimes disagree on the concrete type).

**Done when**
- Every §6.2 global has a positive fixture per listed member and an E133 fixture for one
  unlisted member.
- Every IDL-nullable member is typed `| null` (a test greps the data file for the marker).

**Tests.** Edge: `headers.get("x")` must be narrowed; `fetch` with a `Request` vs a
string; `performance.now()` is `number`; `navigator.hardwareConcurrency` is `int32`;
`event.data` is `unknown`.

#### US-100 Determinism and `types.json`
§3.2, §7, §11 · After: US-099 · Size: S · M1

**Done when**
- Shuffling module discovery order 100 times on a 50-module program yields byte-identical
  `types.json`.
- `types.json` has a documented schema (`schema/types.schema.json`, kept for consumers)
  and an in-house reader in `sjs-diag` that round-trips the writer's output (no validator
  crate): per module, per export, the §13.7 notation and a structured form; it is produced by `lint` and `build` before the checker runs
  (§11) and even when a module has `E000` (that module's exports are `unknown` and
  marked).

**Tests.** The shuffle test; schema validation. Edge: an uncalled generic export prints
as generic; an empty program produces a valid document.

---

### Epic 6 · Checker

#### US-110 Control-flow graph and reachability
§11.1, E117, E108, E109, E114 · After: US-092 · Size: L · M2

**Scope.** Per-function CFG for every statement form, short-circuit operands, `?:`,
`?.` groups, `switch`, `try/catch/finally` (finally-on-every-exit edges), loops with
`break`/`continue`, `throw`, `return`, `await`/`yield`; dominance; reachability.

**Done when**
- `E117` for code after `return`/`throw`/`break`/`continue`, after a loop that cannot
  exit, and inside a branch whose condition is literally `false` (E121 also fires); not
  for a hoisted function declaration.
- `E108` when a case block's end is reachable and it is not the last clause.
- `E109` for a `return` on any path of a `finally` block.
- `E114` refined to "assigned on every constructor path"; `super()` before `this`
  refined to all paths (US-069).

**Tests.** DOT snapshot of the CFG per statement form. Edge: `while (true)` with a
`break` bound to an inner `switch` (loop cannot exit); `try { return } finally {}` then
code (E117); a `finally` that throws is permitted; a case block ending in `if`/`else`
where both return; a field assigned inside a loop (not all paths); E117 once per
unreachable region.

#### US-111 Narrowing core
§3.5 · After: US-110 · Size: L · M2

**Done when**
- `typeof x === "<kind>"` (also `!==` and `switch (typeof x)`), `x === undefined`, `x ===
  null`, `x === <literal>`, discriminant `x.kind === "circle"`, `x instanceof C`,
  `Array.isArray(x)`, `Number.isNaN(x)` narrow in the true branch and remove in the
  false branch; `!`, `&&`, `||`, `?:` compose; early `return`/`throw`/`continue`/`break`
  narrow the rest of the block.
- Narrowing of a `let` is invalidated by any assignment the CFG reaches between
  narrowing and use; **Decision:** a `let` captured and assigned by any closure is never
  narrowed across a call; `const` bindings and properties of never-mutated objects keep
  narrowing across calls; properties of mutated objects lose it at any call.
- Narrowing applies to property paths and to proven array elements (US-115).

**Tests.** A form × position × negation matrix as generated fixtures; the §6.3 idiom.
Edge: `typeof x === "object"` leaves `null`; a union where one member's `kind` is
`string` (no discriminant; E137 on the read); `x === y` narrows both to the
intersection; narrowing in a loop condition; narrowing through a `const` alias of
`typeof x`; reset at the loop back-edge for a `let` assigned in the body.

#### US-112 Truthiness, boolean conditions and constant conditions
§3.5, §4.3, E102, E121 · After: US-111 · Size: S · M2

**Done when**
- `if`, `while`, `?:`, `!`, `&&`, `||` require `boolean`; on `T | undefined` where `T`
  has no falsy member (`""`, `0`, `0n`, `false`, `NaN`) the condition narrows; otherwise
  `E102` with the suggested comparison.
- `E121` for `if (true)`, `if` on a `const` whose literal type is `false`, `while (true)`
  whose body contains no `break`, `return` or `throw` (§4.3), a condition already
  established by an enclosing narrowing, and `?.`/`??` on a never-nullish value.

**Tests.** Edge: `if (xs.length)` is E102 with replacement `xs.length > 0`; `if (s)` for
`string | undefined` is E102; `if (obj)` for `Shape | undefined` narrows; `while (true)`
with a `break` inside a nested `if` is fine; `while (true)` whose only exit is a
`break` bound to an inner `switch` is E121.

#### US-113 Predicate functions
§3.5 · After: US-111 · Size: S · M2

**Done when**
- A function whose body is exactly one `return <narrowing expression over a parameter>`
  narrows that argument in conditions, including negation and composition; a predicate
  built from `Object.hasOwn` and `typeof` chains narrows `unknown` to the checked shape.
- Any other body is not a predicate (no narrowing, no error).

**Tests.** Edge: a predicate over the second parameter; a predicate calling another;
**Decision:** a predicate with a default parameter is still a predicate; a predicate
passed to `filter` narrows the result array's element type.

#### US-114 Exhaustiveness
§3.5, §4.3, E103 · After: US-111 · Size: M · M2

**Done when**
- `switch` on a literal union, `boolean`, or discriminant: every member covered or a
  `default`, else `E103` listing the missing members; **Decision:** a `default` on an
  already-exhaustive switch is `E117` ("default unreachable").
- A `case` label not in the scrutinee's union is `E105`; an open `string`/`number`
  scrutinee without `default` is `E103`.
- An `if`/`else if` chain whose branches all return and whose conditions narrow one
  binding is checked the same way; after an exhaustive switch or chain the binding is
  `never` and any use is `E117`.

**Tests.** Edge: stacked labels covering a union; two shapes sharing `kind: "a"`; a chain
with `return` in one branch and `throw` in another (both abrupt).

#### US-115 Array index proofs
§4.6, E124 · After: US-111 · Size: M · M2

**Done when**
- `arr[i]` is `T` when (a) `i` is the variable of an enclosing `for (let i = 0; i <
  arr.length; i += 1)` over the same `arr`, (b) the read is dominated by `i >= 0 && i <
  arr.length` (or `i < arr.length` with `i` provably non-negative), or (c) `arr` is a
  tuple and `i` a literal in range; otherwise `T | undefined`.
- **Decision:** a mutation of `arr` between proof and read cancels the proof.
- An unproven read used without narrowing is `E124` at the use, naming the narrowing to
  add; a narrowed unproven read is fine.

**Tests.** Edge: `xs.push(1); xs[i]` inside the loop (E124); `i <= xs.length` (not a
proof); `xs.length > i` (reversed, a proof); `i < ys.length` then `xs[i]` (different
array); `const [a, b] = pair` from a 2-tuple (proven); `xs[xs.length - 1]` (not proven;
message suggests `at(-1)`); a non-mutating call between proof and read keeps it.

#### US-116 `int32` refinement
§3.5, §4.1, E104 · After: US-088 · Size: S · M2

**Done when**
- `int32` is inferred exactly for in-range integer literals, `.length`, array indices
  from `indexOf`/`findIndex` and loop variables under US-115, `Math.trunc/floor/ceil/
  round` of a provably finite in-range argument (otherwise `number`), results of
  `& | ^ ~ << >>` and `%` on `int32` operands; everything else is `number`.
- Bitwise operators on a non-`int32` number are `E104` suggesting `Math.trunc(x)` or
  `bigint`.

**Tests.** Edge: `~~x` on a double is E104 at the inner `~`; `1 << 31` is int32
(negative); `arr.length * 2` is number; a `let` assigned only int32s stays int32; `i +=
1` keeps int32 only inside the US-115 loop form.

#### US-117 Regex flag-dependent checks and match typing
§2, E002, §6.1 · After: US-098, US-111 · Size: S · M2

**Done when**
- `matchAll`/`replaceAll` with a regex lacking `g` and `lastIndex` reads on a regex
  lacking `g`/`y` are `E002`; `exec`'s result must be narrowed with `=== null` before
  `.groups` or `[0]` (E137 otherwise); `m[0]` is `string`, `m[n]` for `n` ≤ group count
  is `string | undefined`.

**Tests.** Edge: a regex in a `const` used by both `test` and `matchAll`; flags travel
through a function parameter; `split` with `g` (fine); a `y` regex with `matchAll` (E002).

#### US-118 Errors and the result-object pattern
§3.6, E130, E131 · After: US-111, US-092 · Size: S · M2

**Done when**
- `throw x` where `x` is not an `Error` (or subclass or union of such) is `E130`; `catch
  (e)` binds `unknown`; any read or call on `e` before `instanceof` is `E131`; after
  `e instanceof Error`, `e.message` is `string`.
- The `{ ok: true, value } | { ok: false, error }` pattern is checked exhaustively by
  US-114 (end-to-end fixture).

**Tests.** Edge: `throw e` of an un-narrowed `unknown` `e` is accepted (rethrow); `throw
e` where `e` was narrowed to `string` is E130; a user class not extending `Error` is
E130.

#### US-119 Mutability, `Object.freeze`, S020
§3.4, E122, S020 · After: US-087, US-095 · Size: M · M2

**Done when**
- `Object.freeze(x)` returns the (shallow) frozen type and freezes `x` along the flow;
  any write or mutating call on a frozen value is `E122`.
- `S020` for a `const` bound to an object/array literal that is never mutated and never
  frozen, with the replacement shown and the `hand_edit` flag set.
- Passing a value to a function that mutates its parameter marks the argument mutated
  (whole-program, through unions and callbacks).

**Tests.** Edge: freeze then spread (the copy is mutable); freezing a class instance
(E122 on field assignment in a method); no S020 for a `const` passed to a mutating
function; **Decision:** a write through a nested array of a `const` object counts as
mutation for S020; `Object.isFrozen(x)` does not narrow.

#### US-120 Shape stability and excess properties
§3.3, §4.5, E101, E129 · After: US-089, US-092 · Size: S · M2

**Done when**
- `E101` when an object *literal* is passed directly to a parameter or stored in a typed
  slot whose receiver never reads one of its keys anywhere in the program; the message
  names the unread key and the nearest existing key as the likely typo.
- `E129` is complete: new keys on literals or instances; assigning `undefined` to an
  existing key is permitted (§12 replacement for `delete`).

**Tests.** Edge: a literal passed to a generic parameter (no E101); a key read only in a
test file (read); a key read via `Object.keys` iteration (all keys read); a literal
stored in an array of shapes (E101 applies); **Decision:** E101 is per receiver, so a
shape used at two parameters reports at the one that never reads the key.

#### US-121 Boundary narrowing of `unknown`
§6.3, §5.1, §6.2 · After: US-113 · Size: S · M2

**Done when**
- `typeof x === "object" && x !== null` narrows `unknown` to a dictionary of `unknown`;
  `Object.hasOwn(x, "k")` adds key `k: unknown`; `typeof x.k === "string"` narrows the
  key; `Array.isArray(x)` narrows to `unknown[]`; the composed helper is a predicate
  (US-113).
- **Decision:** any use of `unknown` other than narrowing, `===`, passing to an `unknown`
  parameter, or storing is `E138` (`String(x)` included, since it may call a user
  `toString`).

**Tests.** Edge: `"k" in x` is E000 with the `hasOwn` replacement; narrowing elements of
`unknown[]` in a loop; a JSON module whose file is an array; the `hasOwn` false branch
keeps the key absent.

#### US-122 Removed members and forms
§6, §12, E133 · After: US-095 … US-099 · Size: S · M2

**Done when**
- The generated fixture set covers every removed member once more at the checker level
  (members on values of built-in types: `str.substr`, `arr.length = 0`, `re.compile()`,
  `p.then`).

**Tests.** Edge: a member reached through a union of built-in types where it is removed
on one member; `Function` as a constructor; `eval` as a property name (`o.eval`, fine).

#### US-123 Statement-level checks
E116, E120, E132 · After: US-110 · Size: M · M2

**Done when**
- `E116` for an expression statement with no effect (bare identifier, literal, member
  read, comparison, arithmetic, arrow, object/array literal, template literal); not for
  calls, assignments, `await p`, `yield`, `i++` (S012), or `new C()` (**Decision:** a
  constructor may have effects).
- `E132` exactly as §8.1 defines it: a non-doc comment whose text (consecutive `//` lines
  joined, a leading `*` per line stripped) parses as a statement or declaration and
  contains at least one of `(`, `{`, `[`, `=`, `=>`; prose never qualifies.

**Tests.** Edge: `a?.b` statement (E116), `f?.()` (fine), `"use strict"` (E116: modules
are strict); `// return x` (no listed character: not E132); `// return f(x)` (E132);
`// const x = 1` (E132); `// see foo()` (does not parse: prose); a comment containing a
URL (prose); `// TODO: fix x = 1` (does not parse: prose).

#### US-124 Test-file typing
§10.1–10.4, E301–E304 · After: US-114, US-094, US-068 · Size: L · M2

**Scope.** Types for `describe`, `it` (+`.skip`/`.only`), hooks, `expect` and every
§10.2 matcher as a typed builtin table; per-matcher subject constraints (`toBeNull` only
on `| null`, `toBeTruthy/Falsy` only on `boolean`, `toHaveLength` needs `length: int32`,
`toMatch` needs a regex literal and a `string` subject, `toThrow` a zero-parameter
function, `resolves/rejects` a `Promise`, `toBeCloseTo` a `number`, `toContain` arrays
or strings with a matching element type, `toBe/toEqual/toStrictEqual` overlapping types
(E105), `toBeInstanceOf` a class); matchers are typed against the *narrowed* subject
(§10.4); an unlisted matcher is `E204`.

**Done when**
- Every matcher has accept and reject fixtures per constraint; `expect(x).toBe(1)` on
  `number | undefined` is `E138` and the narrowed form passes.
- `E301` for an `it` body with no reachable `expect(...)` (directly or through a helper
  in the test file); `E302` for `.only` under `--ci`; `E303` for an `async` body without
  `await`; `E304` for `it.skip`/`describe.skip` without a `//` reason on the same or the
  preceding line under `--ci`.
- Assigning an ambient global in a `.test.js` file requires an assignable value (`fetch
  = 1` is `E138`); restoring in `afterEach` is not enforced (not in the spec).

**Tests.** Edge: `expect(fn).toThrow(TypeError)` where `fn` takes a parameter (E138);
`it("name")` with no body (E136); nested `describe`; a test file importing another test
file (accepted).

---

### Epic 7 · Linter, reporter and build

#### US-130 Complexity and size limits
§8.2 · After: US-110 · Size: M · M3

**Done when**
- `E140` function-length, `E141` cyclomatic-complexity, `E142` nesting-depth, `E143`
  parameter-count, `E144` file-length and `E145` class-members fire with the measured
  value and the limit, using exactly the §8.2 measures (body lines excluding blank and comment-only; complexity 1 + each listed
  construct with nested bodies restarting; nesting depth with the body at 0; parameters
  with rest and destructured options counting one; file lines including blank and
  comment; class members including static blocks and `#private`).
- Each measure is a pure function with its own unit tests.

**Tests.** Generated boundary fixtures at limit, limit+1, limit−1 per metric. Edge: 60
body lines plus 10 comment lines (fine); a nested function's lines count toward the
outer's length (**Decision:** lines are lines); a `switch` with 12 cases (13: E141); a
class with 20 members plus a `static {}` block (21: E145); a 600-line file with no final
newline (600).

#### US-131 Naming rules
§8.3 · After: US-100 · Size: M · M3

**Done when**
- `E150`–`E153` exactly as §8.3 defines them, using inferred types to pick UPPER_SNAKE
  (module-level `const` whose type is a primitive or a literal of one) and to decide
  "exactly `boolean`" for E151 in both directions (`boolean | undefined` is not
  covered); `#private` names checked after the `#`.

**Tests.** A name × kind × type matrix. Edge: `const MAX = 1` (fine), `const max = 1`
(E150), `const CONFIG = {}` (E150), `const URL_RE = /x/v` (E150: an object), `let COUNT
= 0` (E150); `isReady = 1` (E151), `ready = true` (E151), `isNaN2` (fine); `my-module.
test.js` (fine), `MyModule.js` and `my_module.js` (E152); `widget.js` exporting `class
Widget` (fine), `widgets.js` exporting it (E152), `class HTTPClient` must be
`http-client.js` and `class XMLHTTPRequest` must be `xmlhttp-request.js` (split before
the last capital of a run).

#### US-132 Structure rules
§8.4 · After: US-067, US-017 · Size: S · M3

**Done when**
- `E160` for a module exporting a class with anything else or two classes; `E161` from
  US-022; `E162` for an exported declaration without a `/** */` block immediately above
  (a blank line breaks attachment); `E163` for a comment containing `TODO` without
  `#<digits>` or an `http(s)://` URL.

**Tests.** Edge: a class module also exporting a `const` (E160); a module of only
`const`s (fine); `export { a, b }` needs the doc comment on each declaration;
**Decision:** a re-export needs no doc comment; `// todo` lower-case (not matched);
`// TODO(#12)` and `// TODO see https://…` (fine), `// TODO 12` (E163).

#### US-133 Doc comments
§13.7, E401–E403, E301, S030 · After: US-132, US-089 · Size: L · M3

**Scope.** Markdown-light parser (paragraphs, fenced blocks, tag lines); `@param` ⇄
parameters, with `options` for a destructured parameter and optional `@param
options.key` lines whose keys must exist in the pattern; `@returns` iff a value is
returned; `@throws` via `throw new C` transitively through project functions in the call
graph; `@field` ⇄ public fields; `@example` extraction with the `*/` rule (E403), handed
to US-147, `E301` when it has no `expect`; `@see ./x.js#name` resolution; `@since` free;
`@deprecated` marks the export and every call site gets `S030` (hand-edit flag); module
comments.

**Done when**
- Every tag has accept and reject fixtures; `E401` for an unknown tag; `E402` for
  `@param` mismatches both ways, a missing `@returns` on a value-returning export, an
  extra one on an `undefined`-returning export, a `@field` naming a `#private` or missing
  public field, a `@see` to a non-export, a `@throws` class the body cannot throw.
- Non-exported doc comments never raise E402 for missing `@param`, only for wrong names.

**Tests.** Edge: `@param options.nested.key` (one level only: **Decision**, E402
beyond); an `@example` containing `*/` inside a string (E403 at the exact column); a
module comment followed by a blank line then `/**` (second attaches to the declaration);
an `@throws` through an arrow stored in a `const`.

#### US-134 S021 narrowable signature
§8.5, §10.6 · After: US-090 · Size: S · M3

**Done when**
- `S021` for an exported function whose body-inferred parameter type is wider than the
  union of all call-site argument types, showing both, with the `hand_edit` flag and no
  textual replacement.

**Tests.** Edge: no call sites (no S021); **Decision:** generic exports are exempt;
callers in test files count.

#### US-135 Human-readable diagnostics
§13.3, §11 · After: US-003, US-081 · Size: S · M3

**Done when**
- Format: `path:line:col: E103 non-exhaustive: <message>`, an indented excerpt with a
  caret span, `current:`/`replacement:` lines for style findings (`(hand edit)` for S020,
  S021, S030), `signature:` with the enclosing export's inferred signature, related
  spans; sorted per US-003; `--quiet` prints only `N errors, M style findings`; no ANSI
  colour.

**Tests.** Snapshots per code family. Edge: a multi-line span; a tab in the excerpt with
the caret aligned by character; a file that failed UTF-8 decoding shows byte offsets.

#### US-136 `report.json` and `lint --json`
§7, §13.3 · After: US-135, US-100, US-070 · Size: M · M3

**Done when**
- Documented schema (`schema/report.schema.json`) with the in-house JSON reader
  round-tripping the writer's output: diagnostics (line/column and byte offsets, §13.1), public
  signatures per export, impure modules in evaluation order with the first impure span,
  globals touched per module; fixed key order; `sjs lint --json` prints the identical
  document to stdout and nothing else.

**Tests.** Schema validation on every fixture program; two runs are byte-identical.
Edge: an empty program; a program with E000 still produces a report; paths relative to
the root's directory using `/` on every platform.

#### US-137 `sjs lint` command
§13.3 · After: US-136, US-134 · Size: S · M3

**Done when**
- Runs all passes, prints diagnostics or JSON, `--signatures` appends one line per export
  sorted by path then name, exit 0/1.

**Tests.** Edge: `--json --signatures` (signatures inside the JSON); `--quiet --json`
(JSON wins); a file unreadable mid-run (70).

#### US-138 `sjs build` command
§7, §13.2 · After: US-137 · Size: S · M3

**Done when**
- On success: `--out` holds byte-identical copies at the same relative paths,
  `report.json`, `types.json`; a test diffs every copy against its source. On any E-code:
  `--out` holds `report.json` alone. The directory is written under a temporary name and
  renamed into place (§7); file modes preserved.

**Tests.** Edge: `--out` inside the source tree; a CRLF source is copied as-is while S008
is reported; JSON modules are copied; symlinked sources are copied as files; an
interrupted build (simulated I/O failure) leaves the previous `dist/` untouched.

---

### Epic 9 · map, docs and help

#### US-150 `sjs map` data model and `json`
§13.6 · After: US-100, US-070 · Size: M · M3

**Done when**
- The four layers are computed from binder and inference only: modules (exports with
  signatures, purity, globals touched), dependencies (DAG with depth from the root),
  types (classes with fields, methods, `extends` chain; structural shapes shared by two
  or more exports, named by their keys), calls (transitive exported-function reach per
  exported function).
- `--format json` writes a document the in-house JSON reader round-trips; `sjs map` exits 0/1 on E-codes from
  the passes it runs.

**Tests.** Snapshot on a fixture program. Edge: a module at two depths (shortest wins);
a shape shared by exactly two exports vs one; recursive exported functions in the call
layer.

#### US-151 `sjs map --format dot`
§13.6 · After: US-150 · Size: S · M3

**Done when**
- Dependency and class graphs as Graphviz with modules clustered by depth; output parses
  with `dot -Tsvg` in CI.

**Tests.** Snapshot; Graphviz parse. Edge: a path needing quoting; an empty program.

#### US-152 `sjs map --format md`
§13.6 · After: US-150 · Size: S · M3

**Done when**
- One Markdown file per module with an exports table (name, signature) and lists of
  importers and imports, plus an index; deterministic file names derived from paths.

**Tests.** Snapshot. Edge: two modules whose paths differ only by directory.

#### US-153 `sjs map --format html`
§13.6 · After: US-150 · Size: M · M3

**Done when**
- A single self-contained page (no external assets, inline CSS/JS) drawing the dependency
  graph by depth with click-through to exports and signatures, and the type and call
  layers as tabs; the default format.

**Tests.** Snapshot; an in-house well-formedness check in `sjs-testkit` (balanced tags, escaped text). Edge: 500 modules render without a layout
failure (a generated program); names are HTML-escaped.

#### US-154 `sjs docs --format md`
§13.7 · After: US-133, US-147, US-150 · Size: M · M3

**Done when**
- One file per module plus `index.md`: module comment, each export with its signature
  from `types.json`, prose, examples with pass/fail status, importers and imports; the
  index carries the dependency layering and every export's summary; internal
  declarations in a collapsed section.

**Tests.** Snapshot. Edge: an export with no description beyond the summary; a
deprecated export marked; an example that failed shows its failure.

#### US-155 `sjs docs --format html`
§13.7 · After: US-154 · Size: M · M3

**Done when**
- A self-contained static site with the same content plus a search box over export
  names and summaries; an example failing to compile fails the docs build (exit 1).

**Tests.** Snapshot; the same well-formedness check. Edge: search works with JavaScript disabled? No: the
search box is JavaScript; the page without it still lists everything (tested by
stripping scripts).

#### US-156 `sjs help`
§13.8 · After: US-005 · Size: S · M3

**Done when**
- `sjs help` lists the eight commands one line each; `sjs help <command>` prints flags,
  defaults, exit codes, and the spec section defining it, extracted from `SPEC.md`
  embedded at build time (`include_str!`); `--help` is a synonym everywhere.

**Tests.** Snapshots. Edge: `sjs help bogus` is a usage error; the embedded spec section
boundaries are tested so a heading rename in `SPEC.md` fails the build rather than
printing the wrong section.

---

### Epic 8 · Test runner

#### US-140 Matcher module in Strict JS
§10.1, §10.2, §11.2 M4 · After: US-002 · Size: M · M4

As the runner I want `describe`/`it`/hooks/`expect` implemented as a Strict JS module
(`runtime/matchers.js`) that installs nothing, so `sjs lint` checks it and the host
shims only attach names to the global object.

**Scope.** A test registry (suites, tests, hooks, `.skip`/`.only`), a sequential executor
returning results, and every §10.2 matcher: `toBe` (`===`), `toEqual` (deep structural,
understanding `Map`, `Set`, typed arrays, `Date`, class instances), `toStrictEqual`
(`undefined` keys and class identity), `toBeUndefined/Defined/Null/Truthy/Falsy`,
comparisons, `toBeCloseTo`, `toContain`, `toHaveLength`, `toMatch`, `toBeInstanceOf`,
`toThrow`, `resolves`/`rejects`, `not`. Timing is injected (§10.3) so the 5 000 ms
timeout is testable.

**Done when**
- The module passes `sjs lint` with zero diagnostics once Epics 4–7 exist (until then
  the PR records that it is written to the subset by hand review).
- `runtime/matchers.test.js` covers every matcher, including `.not` of each.

**Tests.** The matcher tests themselves. Edge: `toEqual` on `NaN` (equal); `-0` vs `0`
in `toBe` (not equal, `Object.is` semantics are not used because `toBe` is `===`,
documented); cyclic structures in `toEqual` (no infinite loop); `toThrow` with a
subclass; `rejects` on a resolved promise fails with a clear message; a test exceeding
the timeout fails and its promise is abandoned.

#### US-141 Node host shim and worker isolation
§10.1, §10.4 · After: US-140, US-124 · Size: M · M4

**Scope.** `runtime/host-node.js` (outside the subset, a few lines) attaches the globals
and runs one test file per `worker_threads` worker with its own module graph; a
structured message protocol to the Rust side (start, pass, fail with diagnostic-style
location, skip, timeout, file done); files run in parallel, tests in a file
sequentially; the shim is embedded in the binary and materialised to a temp dir.

**Done when**
- A failing assertion reports file, line and column of the `expect` call.
- A test file that throws at module level fails the file, not the process.

**Tests.** Integration with sample test files. Edge: two files mutating the same global
do not see each other (isolation); a worker that never exits (infinite loop) is killed
at timeout; a test file with zero tests passes with a note.

#### US-142 Runtime detection
§10.1 · After: US-141 · Size: S · M4

**Done when**
- The runtime that launched `sjs` through a package runner is used: detected from
  `npm_config_user_agent` (npx), `DENO`-provided environment (deno run), or `BUN`
  environment (bunx), **Decision:** in that order of checks; otherwise the first of
  `node` (24+), `deno` (2+), `bun` on `PATH`; none found is a usage error (exit 2); a
  found runtime below the minimum version is skipped with a note and the search
  continues.
- The summary names the runtime and its version.

**Tests.** Integration with a fake `PATH`. Edge: a `node` binary that prints a
non-semver version (treated as not found); `PATH` containing a directory named `node`.

#### US-143 Deno and Bun host shims
§10.1 · After: US-142 · Size: M · M4

**Done when**
- `runtime/host-deno.js` and `runtime/host-bun.js` provide the same protocol using each
  runtime's `Worker`; the matcher module is shared unchanged; the same sample suite
  produces identical TAP on all three hosts (CI matrix).

**Tests.** The three-host matrix. Edge: Deno permissions needed for the worker and temp
dir are passed explicitly; Bun's worker termination on timeout.

#### US-144 Reporters and exit code
§10.4 · After: US-141 · Size: M · M4

**Done when**
- TAP 14 (default) with YAML diagnostics blocks for failures, `dots`, `junit`, `json`;
  the summary names the runtime; skipped tests listed; exit code is failed files capped
  at 255; `.only` narrows the run.

**Tests.** Snapshots per reporter on a fixed suite. Edge: a test name containing `#` or
a newline is escaped in TAP; 300 failing files exit 255; junit timestamps are
deterministic in tests (injected clock).

#### US-145 Coverage instrumentation
§10.5 · After: US-141, US-020 · Size: L · M4

As a programmer I want each module instrumented in memory for statement, branch and
function coverage, mapped to source lines, with `--no-coverage` running the exact bytes.

**Scope.** An instrumenter over the syntax tree that produces a *copy* with counters
(never written to the source tree); branch points: `if`/`else`, `?:`, `&&`/`||`/`??`,
`?.`, `switch` cases, `while`/`for` bodies; function entry counters; a mapping table from
counter to source span; collection from workers; the instrumented copy must itself be
valid ES2025 (not necessarily Strict JS; it is never checked).

**Done when**
- Instrumented and uninstrumented runs produce identical test results on the sample
  suite.
- Line, branch and function percentages match an independent hand count on a fixture.

**Tests.** Edge: a module with top-level `await`; a generator; a class field initializer;
an `@example` block (counts toward its module); a file with 100 % coverage and one with
0 %.

#### US-146 Coverage report and thresholds
§10.5, §13.5 · After: US-145 · Size: M · M4

**Done when**
- Output under `--out` (default `coverage/`): `index.html` with a per-file table,
  one annotated-source page per file with uncovered lines and branches marked,
  `coverage.json`; a text summary after the TAP stream with per-file and aggregate
  figures.
- `--coverage-lines`/`--coverage-branches` are per file and enforced only under `--ci`;
  a file below threshold fails the run (counted as a failed file for the exit code).

**Tests.** Snapshots of the HTML and JSON. Edge: a module with no tests appears with 0 %
and fails the threshold (cannot hide in the aggregate); thresholds of 0 and 100;
`--coverage-lines=101` is a usage error.

#### US-147 `@example` blocks as tests
§13.7 · After: US-133, US-141 · Size: S · M4

**Done when**
- Each `@example` is compiled as a test file in its module's scope (module bindings
  visible, no imports, `expect` as the only extra global) and runs with the file's tests;
  its pass/fail status is recorded for `sjs docs`.

**Tests.** Edge: an example referencing a non-exported binding of the module (visible);
an example that fails to compile fails `sjs test` and `sjs docs`; two examples on one
export run independently.

#### US-148 `sjs test` command
§13.5, §5.3 · After: US-144, US-146, US-147, US-142 · Size: S · M4

**Done when**
- Discovery per §5.3; the whole program (including test files) is checked first and any
  E-code aborts the run with exit 1; `--ci` enables E302/E304 and thresholds;
  `--no-coverage`, `--reporter`, `--out` wired; exit code per §13.1.

**Tests.** Integration. Edge: a test file with an E-code blocks the whole run (the
program is checked as one); `--ci` with no skips and no `.only` is a no-op; `--reporter
bogus` is a usage error.

#### US-149 Portability proof
§11.3 · After: US-148 · Size: S · M4

**Done when**
- CI runs `runtime/*.test.js` and every `tests/conformance/**/*.test.js` under `sjs
  test`, Jest, Vitest (`globals: true`) and Bun, and asserts the identical pass/fail set.

**Tests.** The CI job plus a local script. Edge: a deliberately failing test file is
included to prove the comparison detects divergence.

---

### Epic 10 · Performance, incremental inference, LSP and adoption

#### US-160 Benchmarks and performance gate
§11.3 · After: US-137 · Size: S · M5

**Done when**
- `benches/*.rs` (`harness = false`, `sjs-testkit::bench`) over a generated 50 000-line program and the self-hosted corpus
  (the matcher module, the host shims, every conformance fixture) measure lines per
  second per core; CI fails if throughput drops below 100 000 lines/s/core or regresses
  more than 10 % against the stored baseline.

**Tests.** The benches. Edge: the benchmark program exercises generics and narrowing, not
only declarations.

#### US-161 Incremental module summaries
§11.1 · After: US-160 · Size: L · M5

**Done when**
- Each module's summary (exports' inferred types, globals touched, mutations performed)
  is keyed on content hash; a one-file edit re-infers only that file and its dependents
  until summaries stop changing; results are byte-identical to a cold run (tested on
  1 000 random edits of the 50 000-line program); the one-file re-check completes under
  100 ms.

**Tests.** The equivalence and latency tests. Edge: an edit that changes an export's
type cascades to every transitive importer; an edit inside a function body that does
not change the summary stops at that module; deleting a file.

#### US-162 `sjs lsp`: transport, diagnostics and code actions
§13.9 · After: US-161 · Size: M · M5

**Done when**
- LSP over stdin/stdout with in-house JSON-RPC framing and the `sjs-diag` JSON
  reader/writer: initialize, open/change/save/close, shutdown (exit 0 on clean shutdown); diagnostics published for every E- and S-code; each S-code carries its
  replacement as a code action that applies exactly that one finding when requested;
  S020/S021/S030 offer no action; nothing is applied on the server's initiative.

**Tests.** A scripted LSP client in tests. Edge: an edit arriving while inference runs is
coalesced; a document with E000 still gets diagnostics for other files; the server never
writes to disk (asserted with a read-only tree).

#### US-163 `sjs lsp`: hover and signature help
§13.9 · After: US-162 · Size: S · M5

**Done when**
- Hover shows the inferred type in the §13.7 notation plus the doc summary; signature
  help shows parameters with their inferred types and the active parameter.

**Tests.** Edge: hover on a narrowed binding shows the narrowed type at that position;
hover on a generic shows the instantiation at that call.

#### US-164 `sjs lsp`: definition, references, symbols
§13.9 · After: US-162 · Size: M · M5

**Done when**
- Go-to-definition across modules (incl. re-exports and the JSON default import);
  find-references including test files; document symbols (declarations) and workspace
  symbols (the §13.6 module and export layers).

**Tests.** Edge: a reference inside an `@example`; a symbol in an unreachable module
(E207) still resolves; references to an ambient global list every use.

#### US-165 Acceptance ports and final conformance audit
§11.2 M5, §11.3 · After: US-149, US-156, US-164 · Size: M · M5

**Done when**
- One real module from each of the two teams' codebases is ported, with the host-file
  boundary (§5.2) documented from the experience in `docs/host-boundary.md`.
- The US-007 audit table marks every story merged and every code in §8, §5 and §13.7
  has trigger, near-miss and boundary fixtures; every example in `SPEC.md` is a fixture
  asserting its named code or passing; `cargo mutants` over the whole workspace reports
  zero unexplained survivors; `dist/` byte-identity, the four-runner pass/fail equality
  and the throughput figures of §11.3 are asserted by CI.

**Tests.** The audit itself. Edge: the audit fails if `SPEC.md` gains an example without
a fixture (examples are extracted from fenced blocks and matched to fixture names).
