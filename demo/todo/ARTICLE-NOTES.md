# Notes for the write-up

Raw material for the article. Every external claim below has been checked
against a primary source and carries a citation; anything still unchecked says
so in capitals. Two of the first-draft claims here were wrong, which is the
reason this file exists.

## History — checked

| claim | source |
| --- | --- |
| Leslie Lamport created TLA+ (TLA = Temporal Logic of Actions; TLA+ adds set theory and a module system) | *Specifying Systems*, [lamport.azurewebsites.net/tla/book.html](https://lamport.azurewebsites.net/tla/book.html) |
| "The Temporal Logic of Actions", ACM TOPLAS **vol. 16, no. 3, pp. 872–923, May 1994** | [dl.acm.org/doi/10.1145/177492.177726](https://dl.acm.org/doi/10.1145/177492.177726) |
| *Specifying Systems: The TLA+ Language and Tools for Hardware and Software Engineers*, Addison-Wesley, **2002** | [lamport.azurewebsites.net/tla/book.html](https://lamport.azurewebsites.net/tla/book.html) |
| Lamport received the **2013** ACM A.M. Turing Award (announced March 2014) | [acm.org](https://www.acm.org/media-center/2014/march/acm-turing-award-goes-to-pioneer-who-advanced-reliability-and-consistency-of-computing-systems) |
| PlusCal published as "The PlusCal Algorithm Language", LNCS 5684, pp. 36–60, **2009** | [link.springer.com](https://link.springer.com/chapter/10.1007/978-3-642-03466-4_2) |
| AWS's experience report: Newcombe, Rath, Zhang, Munteanu, Brooker, Deardeuff, "How Amazon Web Services Uses Formal Methods", **CACM 58(4), pp. 66–73, 2015** | [cacm.acm.org](https://cacm.acm.org/research/how-amazon-web-services-uses-formal-methods/) |

### Two corrections to make before writing

**The Turing Award was not for TLA+.** The citation is "for fundamental
contributions to the theory and practice of distributed and concurrent systems,
notably the invention of concepts such as causality and logical clocks, safety
and liveness, replicated state machines, and sequential consistency." TLA+ is
not named in it. Saying "he won the Turing Award for TLA+" is wrong and the
kind of wrong that people who know the field will notice immediately.

**"Specifying Systems" is not free in the usual sense.** Lamport hosts a PDF,
but the page restricts it to personal use — "may not be reproduced or
distributed for commercial purposes, or for any purpose other than for personal
use, without the prior written permission of the publisher." "Free to download
for personal use" is accurate; "free online" is not.

### Still unchecked — verify or cut

- **UNVERIFIED:** use at Microsoft / Azure Cosmos DB, MongoDB, Elastic. Widely
  repeated, not checked here. Either find each company's own engineering post
  or cut the sentence — the AWS paper alone carries the point.
- **SOFT:** PlusCal development is often dated to 2004–05, but the only firm
  date is the 2009 publication. Say "published in 2009" and stop.

### The framing worth getting right

Lamport's insight was that a specification is mathematics, not a program. A
TLA+ spec is a formula; a system satisfies it when every behaviour the system
has is a behaviour the formula allows. That is why refinement is implication,
and it is the reason a tool can be an *evaluator* rather than a checker — you
are asking whether a formula holds of something, not searching for something.

## The toolchain, so the names are right

| | |
| --- | --- |
| **SANY** | the parser and semantic analyser |
| **TLC** | the model checker — explores the reachable state space |
| **TLAPS** | the proof system — checks proofs, does not explore |
| **PlusCal** | an algorithm language that compiles to TLA+ |
| **the Toolbox** | the Eclipse-based IDE |
| **tlatools-rs** | this: parser and evaluator; no exploring, no proving |

**Do not claim this replaces TLC.** It answers a different question. The
article is stronger for saying so: if you want to know what states a system can
reach, use TLC.

## Our numbers — reproducible, and how

| claim | reproduce with |
| --- | --- |
| Reads 1,256 of 1,258 specifications across three public TLA+ corpora (78 + 420 + 760, exactly what `golden/*.tsv` records); the two it misses are two SANY misses | `cargo run --release --example audit -p tla-syntax -- $(find CORPUS -name '*.tla')` |
| Verdicts byte-identical to Java TLC over 39 labelled cases — 6 that must pass, 33 mutants that must each be caught, including *which* check catches which | `diff <(tools/tlc_corpus.py --json) <(tools/corpus.py --json)` |
| Deciding that corpus: TLC 72.3 s, this 0.5 s | quote the **deciding** step; exploration is the same Python in both and takes 3.7 s |
| 164 tests, clippy-pedantic clean, 85% mutation coverage | `cargo test`, `cargo clippy --all-targets`, `cargo mutants` |
| `tla-syntax` and `tla-eval` have no external dependencies | `cargo tree` |

## Where to point readers

| | |
| --- | --- |
| the crates | [tlatools](https://crates.io/crates/tlatools) · [tla-eval](https://crates.io/crates/tla-eval) · [tla-syntax](https://crates.io/crates/tla-syntax) · [tla-oracle](https://crates.io/crates/tla-oracle) |
| install | `cargo install tlatools` |
| the source | [github.com/copyleftdev/tlatools-rs](https://github.com/copyleftdev/tlatools-rs) |
| the demo in this article | [demo/todo](https://github.com/copyleftdev/tlatools-rs/tree/main/demo/todo) |
| the docs | [docs.rs/tla-eval](https://docs.rs/tla-eval) |

## The argument, in one paragraph

For forty years "LGTM" has been the honest limit of code review: the invariants
lived in the reviewer's head, so nothing could check them. Writing them down in
TLA+ makes them checkable. With an agent producing code faster than anyone can
read it, that stops being a nicety. The specification is evaluated *at the
step*, which is why it can name the action you were reaching for and the clause
that stopped it.

### The claim that was wrong, and must not come back

An earlier draft said TLC **cannot** be asked "is this step legal" directly —
that you must encode a two-state trace and attach a liveness property engineered
to fail. **This is false.** A plain `INVARIANT` over the steps-as-data does it,
no liveness and no fairness, in 0.78 s on this spec. Counter-experiment is
reproducible; the pattern is published work:

> Cirstea, **Kuppe**, Loillier, Merz, *Validating Traces of Distributed Programs
> Against TLA+ Specifications*, [arXiv:2404.16075](https://arxiv.org/abs/2404.16075).
> Kuppe maintains the TLA+ tools. Safety checking, explicitly not liveness.

That paper is also prior art for "a trace checker" as a follow-up idea. Cite it
as the thing this complements; do not propose it as novel.

**The defensible claim is narrower:** TLC tells you *that* the step is illegal,
not *which conjunct* failed, and the ~0.7 s is JVM boot plus SANY parse paid on
every query. Structured blame instead of a boolean, cheap enough for an edit
loop.

## Things not to overclaim

- This does not verify a program. It checks the transitions you give it. If
  your walk of the implementation misses a path, nothing checks that path.
- The specification can be wrong. It is what you argued about, not what is
  true. `ClearCompleted` could have been written to clear everything, and then
  the "buggy" implementation is the correct one.
- 64-bit integers, no real arithmetic, no temporal formulas, proofs skipped.
- The demo's state space is nine states. That is a demo. Say so.
- **The agentic loop was measured and the result is null.** `focus-001`, 200
  units: blind 90.5% [85.6–93.8] vs tla 92.5% [88.0–95.4], McNemar exact
  p=0.125. Formal stratum n=13, 46.2% → 69.2%, p=0.25 — suggestive and
  underpowered, not a result. Report the null; do not say the experiment is
  unrun (an earlier draft did, and it was already three runs / 683 attempts old).

- **Stuttering: fixed in 0.2.0**, and now the article's best anecdote. `refines`
  checks `[Next]_vars`. The fix made `bank_transfer`'s `m03_self_transfer`
  survive — a transfer to the same account nets to zero, so it changes nothing,
  so it *is* a stuttering step and the spec permits it. The benchmark's
  obligation was stricter than the specification it checks. Give Java TLC the
  same `[Next]_vars` and it passes that mutant too, so the byte-identical claim
  holds — but state the obligation when making it.

## Follow-ups that would make good second articles
- Power the experiment properly on the formal stratum, where the only
  suggestive signal is.
- **Not** a trace checker — [arXiv:2404.16075](https://arxiv.org/abs/2404.16075)
  already did it, better, with the tools maintainer as an author.
