#!/usr/bin/env python3
"""Check a to-do implementation against Todo.tla.

    ./check.py impl/correct.py
    ./check.py impl/completes_anything.py

Three steps, and only the middle one is interesting:

  1. Walk the implementation. Ask it what it can do, do each of those things,
     and keep going until nothing new turns up. That gives every state it can
     reach and every step it can take between them — a few dozen of each here.

  2. Hand those to `tlatools check`, which asks the specification about each
     one. This is the part TLC cannot do: the states are already known, and
     the question is whether the specification permits *these* steps.

  3. Print the answer the way an agent would receive it.

Nothing in the implementation knows the specification exists, and nothing in
the specification knows the implementation is written in Python.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
IDS = ["a", "b"]

# How to read the implementation's states as TLA+ values. JSON cannot tell a
# set from a sequence, so the shape has to be said out loud.
SCHEMA = {"tasks": {"kind": "map", "of": {"kind": "str"}}}
CONSTANTS = {
    "Ids": {"value": IDS, "schema": {"kind": "set", "of": {"kind": "str"}}}
}


def load(path: Path):
    sys.path.insert(0, str(path.resolve().parent))
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.Todo


def explore(Todo, limit: int = 500):
    """Every state the implementation can reach, and every step between them."""
    import copy

    def key(state):
        return json.dumps(state, sort_keys=True)

    root = Todo()
    states, index, edges = [root.state()], {key(root.state()): 0}, []
    frontier = [root]
    while frontier and len(states) < limit:
        instance = frontier.pop()
        here = index[key(instance.state())]
        for action in instance.actions():
            child = copy.deepcopy(instance)
            child.step(copy.deepcopy(action))
            state = child.state()
            if key(state) not in index:
                index[key(state)] = len(states)
                states.append(state)
                frontier.append(child)
            label = f"{action[0]}({action[1]})" if action[1] else action[0]
            edges.append([here, index[key(state)], label])
    return states, edges


def check(states, edges):
    job = {
        "spec": (HERE / "Todo.tla").read_text(),
        "constants": CONSTANTS,
        "schema": SCHEMA,
        "states": states,
        "edges": edges,
    }
    binary = HERE.parent.parent / "target/release/tlatools"
    if not binary.exists():
        sys.exit("build it first: cargo build --release")
    done = subprocess.run(
        [str(binary), "check", "-"], input=json.dumps(job),
        capture_output=True, text=True,
    )
    if not done.stdout.strip():
        sys.exit(done.stderr.strip() or "tlatools said nothing")
    return json.loads(done.stdout)


def describe(states, report) -> str:
    """The verdict, worded the way you would put it to whoever wrote the code."""
    if report["status"] == "pass":
        stats = report["stats"]
        return (
            f"The implementation refines the specification.\n"
            f"{stats['states']} states and {stats['edges']} steps, all permitted."
        )

    if report["status"] != "refines":
        return f"{report['status']}: {report['detail']}"

    edge = report["edge"]
    lines = [
        "The implementation takes a step the specification does not permit.",
        "",
        f"  from   {render(states[edge['source']])}",
        f"  doing  {edge['label']}",
        f"  to     {render(states[edge['target']])}",
        "",
        "The closest the specification came:",
    ]
    for blocked in report.get("blocked", [])[:3]:
        why = (
            "was available, but does not produce that state"
            if blocked["about_next_state"]
            else "was not available here"
        )
        lines.append(
            f"  {blocked['action']} {why},"
            f" because {blocked['conjunct']} does not hold"
            f" ({blocked['satisfied']} of its {blocked['total']} clauses do)"
        )
    return "\n".join(lines)


def render(state) -> str:
    return ", ".join(f"{k}={v}" for k, v in sorted(state["tasks"].items()))


def main() -> int:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    path = Path(sys.argv[1])
    states, edges = explore(load(path))
    report = check(states, edges)
    print(f"== {path.name}")
    print(describe(states, report))
    return 0 if report["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
