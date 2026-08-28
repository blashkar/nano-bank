# cxo/escalations.py — in-process store of pending personal-manager escalations.
# The DURABLE record is the cx_issues row; this is only the "look now" pointer the
# CXO surfaces. Bounded so a flood can't grow unbounded.
from __future__ import annotations
import threading

_LOCK = threading.Lock()
_PENDING: list[dict] = []
_MAX = 50


def record(item: dict) -> None:
    with _LOCK:
        _PENDING.append(dict(item))
        while len(_PENDING) > _MAX:
            _PENDING.pop(0)


def pending() -> list[dict]:
    with _LOCK:
        return list(_PENDING)


def clear() -> None:
    with _LOCK:
        _PENDING.clear()
