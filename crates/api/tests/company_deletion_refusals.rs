//! What the operator's deletion of a company refuses (ADR-192), deleting
//! nothing: an unknown slug; a company that is not suspended; a run of it
//! still queued or under way, which stops at its next step; an evaluation
//! of its skills being played; a confirmation that is not its slug. Once
//! nothing stands in the way, confirmed, the company goes, with a run
//! awaiting a decision and an evaluation still queued.

mod support;

use mimorya_api::services::provisioning::{CompanyDeletion, CompanyStatusChange, delete_company};
use mimorya_core::{DomainError, ErrorCode};
use sqlx::PgPool;
use support::TestApi;
use support::companies::{create, rows_of};
use support::personal_belongings::{belongings, company_parts};
use support::retention::owner;
use support::suspension::change;

const SLUG: &str = "acme-refusee";

async fn refused(owner: &PgPool, slug: &str, confirm: Option<&str>) -> DomainError {
    delete_company(owner, slug, confirm)
        .await
        .expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error")
}

async fn set_run(owner: &PgPool, run: uuid::Uuid, status: &str) {
    sqlx::query("update agent_runs set status = $2, finished_at = null where id = $1")
        .bind(run)
        .bind(status)
        .execute(owner)
        .await
        .unwrap();
}

/// An evaluation of a skill of the company, written as `queued`.
async fn queued_evaluation(owner: &PgPool, tenant_id: uuid::Uuid) -> uuid::Uuid {
    sqlx::query_scalar(
        "with skill as (insert into skills (tenant_id, name, description) \
         values ($1, 'procedure-refusee', 'Une procédure de l’entreprise') returning id) \
         insert into skill_evaluations (tenant_id, skill_id, version, cases) \
         select $1, id, 1, '[]' from skill returning id",
    )
    .bind(tenant_id)
    .fetch_one(owner)
    .await
    .unwrap()
}

async fn set_evaluation(owner: &PgPool, evaluation: uuid::Uuid, status: &str) {
    sqlx::query("update skill_evaluations set status = $2 where id = $1")
        .bind(evaluation)
        .bind(status)
        .execute(owner)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_suspended_company_without_a_living_run_goes_once_confirmed() {
    let api = TestApi::start().await;
    let owner = owner().await;
    let created = create(&api, SLUG, "jeanne.martin@acme-refusee.example").await;
    let parts = company_parts(created.tenant_id, "refus", 3001).await;
    let held = belongings(&parts, created.admin_id, "de-jeanne", None, 51, 10).await;
    let before = rows_of(&owner, created.tenant_id).await;

    let unknown = refused(&owner, "nulle-part", Some("nulle-part")).await;
    assert_eq!(unknown.code, ErrorCode::NotFound);
    assert_eq!(unknown.message, "Aucune entreprise ne porte ce sigle");
    let active = refused(&owner, SLUG, Some(SLUG)).await;
    assert_eq!(active.code, ErrorCode::Conflict);
    assert!(
        active.message.starts_with("Seule une entreprise suspendue"),
        "{}",
        active.message
    );
    assert_eq!(rows_of(&owner, created.tenant_id).await, before);

    change(&api, SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    // Queued, an evaluation waits for its company to be active again.
    let evaluation = queued_evaluation(&owner, created.tenant_id).await;
    let suspended = rows_of(&owner, created.tenant_id).await;
    for living in ["running", "queued"] {
        set_run(&owner, held.run, living).await;
        let refusal = refused(&owner, SLUG, Some(SLUG)).await;
        assert_eq!(refusal.code, ErrorCode::Conflict, "{living}");
        assert!(
            refusal.message.starts_with("Un run de l’entreprise"),
            "{}",
            refusal.message
        );
    }
    set_run(&owner, held.run, "awaiting_approval").await;
    set_evaluation(&owner, evaluation, "running").await;
    let playing = refused(&owner, SLUG, Some(SLUG)).await;
    assert_eq!(playing.code, ErrorCode::Conflict);
    assert!(
        playing
            .message
            .contains("une évaluation de ses compétences se joue"),
        "{}",
        playing.message
    );
    assert_eq!(rows_of(&owner, created.tenant_id).await, suspended);
    set_evaluation(&owner, evaluation, "queued").await;
    let mistyped = refused(&owner, SLUG, Some("acme")).await;
    assert_eq!(mistyped.code, ErrorCode::Validation);
    assert_eq!(rows_of(&owner, created.tenant_id).await, suspended);

    // A run awaiting a decision and a queued evaluation stop nothing: they
    // go with their company.
    let gone = delete_company(&owner, SLUG, Some(SLUG)).await.unwrap();
    assert!(matches!(gone, CompanyDeletion::Deleted(_)), "{gone:?}");
    assert_eq!(rows_of(&owner, created.tenant_id).await, Vec::new());
    owner.close().await;
}
