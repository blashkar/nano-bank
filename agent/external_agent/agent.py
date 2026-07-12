"""Autonomous external agent — talks ONLY to the branch gateway (no bank creds).

Given a high-level instruction it plans a list of steps and executes them via the
branch `/agent-gateway/*` endpoints: `act` (mandate-gated operations) and
`message` (A2A to the personal manager). It never holds a bank URL or bank/agent
credentials — only the gateway base + gateway token.
"""
from __future__ import annotations
import json
from typing import Optional
import httpx


class GatewayHTTP:
    def __init__(self, base, token, http: Optional[httpx.Client] = None):
        self.base = base.rstrip("/")
        self.h = {"Authorization": f"Bearer {token}"}
        self.http = http or httpx.Client(timeout=180)

    def mandate(self):
        return self.http.get(f"{self.base}/agent-gateway/mandate", headers=self.h).json()

    def act(self, op, params):
        return self.http.post(f"{self.base}/agent-gateway/act", headers=self.h,
                              json={"operation": op, "params": params}).json()

    def message(self, msg):
        return self.http.post(f"{self.base}/agent-gateway/message", headers=self.h,
                             json={"message": msg}).json()


PLANNER_SYS = (
    "You are an autonomous banking agent operating under a mandate through a gateway. "
    "Given the user's high-level instruction, output ONLY a JSON list of steps. Each step is "
    '{"kind":"act","operation":"transfer_out|open_account|register_payee","params":{...}} '
    'or {"kind":"message","text":"..."} to ask the manager a question. '
    'For a bill payment use {"kind":"act","operation":"transfer_out","params":{"amount":"50"}} '
    "(the gateway routes it to the biller). Only use granted capabilities; keep it minimal."
)


class ExternalAgent:
    def __init__(self, gateway, llm=None, plan=None):
        self.gw = gateway
        self.llm = llm
        self._plan = plan

    @classmethod
    def from_plan(cls, plan, gateway):
        """Deterministic plan (tests/demos): list of ('act', op, params) or ('message', text)."""
        return cls(gateway=gateway, plan=plan)

    @classmethod
    def http(cls, gateway_base, gateway_token, llm=None):
        return cls(gateway=GatewayHTTP(gateway_base, gateway_token), llm=llm)

    def _make_plan(self, instruction):
        if self._plan is not None:
            return self._plan
        from langchain_core.messages import SystemMessage, HumanMessage
        out = self.llm.invoke([SystemMessage(PLANNER_SYS), HumanMessage(instruction)])
        content = out.content if hasattr(out, "content") else str(out)
        steps = json.loads(_strip_fence(content))
        norm = []
        for s in steps:
            if s.get("kind") == "act":
                norm.append(("act", s["operation"], s.get("params", {})))
            else:
                norm.append(("message", s.get("text", "")))
        return norm

    def run(self, instruction: str) -> list[dict]:
        events = [{"kind": "plan", "instruction": instruction}]
        for step in self._make_plan(instruction):
            if step[0] == "act":
                _, op, params = step
                res = self.gw.act(op, params)
                events.append({"kind": "act", "operation": op, "params": params, "result": res})
            else:
                _, msg = step
                res = self.gw.message(msg)
                events.append({"kind": "message", "text": msg, "answer": res.get("answer"),
                               "trace": res.get("trace")})
        events.append({"kind": "result", "steps": len(events) - 1})
        return events


def _strip_fence(s: str) -> str:
    s = s.strip()
    if s.startswith("```"):
        s = s.split("\n", 1)[1] if "\n" in s else s
        s = s.rsplit("```", 1)[0]
    return s.strip()
