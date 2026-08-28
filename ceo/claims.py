"""Directive-honesty guard for the Agent CEO. Domain FIGURES are grounded by the
shared number verifier (consult-tool outputs are tool results in the trace); the
CEO's unique integrity risk is overclaiming a DIRECTIVE — saying an officer acted
when the read-back proved no lever fired. Deterministic, cue-based, no LLM."""
from __future__ import annotations
import re

# Words that assert the directive was carried out (vs merely proposed / declined).
_COMPLETION = re.compile(
    r"\b(done|executed|completed|carried out|actioned|implemented|successfully"
    r"|was (?:cut|rolled back|done|executed)|has (?:cut|run|executed|rolled back)"
    r"|cut the batch|rolled back)\b", re.I)

# Bare word-matching on _COMPLETION can't tell "was executed" from "was NOT
# executed" — and the honest report this guard exists to allow ("no lever
# fired, the batch was not cut") uses exactly the completion vocabulary it's
# watching for. A negator in the few words right before the match means the
# sentence is denying completion, not claiming it.
_NEGATORS = {"not", "never", "no", "n't", "didn't", "doesn't", "wasn't", "weren't",
            "hasn't", "haven't", "isn't", "aren't", "won't", "wouldn't", "couldn't",
            "shouldn't", "can't", "cannot", "failed", "unable"}
_WORD = re.compile(r"[\w']+")

# A false officer_acted in a direct_* tool output, tolerant of dict OR str(dict).
_ACTED_FALSE = re.compile(r"officer_acted['\"]?\s*[:=]\s*(?:False|false)")


def _preceded_by_negation(answer: str, match_start: int, lookback_words: int = 5) -> bool:
    words = _WORD.findall(answer[:match_start].lower())
    return any(w in _NEGATORS for w in words[-lookback_words:])


def _has_completion_claim(answer: str) -> bool:
    return any(not _preceded_by_negation(answer, m.start())
              for m in _COMPLETION.finditer(answer))


def _peers_without_lever(trace: list[dict]) -> list[str]:
    out: list[str] = []
    for ev in trace:
        if ev.get("kind") != "tool":
            continue
        name = ev.get("name") or ""
        if not name.startswith("direct_"):
            continue
        raw = ev.get("output")
        acted_false = False
        if isinstance(raw, dict):
            acted_false = raw.get("officer_acted") is False
        else:
            acted_false = bool(_ACTED_FALSE.search(str(raw)))
        if acted_false:
            out.append(name[len("direct_"):])
    return out


def unsupported_claims(answer: str, trace: list[dict]) -> list[str]:
    peers = _peers_without_lever(trace)
    if not peers or not _has_completion_claim(answer or ""):
        return []
    return [f"claimed a directive to the {p.upper()} completed, but the read-back "
            f"showed no lever fired (officer_acted=false)" for p in dict.fromkeys(peers)]
