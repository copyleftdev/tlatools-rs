---
title: "Write down every guarantee before you write any code"
published: false
tags: ai, rust, formalmethods, testing
---

Here is every promise a to-do list makes.

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

Not a summary. Not the important ones. **All of them.** A task cannot go from
absent straight to done. Clearing completed items leaves the open ones alone.
You cannot delete something that was never there. Nine lines, and when you've
read them you have read the entire contract.

Now go find that list for the system you work on.

You can't. It doesn't exist. It's distributed across a test suite that asserts
outcomes rather than rules, some validation scattered through handlers, and the
memory of whoever's been there longest. The guarantees are real — your users
depend on every one of them — and there is no file you can open to see them.

That's the gap I want to talk about, because you can close it in an afternoon,
and because something has changed recently that makes closing it pay for itself.

## The prime mark and two operators

That's most of the syntax, so let's get it out of the way.

`tasks'` means "tasks, in the next state." `/\` is *and*. `\E` is "there
exists." A definition like `Complete(i)` is a formula relating the current state
to the next one — read it out loud: *the task is open, and afterwards it is
done.*

That's it. That's the language, near enough, for this purpose.

[The real file][demo] adds about eight lines of scaffolding around what you saw:
a module header, a `TypeOK` saying a task is always in exactly one of the three
states, and the two lines that tie the actions together —

```tla
Next == \/ \E i \in Ids : Add(i) \/ Complete(i) \/ Reopen(i) \/ Delete(i)
        \/ ClearCompleted

Spec == Init /\ [][Next]_tasks
```

`Next` is "any one of the moves happens." `Spec` is "start legally, and then only
ever make legal moves." That second line turns out to matter more than it looks,
and I'll come back to it.

Notice what isn't in there. No database. No HTTP. No mention of whether the
button is blue or whether completion is optimistic in the UI. A specification
isn't a program and doesn't compile to one — it's a formula that says which
state changes are permitted. Everything else is out of scope by construction,
which is exactly why the list can be nine lines and still be complete.

And notice `ClearCompleted` has two halves: the button only exists when
something is done, **and** it leaves everything else alone. Two separate
promises in one action. Hold that thought.

## The list is short, finite, and worth arguing about

The objection I expect is that a real system's list would be enormous.

It's smaller than you think, because it's a list of *rules*, not behaviours. The
behaviours are combinatorial — nine states here, and a real system has
astronomically many. The rules that generate them are not. Five actions cover
every to-do list that has ever been correct.

It's also the part of the design worth arguing about. When two engineers
disagree about whether reopening a completed task should be allowed, that
argument currently happens in a code review, in a comment thread, three weeks
after someone already built one of the answers. Written as a spec, the argument
takes four minutes and happens before anyone opens an editor.

That's the [AWS result][aws], really. They wrote up their experience in CACM in
2015 and the headline everyone quotes is about proving systems correct. The part
that actually replicates is quieter: **writing the spec found bugs before any
code existed** — in systems their best engineers had already designed and
reviewed. Not bugs the tests missed. Bugs the *design* had, findable by writing
the guarantees down and reading them back.

This is forty-year-old technology, and most of us skipped it because it looked
like homework. TLA+ is Leslie Lamport's; the temporal logic underneath it landed
in [TOPLAS in 1994][toplas], the language and tools got a [book in 2002][book],
and Lamport picked up the [2013 Turing Award][turing] along the way. (Not *for*
TLA+, worth saying, since people get this wrong: the citation is logical clocks,
safety and liveness, replicated state machines, sequential consistency. TLA+ is
downstream of that work, not the reason for the medal.)

Its reputation for being academic is partly earned and mostly out of date. You
do not need the proof system. You do not need to verify anything. You need the
part where you write the guarantees down.

## What changed

Writing the list has always been worth it and has always been easy to defer,
because the code was going to be written slowly by people who mostly remembered
the rules.

That is no longer the situation. Something else is writing the code now, quickly,
and it does not remember anything. It has never met your system's rules and has
no way to infer the ones that aren't in the file it's looking at. It will write
something plausible.

Plausible is the problem. Plausible code passes review — this is where "looks
good to me" comes from, and it was always an honest confession: the reviewer is
reporting that nothing jumped out, because checking against the full set of
invariants was never an option. Nobody had the list.

So: write the list. Then check the generated code against it, mechanically,
every time. That second half needs a tool.

## tlatools-rs

```bash
cargo install tlatools
```

A TLA+ parser and evaluator in Rust. Not a model checker — it doesn't explore
anything. It answers questions about states you already have:

```rust
let spec = Spec::from_file("Todo.tla")?;
let eval = Evaluator::new(&spec, constants)?;

eval.holds_at("Init", &state)?;         // legal starting state?
eval.step_allowed("Next", &from, &to)?; // legal step?
```

The loop is three pieces. **You write the list** — short, arguable, and it
barely changes. **The agent writes the implementation** — any language, any
framework, any speed. **A script walks the implementation and asks the list
about every step it takes.** That third piece is thirty lines: ask the
implementation what it can do, do each of those things, record where you landed,
repeat until nothing new turns up.

```
$ ./check.py impl/correct.py
The implementation refines the specification.
9 states and 35 steps, all permitted.
```

Nine states because there are two tasks and three states each. A real app has
more, and the walk is the expensive part, not the checking.

## Two bugs the list catches

Here's an agent-plausible one. The completion handler takes an id and marks it
done. It doesn't check the task was open — why would it, the button only shows
up on open tasks. (The button. Not the handler.)

This is exactly the bug that survives review. It reads correctly. The missing
check is missing somewhere you aren't looking.

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
it wasn't. It's a ranked shortlist rather than a single guess — `Add` also nearly
fits from this state, and saying so is more honest than pretending to know which
one you meant.

Now the other one. `ClearCompleted` — the action with two promises. This
implementation keeps the first and breaks the second. It clears the whole list:

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
lose a task they hadn't finished.

The tool can tell them apart because it knows which failing clause mentions the
next state. Neither bug is exotic. Both are invisible to a test suite that
checks outcomes, and both are named instantly by a list you wrote in nine lines.

## Feedback in the language the rule was written in

You cannot fix what you cannot describe, and neither can a model.

- "Tests failed." — Try again. Randomly.
- "Expected `{a: open}`, got `{a: absent}`." — Better. Now infer the rule.
- "`ClearCompleted` was available, but does not produce that state, because
  `tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]` does
  not hold." — The action, the condition, and the state it was in.

That third one is a prompt.

**And I measured whether it helps an agent, and it didn't — not detectably.**
200 tasks, each attempted with an uninformative retry and with the failure text
fed back: 90.5% [85.6–93.8] against 92.5% [88.0–95.4], McNemar exact p=0.125.
That is a null. On the formal-reasoning subset the gap was 46.2% → 69.2%, which
looks like something, except n=13 and p=0.25, which means it looks like
something in the way small numbers often do.

I'm reporting it because I ran it. The honest state of the claim: the mechanism
is sound, the message is strictly more information than a boolean, and I have no
evidence it moves the pass rate. If you were going to adopt this because "agents
do better with good errors" — don't, yet. Adopt it because **you** now have the
list, and something checks it.

## The list as a grader

`tlatools check` takes a JSON job — spec, states, steps, constants — and returns
a verdict with an exit status: `0` it refines, `1` it doesn't, `2` the question
was malformed.

```bash
tlatools check job.json || exit 1
```

Point it at N candidate implementations and it tells you which satisfy the list
and, for the ones that don't, exactly where they diverge. If you're generating
code, evaluating models, or grading a benchmark, that's a grader with no rubric
to write and no partial credit to argue about. The list *is* the rubric.

| | |
| --- | --- |
| `init` | is the starting state legal? |
| `refines` | is every step one the spec permits? |
| `coverage` | does the implementation reach the outcomes it should? |

That third one exists because refinement alone is satisfied perfectly by an
implementation that does nothing. Ask me how I know.

## Can you trust the checker?

Fair question to ask of anything that grades your code.

**It agrees with the reference implementation.** Over a labelled corpus of 39
cases — six implementations that must pass, thirty-three seeded bugs that must
each be caught — its verdicts are byte-identical to Java TLC's, including *which*
of the three checks catches which bug.

Getting that diff empty taught me something I'd otherwise have shipped wrong,
and it's the best argument in this article for writing guarantees down precisely.

Remember `Spec == Init /\ [][Next]_tasks`, the line I said would matter. Those
brackets are load-bearing: `[Next]_tasks` means *`Next`, **or nothing changed***.
Stuttering is always permitted, in every TLA+ specification ever written. I had
been checking bare `Next`, so any implementation that idled or retried got
flagged for a step the spec explicitly allows.

Fixing that made one seeded bug survive: a transfer from a bank account to
itself. Which felt like a regression, until I read the spec again. A
self-transfer nets to zero. It changes nothing. It **is** a stuttering step, and
the spec says stuttering is fine — so that implementation genuinely satisfies
the list, and the benchmark and I had both been wrong about it. Catching that
one needs an abstraction where the operation is visible in the state at all. You
cannot tighten a refinement check into seeing something the state space doesn't
record.

Hand TLC the same `[Next]_vars` obligation and it passes that mutant too. The
agreement holds; what moved was my understanding of what I'd written down. The
list is only as good as your reading of it, and a tool that disagrees with you
is doing you a favour.

**It reads the language.** 1,256 of the 1,258 specifications in three public
TLA+ corpora: the examples repo, the community modules, and the TLA+ tools' own
test suite. The two it doesn't read are two that SANY, the official parser,
doesn't read either. How every one of those files is read is recorded in
`golden/`, so a change names the files it changed instead of moving a number.

**It's checked in the boring ways.** 166 tests, clippy-pedantic clean, and a
robustness suite that feeds back every prefix and every dropped line of every
fixture — because a parser that panics on a half-saved file is a parser you
can't put in a loop. `tla-syntax` and `tla-eval` have no external dependencies at
all.

## About TLC, precisely

TLC is the model checker TLA+ ships with. It explores: from your initial state,
apply every action, walk the reachable state space looking for a violation.
That's the right question when you're designing a protocol and don't yet know
what your system can do.

Here you already have the implementation, so the question is different — are
*these* steps, the ones the code just took, permitted by the list?

**TLC can be asked that.** I want to be precise, because I had this wrong in an
earlier draft and someone would have caught it: you encode the steps as data,
write a plain safety invariant asserting each one is enabled, and run the
checker. No liveness property, no engineered failure. On the to-do spec it finds
the bad step in 0.78 seconds. There's published work doing this properly —
[*Validating Traces of Distributed Programs Against TLA+ Specifications*][trace]
by Cirstea, Kuppe, Loillier and Merz, and Kuppe maintains the TLA+ tools. If you
want trace validation on a production system, start there.

What's left is narrower, and true. TLC tells you *that* the step is illegal, not
*which conjunct* failed. And that 0.78 s is almost entirely JVM boot and SANY
parse, paid again on every query — fine once, not fine in a loop that runs on
every agent edit.

Structured blame instead of a boolean, at a couple of orders of magnitude less
latency. That's the pitch. These compose; they don't compete.

## What it won't do

- **It's not a model checker.** If you want to know what states your system can
  reach, use TLC.
- **It doesn't verify your program.** It checks the transitions you hand it. If
  your walk misses a path, nothing checks that path.
- **Your list can be wrong.** It's what you argued about, not what's true. I
  could have written `ClearCompleted` to clear everything, and then the "buggy"
  implementation would be the correct one. The list being short and readable is
  the only defence, which is an argument for keeping it short and readable.
- Integers are 64-bit, real arithmetic isn't implemented, temporal formulas are
  refused rather than guessed at, and TLAPS proofs are skipped.

## Try it

```bash
git clone https://github.com/copyleftdev/tlatools-rs
cd tlatools-rs
cargo build --release
demo/todo/check.py demo/todo/impl/completes_anything.py
```

The whole demo is [`demo/todo`][demo] — the spec above, three implementations,
and the script that checks them. CI runs it on every push, so if it's broken
when you get there, that's a bug and I'd like to hear about it.

- **crates:** [tlatools][c-tlatools] · [tla-eval][c-eval] · [tla-syntax][c-syntax] · [tla-oracle][c-oracle]
- **source:** [github.com/copyleftdev/tlatools-rs][repo]
- **docs:** [docs.rs/tla-eval][docs]

If you have a TLA+ file it reads wrongly, that's the most useful thing you can
send me.

---

Start with one. Pick the part of your system where a wrong state change would
actually hurt — the money, the permissions, the thing with a state machine
nobody fully trusts. Write down what it's allowed to do. It'll take an afternoon
and it will be shorter than you expect.

You'll find something while writing it. Everyone does; that's the AWS result and
it isn't subtle. And then you'll have the file — the one that doesn't exist for
any system you currently work on — and everything written afterwards, by you or
by a machine, can be checked against it.

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
