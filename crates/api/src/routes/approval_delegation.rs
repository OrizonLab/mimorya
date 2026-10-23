//! The signed-in person's absence (ADR-071): whom they name to stand in for
//! them on the requests they decide as a manager, until when, the absences
//! they cover and, if they decide requests, whom they may name (ADR-073).
//! Their own resource: nobody declares another's here (an account
//! administrator does, for a collaborator who cannot, `admin/absences.rs`,
//! ADR-078).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use mimorya_core::approval_routing::DECIDE;

use crate::app::AppState;
use crate::contract::{Absence, ApprovalDelegations, ApprovalPerson, Delegation, Done};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::runs::{AbsenceInput, declare_absence, delegations, end_absence};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/approval-delegation",
        get(read).put(declare).delete(withdraw),
    )
}

async fn read(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ApprovalDelegations>> {
    let access = state.access.context(identity).await?;
    let decide = access.holds(DECIDE);
    let found = delegations(&state.db, identity, decide, Utc::now()).await?;
    Ok(Json(ApprovalDelegations {
        own: found.own.as_ref().map(Delegation::from),
        standing_in_for: found.standing_in_for.iter().map(Delegation::from).collect(),
        candidates: found
            .candidates
            .into_iter()
            .map(|c| ApprovalPerson {
                id: c.user_id,
                display_name: c.display_name,
            })
            .collect(),
    }))
}

async fn declare(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<Absence>,
) -> ApiResult<Json<Done>> {
    let input = AbsenceInput {
        delegate_user_id: body.delegate_id,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    declare_absence(&state.db, identity, input, Utc::now()).await?;
    Ok(Json(Done::OK))
}

async fn withdraw(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Done>> {
    end_absence(&state.db, identity, Utc::now()).await?;
    Ok(Json(Done::OK))
}
