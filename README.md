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

## Using it

```rust
let spec = Spec::parse(&std::fs::read_to_string("TwoPhase.tla")?)?;
let eval = Evaluator::new(&spec, constants)?;

eval.holds_at("TPInit", &state)?;          // is this a legal initial state?
eval.step_allowed("TPNext", &from, &to)?;  // is this a legal step?
```

No generated modules, no subprocess, no counterexample to parse back out: the
answer to the question asked is the value returned.

## Status

**Phase 0 — parser.** Complete. `tla-syntax` lexes and parses the module,
declaration and expression grammar, including TLA+'s column-scoped bulleted
conjunction lists, `EXCEPT` updates, function/record/set constructors and
`INSTANCE ... WITH` substitution. It parses all six benchmark specifications and
the generated refinement modules; temporal operators parse but carry no meaning
here.

**Phase 1 — evaluator.** Complete. `tla-eval` evaluates a specification's
predicates and actions at concrete states. It runs every benchmark spec,
including the ones that stress the corners: `RECURSIVE` operators, `CHOOSE`,
nested set comprehensions, `EXCEPT` with `@` and multiple sequential updates,
record sets and function sets.

Two choices are worth stating. Sequences, records and functions are one thing in
TLA+, so a value's representation is derived from its domain rather than from
how it was written — otherwise `[r \in {"a"} |-> 1]` and `[a |-> 1]` would
compare unequal. And a formula that one state cannot decide is refused rather
than guessed: `[]P`, `WF_v(A)` and `ENABLED A` return an error naming why.

Verified against Java TLC, not only against itself. Seven transitions across
four protocols — Chang-Roberts forwarding, Raft's majority rule, two-phase
commit's prepare barrier, Paxos's Phase2a safety condition — were encoded into
the oracle's trace form and run through `tla2tools.jar`. TLC returns the same
verdict on all seven, in both directions.

Next: a drop-in replacement for the oracle's TLC calls, run against every
reference implementation and mutant the benchmark ships.

## Layout

```
crates/tla-syntax    lexer, parser, AST
crates/tla-eval      values and the ground evaluator
specs/               the specifications both crates are tested on
```

## Development

```bash
cargo test
cargo clippy --all-targets
```
