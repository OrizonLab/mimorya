//! A collaborator's absence declared by an account administrator for them
//! (ADR-078): read with whom one may name, declared or replaced, ended
//! (`user:manage`, company-wide, never on one's own account). The
//! collaborator is told by e-mail; the answer says whether they were.

use axum::Json;
use axum::extract::State;
use chrono::Utc;

use super::users::link_sender;
use crate::app::AppState;
use crate::contract::admin::{AbsenceRecorded, CollaboratorAbsence};
use crate::contract::{Absence, ApprovalPerson, Delegation};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{absence_of, declare_absence_for, end_absence_for};
use crate::services::runs::AbsenceInput;

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(user_id): PathId,
) -> ApiResult<Json<CollaboratorAbsence>> {
    let access = state.access.context(identity).await?;
    let found = absence_of(&link_sender(&state), &access, user_id, Utc::now()).await?;
    Ok(Json(CollaboratorAbsence {
        absence: found.absence.as_ref().map(Delegation::from),
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

pub async fn declare(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(user_id): PathId,
    Body(body): Body<Absence>,
) -> ApiResult<Json<AbsenceRecorded>> {
    let access = state.access.context(identity).await?;
    let input = AbsenceInput {
        delegate_user_id: body.delegate_id,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    let sender = link_sender(&state);
    let notified = declare_absence_for(&sender, &access, user_id, input, Utc::now()).await?;
    Ok(Json(AbsenceRecorded { notified }))
}

pub async fn end(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(user_id): PathId,
) -> ApiResult<Json<AbsenceRecorded>> {
    let access = state.access.context(identity).await?;
    let notified = end_absence_for(&link_sender(&state), &access, user_id, Utc::now()).await?;
    Ok(Json(AbsenceRecorded { notified }))
}
