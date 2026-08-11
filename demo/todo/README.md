# A to-do list, checked against what a to-do list is

A worked example: a 40-line TLA+ specification of a to-do list, three Python
implementations, and a script that asks the specification about each one.

```console
$ cargo build --release
$ demo/todo/check.py demo/todo/impl/correct.py
== correct.py
The implementation refines the specification.
9 states and 35 steps, all permitted.
```

Nothing in the implementation knows the specification exists. Nothing in the
specification knows the implementation is Python.

## The two bugs

**A missing guard.** The complete button doesn't check that the task is open —
a plausible thing to write, and a very plausible thing for a language model to
write:

```console
$ demo/todo/check.py demo/todo/impl/completes_anything.py
The implementation takes a step the specification does not permit.

  from   a=absent, b=absent
  doing  complete(a)
  to     a=done, b=absent

The closest the specification came:
  Add(i = "a") was available, but does not produce that state,
    because tasks' = [tasks EXCEPT ![i] = Open] does not hold (1 of its 2 clauses do)
  Complete(i = "a") was not available here,
    because tasks[i] = Open does not hold (1 of its 2 clauses do)
```

The second line is the bug, named: `Complete` requires the task to be open, and
it wasn't. The tool offers a ranked shortlist rather than pretending to know
which one you meant — `Add` also nearly fits, and saying so is honest.

**A wrong effect.** "Clear completed" empties the whole list. The guard is
right — the button only appears when something is done — so it looks like it
works, right up until you lose an open task:

```console
$ demo/todo/check.py demo/todo/impl/clear_removes_everything.py
  from   a=open, b=done
  doing  clear_completed
  to     a=absent, b=absent

The closest the specification came:
  ClearCompleted was available, but does not produce that state,
    because tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]
    does not hold (1 of its 2 clauses do)
```

Note the difference in wording. One says the action *was not available*; the
other says it *was available but produced the wrong thing*. Those are different
bugs and they want different fixes, and the tool can tell them apart because it
knows which failing clause mentions the next state.

## Why this needs an evaluator rather than a model checker

TLC would find these too — by exploring the specification's own state space and
comparing. But it answers a different question: *what states can this reach?*

Here the states are already known. They came from running the implementation.
The question is whether the specification permits **these** steps, and a model
checker cannot be asked that directly — you have to encode each step as a
two-state trace and make a liveness property fail to get an answer out.

Asking directly is what makes the counterexample good. The specification is
evaluated *at the offending pair of states*, so it can say which action came
closest and which clause stopped it. A search that fails to find a pair has
nothing to report but its own failure.

## The agentic loop

The three pieces fit together like this:

1. **A person writes the specification.** It is short, it is the part that is
   worth arguing about, and it does not change often. `Todo.tla` is 40 lines
   and describes every to-do list that has ever been correct.

2. **An agent writes the implementation**, in whatever language, with whatever
   framework, at whatever speed.

3. **The oracle checks the second against the first**, and when they disagree,
   says which action the code was reaching for and which clause it broke.

Step 3 is what makes step 2 safe to automate. `Complete(i = "a") was not
available here, because tasks[i] = Open does not hold` is a sentence an agent
can act on: it names the action, the condition, and the state. Compare "tests
failed", or a model checker's trace that stutters at index 37.

The loop is worth running because the two halves fail differently. A person
writing a specification is thinking about what is allowed. An agent writing an
implementation is thinking about what to do next. Bugs live in the gap, and
this is a machine for finding that gap and describing it in words.

## Files

| | |
| --- | --- |
| `Todo.tla` | the specification — three task states and the moves between them |
| `impl/correct.py` | an implementation that agrees with it |
| `impl/completes_anything.py` | a missing guard |
| `impl/clear_removes_everything.py` | a wrong effect |
| `check.py` | walks an implementation, asks the specification, prints the answer |

`check.py` is 130 lines and most of it is the walk. The checking is one
subprocess call and one JSON document.
