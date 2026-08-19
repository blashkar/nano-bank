#!/usr/bin/env python3
"""Build the self-contained animated boardroom page: inline both canonical
recordings into boardroom.template.html and write boardroom.html. The recordings
carry every officer's captured speech (contributions[].text), so the page replays
the whole C-suite session with ZERO model delay — open the file in a browser.

    python demos/10-ceo/present/build_boardroom.py
"""
from __future__ import annotations
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
TEMPLATE = os.path.join(HERE, "boardroom.template.html")
OUT = os.path.join(HERE, "boardroom.html")
REC = os.path.join(HERE, "recordings")
MARKER = "/*__RECORDINGS__*/"


def _load(name: str) -> dict:
    path = os.path.join(REC, name, "canonical.json")
    if not os.path.exists(path):
        return {"beats": []}
    with open(path, encoding="utf-8") as f:
        d = json.load(f)
    # keep only what the page needs (beats: title/question/contributions/answer/outcome)
    beats = []
    for b in d.get("beats", []):
        beats.append({
            "beat": b.get("beat"), "title": b.get("title", ""),
            "question": b.get("question", ""), "shows": b.get("shows", ""),
            "contributions": [{"officer": c.get("officer"), "role": c.get("role"),
                               "text": c.get("text", ""), "acted": c.get("acted")}
                              for c in b.get("contributions", [])],
            "answer": b.get("answer", ""), "outcome": b.get("outcome", {}),
        })
    return {"beats": beats}


def build() -> str:
    with open(TEMPLATE, encoding="utf-8") as f:
        html = f.read()
    data = {"meeting": _load("meeting"), "debate": _load("debate")}
    payload = json.dumps(data, ensure_ascii=False)
    # inline: replace the marker (which precedes the default {} literal) AND the
    # default literal that follows it, so `const RECORDINGS = <payload>;`.
    idx = html.index(MARKER)
    end = html.index(";", idx)
    html = html[:idx] + payload + html[end:]
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(html)
    return OUT


if __name__ == "__main__":
    out = build()
    d = json.loads(open(out, encoding="utf-8").read().split("const RECORDINGS = ", 1)[1]
                   .split(";\n", 1)[0])
    for k, v in d.items():
        print(f"  {k}: {len(v['beats'])} beats,",
              sum(len(b['contributions']) for b in v['beats']), "contributions")
    print("wrote", out)
