//! No-op adapter: every assessment allows, nothing leaves the process. The
//! default backend, so the bank builds, runs, and passes its tests unchanged
//! until fraud screening is opted in — and the operational kill switch
//! (`NANO_BANK__FRAUD__BACKEND=off` + restart) thereafter.

use async_trait::async_trait;
use uuid::Uuid;

use super::{Disposition, FraudAction, FraudCheck, FraudCheckError, FraudDecision, FraudRequest};

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

    /// Never reached: the drainer skips entirely when the backend is off,
    /// rather than claiming rows and burning their retry budget against an
    /// engine nobody asked it to call.
    async fn report_denial(&self, _payload: &serde_json::Value) -> Result<(), FraudCheckError> {
        Ok(())
    }

    /// Also never reached: in off-mode nothing is ever held, so no movement is
    /// ever parked and nothing has a disposition to ask about. Answering
    /// "allowed, no case" is the truthful reading of a backend that allows
    /// everything — and it is inert, because releasing needs a `cleared`
    /// verdict, which this can never produce.
    async fn disposition(&self, _operation_id: Uuid) -> Result<Disposition, FraudCheckError> {
        Ok(Disposition {
            action: "allow".to_string(),
            case_status: None,
            raw_case_status: None,
        })
    }
}
