//! Who is told of the agent's proposals (ADR-102): each account that may
//! decide them on the whole company, as the company's rules say now, which
//! the administration requires too (`holds_company_wide`). Not an
//! administrator of a unit only, nor a disabled account, nor a role whose
//! assignment expired, nor someone a deny policy keeps from the skills:
//! with nobody to tell, the proposals wait for an administrator. How they
//! are told: `skill_proposal_notices.rs`.

mod support;

use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::skill_proposal_notices::{
    announce, announced, forget_notices, notices, one_proposal, told,
};
use support::skill_proposals::restore_the_report;
use support::{TestApi, email, owner_url};
use uuid::Uuid;

/// Runs `statement`, bound to `binds`, as the database's owner.
async fn owner_runs(statement: &str, binds: &[Uuid]) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query.execute(&owner).await.unwrap();
    owner.close().await;
}

/// The company's administrators' role.
async fn admin_role() -> Uuid {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let role =
        sqlx::query_scalar("select id from roles where tenant_id = $1 and key = 'tenant_admin'")
            .bind(tenant::ID)
            .fetch_one(&owner)
            .await
            .unwrap();
    owner.close().await;
    role
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_may_decide_on_the_whole_company_is_told() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    let role = admin_role().await;
    // Marc administers the skills of his unit only; Hugo's account is
    // disabled; Lea's role expired yesterday.
    owner_runs(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type, \
         scope_org_unit_id, include_descendants) select $1, $2, $3, 'org_unit', \
         m.org_unit_id, true from org_memberships m where m.user_id = $2 limit 1",
        &[tenant::ID, user::MARC, role],
    )
    .await;
    owner_runs(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type) \
         values ($1, $2, $3, 'tenant')",
        &[tenant::ID, user::HUGO, role],
    )
    .await;
    owner_runs(
        "update users set status = 'disabled' where id = $1",
        &[user::HUGO],
    )
    .await;
    owner_runs(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type, expires_at) \
         values ($1, $2, $3, 'tenant', now() - interval '1 day')",
        &[tenant::ID, user::LEA, role],
    )
    .await;

    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), [email(user::PAUL)]);

    owner_runs(
        "delete from role_assignments where role_id = $1 and user_id in ($2, $3, $4)",
        &[role, user::MARC, user::HUGO, user::LEA],
    )
    .await;
    owner_runs(
        "update users set status = 'active' where id = $1",
        &[user::HUGO],
    )
    .await;
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn with_nobody_who_may_decide_the_proposals_wait_for_one() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    // A deny on the skills' administration, for every person: the
    // administration refuses to write one, but the check honours it, as
    // an administration change would.
    let policy = Uuid::new_v4();
    owner_runs(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition) values ($1, $2, 'Refus de test', 'deny', array['skill:manage'], \
         '{\"op\": \"eq\", \"left\": {\"attr\": \"context.actorType\"}, \
         \"right\": {\"value\": \"user\"}}'::jsonb)",
        &[policy, tenant::ID],
    )
    .await;

    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert_eq!(announced(&api).await, [(2, false)]);
    assert!(notices(tenant::ID).await.is_empty());

    // An administrator again: told then.
    owner_runs("delete from access_policies where id = $1", &[policy]).await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), [email(user::PAUL)]);
    assert_eq!(
        notices(tenant::ID).await[0].2,
        json!({ "proposals": 1, "skills": ["synthese-hebdo-interventions"] })
    );
    restore_the_report(&api).await;
    forget_notices().await;
}
