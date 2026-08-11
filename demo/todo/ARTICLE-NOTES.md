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
| Reads 1,266 of 1,268 specifications across three public TLA+ corpora; the two it misses are two SANY misses | `cargo run --release --example audit -p tla-syntax -- $(find CORPUS -name '*.tla')` |
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

TLC asks what states a system can reach. That is the right question when you
are designing a protocol and the wrong one when you already have a trace, a
transition, or an implementation in front of you — and with an agent writing
code, you always do. Asking "may this step happen?" directly means the
specification is evaluated *at the step*, which is why it can name the action
you were reaching for and the clause that stopped it, rather than reporting a
search that failed to find something.

## Things not to overclaim

- This does not verify a program. It checks the transitions you give it. If
  your walk of the implementation misses a path, nothing checks that path.
- The specification can be wrong. It is what you argued about, not what is
  true. `ClearCompleted` could have been written to clear everything, and then
  the "buggy" implementation is the correct one.
- 64-bit integers, no real arithmetic, no temporal formulas, proofs skipped.
- The demo's state space is nine states. That is a demo. Say so.
- **The agentic loop is argued, not measured.** The claim that formal feedback
  beats an uninformative retry rests on the quality of the message, not on
  data. The experiment that would settle it is built and has not been run. Say
  "here is why I expect this to help", not "this helps".

## Follow-ups that would make good second articles

- Run the experiment: feed the failure text back as the next prompt and measure
  it against an equal number of uninformative retries.
- A trace checker: take a production log, decode it into states, and ask
  whether the run was one the specification allows.
