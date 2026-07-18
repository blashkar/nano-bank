//! No-op adapter: every assessment allows, nothing leaves the process. The
//! default backend, so the bank builds, runs, and passes its tests unchanged
//! until fraud screening is opted in — and the operational kill switch
//! (`NANO_BANK__FRAUD__BACKEND=off` + restart) thereafter.

use async_trait::async_trait;
use uuid::Uuid;

use super::{FraudAction, FraudCheck, FraudCheckError, FraudDecision, FraudRequest};

pub struct NoopFraudCheck;

#[async_trait]
impl FraudCheck for NoopFraudCheck {
    fn backend(&self) -> &'static str {
        "off"
    }

    async fn assess(&self, _req: &FraudRequest) -> Result<FraudDecision, FraudCheckError> {
        Ok(FraudDecision {
            decision_id: Uuid::nil(),
            action: FraudAction::Allow,
            engine_mode: "off".to_string(),
            message_for_customer: None,
        })
    }

    async fn rescore(&self, _req: FraudRequest, _executed: bool) {}
}
