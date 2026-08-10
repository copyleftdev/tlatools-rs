# tlatools-rs

[![Tip my tokens](https://tokentip.to/badge/copyleftdev.svg?logo=1)](https://tokentip.to/@copyleftdev)

A TLA+ **evaluator** in Rust: given a specification and a concrete state — or a
concrete pair of states — decide whether a predicate of that specification holds.

This is deliberately not a model checker. It exists because the interesting
question is often not "what states can this system reach?" but "is *this*
transition one the specification permits?", and TLC cannot be asked that
directly.

## Why an evaluator and not a checker

The motivating consumer is [`tla-for-ai`](../tla-for-ai), a benchmark whose
oracle decides whether a candidate implementation refines a trusted spec. Safety
refinement decomposes into two obligations, and both are ground questions:

| obligation | the question | what it needs |
| --- | --- | --- |
| `init` | is the implementation's first state a legal initial state? | evaluate `Init` at one state |
| `refines` | is every reachable transition a step the spec allows? | evaluate `Next` at each `(src, dst)` pair |
| `coverage` | are the required outcomes reachable? | evaluate a predicate at each reached state |

None of these searches a state space. All three are predicate evaluation at
states that are already known.

Because TLC only answers reachability questions, the existing oracle has to
smuggle each obligation past it: every edge is encoded as a two-state trace, and
a liveness property (`<>Complete`) is made to fail so TLC will name the edge that
could not be extended. The cost of that encoding is the whole shape of the
harness — transitions batched 1500 at a time because a single module holding a
438-state protocol runs to tens of megabytes and TLC will not parse it, a fresh
JVM per batch, the counterexample recovered by regex from TLC's stdout, and edge
indices corrected by chunk offset afterwards.

An evaluator answers the question that was actually being asked, and the
encoding goes away with it.

## Status

**Phase 0 — parser.** Complete. `tla-syntax` lexes and parses the module,
declaration and expression grammar, including TLA+'s column-scoped bulleted
conjunction lists, `EXCEPT` updates, function/record/set constructors and
`INSTANCE ... WITH` substitution. It parses all six benchmark specifications and
the generated refinement modules; temporal operators parse but carry no meaning
here.

Next: `tla-eval` (values and the ground evaluator), then a drop-in replacement
for the oracle's TLC calls, differentially tested against Java TLC on every
reference implementation and mutant the benchmark ships.

## Layout

```
crates/tla-syntax    lexer, parser, AST
```

## Development

```bash
cargo test
cargo clippy --all-targets
```
