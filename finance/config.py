from __future__ import annotations
import os
from dataclasses import dataclass
from decimal import Decimal
from typing import Mapping, Optional


_DEFAULT_WEIGHTS = {
    "CashReserves": Decimal("0"),
    "Bank": Decimal("0.20"),            # interbank / central-bank claim
    "TreasuryPlacement": Decimal("0.20"),
    "CardReceivable": Decimal("0.75"),
    "OverdraftReceivable": Decimal("1.00"),
    "LoansReceivable": Decimal("1.00"),
}
# Any asset role without an explicit weight is risk-weighted at this rate.
# It must never be 0: an unmapped asset silently treated as risk-free collapses
# RWA, and with it economic capital, which makes RAROC explode.
_DEFAULT_ASSET_WEIGHT = Decimal("1.00")
_DEFAULT_LOSS = {
    "CardReceivable": Decimal("0.03"),
    "OverdraftReceivable": Decimal("0.02"),
    "LoansReceivable": Decimal("0.015"),
}


@dataclass(frozen=True)
class RiskConfig:
    """Basel-lite capital model for RAROC (spec #5 replaces this behind raroc())."""
    risk_weights: dict
    loss_rates: dict
    target_ratio: Decimal
    default_asset_weight: Decimal = _DEFAULT_ASSET_WEIGHT

    @classmethod
    def default(cls) -> "RiskConfig":
        return cls(risk_weights=dict(_DEFAULT_WEIGHTS),
                   loss_rates=dict(_DEFAULT_LOSS),
                   target_ratio=Decimal("0.10"))

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "RiskConfig":
        e = os.environ if env is None else env
        weights = dict(_DEFAULT_WEIGHTS)
        loss = dict(_DEFAULT_LOSS)
        for role in list(weights):
            if (v := e.get(f"RISK_WEIGHT_{role}")) is not None:
                weights[role] = Decimal(v)
        for role in list(loss):
            if (v := e.get(f"RISK_LOSS_{role}")) is not None:
                loss[role] = Decimal(v)
        ratio = Decimal(e.get("RISK_TARGET_RATIO", "0.10"))
        default_w = Decimal(e.get("RISK_DEFAULT_ASSET_WEIGHT",
                                  str(_DEFAULT_ASSET_WEIGHT)))
        # Enforce the invariant the fallback weight exists to protect: a zero (or
        # negative) default treats every unmapped asset as risk-free, collapsing
        # RWA and economic capital and making RAROC explode. Fail loudly at load
        # rather than emit silently-wrong capital numbers downstream.
        if default_w <= 0:
            raise ValueError(
                "RISK_DEFAULT_ASSET_WEIGHT must be > 0 "
                f"(got {default_w}); a zero/negative default risk-weights "
                "unmapped assets as risk-free and collapses the capital model")
        return cls(risk_weights=weights, loss_rates=loss, target_ratio=ratio,
                   default_asset_weight=default_w)


@dataclass
class Settings:
    db: dict
    nano_bank_api: str
    mcp_port: int

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None) -> "Settings":
        e = os.environ if env is None else env

        def g(k, d=""):
            return e.get(k, d)

        return cls(
            db=dict(
                host=g("DB_HOST", "::1"),
                port=int(g("DB_PORT", "5432")),
                dbname=g("DB_NAME", "nano_bank_db"),
                user=g("DB_USER", "nanobank_user"),
                password=g("DB_PASSWORD", "secure_nano_password_2024!"),
            ),
            nano_bank_api=g("NANO_BANK_API", "http://localhost:8081"),
            mcp_port=int(g("MCP_PORT", "8088")),
        )
