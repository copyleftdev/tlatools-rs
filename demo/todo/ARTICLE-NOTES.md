# Notes for the write-up

Raw material for the article, kept beside the demo so the two do not drift.
**Everything in the History section should be checked before publishing** — it
is written from memory, and a piece about formal methods getting its own dates
wrong would be a bad look. Sources to check against are named.

## History — verify these

| claim | confidence | check against |
| --- | --- | --- |
| Leslie Lamport created TLA+ | high | lamport.azurewebsites.net |
| TLA = Temporal Logic of Actions; TLA+ adds set theory and a module system | high | *Specifying Systems* |
| "The Temporal Logic of Actions" published in ACM TOPLAS, 1994 | medium — check the year | ACM DL |
| *Specifying Systems* published 2002, and is free online | medium — check the year | lamport.azurewebsites.net/tla/book.html |
| Lamport received the Turing Award in 2013 | medium — check the year | amturing.acm.org |
| The Turing Award was for distributed and concurrent systems broadly, not for TLA+ specifically | high | the citation |
| PlusCal, an algorithm language that compiles to TLA+, came later | high, date unknown | Lamport's site |
| AWS wrote up their use in CACM, "How Amazon Web Services Uses Formal Methods", 2015 | medium — check the year | cacm.acm.org |
| Also used at Microsoft (Azure Cosmos DB), MongoDB, Elastic | medium — check each | their engineering blogs |

The one framing worth getting right: **Lamport's insight was that a
specification is mathematics, not a program.** A TLA+ spec is a formula, and a
system satisfies it if every behaviour the system has is a behaviour the
formula allows. That is why refinement is just implication, and why this tool
can be an evaluator rather than a checker.

## The toolchain, so the article gets the names right

| | |
| --- | --- |
| **SANY** | the parser and semantic analyser |
| **TLC** | the model checker — explores the reachable state space |
| **TLAPS** | the proof system — checks proofs, does not explore |
| **PlusCal** | an algorithm language that compiles to TLA+ |
| **the Toolbox** | the Eclipse-based IDE |
| **tlatools-rs** | this: parser and evaluator, no exploring, no proving |

**Be careful not to claim this replaces TLC.** It answers a different question.
The article is stronger if it says so plainly: if you want to know what states
a system can reach, use TLC. This is for when you already have the states.

## Numbers that are ours, and are checked

These come from this repository and are reproducible:

- Reads **1,266 of 1,268** specifications across the three public TLA+ corpora
  (Examples, CommunityModules, the tools' own test models). The two it does not
  are two SANY does not either.
- Verdicts **byte-identical to Java TLC** over a labelled corpus of 39 cases —
  6 implementations that must pass, 33 mutants that must each be caught,
  including which check catches which.
- Deciding that corpus: TLC **72.3 s**, this **0.5 s**. Exploration is the same
  Python in both and takes 3.7 s, so quote the deciding step, not the total.
- **164 tests**, clippy-pedantic clean, **85%** mutation coverage.
- `tla-syntax` and `tla-eval` have **no external dependencies**.

Reproduce the first with `cargo run --release --example audit`, the rest from
the README.

## The argument, in one paragraph

TLC asks what states a system can reach. That is the right question when you
are designing a protocol and the wrong one when you already have a trace, a
transition, or an implementation in front of you — and with an agent writing
code, you always do. Asking "may this step happen?" directly means the
specification is evaluated *at the step*, which is why it can tell you the
action you were reaching for and the clause that stopped it, instead of a
search that failed to find something.

## Things not to overclaim

- This does not verify a program. It checks the transitions you gave it. If
  your walk of the implementation misses a path, nothing checks that path.
- The specification can be wrong. It is the thing you argued about, not the
  thing that is true. `ClearCompleted` in the demo could have been written to
  clear everything, and then the "buggy" implementation would be the correct
  one.
- 64-bit integers, no real arithmetic, no temporal formulas, proofs skipped.
  The README's "What it will not do" is honest and the article should be too.
- The demo's state space is nine states. That is a demo. Say so.

## Loose ends that would make good follow-ups

- Wiring this into an actual agent loop, with the failure text fed back as the
  next prompt, and measuring whether it beats an uninformative retry. That
  experiment exists at `/home/ops/Project/tla-for-ai` and has not been run to
  a conclusion.
- A trace checker: take a production log, decode it into states, and ask
  whether the run was one the specification allows.
