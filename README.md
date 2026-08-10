# tlatools-rs

[![Tip my tokens](https://tokentip.to/badge/copyleftdev.svg?logo=1)](https://tokentip.to/@copyleftdev)

A TLA+ **evaluator** in Rust: given a specification and a concrete state — or a
concrete pair of states — decide whether a predicate of that specification holds.

This is deliberately not a model checker. It exists because the interesting
question is often not "what states can this system reach?" but "is *this*
transition one the specification permits?", and TLC cannot be asked that
directly.

**Reads 1,214 of the 1,258 specifications** in every public TLA+ corpus we
could find — the [examples](https://github.com/tlaplus/Examples), the
[community modules](https://github.com/tlaplus/CommunityModules), and the
[tools' own test models](https://github.com/tlaplus/tlaplus), which include
specifications deliberately written to be rejected. Exactly how each one is
read is recorded in `golden/`.

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

## Files

```console
$ tlatools parse spec/*.tla          # one tab-separated line per file
spec/Paxos.tla	ok	Paxos	19 units
spec/Broken.tla	error	14:3	expected an expression

$ tlatools fmt Paxos.tla             # one canonical form, for comparing two files
```

`Spec::from_file` reads a `.tla` file and resolves whatever it extends or
instantiates from the directory beside it, which is where TLA+ tools look.

Real files are messier than curated ones, so: a UTF-8 byte-order mark is
skipped, CRLF and lone-CR line endings both end a line, and a tab counts as one
column. That last one matters, because bulleted lists are scoped by column —
mixing tabs and spaces to indent the bullets of one list is ambiguous in any
tool and best avoided. Of the 1,258 specifications surveyed, 55 contain tabs
and 3 use CRLF.

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
CARGO_MUTANTS_JOBS=4 nice -n 19 cargo mutants   # are the tests worth having?
cargo run --example audit -p tla-syntax -- $(find CORPUS -name '*.tla')
cargo run --example depth -p tla-syntax -- $(find CORPUS -name '*.tla')

TLA_EXAMPLES=... TLA_COMMUNITY=... TLA_TESTS=... tools/golden.sh --check
UPDATE_GOLDEN=1 cargo test -p tla-syntax --test golden
```

`golden/` holds two things. `golden/*.tsv` records the verdict for each of the
1,258 corpus specifications, so a behaviour change names the files it changed
instead of moving a count. `golden/fmt/` holds the full canonical form of the
specifications this repository vendors, so a change in the parser *or* the
printer shows up as a readable diff.

Mutation coverage is 85% (603 of 712 viable mutants caught). The run is
deliberately bounded to four jobs and niced — it will otherwise take every core
on the machine, for twenty minutes. CI gates the
*diff* at zero survivors, so it rises rather than drifts. The survivors are
concentrated in the parser's internal bookkeeping and are listed by
`cargo mutants`.

`examples/depth.rs` is where the nesting limit comes from. Across the public
corpus the deepest expression nests 24; the default limit is 256, which costs
about 512 KiB of stack in an optimised build and 5 MiB in an unoptimised one.
A caller with less stack than that can use `parse_module_bounded`. SANY, the
reference parser, has no such limit and dies with a `StackOverflowError`
somewhere past 500.

## What it will not do

- **Integers are 64-bit.** TLA+'s are unbounded, so this is not the language.
  It is, however, wider than the reference implementation: TLC's integers are
  32-bit, and it refuses the literal `2147483648` outright. Both report
  overflow rather than wrapping, so neither is ever silently wrong.
- **Temporal formulas are refused, not evaluated.** `[]P`, `<>P`, `WF_v(A)` and
  `ENABLED A` are about behaviours; this crate is about states and steps.
- **Proofs are skipped, not checked.** TLAPS proof syntax is recognised so the
  module around it can be read.
- **Enumeration is bounded** at 2²⁰ elements, and reports the limit rather than
  exhausting memory.
