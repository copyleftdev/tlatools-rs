# tlatools-rs

[![Tip my tokens](https://tokentip.to/badge/copyleftdev.svg?logo=1)](https://tokentip.to/@copyleftdev)

A TLA+ **evaluator** in Rust: given a specification and a concrete state — or a
concrete pair of states — decide whether a predicate of that specification holds.

This is deliberately not a model checker. It exists because the interesting
question is often not "what states can this system reach?" but "is *this*
transition one the specification permits?", and TLC cannot be asked that
directly.

**Reads 425 of the 430 specifications** in the public
[TLA+ examples](https://github.com/tlaplus/Examples) corpus and the standard
modules shipped with `tla2tools` — user-defined operators, TLAPS proofs, nested
modules, higher-order parameters and all. `cargo run --example audit -p
tla-syntax -- $(find CORPUS -name '*.tla')` reproduces the count and lists
whatever still fails.

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

As a library, the evaluator answers one question at a time:

```rust
let spec = Spec::parse(&std::fs::read_to_string("TwoPhase.tla")?)?;
let eval = Evaluator::new(&spec, constants)?;

eval.holds_at("TPInit", &state)?;          // is this a legal initial state?
eval.step_allowed("TPNext", &from, &to)?;  // is this a legal step?
```

As a command, the oracle takes a whole state graph and returns a verdict:

```console
$ tlatools check job.json
{"status":"refines","detail":"no action of the specification takes ... to ...",
 "edge":{"index":37,"source":12,"target":40,"label":"put(3)"},
 "stats":{"states":9,"edges":10,"edges_checked":38,"outcomes_checked":0}}
```

The exit status is the verdict — 0 refines, 1 does not, 2 the job could not be
carried out — so a caller can branch without parsing. No generated modules, no
JVM, no counterexample to scrape back out of stdout.

## Status

**Phase 0 — parser.** Complete. `tla-syntax` lexes and parses the module,
declaration and expression grammar, including TLA+'s column-scoped bulleted
conjunction lists, `EXCEPT` updates, function/record/set constructors and
`INSTANCE ... WITH` substitution. It parses all six benchmark specifications and
the generated refinement modules; temporal operators parse but carry no meaning
here.

**Phase 1 — evaluator.** Complete. `tla-eval` evaluates a specification's
predicates and actions at concrete states, including operators the
specification defines itself (infix, prefix and postfix), operators passed as
arguments, `LAMBDA`, `EXTENDS`, and `INSTANCE ... WITH`. It runs every benchmark spec,
including the ones that stress the corners: `RECURSIVE` operators, `CHOOSE`,
nested set comprehensions, `EXCEPT` with `@` and multiple sequential updates,
record sets and function sets.

Substitution is held as an expression rather than a value, because priming has
to reach through it: under `n <- a`, an `n'` inside the instantiated module
means `a'`. Evaluating the substitution eagerly would lose that, and quietly.

Two further choices are worth stating. Sequences, records and functions are one thing in
TLA+, so a value's representation is derived from its domain rather than from
how it was written — otherwise `[r \in {"a"} |-> 1]` and `[a |-> 1]` would
compare unequal. And a formula that one state cannot decide is refused rather
than guessed: `[]P`, `WF_v(A)` and `ENABLED A` return an error naming why.

**Phase 2 — the oracle.** Complete. `tla-oracle` decides all three obligations
from a state graph, and `tlatools check` is a JSON-in, JSON-out command the
existing Python harness can call in place of TLC. A rejected step is reported
with the action that came closest to permitting it and the clause that blocked
it.

The one obligation still left to TLC is `spec_check`, which model-checks a
trusted specification against its own invariants. That is a genuine
reachability question, and no ground evaluator answers it — the division is
deliberate, not a gap.

## Does it decide what TLC decides?

The benchmark is a labelled corpus: six reference implementations that must
pass, thirty-three mutants that must each be caught, and a record of which arm
catches each one. Both oracles were run over it and their verdicts compared.

```
$ diff <(tools/tlc_corpus.py --json) <(tools/corpus.py --json)
$
```

Identical. Thirty-nine verdicts, every one agreeing — not merely pass/fail, but
the same arm for every mutant, down to `m06_prepared_is_a_list` landing on
`contract_violation` and the do-nothing mutants landing on `coverage`.

Timing, measured on the same machine with the same exploration:

| | exploration | deciding | total |
| --- | --- | --- | --- |
| Java TLC | 3.7 s | 72.3 s | 76.0 s |
| this | 3.7 s | 0.5 s | 4.2 s |

Exploration is the benchmark's own Python worker and is unchanged, which is why
it appears in both rows; it now dominates. The honest claim is about the
deciding step, and it is not really a claim about Rust — TLC was being asked to
search a state space in order to answer a question about two known states.

## What the extra information buys

Matching TLC's verdict is the floor. Evaluating rather than searching also
answers a question a model checker structurally cannot, because it never
evaluates the specification at the offending pair of states: **what was the
implementation trying to do, and what stopped it?**

`Next` is a disjunction of actions and an action is a conjunction of a guard and
an effect, so a rejected step has a best explanation — the action that came
closest, and its first clause that does not hold. Running the benchmark's own
mutants through it:

| mutant | closest action | blocked on |
| --- | --- | --- |
| `raft/m01_no_majority_required` | `BecomeLeader(c = "s1")` | `Cardinality(votesGranted[c]) * 2 > Cardinality(Server)` |
| `bounded_buffer/m02_off_by_one_capacity` | `Put` | `Len(buf) < Capacity` |
| `two_phase_commit/m01_commit_without_all_prepared` | `TMCommit` | `tmPrepared = RM` |

Each names the injected bug exactly. Compare TLC on the same edge, which can
only report that a trace stuttered at index `tv_p`.

Two details make it land on the right action. Closeness counts *every* conjunct
that holds, not the prefix before the first failure — otherwise `Put`, whose
very first conjunct is the one that fails, scores zero and loses to an action
nobody was attempting. And a failing conjunct that constrains the successor
state is reported differently from one that does not: the first says the action
was available but produced the wrong state, the second says it was not available
at all.

## Layout

```
crates/tla-syntax    lexer, parser, AST
crates/tla-eval      values and the ground evaluator
crates/tla-oracle    the three obligations, over a state graph
crates/tlatools      the command-line interface
specs/               the specifications the crates are tested on
tools/               corpus runners for this oracle and for TLC
```

## Development

```bash
cargo test
cargo clippy --all-targets
cargo mutants                    # are the tests worth having?
cargo run --example audit -p tla-syntax -- $(find CORPUS -name '*.tla')
cargo run --example depth -p tla-syntax -- $(find CORPUS -name '*.tla')
```

Mutation coverage is 85% (603 of 712 viable mutants caught). CI gates the
*diff* at zero survivors, so it rises rather than drifts. The survivors are
concentrated in the parser's internal bookkeeping and are listed by
`cargo mutants`.

`examples/depth.rs` is where the recursion limit comes from: it reports how
deeply real specifications nest and how far the parser gets on a small stack,
and the constant is set from those two numbers rather than chosen.

## What it will not do

- **Integers are 64-bit.** TLA+'s are unbounded. Arithmetic that would overflow
  reports an error rather than wrapping, so nothing is silently wrong, but a
  specification that genuinely needs big numbers is out of reach.
- **Temporal formulas are refused, not evaluated.** `[]P`, `<>P`, `WF_v(A)` and
  `ENABLED A` are about behaviours; this crate is about states and steps.
- **Proofs are skipped, not checked.** TLAPS proof syntax is recognised so the
  module around it can be read.
- **Enumeration is bounded** at 2²⁰ elements, and reports the limit rather than
  exhausting memory.
