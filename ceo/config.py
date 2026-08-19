from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    ollama_api_key: str
    ollama_base_url: str
    ceo_model: str
    ceo_model_fallback: str
    cfo_url: str
    coo_url: str
    cto_url: str
    cxo_url: str
    db: dict
    qdrant_url: str
    memory_collection: str
    memory_namespace: str
    api_port: int
    console_port: int
    context_token_threshold: int
    subagent_max_depth: int

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env

        def g(k, d=""):
            return e.get(k, d)

        return cls(
            ollama_api_key=g("OLLAMA_API_KEY"),
            ollama_base_url=g("OLLAMA_BASE_URL", "https://ollama.com/v1"),
            ceo_model=g("CEO_MODEL", "kimi-k2.6"),
            ceo_model_fallback=g("CEO_MODEL_FALLBACK", "kimi-k2.6"),
            cfo_url=g("CFO_URL", "http://cfo:8089"),
            coo_url=g("COO_URL", "http://coo:8093"),
            cto_url=g("CTO_URL", "http://cto:8095"),
            cxo_url=g("CXO_URL", "http://cxo:8098"),
            db=dict(
                host=g("DB_HOST", "::1"),
                port=int(g("DB_PORT", "5432")),
                dbname=g("DB_NAME", "nano_bank_db"),
                user=g("DB_USER", "nanobank_user"),
                password=g("DB_PASSWORD", "secure_nano_password_2024!"),
            ),
            qdrant_url=g("QDRANT_URL", "http://agent-qdrant:6333"),
            memory_collection=g("MEMORY_COLLECTION", "ceo_memory"),
            memory_namespace=g("MEMORY_NAMESPACE", "ceo"),
            api_port=int(g("API_PORT", "8099")),
            console_port=int(g("CONSOLE_PORT", "8511")),
            context_token_threshold=int(g("CONTEXT_TOKEN_THRESHOLD", "60000")),
            subagent_max_depth=int(g("SUBAGENT_MAX_DEPTH", "2")),
        )

    def peer_registry(self) -> dict:
        return {
            "peers": {"cfo": self.cfo_url, "coo": self.coo_url,
                      "cto": self.cto_url, "cxo": self.cxo_url},
            "directable": {"coo", "cto"},
        }
