# Changelog

## Unreleased

First working version. Not yet published.

### Added

- `tla-syntax` — lexer, parser, AST and printer for TLA+. Reads 425 of the 430
  specifications in the public corpus.
- `tla-eval` — evaluates a specification's predicates and actions at concrete
  states, with `EXTENDS`, `INSTANCE ... WITH`, operators a specification
  defines for itself, operators passed as arguments, and `LAMBDA`.
- `tla-oracle` — decides whether an implementation's state graph refines a
  specification, and says which action came closest when it does not.
- `tlatools` — `tlatools check`, JSON in and JSON out, with the verdict in the
  exit status.

### Known limits

- Integers are 64-bit; TLA+'s are unbounded. Overflow is an error, not a wrap.
- Temporal formulas are refused rather than evaluated.
- TLAPS proofs are skipped rather than checked.
- Enumeration is capped at 2²⁰ elements.
