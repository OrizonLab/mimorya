//! The skills' administrators told of the agent's proposals (ADR-102): a
//! pass of the notices, a proposal to tell of, and what the audit and the
//! versions say of them.

use mimorya_api::services::skill_proposal_notice::{PROPOSALS_ANNOUNCED, announce_skill_proposals};
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::Value;
use uuid::Uuid;

use super::skill_proposals::{proposed, read_proposed, validate, version_of_the_report};
use super::{TestApi, owner_url};

/// The subject of the notice of one proposal.
pub const ONE: &str = "Une nouvelle proposition de compétence attend votre décision";

/// Announces the proposals, as a process that executes runs does every
/// minute; how many.
pub async fn announce(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    announce_skill_proposals(&api.app.services).await.unwrap()
}

/// A proposal of Lucas's for the weekly report, validated: the run.
pub async fn one_proposal(api: &TestApi) -> Uuid {
    let lucas = api.sign_in(user::LUCAS).await;
    let (run, approval) = proposed(api, &lucas, version_of_the_report(), read_proposed()).await;
    validate(api, &lucas, &approval).await;
    run
}

/// Whom the notices of one proposal went to.
pub fn told(api: &TestApi) -> Vec<String> {
    api.mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .map(|m| m.to)
        .collect()
}

/// The notices audited in `tenant_id`: recipient, outcome and metadata.
pub async fn notices(tenant_id: Uuid) -> Vec<(Option<Uuid>, String, Value)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let rows = sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where tenant_id = $1 and action = $2 and actor_type = 'system' order by id",
    )
    .bind(tenant_id)
    .bind(PROPOSALS_ANNOUNCED)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    rows
}

/// The versions of the weekly report past its first, and whether each was
/// announced.
pub async fn announced(api: &TestApi) -> Vec<(i32, bool)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select version, announced_at is not null from skill_versions \
         where skill_id = $1 and version > 1 order by version",
    )
    .bind(DEMO_SKILL_ID)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Forgets the notices a test audited, for the next ones.
pub async fn forget_notices() {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from audit_log where action = $1")
        .bind(PROPOSALS_ANNOUNCED)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
