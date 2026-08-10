#!/usr/bin/env python3
"""Run the tla-for-ai benchmark corpus through the Rust oracle.

The benchmark ships, for every task, a reference implementation that must pass
and a set of mutants that must each be caught -- together with which arm of the
oracle catches them. That is a labelled corpus with known-correct answers, and
it is the strongest available check that this oracle decides what TLC decides.

State-space exploration still belongs to the benchmark's own worker; only the
verdict is computed here.

    tools/corpus.py                     # every task
    tools/corpus.py raft_election       # one
    tools/corpus.py --json              # machine-readable, for diffing
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BENCH = Path(os.environ.get("TLA_FOR_AI", "/home/ops/Project/tla-for-ai"))

DEFAULT_LIMITS = {
    "max_states": 400,
    "max_depth": 25,
    "max_paths": 300,
    "max_actions": 64,
    "max_memory_mb": 1024,
    "explore_timeout": 60,
}


def tlatools_binary() -> Path:
    path = REPO / "target" / "release" / "tlatools"
    if not path.exists():
        path = REPO / "target" / "debug" / "tlatools"
    if not path.exists():
        sys.exit("build the oracle first: cargo build --release")
    return path


def load_tasks(only: list[str]) -> list[tuple[str, dict, Path]]:
    tasks = []
    for d in sorted((BENCH / "tasks").iterdir()):
        if not (d / "task.json").exists():
            continue
        if only and d.name not in only:
            continue
        tasks.append((d.name, json.loads((d / "task.json").read_text()), d))
    if not tasks:
        sys.exit(f"no tasks found under {BENCH / 'tasks'}")
    return tasks


def explore(impl: Path, meta: dict) -> dict:
    """Run the benchmark's own explorer to get the implementation's state graph."""
    limits = {**DEFAULT_LIMITS, **meta.get("limits", {})}
    job = {
        "scenarios": {n: s["config"] for n, s in meta["scenarios"].items()},
        "limits": limits,
    }
    with tempfile.TemporaryDirectory() as tmp:
        job_path = Path(tmp) / "job.json"
        job_path.write_text(json.dumps(job))
        proc = subprocess.run(
            [sys.executable, str(BENCH / "harness" / "worker.py"), str(impl), str(job_path)],
            capture_output=True,
            text=True,
            timeout=limits["explore_timeout"],
        )
    if not proc.stdout.strip():
        return {"ok": False, "error": proc.stderr[-400:] or "worker produced no output"}
    return json.loads(proc.stdout)


def spec_source(task_dir: Path) -> str:
    files = sorted((task_dir / "spec").glob("*.tla"))
    if len(files) != 1:
        sys.exit(f"{task_dir.name}: expected exactly one .tla, found {len(files)}")
    return files[0].read_text()


def verdict(binary: Path, meta: dict, spec: str, scenario: dict, graph: dict) -> dict:
    job = {
        "spec": spec,
        "init_op": meta.get("init_op", "Init"),
        "next_op": meta.get("next_op", "Next"),
        "constants": meta.get("constants", {}),
        "schema": meta["schema"],
        "states": graph["states"],
        "edges": graph["edges"],
        "outcomes": scenario.get("outcomes", {}),
    }
    proc = subprocess.run(
        [str(binary), "check", "-"],
        input=json.dumps(job),
        capture_output=True,
        text=True,
    )
    if not proc.stdout.strip():
        return {"status": "error", "detail": proc.stderr.strip()[-400:]}
    return json.loads(proc.stdout)


def check_candidate(binary: Path, meta: dict, spec: str, impl: Path) -> dict:
    """One verdict per candidate: the first scenario that fails decides it."""
    graphs = explore(impl, meta)
    if not graphs.get("ok"):
        return {"status": "crash", "detail": graphs.get("error", "")[:300]}
    for name, scenario in meta["scenarios"].items():
        report = verdict(binary, meta, spec, scenario, graphs["scenarios"][name])
        if report["status"] != "pass":
            report["scenario"] = name
            return report
    return {"status": "pass", "detail": "all scenarios pass"}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("tasks", nargs="*")
    ap.add_argument("--json", action="store_true", help="emit results as JSON")
    args = ap.parse_args()

    binary = tlatools_binary()
    results, ok = {}, True

    for task_id, meta, task_dir in load_tasks(args.tasks):
        spec = spec_source(task_dir)
        entry = {"reference": None, "mutants": {}}

        ref = check_candidate(binary, meta, spec, task_dir / "reference" / "impl.py")
        entry["reference"] = ref["status"]
        if ref["status"] != "pass":
            ok = False

        mutants = sorted(
            p for p in (task_dir / "mutants").glob("*.py") if not p.name.startswith("_")
        )
        for m in mutants:
            v = check_candidate(binary, meta, spec, m)
            entry["mutants"][m.stem] = v["status"]
            if v["status"] == "pass":
                ok = False

        results[task_id] = entry
        if not args.json:
            report(task_id, entry, ref)

    if args.json:
        print(json.dumps(results, indent=2, sort_keys=True))
    else:
        print("\ncorpus " + ("PASSED" if ok else "FAILED"))
    return 0 if ok else 1


def report(task_id: str, entry: dict, ref: dict) -> None:
    survived = [m for m, s in entry["mutants"].items() if s == "pass"]
    good = entry["reference"] == "pass" and not survived
    print(f"{'OK  ' if good else 'BAD '} {task_id}")
    if entry["reference"] == "pass":
        print("  reference                    PASS")
    else:
        print(f"  reference                    {entry['reference'].upper()}  "
              f"<-- must pass: {ref.get('detail', '')[:120]}")
    for name, status in entry["mutants"].items():
        if status == "pass":
            print(f"  {name:<28} SURVIVED  <-- oracle failed to catch this")
        else:
            print(f"  {name:<28} caught by {status}")


if __name__ == "__main__":
    raise SystemExit(main())
