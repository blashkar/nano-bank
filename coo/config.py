from __future__ import annotations
import os
from dataclasses import dataclass
from typing import Mapping, Optional


@dataclass
class Settings:
    ollama_api_key: str
    ollama_base_url: str
    coo_model: str
    operations_mcp_url: str
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
            coo_model=g("COO_MODEL", "glm-5.2"),
            operations_mcp_url=g("OPERATIONS_MCP_URL", "http://localhost:8092/mcp"),
            qdrant_url=g("QDRANT_URL", "http://localhost:8600"),
            memory_collection=g("MEMORY_COLLECTION", "coo_memory"),
            memory_namespace=g("MEMORY_NAMESPACE", "coo"),
            api_port=int(g("API_PORT", "8093")),
            console_port=int(g("CONSOLE_PORT", "8507")),
            context_token_threshold=int(g("CONTEXT_TOKEN_THRESHOLD", "60000")),
            subagent_max_depth=int(g("SUBAGENT_MAX_DEPTH", "2")),
        )
