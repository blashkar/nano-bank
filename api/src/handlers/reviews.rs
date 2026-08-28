//! Held customer movements — the customer's side of a fraud hold.
//!
//! When the engine holds a movement, the rail parks it in `pending_reviews`
//! instead of declining, and the customer gets a **202 + a review id**. This
//! module is where that parked movement waits, and where it goes when a
//! reviewer decides.
//!
//! Before this existed, a `hold_review` was a dead end: the bank declined, the
//! money went home, and clearing the case changed nothing for the customer.
//! `FRAUD_ENGINE_ARCHITECTURE_V2` §7 says the movement is held with an explicit
//! expiry and **on pass, execute**; §14 puts a 15-minute SLA on held
//! transactions "because money is waiting".
//!
//! ## Who decides, and who asks
//!
//! The verdict lives in the **engine** (`cases`), not here — so the bank polls
//! `GET /v1/decisions/{operation_id}/disposition` rather than the engine
//! calling back. That is the same direction the agent plane already polls its
//! own approvals, and it means the engine needs no knowledge of bank rails.
//!
//! Resolution is **lazy, on read**: a poll is what advances a parked movement,
//! exactly as `get_approval_status` reclaims and expires on read. No scheduler.
//! The cost is that a movement nobody polls sits until someone does — acceptable
//! while the customer polling *is* the liveness path, and the expiry backstop
//! still applies whenever anyone looks.
//!
//! ## Status contract
//!
//! `held → executing → executed`, or `held → refused | expired`.
//!
//! `executed` always carries `transaction_id`, written in the same statement,
//! so a customer polling can treat it as final. `executing` is the transient
//! claim while the released movement posts; a crash mid-release leaves the row
//! reclaimable once `claimed_at` ages past the lease, and an idempotency key
//! stops a reclaim from paying twice.
//!
//! That last clause used to be a claim rather than a fact. `idempotency_key` is
//! optional for callers and both rails' uniqueness guards skip NULL, so a
//! keyless movement could be reclaimed and paid a second time — while this
//! comment asserted it could not. `park` now derives a key from the review when
//! the caller gave none; see its doc.
//!
//! ## The gap, stated
//!
//! This parks the **intent**, not the funds. §7 wants the funds reserved and
//! unmoved; the bank has no balance-reservation primitive and `pending_approvals`
//! has the same limitation. So a released movement re-checks funds when it
//! executes and can fail there — the customer is told, and the review ends
//! `refused` with the reason.

use axum::{
    extract::{Path, State},
    response::Json,
    routing::get,
    Router,
};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::errors::AppError;
use crate::fraud::gate::FraudLink;
use crate::fraud::{CaseStatus, FraudCheckError};
use crate::handlers::AppState;
use crate::middleware::auth::AuthenticatedCustomer;

/// How long a release may hold the `executing` claim before another poll may
/// reclaim it. Generous relative to a transfer post: reclaiming a release that
/// is merely slow is safe (the idempotency key adopts the money that moved),
/// but doing it needlessly costs a second engine round trip.
const RECLAIM_AFTER_SECONDS: i64 = 60;

pub fn review_routes() -> Router<AppState> {
    Router::new().route("/:id", get(get_review))
}

/// What a customer sees when they ask about a held movement.
#[derive(Debug, Serialize)]
pub struct ReviewStatus {
    pub review_id: Uuid,
    pub rail: String,
    pub amount: Decimal,
    /// held | executing | executed | refused | expired
    pub status: String,
    /// The movement, once released. `None` in every other state.
    pub transaction_id: Option<Uuid>,
    pub resolution_note: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A parked movement as the release path needs it back.
#[derive(Debug, sqlx::FromRow)]
struct ParkedReview {
    review_id: Uuid,
    customer_id: Uuid,
    rail: String,
    amount: Decimal,
    movement: serde_json::Value,
    operation_id: Uuid,
    decision_id: Option<Uuid>,
    status: String,
    transaction_id: Option<Uuid>,
    resolution_note: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// The columns the release path and the customer projection actually read.
// `account_id` and `idempotency_key` are written at park time (they are what
// the open-park index and the audit trail need) but never read back here.
const REVIEW_COLUMNS: &str = "review_id, customer_id, rail, amount, \
     movement, operation_id, decision_id, status, transaction_id, \
     resolution_note, created_at, expires_at, resolved_at";

impl From<&ParkedReview> for ReviewStatus {
    fn from(r: &ParkedReview) -> Self {
        Self {
            review_id: r.review_id,
            rail: r.rail.clone(),
            amount: r.amount,
            status: r.status.clone(),
            transaction_id: r.transaction_id,
            resolution_note: r.resolution_note.clone(),
            created_at: r.created_at,
            expires_at: r.expires_at,
            resolved_at: r.resolved_at,
        }
    }
}

// ---------------------------------------------------------------------------
// parking
// ---------------------------------------------------------------------------

/// The review a retry of this exact request is already parked behind, if any.
///
/// Called by the rails **before screening**, alongside their existing
/// posted-movement replay check. Without it a client retrying a held request
/// finds nothing in `transactions` (a parked movement has posted nothing), so
/// it screens again: a second decision with a fresh `operation_id`, and the
/// velocity windows counting one customer intent twice. #28 established that a
/// replay must short-circuit *before* the engine is called; a parked movement
/// is a replay the posted tables cannot see.
pub(crate) async fn open_park_for(
    state: &AppState,
    customer_id: Uuid,
    idempotency_key: Option<&str>,
) -> Result<Option<ReviewStatus>, AppError> {
    let Some(key) = idempotency_key else {
        // Nothing to match on. A keyless caller cannot be deduplicated here,
        // exactly as it cannot be in the posted-movement replay check.
        return Ok(None);
    };
    let parked: Option<ParkedReview> = sqlx::query_as(&format!(
        "SELECT {REVIEW_COLUMNS} FROM pending_reviews \
         WHERE customer_id = $1 AND idempotency_key = $2 \
           AND status IN ('held', 'executing')",
    ))
    .bind(customer_id)
    .bind(key)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?;
    Ok(parked.as_ref().map(ReviewStatus::from))
}

/// What a rail hands over when the engine holds its movement.
pub(crate) struct ParkRequest<'a> {
    pub customer_id: Uuid,
    /// The funding account — what the money would have left.
    pub account_id: Uuid,
    pub rail: &'static str,
    pub amount: Decimal,
    pub idempotency_key: Option<&'a str>,
    /// Everything needed to execute this movement later, shaped by `rail`.
    /// Must contain no plaintext secret — hash before parking, as the send path
    /// would have. `park` fills in an `idempotency_key` here when the caller
    /// had none; see its doc for why the release cannot be safe without one.
    pub movement: serde_json::Value,
    pub link: &'a FraudLink,
}

/// Park a held movement, or adopt the park a retry already created.
///
/// `ON CONFLICT … DO UPDATE` rather than `DO NOTHING` for the same reason
/// `park_pending_approval` does it: `DO UPDATE` always returns a row, so a
/// retry adopts the existing park instead of needing a losing branch that
/// re-reads. The update is a self-assignment — a duplicate must never rewrite
/// the amount or the expiry the first park recorded.
///
/// ## Every parked movement leaves here with a release key
///
/// The reclaim lease is only safe because a re-execution collides with the
/// first attempt and adopts it instead of paying again — and that collision
/// needs a key. `idempotency_key` is optional for callers, and **both rails'
/// uniqueness guards skip NULL**: the transfer index is
/// `WHERE (metadata->>'idempotency_key') IS NOT NULL`, and interac's
/// `UNIQUE (sender_customer_id, idempotency_key)` treats repeated NULLs as
/// non-conflicting. So a keyless movement that crashed between posting the
/// money and recording `executed` was reclaimed and paid a second time.
///
/// When the caller supplies no key, one is derived from the review itself.
/// `review_id` is minted here rather than defaulted by the database precisely
/// so the key can exist before the row does — and being the review's own id, it
/// is stable across every reclaim by construction.
///
/// It goes into `movement` (what the release path replays), not into the
/// `idempotency_key` column, which stays the *caller's* key: that column is
/// what a retry is deduplicated against, and inventing a value there would
/// make every keyless park look like a distinct caller intent.
pub(crate) async fn park(state: &AppState, req: ParkRequest<'_>) -> Result<ReviewStatus, AppError> {
    let review_id = Uuid::new_v4();
    let mut movement = req.movement;
    if req.idempotency_key.is_none() {
        movement["idempotency_key"] = json!(format!("review-{review_id}"));
    }

    let parked: ParkedReview = sqlx::query_as(&format!(
        "INSERT INTO pending_reviews \
         (review_id, customer_id, account_id, rail, amount, idempotency_key, movement, \
          operation_id, decision_id, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 CURRENT_TIMESTAMP + $10 * INTERVAL '1 minute') \
         ON CONFLICT (customer_id, idempotency_key) \
         WHERE status IN ('held', 'executing') AND idempotency_key IS NOT NULL \
         DO UPDATE SET customer_id = pending_reviews.customer_id \
         RETURNING {REVIEW_COLUMNS}",
    ))
    .bind(review_id)
    .bind(req.customer_id)
    .bind(req.account_id)
    .bind(req.rail)
    .bind(req.amount)
    .bind(req.idempotency_key)
    .bind(&movement)
    .bind(req.link.operation_id)
    .bind(req.link.decision_id)
    .bind(state.settings.fraud.review_ttl_minutes)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::Database)?;

    tracing::info!(
        review_id = %parked.review_id, operation_id = %req.link.operation_id,
        rail = req.rail, amount = %req.amount,
        "⏸ movement held for fraud review"
    );
    Ok(ReviewStatus::from(&parked))
}

// ---------------------------------------------------------------------------
// polling, and what a poll makes happen
// ---------------------------------------------------------------------------

/// Ask what became of a held movement, advancing it if the verdict is in.
///
/// Scoped to the polling customer — another customer's review is a 404, not a
/// 403, so a stranger cannot confirm one exists.
async fn get_review(
    State(state): State<AppState>,
    auth: AuthenticatedCustomer,
    Path(review_id): Path<Uuid>,
) -> Result<Json<ReviewStatus>, AppError> {
    reclaim_stranded(&state, review_id, auth.customer_id).await?;

    let parked = load(&state, review_id, auth.customer_id).await?;
    if parked.status != "held" {
        // Terminal, or claimed by a release in flight. Either way this poll has
        // nothing to advance.
        return Ok(Json(ReviewStatus::from(&parked)));
    }

    // Expiry BEFORE the engine call: a movement past its window is decided
    // regardless of what a reviewer says now, and asking anyway would let an
    // engine outage keep an expired movement alive.
    if parked.expires_at <= chrono::Utc::now() {
        let expired = finish(
            &state,
            review_id,
            "expired",
            Some("held past its review window"),
        )
        .await?;
        return Ok(Json(expired));
    }

    let disposition = state
        .fraud
        .disposition(parked.operation_id)
        .await
        .map_err(|e| {
            // NOT "still open": an outage that read as "no verdict yet" would be
            // indistinguishable from a reviewer who has not decided, and the
            // movement would sit until it expired on a lie.
            tracing::warn!(
                review_id = %review_id, operation_id = %parked.operation_id, error = %e,
                "could not read the disposition of a held movement"
            );
            match e {
                FraudCheckError::Backend { .. } => AppError::Internal(
                    "could not determine the status of this transaction".to_string(),
                ),
                _ => AppError::ServiceUnavailable(
                    "review status is unavailable right now — please retry".to_string(),
                ),
            }
        })?;

    match disposition.case_status {
        // Reviewed and cleared: the one verdict that moves money.
        Some(CaseStatus::Cleared) => Ok(Json(release(&state, parked).await?)),
        Some(CaseStatus::ConfirmedFraud) => Ok(Json(
            finish(
                &state,
                review_id,
                "refused",
                Some("reviewed and confirmed as fraud"),
            )
            .await?,
        )),
        // Still waiting. `open` is a case nobody has ruled on yet, and `None` is
        // no case at all — neither is a release, and the difference matters
        // enough that the engine reports them separately.
        Some(CaseStatus::Open) | None => Ok(Json(ReviewStatus::from(&parked))),
        Some(CaseStatus::Unrecognized) => {
            tracing::error!(
                review_id = %review_id, verdict = ?disposition.raw_case_status,
                "engine returned a case verdict this bank does not understand"
            );
            Ok(Json(ReviewStatus::from(&parked)))
        }
    }
}

async fn load(
    state: &AppState,
    review_id: Uuid,
    customer_id: Uuid,
) -> Result<ParkedReview, AppError> {
    sqlx::query_as(&format!(
        "SELECT {REVIEW_COLUMNS} FROM pending_reviews \
         WHERE review_id = $1 AND customer_id = $2",
    ))
    .bind(review_id)
    .bind(customer_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("review not found".to_string()))
}

/// Return a release that died mid-flight to `held` so a later poll can retry
/// it. Unaudited on purpose: a lease timeout restores the prior state, it is
/// not a decision about the movement.
///
/// **Scoped to the polling customer.** This runs before the customer-scoped
/// `load()`, so without the `customer_id` bind any authenticated caller holding
/// a stranger's `review_id` would flip that stranger's stranded claim back to
/// `held` — the read that follows 404s, but the write has already landed. A
/// handler must not write a row it may not read; `approvals.rs` learned the
/// same lesson in `a5c14d9` and carries the caller's scope through
/// `ExpiryScope` for exactly this reason.
async fn reclaim_stranded(
    state: &AppState,
    review_id: Uuid,
    customer_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE pending_reviews SET status = 'held', claimed_at = NULL \
         WHERE review_id = $1 AND customer_id = $2 \
           AND status = 'executing' AND transaction_id IS NULL \
           AND claimed_at <= CURRENT_TIMESTAMP - $3 * INTERVAL '1 second'",
    )
    .bind(review_id)
    .bind(customer_id)
    .bind(RECLAIM_AFTER_SECONDS)
    .execute(&state.pool)
    .await
    .map_err(AppError::Database)?;
    Ok(())
}

/// Move a review to a terminal state. Conditional on it still being `held`, so
/// two concurrent polls cannot both resolve it.
async fn finish(
    state: &AppState,
    review_id: Uuid,
    status: &str,
    note: Option<&str>,
) -> Result<ReviewStatus, AppError> {
    let updated: Option<ParkedReview> = sqlx::query_as(&format!(
        "UPDATE pending_reviews \
         SET status = $2, resolution_note = $3, resolved_at = CURRENT_TIMESTAMP \
         WHERE review_id = $1 AND status = 'held' \
         RETURNING {REVIEW_COLUMNS}",
    ))
    .bind(review_id)
    .bind(status)
    .bind(note)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?;

    match updated {
        Some(row) => {
            tracing::info!(review_id = %review_id, status, "⏹ held movement resolved");
            Ok(ReviewStatus::from(&row))
        }
        // Someone else resolved it between our read and our write. Their
        // outcome stands; report it rather than overwriting it.
        None => {
            let row: ParkedReview = sqlx::query_as(&format!(
                "SELECT {REVIEW_COLUMNS} FROM pending_reviews WHERE review_id = $1",
            ))
            .bind(review_id)
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::Database)?;
            Ok(ReviewStatus::from(&row))
        }
    }
}

/// Execute a cleared movement — the point of the whole mechanism.
async fn release(state: &AppState, parked: ParkedReview) -> Result<ReviewStatus, AppError> {
    // Claim it first. A release that is not exclusive is a release that can pay
    // twice, and the claim is what makes two concurrent polls safe.
    let claimed: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE pending_reviews SET status = 'executing', claimed_at = CURRENT_TIMESTAMP \
         WHERE review_id = $1 AND status = 'held' RETURNING review_id",
    )
    .bind(parked.review_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Database)?;
    if claimed.is_none() {
        // Lost the race; the winner's outcome is the answer.
        return Ok(ReviewStatus::from(
            &load(state, parked.review_id, parked.customer_id).await?,
        ));
    }

    let executed = execute_parked(state, &parked).await;

    match executed {
        Ok(transaction_id) => {
            let row: ParkedReview = sqlx::query_as(&format!(
                "UPDATE pending_reviews \
                 SET status = 'executed', transaction_id = $2, claimed_at = NULL, \
                     resolution_note = 'reviewed and cleared', \
                     resolved_at = CURRENT_TIMESTAMP \
                 WHERE review_id = $1 RETURNING {REVIEW_COLUMNS}",
            ))
            .bind(parked.review_id)
            .bind(transaction_id)
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::Database)?;

            // Nothing is reported to the engine here, deliberately. The label
            // for this decision is written engine-side when the reviewer
            // resolves the case (`case_cleared`, carrying the operation_id) —
            // the bank is not part of that path.
            //
            // An earlier version fired a `released_after_review` event at
            // /v1/outcomes. It never once worked: `OutcomeEventType` is a closed
            // set the engine validates against, so every call was rejected 400,
            // silently, because the send was fire-and-forget. And `read_labels`
            // filters to LABEL_EVENTS, so even a delivered one would have
            // trained and evaluated nothing.
            tracing::info!(
                review_id = %parked.review_id, %transaction_id,
                "▶️ cleared review released — money moved"
            );
            Ok(ReviewStatus::from(&row))
        }
        Err(e) => {
            // The verdict was "release" and the release failed — most likely
            // the funds this park never reserved. Terminal and honest: the
            // customer is told why rather than left parked forever.
            let note = format!("cleared, but could not be completed: {e}");
            let row: ParkedReview = sqlx::query_as(&format!(
                "UPDATE pending_reviews \
                 SET status = 'refused', claimed_at = NULL, resolution_note = $2, \
                     resolved_at = CURRENT_TIMESTAMP \
                 WHERE review_id = $1 RETURNING {REVIEW_COLUMNS}",
            ))
            .bind(parked.review_id)
            .bind(&note)
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::Database)?;
            tracing::warn!(review_id = %parked.review_id, error = %e,
                "cleared review could not be executed");
            Ok(ReviewStatus::from(&row))
        }
    }
}

/// Run the parked movement on its own rail, returning the money row it posted.
///
/// The released movement is **not re-screened**. The engine already ruled on it
/// and a human overrode the hold; screening again would either replay the same
/// decision (and hold it forever) or take a second, differently-timed reading
/// that the reviewer never saw. So the original `FraudLink` is carried through,
/// and the money row is tagged with the decision that actually gated it.
async fn execute_parked(state: &AppState, parked: &ParkedReview) -> Result<Uuid, AppError> {
    let link = FraudLink::released(parked.operation_id, parked.decision_id);
    match parked.rail.as_str() {
        "interac_etransfer" => {
            let spec: crate::handlers::interac::ResolvedSend =
                serde_json::from_value(parked.movement.clone())
                    .map_err(|e| AppError::Internal(format!("unreadable parked send: {e}")))?;
            let (_, transaction_id) =
                crate::handlers::interac::send_resolved(state, parked.customer_id, &spec, &link)
                    .await?;
            Ok(transaction_id)
        }
        "transfer" => {
            let spec: ParkedTransfer = serde_json::from_value(parked.movement.clone())
                .map_err(|e| AppError::Internal(format!("unreadable parked transfer: {e}")))?;
            let posted = crate::handlers::transactions::execute_released_transfer(
                state,
                parked.customer_id,
                &spec,
                link,
            )
            .await?;
            Ok(posted)
        }
        other => Err(AppError::Internal(format!(
            "parked movement on an unknown rail: {other}"
        ))),
    }
}

/// An internal transfer as parked. Mirrors `TransferSpec`, owned rather than
/// borrowed because it round-trips through JSON.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ParkedTransfer {
    pub from_account_id: Uuid,
    pub to_account_id: Uuid,
    pub amount: Decimal,
    pub description: String,
    pub external_reference: Option<String>,
    pub idempotency_key: Option<String>,
}
