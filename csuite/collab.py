"""Shared C-suite consult/direct primitive (board-vision item #1).

`consult_<peer>` relays a peer officer's grounded `/ask` answer, attributed.
`direct_<peer>` POSTs an imperative to the peer's `/ask` — the peer's OWN agent
self-verifies and acts via its EXISTING audited lever — then reads back the peer's
fresh ledger row to prove a lever fired and records a directing-actor directive row
via an injected AuditPort. Wired into the CEO first; reusable by the Phase-2 board.
"""
from __future__ import annotations
from typing import Optional, Protocol, runtime_checkable

from langchain_core.tools import StructuredTool


async def post_ask(base_url: str, message: str, client=None) -> dict:
    url = base_url.rstrip("/") + "/ask"
    if client is not None:
        r = await client.post(url, json={"message": message})
    else:
        import httpx
        async with httpx.AsyncClient(timeout=600) as c:
            r = await c.post(url, json={"message": message})
    r.raise_for_status()
    return r.json()


def consult_tool(peer: str, base_url: str, *, client=None) -> StructuredTool:
    async def _consult(question: str) -> dict:
        resp = await post_ask(base_url, question, client)
        return {"officer": peer, "answer": resp.get("answer", "")}

    return StructuredTool.from_function(
        coroutine=_consult, name=f"consult_{peer}",
        description=(f"Consult the {peer.upper()} and relay its grounded answer "
                     "to a question, attributed. Read-only: the officer analyses "
                     "and reports; it does not act."))


@runtime_checkable
class AuditPort(Protocol):
    def latest_actor_seq(self, actor: str) -> int: ...
    def rows_since(self, actor: str, seq: int) -> list[dict]: ...
    def direct(self, peer: str, params: dict, effect: dict) -> dict: ...


def direct_tool(peer: str, base_url: str, audit: AuditPort, *,
                client=None) -> StructuredTool:
    async def _direct(directive: str, rationale: str = "") -> dict:
        before = audit.latest_actor_seq(peer)
        resp = await post_ask(base_url, directive, client)
        officer_response = resp.get("answer", "")
        new = audit.rows_since(peer, before)
        # A ledger-seq snapshot proves something landed for `peer` in this
        # window — it isn't a token tying a row to THIS directive specifically.
        # Exactly one new row is the attributable case. More than one (a second
        # writer landed in the same window — a concurrent directive, or any
        # other trigger of the officer's lever) can't be honestly pinned on
        # this call alone, so say so instead of silently taking the last row
        # as "the" result.
        ambiguous = len(new) > 1
        officer_row = new[-1] if len(new) == 1 else None
        effect = {"officer_acted": bool(new),
                  "officer_row": officer_row,
                  "officer_response": officer_response,
                  "ambiguous": ambiguous}
        if ambiguous:
            effect["candidate_rows"] = new
        audit.direct(peer, {"directive": directive, "rationale": rationale}, effect)
        return {"peer": peer, "directive": directive, **effect}

    return StructuredTool.from_function(
        coroutine=_direct, name=f"direct_{peer}",
        description=(
            f"DIRECT the {peer.upper()} to act on an imperative. The {peer.upper()}'s "
            "own agent self-verifies and acts via its audited lever — you bypass no "
            "guardrail. This reads back the officer's fresh ledger row to prove a "
            "lever actually fired (officer_acted=false means the officer only "
            "deliberated or refused; ambiguous=true means more than one row landed "
            "in the window and it can't be honestly pinned on this directive alone — "
            "report that uncertainty, don't guess), and records the CEO directive "
            "row. Pass a `directive` (the imperative) and a `rationale` (your "
            "grounded why)."))


def build_tools(registry: dict, audit: AuditPort, *, client=None) -> list:
    peers: dict = registry["peers"]
    directable = set(registry.get("directable", ()))
    tools = [consult_tool(name, url, client=client) for name, url in peers.items()]
    tools += [direct_tool(name, peers[name], audit, client=client)
              for name in peers if name in directable]
    return tools
