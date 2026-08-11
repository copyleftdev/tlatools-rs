# Changelog

All notable changes to this project are recorded here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **`refines` now checks `[Next]_vars`, not `Next`.** A specification is
  `Init /\ [][Next]_vars`, so a step that leaves every variable unchanged is
  permitted whatever `Next` says. Such steps were previously reported as
  refinement violations, which rejected any implementation that idles, retries
  or re-renders. `Stats` gains `stutter_steps`, so a graph that is mostly
  self-loops is visible rather than silently passing.

  This is a behaviour change: a self-loop that used to fail now passes. Java TLC
  agrees — given the same `[Next]_vars` obligation, its verdicts remain
  byte-identical over all 39 labelled cases.

- Blocked-conjunct messages said "1 of its 2 conjuncts do"; they now say "hold".

## [0.1.0] — 2026-08-10

First release.

### Added

- **`tla-syntax`** — lexer, parser, AST and printer for TLA+, with no
  dependencies. Reads 1,256 of the 1,258 specifications in the public
  [Examples](https://github.com/tlaplus/Examples),
  [CommunityModules](https://github.com/tlaplus/CommunityModules) and
  [tlaplus](https://github.com/tlaplus/tlaplus) corpora; the two it does not
  are two that SANY does not either. Recursion is bounded, so no input causes a
  stack overflow.
- **`tla-eval`** — evaluates a specification's predicates and actions at
  concrete states, with no dependencies. `EXTENDS`, `INSTANCE ... WITH`, nested
  modules, user-defined operators in every fixity, higher-order operators and
  `LAMBDA`.
- **`tla-oracle`** — decides whether an implementation's reachable state graph
  refines a specification, and reports which action came closest when it does
  not, together with the clause that blocked it.
- **`tlatools`** — `parse`, `fmt` and `check`, with the verdict in the exit
  status.

### Verified

- Verdicts are byte-identical to Java TLC over a labelled corpus of 39 cases,
  including which check catches each of 33 mutants.
- 164 tests, clippy-pedantic clean, 85% mutation coverage.
- `golden/` records how each of 1,258 corpus specifications is read.

### Known limits

- Integers are 64-bit. TLA+'s are unbounded; TLC's are 32-bit. Overflow is an
  error, never a wrap.
- Real arithmetic is not implemented. Decimals are parsed and kept exactly as
  written, and evaluating one is an error.
- Temporal formulas are refused rather than evaluated.
- TLAPS proofs are skipped rather than checked.
- Enumeration is capped at 2²⁰ elements.

[0.1.0]: https://github.com/copyleftdev/tlatools-rs/releases/tag/v0.1.0
