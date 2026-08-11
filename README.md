# tlatools-rs

[![ci](https://github.com/copyleftdev/tlatools-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/copyleftdev/tlatools-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tlatools.svg)](https://crates.io/crates/tlatools)
[![docs.rs](https://img.shields.io/docsrs/tla-eval)](https://docs.rs/tla-eval)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tip my tokens](https://tokentip.to/badge/copyleftdev.svg?logo=1)](https://tokentip.to/@copyleftdev)

**Ask a TLA+ specification about a state, and get an answer.**

```console
$ tlatools parse spec/*.tla
spec/Paxos.tla	ok	Paxos	19 units
spec/Draft.tla	error	14:3	expected an expression
```

Reads **1,256 of the 1,258** specifications in three public TLA+ corpora —
[Examples](https://github.com/tlaplus/Examples),
[CommunityModules](https://github.com/tlaplus/CommunityModules) and the
[tlaplus](https://github.com/tlaplus/tlaplus) tools' own test suite. The two it
doesn't are two that SANY, the reference parser, doesn't either.

## Why this exists

TLC answers one question: *what states can this system reach?* That is the
right question surprisingly often, and the wrong one the rest of the time.

Sometimes you already have the states. You have a trace from production, an
implementation's transition, a candidate refinement — and you want to know
whether the specification permits **this** step.

TLC can answer that: encode the steps as data, assert with a plain safety
invariant that each one is enabled, and check it. See [Validating Traces of
Distributed Programs Against TLA+ Specifications](https://arxiv.org/abs/2404.16075)
(Cirstea, Kuppe, Loillier, Merz) for the thorough treatment. What you get back
is a verdict, and it costs roughly 0.7 s of JVM boot and SANY parse per query.

This evaluates the specification *at the pair of states*, which is cheap enough
for an edit loop and can name the conjunct that failed.

```rust
let spec = Spec::from_file("TwoPhase.tla")?;
let eval = Evaluator::new(&spec, constants)?;

eval.holds_at("TPInit", &state)?;          // is this a legal initial state?
eval.step_allowed("TPNext", &from, &to)?;  // is this a legal step?
```

No generated modules, no JVM, no counterexample to parse back out.

## What asking directly buys

When a step is refused, the specification can say what you were *trying* to do:

```console
$ tlatools check job.json
no action of the specification takes [...] to [...]. The closest was
`BecomeLeader(c = "s1")`, which was not available here:
`Cardinality(votesGranted[c]) * 2 > Cardinality(Server)` does not hold
(3 of its 4 conjuncts hold)
```

That is Raft's majority rule, named exactly. A model checker will tell you the
step is not permitted; it will not tell you *which conjunct* failed, because it
searches for a matching pair rather than evaluating the specification at the
pair you handed it.

## A worked example

[`demo/todo`](demo/todo) is a to-do list: a 40-line specification, three Python
implementations, and a script that asks the specification about each one. Two
of the implementations have bugs, and it names both:

```console
$ demo/todo/check.py demo/todo/impl/clear_removes_everything.py
  from   a=open, b=done
  doing  clear_completed
  to     a=absent, b=absent

  ClearCompleted was available, but does not produce that state,
    because tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]
    does not hold (1 of its 2 clauses hold)
```

## Install

```console
$ cargo install tlatools     # the command
$ cargo add tla-eval         # the library
```

## The commands

| | |
| --- | --- |
| `tlatools parse FILE...` | read each file; one tab-separated line each |
| `tlatools fmt FILE` | write a module back out in one canonical form |
| `tlatools check [JOB]` | decide whether a state graph refines a specification |

Exit status is the answer — `0` yes, `1` no, `2` the question could not be
asked — so a script can branch without parsing anything.

## The crates

| crate | what it is |
| --- | --- |
| [`tla-syntax`](crates/tla-syntax) | lexer, parser, AST, printer |
| [`tla-eval`](crates/tla-eval) | evaluates predicates and actions at concrete states |
| [`tla-oracle`](crates/tla-oracle) | decides whether a state graph refines a specification |
| [`tlatools`](crates/tlatools) | the command |

`tla-syntax` and `tla-eval` have **no external dependencies at all**.

## How much of TLA+

Near enough all of it, and measured rather than claimed:

- user-defined operators in every fixity, including ones declared by shape
  (`_+_`, `-._`, `_^#`)
- `EXTENDS`, `INSTANCE ... WITH`, nested modules, instances of instances
- higher-order operators, `LAMBDA`, operators passed by symbol
- `RECURSIVE`, `CHOOSE`, `EXCEPT` with `@`, records, functions, sequences
- TLAPS proofs, recognised and skipped — this evaluates, it does not prove

Real files too: a byte-order mark is skipped, CRLF and lone-CR line endings both
end a line, and the prose around a module is not mistaken for TLA+.

## How it is checked

**Against the reference implementation.** Verdicts were compared with Java TLC
over a labelled corpus of 39 cases — six implementations that must pass and
thirty-three mutants that must each be caught. Byte-identical, including *which*
check catches each mutant, with both sides discharging the specification's own
obligation `Init /\ [][Next]_vars`.

That last clause is not pedantry. Hand TLC a bare `Next` instead and it parts
company with this tool on exactly one case: a mutant whose "bug" is transferring
money from an account to itself, which nets to zero and so changes nothing.
`[Next]_vars` permits it, because a step that changes nothing is a step every
specification allows. Catching that one needs an abstraction where the operation
is observable, not a stricter refinement check.

**Against three public corpora.** `golden/*.tsv` records how each of 1,258
files is read, so a change names the files it changed rather than moving a
count. `golden/fmt/` holds full canonical output for the vendored
specifications, so a change in the parser *or* the printer is a readable diff.

**Against itself.** 164 tests, clippy-pedantic clean, 85% mutation coverage, and
a robustness suite that feeds back every prefix and every dropped line of every
fixture — a parser must never panic, whatever it is handed.

## What it will not do

- **Real arithmetic.** A decimal is parsed and kept exactly as written, because
  TLA+ decimals are exact rationals — but evaluating one is an error, not a
  rounded guess.
- **Unbounded integers.** These are 64-bit. That is not the language, though it
  is wider than the reference implementation, whose integers are 32-bit. Both
  report overflow rather than wrapping.
- **Temporal formulas.** `[]P`, `<>P`, `WF_v(A)` and `ENABLED A` are about
  behaviours; this is about states and steps. They are refused with a reason,
  never guessed.
- **Proofs.** Recognised so the module around them can be read; checking them is
  TLAPS's job.
- **Reachability.** This is not a model checker. If you need to know what states
  a system can reach, you want TLC — and the two compose happily.

## Development

```console
$ cargo test
$ cargo clippy --all-targets
$ CARGO_MUTANTS_JOBS=4 nice -n 19 cargo mutants   # bounded; it will eat a machine
```

Against the corpora, which live wherever you put them:

```console
$ cargo run --release --example audit -p tla-syntax -- $(find CORPUS -name '*.tla')
$ cargo run --release --example depth -p tla-syntax -- $(find CORPUS -name '*.tla')
$ TLA_EXAMPLES=... TLA_COMMUNITY=... TLA_TESTS=... tools/golden.sh --check
```

`examples/depth.rs` is worth a look: it is where the parser's nesting limit
comes from, measured in a child process because a stack overflow aborts rather
than unwinding and so cannot be caught from inside.

**Contributions welcome.** If you have a TLA+ file this reads wrongly, that is
the most useful thing you can send. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT — see [LICENSE](LICENSE).
