//! HTTP adapter for the real fraud engine (`nano-bank-fraud-engine`, :8092).
//! Tight total timeout, bearer service token, and a small circuit breaker so a
//! dead engine costs one fast error instead of a full timeout per transaction.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;

use super::{FraudAction, FraudCheck, FraudCheckError, FraudDecision, FraudRequest};

const BREAKER_THRESHOLD: u32 = 5;
const BREAKER_OPEN_SECS: u64 = 10;

pub struct EngineFraudCheck {
    base_url: String,
    token: String,
    http: reqwest::Client,
    consecutive_failures: AtomicU32,
    open_until: Mutex<Option<Instant>>,
}

impl EngineFraudCheck {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>, timeout_ms: u64) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_millis(timeout_ms))
                .connect_timeout(Duration::from_millis(timeout_ms.min(50)))
                .build()
                .expect("reqwest client"),
            consecutive_failures: AtomicU32::new(0),
            open_until: Mutex::new(None),
        }
    }

    fn circuit_open(&self) -> bool {
        let mut open = self.open_until.lock().expect("breaker lock");
        match *open {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                // Half-open: let this request probe; a failure re-opens below.
                *open = None;
                false
            }
            None => false,
        }
    }

    fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= BREAKER_THRESHOLD {
            *self.open_until.lock().expect("breaker lock") =
                Some(Instant::now() + Duration::from_secs(BREAKER_OPEN_SECS));
        }
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        *self.open_until.lock().expect("breaker lock") = None;
    }

    fn wire_body(req: &FraudRequest) -> serde_json::Value {
        json!({
            "idempotency_key": req.idempotency_key,
            "transaction": {
                "operation_id": req.operation_id,
                "type": req.kind,
                // string-decimal on the wire, never a float
                "amount": req.amount.to_string(),
                "currency": "CAD",
                "from_account_id": req.from_account_id,
                "to_account_id": req.to_account_id,
                "payee_handle": req.payee_handle,
                "description": req.description,
                "external_reference": req.external_reference,
                "merchant": req.merchant,
            },
            "customer_id": req.customer_id,
            "initiated_via": req.initiated_via,
            "agent": req.agent.as_ref().map(|a| json!({
                "agent_id": a.agent_id,
                "mandate_id": a.mandate_id,
                "cap_override": a.cap_override,
                "approval_latency_seconds": a.approval_latency_seconds,
            })),
            "session": req.session.as_ref().map(|s| json!({
                "session_id": s.session_id,
                "ip_address": s.ip_address,
                "user_agent": s.user_agent,
                "device_fingerprint": s.device_fingerprint,
                "session_created_at": s.session_created_at,
                "last_activity_at": s.last_activity_at,
            })),
            "requested_at": chrono::Utc::now(),
        })
    }
}

fn parse_action(action: &str) -> FraudAction {
    match action {
        "allow" => FraudAction::Allow,
        "block" => FraudAction::Block,
        "challenge" => FraudAction::Challenge,
        "delay_and_warn" => FraudAction::DelayAndWarn,
        // hold_review and anything the contract grows later: safest treatment
        _ => FraudAction::HoldReview,
    }
}

#[async_trait]
impl FraudCheck for EngineFraudCheck {
    fn backend(&self) -> &'static str {
        "engine"
    }

    async fn assess(&self, req: &FraudRequest) -> Result<FraudDecision, FraudCheckError> {
        if self.circuit_open() {
            return Err(FraudCheckError::Transport("circuit open".to_string()));
        }
        let sent = self
            .http
            .post(format!("{}/v1/decisions", self.base_url))
            .bearer_auth(&self.token)
            .json(&Self::wire_body(req))
            .send()
            .await;
        let resp = match sent {
            Ok(resp) => resp,
            Err(e) => {
                self.record_failure();
                return Err(if e.is_timeout() {
                    FraudCheckError::Timeout
                } else {
                    FraudCheckError::Transport(e.to_string())
                });
            }
        };
        let status = resp.status();
        if status.is_server_error() {
            self.record_failure();
            let body = resp.text().await.unwrap_or_default();
            return Err(FraudCheckError::Transport(format!("engine 5xx: {body}")));
        }
        if !status.is_success() {
            // 4xx = bank-side contract bug, not engine outage: don't trip the
            // breaker for it, but surface it distinctly.
            let body = resp.text().await.unwrap_or_default();
            return Err(FraudCheckError::Backend {
                status: status.as_u16(),
                body,
            });
        }
        self.record_success();
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| FraudCheckError::Transport(e.to_string()))?;
        let decision_id = value
            .get("decision_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| FraudCheckError::Transport("missing decision_id".to_string()))?;
        let action = parse_action(value.get("action").and_then(|v| v.as_str()).unwrap_or(""));
        Ok(FraudDecision {
            decision_id,
            action,
            engine_mode: value
                .get("engine_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            message_for_customer: value
                .get("message_for_customer")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        })
    }

    async fn rescore(&self, req: FraudRequest, executed: bool) {
        let body = json!({
            "original_request": Self::wire_body(&req),
            "executed": executed,
            "failed_open_at": chrono::Utc::now(),
        });
        // Best-effort by contract: never let this influence a request path.
        let result = self
            .http
            .post(format!("{}/v1/rescore", self.base_url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await;
        if let Err(e) = result {
            tracing::warn!(operation_id = %req.operation_id, error = %e, "fraud rescore not delivered");
        }
    }
}
