#!/usr/bin/env python3
"""Capture the Build tab's data: the real coder run that carried out the board's
ruling (see debate.py beat 6 — "I would direct the CTO next... engineer the
stagger into the recurrence engine itself"). Unlike meeting/debate, this is not
a scripted round-table beat sequence — it fetches the CTO's actual delegation
and the coder's actual work (reasoning, tool calls, diff, tests, PR) straight
from the coder service's run store, and writes it in the shape the Build tab's
step-through viewer expects.

    python demos/10-ceo/present/fetch_build.py [branch]

With no branch, uses the most recent run the coder has on record.
"""
from __future__ import annotations
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import coder_client  # noqa: E402

OUT_DIR = os.path.join(HERE, "recordings", "build")


def _ruling_excerpt() -> str:
    """The debate's own closing directive, for the Build tab's framing header —
    so it reads as a continuation of the board's ruling, not a new topic."""
    path = os.path.join(HERE, "recordings", "debate", "canonical.json")
    try:
        d = json.load(open(path, encoding="utf-8"))
        answer = d["beats"][-1]["answer"]
    except Exception:  # noqa: BLE001
        return ""
    marker = "### Next Officer to Direct"
    i = answer.find(marker)
    return answer[i:].strip() if i >= 0 else ""


def main() -> int:
    branch = sys.argv[1] if len(sys.argv) > 1 else None
    if not branch:
        branches = coder_client.list_branches()
        if not branches:
            print("no coder runs on record", file=sys.stderr)
            return 1
        branch = branches[-1]
    run = coder_client.fetch_run(branch)
    if not run:
        print(f"could not fetch run for branch {branch!r}", file=sys.stderr)
        return 1

    rec = dict(run)
    rec["ruling_excerpt"] = _ruling_excerpt()

    os.makedirs(OUT_DIR, exist_ok=True)
    out = os.path.join(OUT_DIR, "canonical.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(rec, f, ensure_ascii=False, indent=2)
    print(f"wrote {out}  (branch={branch}, outcome={rec.get('outcome')}, "
          f"steps={len(rec.get('steps') or [])}, diff={len(rec.get('diff') or '')} chars)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
