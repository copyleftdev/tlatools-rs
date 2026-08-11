---
title: "LGTM is what you say when you can't see the invariants"
published: false
tags: ai, rust, formalmethods, testing
---

Every code review you have ever approved, you approved with a sentence like
"looks good to me."

Think about what that sentence is actually claiming. Not "I checked that this
preserves every property the system depends on." It's "I read this, and nothing
jumped out." Those are extremely different statements, and we've spent forty
years letting the second one stand in for the first, because the first one was
never available.

It was never available because the invariants were in somebody's head. The rule
that a task can't go from *absent* to *done*. The rule that clearing completed
items leaves the open ones alone. Nobody wrote those down. They lived in the
reviewer's memory of how the system was supposed to work, and a reviewer with a
good memory caught maybe most of them on a good day.

Now something else is writing the code, at a speed no reviewer can track, and
"nothing jumped out" is all we've got. That's the problem worth solving. Not
"can agents write code" — they can. **Can you see what they broke.**

## Writing the invariants down

There's a forty-year-old answer to "write the invariants down" that most of us
skipped because it looked like homework.

TLA+ is a specification language. Leslie Lamport built it — the temporal logic
underneath it landed in [TOPLAS in 1994][toplas], the language and tools got a
book in [2002][book], and Lamport picked up the [2013 Turing Award][turing]
along the way. (Not *for* TLA+, worth saying: the citation is about logical
clocks, safety and liveness, replicated state machines and sequential
consistency. TLA+ is downstream of that work, not the reason for the medal.)

Its reputation for being academic is partly earned and mostly out of date. AWS
engineers wrote up their experience in [CACM in 2015][aws] and the useful part
wasn't "we proved our system correct." It was that writing the spec found bugs
before any code existed, in systems their best people had already reviewed.
Their best people had said LGTM.

Here's the part that matters. A TLA+ spec is not a program. It's a formula that
says which state changes are allowed. You describe what may happen, and
everything else — storage, HTTP, whether the button is blue — is out of scope by
construction.

A to-do list, completely:

```tla
VARIABLE tasks

Init == tasks = [i \in Ids |-> "absent"]

Add(i)      == tasks[i] = "absent" /\ tasks' = [tasks EXCEPT ![i] = "open"]
Complete(i) == tasks[i] = "open"   /\ tasks' = [tasks EXCEPT ![i] = "done"]
Reopen(i)   == tasks[i] = "done"   /\ tasks' = [tasks EXCEPT ![i] = "open"]
Delete(i)   == tasks[i] # "absent" /\ tasks' = [tasks EXCEPT ![i] = "absent"]

ClearCompleted ==
  /\ \E i \in Ids : tasks[i] = "done"
  /\ tasks' = [i \in Ids |-> IF tasks[i] = "done" THEN "absent" ELSE tasks[i]]
```

The prime mark means "in the next state." `/\` is *and*. That's most of the
syntax you need. Read `Complete(i)` out loud: the task is open, and afterwards
it is done. Nothing about databases.

Those are the invariants. All of them, for this system, on one screen. That's
the artifact a reviewer never had.

Notice `ClearCompleted` has two halves — the button only exists when something
is done, *and* it leaves everything else alone. Hold that thought.

## Asking the spec about code that already exists

TLA+ ships with TLC, a model checker. TLC explores: it starts at your initial
state, applies every action, and walks the reachable state space looking for a
violation. That's the right question when you're designing a protocol and don't
yet know what your system can do.

With an agent writing code, you're in a different position. You already have the
implementation. You can run it, watch it move from state to state, and collect
the transitions it actually took. Now the question isn't "what could happen?" —
it's whether **these** steps, the ones the code just took, are steps the spec
allows.

TLC can be asked this. Let me be precise, because I got this wrong in an earlier
draft of this post and someone would have caught it: you encode your steps as
data, write a plain safety invariant asserting each one is enabled, and run the
checker. No liveness property, no engineered failure. It works, and it's fast
enough — on the to-do spec, TLC finds the bad step in **0.78 seconds**.

There's published work doing exactly this properly: [*Validating Traces of
Distributed Programs Against TLA+ Specifications*][trace] by Cirstea, Kuppe,
Loillier and Merz — Kuppe maintains the TLA+ tools. They instrument real Java
programs, record traces, and validate them against specs through TLC. If you
want trace validation on a production system, start there, not here.

So what's left?

Two things. First, TLC tells you *that* the step is illegal. It won't tell you
*which conjunct* failed. Second, that 0.78 seconds is almost entirely JVM boot
and SANY parse, paid again on every query — fine once, not fine in a loop that
runs on every agent edit.

Structured blame instead of a boolean, at a couple of orders of magnitude less
latency. That's a narrower pitch than "TLC can't do this," and it has the
advantage of being true.

## tlatools-rs

```bash
cargo install tlatools
```

A TLA+ parser and evaluator in Rust. Not a model checker — it doesn't explore
anything. It answers two questions about states you already have:

```rust
let spec = Spec::from_file("Todo.tla")?;
let eval = Evaluator::new(&spec, constants)?;

eval.holds_at("Init", &state)?;         // legal starting state?
eval.step_allowed("Next", &from, &to)?; // legal step?
```

No JVM, no generated modules, no parsing a counterexample back out of stdout.

The loop is three pieces. **You write the spec** — it's short, it's the part
worth arguing about, and it barely changes. **The agent writes the
implementation** — any language, any framework, any speed. **A script walks the
implementation and asks the spec about every step** — thirty lines: ask the
implementation what it can do, do each of those things, record where you landed,
repeat until nothing new turns up.

```
$ ./check.py impl/correct.py
The implementation refines the specification.
9 states and 35 steps, all permitted.
```

Nine states because there are two tasks and three states each. A real app has
more, and the walk is the expensive part, not the checking.

## What the reviewer couldn't see

Here's an agent-plausible bug. The completion handler takes an id and marks it
done. It doesn't check the task was open — why would it, the button only shows
up on open tasks. (The button. Not the handler.)

This is *exactly* the bug that survives review. It reads correctly. The missing
check is missing somewhere you're not looking.

```
$ ./check.py impl/completes_anything.py
The implementation takes a step the specification does not permit.

  from   a=absent, b=absent
  doing  complete(a)
  to     a=done, b=absent

The closest the specification came:
  Add(i = "a") was available, but does not produce that state,
    because tasks' = [tasks EXCEPT ![i] = Open] does not hold (1 of its 2 clauses hold)
  Complete(i = "a") was not available here,
    because tasks[i] = Open does not hold (1 of its 2 clauses hold)
```

The second line is the bug, named: `Complete` requires the task to be open, and
it wasn't. It's a ranked shortlist rather than a single guess — `Add` also
nearly fits from this state, and saying so is more honest than pretending to
know which one you meant.

Now the other one. `ClearCompleted` — remember it had two halves? This
implementation gets the guard right and the effect wrong. It clears the whole
list:

```
$ ./check.py impl/clear_removes_everything.py
  from   a=open, b=done
  doing  clear_completed
  to     a=absent, b=absent

The closest the specification came:
  ClearCompleted was available, but does not produce that state,
    because tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]
    does not hold (1 of its 2 clauses hold)
```

**"Was not available here" versus "was available, but does not produce that
state."** Different sentences because they're different bugs. One is a missing
guard. The other is a correct guard and a wrong effect — which is worse, because
the button *looks* like it works. You'd demo it. You'd ship it. Someone would
lose a task they hadn't finished. And you'd have approved it, because it looks
good.

The tool can tell them apart because it knows which failing clause mentions the
next state.

## Feedback an agent can act on

You cannot fix what you cannot describe, and neither can a model.

- "Tests failed." — Try again. Randomly.
- "Expected `{a: open}`, got `{a: absent}`." — Better. Now infer the rule.
- "`ClearCompleted` was available, but does not produce that state, because
  `tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]` does
  not hold." — The action, the condition, and the state it was in.

That third one is a prompt. It names the rule that was broken in the language
the rule was written in.

**And I measured whether that helps, and it didn't — not detectably.** 200 tasks,
each attempted with an uninformative retry and with the failure text fed back:
90.5% [85.6–93.8] against 92.5% [88.0–95.4], McNemar exact p=0.125. That is a
null. On the formal-reasoning subset the gap was 46.2% → 69.2%, which looks like
something, except n=13 and p=0.25, which means it looks like something in the
way small numbers often do.

I'm reporting it because I ran it. The honest state of the claim is: the
mechanism is sound, the message is strictly more information than a boolean, and
I do not have evidence it moves the pass rate. If you were going to adopt this
on the strength of "agents do better with good errors," don't yet. Adopt it
because *you* can see the invariants now.

## Using the oracle as a grader

`tlatools check` takes a JSON job — spec, states, steps, constants — and returns
a verdict with an exit status: `0` it refines, `1` it doesn't, `2` the question
was malformed. Which makes it a CI step:

```bash
tlatools check job.json || exit 1
```

Or a scoring function. Point it at N candidate implementations and it tells you
which refine the spec and, for the ones that don't, exactly where they diverge.
If you're generating code, evaluating models, or grading a benchmark, that's a
grader with no rubric to write and no partial credit to argue about. The spec
*is* the rubric.

| | |
| --- | --- |
| `init` | is the starting state legal? |
| `refines` | is every step one the spec permits? |
| `coverage` | does the implementation reach the outcomes it should? |

That third one exists because refinement alone is satisfied perfectly by an
implementation that does nothing. Ask me how I know.

## Is the tool itself any good?

Fair question to ask of anything that grades your code. Three answers, all
reproducible:

**It agrees with the reference implementation.** Over a labelled corpus of 39
cases — six implementations that must pass, thirty-three seeded bugs that must
each be caught — its verdicts are byte-identical to Java TLC's, including
*which* of the three checks catches which bug.

```
$ diff <(tools/tlc_corpus.py --json) <(tools/corpus.py --json)
$
```

Getting that diff empty taught me something I'd otherwise have shipped wrong. A
TLA+ spec is `Init /\ [][Next]_vars`, and `[Next]_vars` means *`Next`, or
nothing changed*. Stuttering is always allowed. I'd been checking bare `Next`,
so any implementation that idled or retried got flagged for a step the spec
permits.

Fixing it made one seeded bug survive: a transfer from an account to itself.
Which felt like a regression until I read the spec again. A self-transfer nets
to zero. It changes nothing. It **is** a stuttering step, and the specification
says stuttering is fine — so the implementation genuinely refines the spec, and
the benchmark and I had both been wrong about it. Catching that bug needs an
abstraction where the operation is visible in the state at all. You can't
tighten a refinement check into seeing something the state space doesn't record.

Hand TLC the same `[Next]_vars` obligation and it passes that mutant too. The
agreement holds; what moved was my understanding of what was being asked.

**It reads the language.** 1,256 of the 1,258 specifications in three public
TLA+ corpora: the examples repo, the community modules, and the TLA+ tools' own
test suite. The two it doesn't read are two that SANY doesn't read either. How
every one of those files is read is recorded in `golden/`, so a change names the
files it changed instead of moving a number.

**It's checked in the boring ways.** 164 tests, clippy-pedantic clean, 85%
mutation coverage, and a robustness suite that feeds back every prefix and every
dropped line of every fixture — because a parser that panics on a half-saved
file is a parser you can't put in a loop. `tla-syntax` and `tla-eval` have no
external dependencies at all.

## What it won't do

- **It's not a model checker.** If you want to know what states your system can
  reach, use TLC. These compose; they don't compete.
- **It doesn't verify your program.** It checks the transitions you hand it. If
  your walk misses a path, nothing checks that path.
- **Your spec can be wrong.** It's what you argued about, not what's true. I
  could have written `ClearCompleted` to clear everything, and then the "buggy"
  implementation would be the correct one. The spec being small and readable is
  the only defence, which is an argument for keeping it small and readable.
- Integers are 64-bit, real arithmetic isn't implemented, temporal formulas are
  refused rather than guessed at, and TLAPS proofs are skipped.

## Try it

```bash
git clone https://github.com/copyleftdev/tlatools-rs
cd tlatools-rs
cargo build --release
demo/todo/check.py demo/todo/impl/completes_anything.py
```

The whole demo is [`demo/todo`][demo] — a 40-line spec, three implementations,
and the script that checks them. CI runs it on every push, so if it's broken
when you get there, that's a bug and I'd like to hear about it.

- **crates:** [tlatools][c-tlatools] · [tla-eval][c-eval] · [tla-syntax][c-syntax] · [tla-oracle][c-oracle]
- **source:** [github.com/copyleftdev/tlatools-rs][repo]
- **docs:** [docs.rs/tla-eval][docs]

If you have a TLA+ file it reads wrongly, that's the most useful thing you can
send me.

The pitch isn't that this makes agents smarter. It's that "looks good to me" was
always a confession, and now there's somewhere to write the invariants down and
something that will read them back to you when they break.

[toplas]: https://dl.acm.org/doi/10.1145/177492.177726
[book]: https://lamport.azurewebsites.net/tla/book.html
[turing]: https://amturing.acm.org/award_winners/lamport_1205376.cfm
[aws]: https://cacm.acm.org/research/how-amazon-web-services-uses-formal-methods/
[trace]: https://arxiv.org/abs/2404.16075
[demo]: https://github.com/copyleftdev/tlatools-rs/tree/main/demo/todo
[repo]: https://github.com/copyleftdev/tlatools-rs
[docs]: https://docs.rs/tla-eval
[c-tlatools]: https://crates.io/crates/tlatools
[c-eval]: https://crates.io/crates/tla-eval
[c-syntax]: https://crates.io/crates/tla-syntax
[c-oracle]: https://crates.io/crates/tla-oracle
