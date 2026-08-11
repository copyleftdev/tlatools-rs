#!/usr/bin/env python3
"""The same corpus, decided by Java TLC through tla-for-ai's own oracle.

Emits the JSON that `tools/corpus.py --json` emits, so the two can be diffed
directly. Any difference is a disagreement between this evaluator and TLC on a
case whose answer is already known.

    diff <(tools/tlc_corpus.py --json) <(tools/corpus.py --json)

The benchmark's `spec_check` obligation is deliberately absent: it model-checks
the trusted specification against its own invariants, which is a reachability
question and not something a ground evaluator answers.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path

BENCH = Path(os.environ.get("TLA_FOR_AI", "../tla-for-ai"))
sys.path.insert(0, str(BENCH))

from harness import oracle, task as task_mod  # noqa: E402

WORK = Path(os.environ.get("TMPDIR", "/tmp")) / "tlc-corpus"


def status_of(verdict) -> str:
    if verdict.passed:
        return "pass"
    failed = [s.status for s in verdict.scenarios if not s.passed]
    return failed[0] if failed else "error"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("tasks", nargs="*")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    tasks = [task_mod.load(t) for t in args.tasks] if args.tasks else task_mod.load_all()
    results = {}
    started = time.monotonic()

    for t in tasks:
        entry = {"reference": None, "mutants": {}}
        entry["reference"] = status_of(
            oracle.check_candidate(t, t.reference_impl, WORK / t.id / "reference")
        )
        for m in t.mutants:
            entry["mutants"][m.stem] = status_of(
                oracle.check_candidate(t, m, WORK / t.id / m.stem)
            )
        results[t.id] = entry
        if not args.json:
            print(f"{t.id}: reference {entry['reference']}, "
                  f"{len(entry['mutants'])} mutants")

    if args.json:
        print(json.dumps(results, indent=2, sort_keys=True))
    else:
        print(f"\n{time.monotonic() - started:.1f}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
