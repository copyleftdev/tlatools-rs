# Changelog

All notable changes to this project are recorded here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — unreleased

First release.

### Added

- **`tla-syntax`** — lexer, parser, AST and printer for TLA+, with no
  dependencies. Reads 1,266 of the 1,268 specifications in the public
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
- `golden/` records how each of 1,268 corpus specifications is read.

### Known limits

- Integers are 64-bit. TLA+'s are unbounded; TLC's are 32-bit. Overflow is an
  error, never a wrap.
- Real arithmetic is not implemented. Decimals are parsed and kept exactly as
  written, and evaluating one is an error.
- Temporal formulas are refused rather than evaluated.
- TLAPS proofs are skipped rather than checked.
- Enumeration is capped at 2²⁰ elements.

[0.1.0]: https://github.com/copyleftdev/tlatools-rs/releases/tag/v0.1.0
