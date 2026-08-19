"""The CEO's ONLY writer: read-back the peer's ledger rows and append the CEO's own
directive row to the existing hash-chained agent_action_ledger via
append_agent_action('ceo', …). psycopg2, mirroring finance/db.py. No bank change."""
from __future__ import annotations
import json
from typing import Callable, Optional


class Audit:
    def __init__(self, db_params: dict, connect: Optional[Callable] = None):
        self._db = db_params
        self._connect = connect or (lambda: __import__("psycopg2").connect(**db_params))

    def latest_actor_seq(self, actor: str) -> int:
        conn = self._connect()
        try:
            with conn.cursor() as cur:
                cur.execute(
                    "SELECT COALESCE(MAX(seq),0) FROM agent_action_ledger WHERE actor=%s",
                    (actor,))
                row = cur.fetchone()
                return int(row[0]) if row else 0
        finally:
            conn.close()

    def rows_since(self, actor: str, seq: int) -> list[dict]:
        conn = self._connect()
        try:
            with conn.cursor() as cur:
                cur.execute(
                    "SELECT seq, action, effect FROM agent_action_ledger "
                    "WHERE actor=%s AND seq>%s ORDER BY seq", (actor, seq))
                return [{"seq": r[0], "action": r[1], "effect": r[2]}
                        for r in cur.fetchall()]
        finally:
            conn.close()

    def direct(self, peer: str, params: dict, effect: dict) -> dict:
        conn = self._connect()
        try:
            with conn:
                with conn.cursor() as cur:
                    cur.execute(
                        "SELECT seq, entry_hash FROM "
                        "append_agent_action('ceo', %s, %s::jsonb, %s::jsonb)",
                        (f"direct_{peer}", json.dumps(params), json.dumps(effect)))
                    row = cur.fetchone()
                    return {"seq": row[0], "entry_hash": row[1]}
        finally:
            conn.close()
