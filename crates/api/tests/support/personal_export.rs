//! What the export tests write as an operator writes it (ADR-187): a
//! mission in a conversation and an absence with its stand-in, which the
//! API makes only through a model or an administrator.

use uuid::Uuid;

use super::retention::owner;

/// A mission of `person` in the conversation `conversation_id`, titled
/// `title`.
pub async fn mission(tenant_id: Uuid, person: Uuid, conversation_id: Uuid, title: &str) {
    let owner = owner().await;
    sqlx::query(
        "insert into missions (tenant_id, owner_user_id, title, description, conversation_id, \
         limits) values ($1, $2, $3, 'Une mission exportée', $4, '{}')",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(title)
    .bind(conversation_id)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// An absence of `person` next week, `delegate` standing in.
pub async fn absence(tenant_id: Uuid, person: Uuid, delegate: Uuid) {
    let owner = owner().await;
    sqlx::query(
        "insert into approval_delegations (tenant_id, user_id, delegate_user_id, starts_at, \
         ends_at, declared_by_user_id) \
         values ($1, $2, $3, now() + interval '7 days', now() + interval '14 days', $2)",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(delegate)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
