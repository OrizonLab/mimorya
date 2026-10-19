//! Memory management for users: see what the assistant remembers, add,
//! delete (right to erasure) and validate proposed shared knowledge.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::memory::{
    MemoryKind, MemoryScope, MemorySource, MemoryStatus, MemoryTarget, can_write_memory,
    memory_read_filter,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::memory::{
    MemoryRow, NewMemory, create_memory, delete_memory, get_memory, list_visible_memory,
    settle_memory_proposal,
};
use serde::Deserialize;
use serde_json::json;
use validator::Validate;

use crate::app::AppState;
use crate::contract::{Created, Done, MemoryItem, MemoryList};
use crate::http::{ApiResult, Body, PathId, trimmed};
use crate::plugins::Auth;
use crate::services::AccessContext;

const MEMORY_PAGE: i64 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/memory", get(list).post(create))
        .route("/memory/{id}", delete(remove))
        .route("/memory/{id}/review", post(review))
}

/// Personal memories belong to their owner; shared ones to whoever may write the space.
fn can_manage(access: &AccessContext, memory: &MemoryRow) -> bool {
    let Some(scope) = MemoryScope::parse(&memory.scope) else {
        return false;
    };
    if scope == MemoryScope::User {
        return memory.owner_user_id == Some(access.subject.user_id);
    }
    can_write_memory(
        &access.engine,
        &access.subject,
        MemoryTarget {
            scope,
            org_unit_id: memory.org_unit_id,
        },
        &Actor::user(),
    )
}

fn item_view(access: &AccessContext, m: &MemoryRow) -> anyhow::Result<MemoryItem> {
    let org_unit = m
        .org_unit_id
        .and_then(|id| access.org_tree().get(id))
        .map(|u| u.name.clone());
    MemoryItem::from_row(m, org_unit, can_manage(access, m))
}

async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<MemoryList>> {
    let access = state.access.context(identity).await?;
    let filter = memory_read_filter(&access.engine, &access.subject, &Actor::user());
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_visible_memory(
        &mut tx,
        &filter,
        access.subject.clearance(),
        true,
        MEMORY_PAGE,
    )
    .await?;
    tx.commit().await?;
    let items = rows
        .iter()
        // Proposals are only shown to those who can validate them.
        .filter(|m| m.status == "active" || can_manage(&access, m))
        .map(|m| item_view(&access, m))
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(MemoryList { items }))
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Space {
    Personal,
    Team,
    Company,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ManualKind {
    Preference,
    Fact,
    Procedure,
    Decision,
    Lesson,
}

impl ManualKind {
    fn kind(self) -> MemoryKind {
        match self {
            Self::Preference => MemoryKind::Preference,
            Self::Fact => MemoryKind::Fact,
            Self::Procedure => MemoryKind::Procedure,
            Self::Decision => MemoryKind::Decision,
            Self::Lesson => MemoryKind::Lesson,
        }
    }
}

#[derive(Deserialize, Validate)]
struct CreateBody {
    space: Space,
    kind: ManualKind,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 120))]
    title: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 4000))]
    content: String,
}

async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreateBody>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let target = match body.space {
        Space::Personal => MemoryTarget {
            scope: MemoryScope::User,
            org_unit_id: None,
        },
        Space::Team => MemoryTarget {
            scope: MemoryScope::OrgUnit,
            org_unit_id: access.profile.primary_unit_id,
        },
        Space::Company => MemoryTarget {
            scope: MemoryScope::Tenant,
            org_unit_id: None,
        },
    };
    if !can_write_memory(&access.engine, &access.subject, target, &Actor::user()) {
        return Err(
            DomainError::forbidden("Vous ne pouvez pas écrire dans cet espace de mémoire").into(),
        );
    }
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let id = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: identity.tenant_id,
            scope: target.scope,
            owner_user_id: (target.scope == MemoryScope::User).then_some(identity.user_id),
            org_unit_id: target.org_unit_id,
            kind: body.kind.kind(),
            title: body.title,
            content: body.content,
            sensitivity: Sensitivity::Internal,
            status: MemoryStatus::Active,
            source: MemorySource::manual(None),
            created_by_user_id: Some(identity.user_id),
            created_by_run_id: None,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        AuditEntry::new(identity.tenant_id, "user", "memory.created", "success")
            .by_user(identity.user_id)
            .resource("memory", id)
            .metadata(json!({ "scope": target.scope.as_str() })),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let memory = get_memory(&mut tx, id)
        .await?
        .filter(|m| can_manage(&access, m))
        .ok_or_else(|| DomainError::not_found("Élément de mémoire introuvable"))?;
    delete_memory(&mut tx, id).await?;
    // The trace is kept, never the content.
    append_audit(
        &mut tx,
        AuditEntry::new(identity.tenant_id, "user", "memory.deleted", "success")
            .by_user(identity.user_id)
            .resource("memory", id)
            .metadata(json!({ "scope": memory.scope })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Done::OK))
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ReviewDecision {
    Approve,
    Reject,
}

#[derive(Deserialize, Validate)]
struct ReviewBody {
    decision: ReviewDecision,
}

async fn review(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<ReviewBody>,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    get_memory(&mut tx, id)
        .await?
        .filter(|m| m.status == "proposed" && can_manage(&access, m))
        .ok_or_else(|| DomainError::not_found("Proposition introuvable"))?;
    let approve = body.decision == ReviewDecision::Approve;
    let status = if approve {
        MemoryStatus::Active
    } else {
        MemoryStatus::Archived
    };
    if !settle_memory_proposal(&mut tx, id, status).await? {
        return Err(DomainError::not_found("Proposition introuvable").into());
    }
    let action = if approve {
        "memory.approved"
    } else {
        "memory.rejected"
    };
    append_audit(
        &mut tx,
        AuditEntry::new(identity.tenant_id, "user", action, "success")
            .by_user(identity.user_id)
            .resource("memory", id),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Done::OK))
}
