//! The signed-in person's absence (ADR-071): whom they name to stand in for
//! them on the requests they decide as a manager, until when, and the
//! absences they cover. Their own resource: nobody declares another's.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;

use crate::app::AppState;
use crate::contract::{Absence, ApprovalDelegations, Delegation, Done};
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
    let found = delegations(&state.db, identity, Utc::now()).await?;
    Ok(Json(ApprovalDelegations {
        own: found.own.as_ref().map(Delegation::from),
        standing_in_for: found.standing_in_for.iter().map(Delegation::from).collect(),
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
