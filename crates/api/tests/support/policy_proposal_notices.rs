//! The other administrators of the access policies told of the allow
//! policies proposed (ADR-170): a pass of the notices, whom they went to,
//! and what the audit and the proposals say of them.

use mimorya_api::services::policy_proposal_notice::{
    POLICY_PROPOSALS_ANNOUNCED, announce_policy_proposals,
};
use mimorya_core::demo::tenant;
use serde_json::Value;
use uuid::Uuid;

use super::{TestApi, owner_url};

/// The subject of the notice of one proposal.
pub const ONE: &str = "Une autorisation proposée attend votre décision";

/// Announces the proposals, as a process that executes runs does every
/// minute; how many.
pub async fn announce(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    announce_policy_proposals(&api.app.services).await.unwrap()
}

/// The addresses the notices of one proposal went to.
pub fn told(api: &TestApi) -> Vec<String> {
    api.mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .map(|m| m.to)
        .collect()
}

/// The notices audited: recipient, outcome and metadata.
pub async fn notices() -> Vec<(Option<Uuid>, String, Value)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let rows = sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where tenant_id = $1 and action = $2 and actor_type = 'system' order by id",
    )
    .bind(tenant::ID)
    .bind(POLICY_PROPOSALS_ANNOUNCED)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    rows
}

/// The notices audited, forgotten after, for the next tests.
pub async fn notices_and_forget() -> Vec<(Option<Uuid>, String, Value)> {
    let rows = notices().await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from audit_log where action = $1")
        .bind(POLICY_PROPOSALS_ANNOUNCED)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    rows
}

/// Whether the proposal `id` was recorded as announced.
pub async fn announced(api: &TestApi, id: &str) -> bool {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let announced =
        sqlx::query_scalar("select announced_at is not null from policy_proposals where id = $1")
            .bind(id.parse::<Uuid>().unwrap())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    announced
}
