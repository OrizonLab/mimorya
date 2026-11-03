//! Who is told of the allow policies proposed (ADR-170): each account that
//! may decide them on the whole company, as the company's rules say now,
//! which their decision requires too (`company_wide_administrators`), but
//! their author. Not an administrator of a unit only, nor a disabled
//! account, nor a role whose assignment expired, nor someone a deny policy
//! keeps from the policies: with nobody else to tell, a proposal waits.
//! Nor is a proposal told while its author no longer administers the
//! policies, which nobody may approve then, nor once it expired, nor while
//! its company is suspended, whose people cannot sign in to decide. How
//! they are told: `policy_proposal_notices.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, tenant, user};
use mimorya_db::repositories::policy_proposal_notices::companies_with_unannounced_policy_proposals;
use serde_json::json;
use support::TestApi;
use support::owner::owner_runs;
use support::policy_proposal_notices::{announce, announced, notices_and_forget, told};
use support::policy_proposals::{completed_interventions, propose, retire, second_admin, withdraw};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_may_decide_on_the_whole_company_is_told() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = propose(&paul, completed_interventions()).await;
    // Marc administers his unit only; Hugo's account is disabled; Lea's
    // role expired yesterday.
    let admin = role::TENANT_ADMIN;
    owner_runs(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type, \
         scope_org_unit_id, include_descendants) select $1, $2, $3, 'org_unit', \
         m.org_unit_id, true from org_memberships m where m.user_id = $2 limit 1",
        &[tenant::ID, user::MARC, admin],
    )
    .await;
    owner_runs(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type) \
         values ($1, $2, $3, 'tenant')",
        &[tenant::ID, user::HUGO, admin],
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
        &[tenant::ID, user::LEA, admin],
    )
    .await;

    // Nobody but Paul may decide it: it waits.
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert!(!announced(&api, &id).await);
    assert!(notices_and_forget().await.is_empty());
    // An administrator of the whole company: told, alone.
    let (_, claire_id) = second_admin(&api, &paul, "claire.seule").await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), ["claire.seule@neo-maintenance.example"]);
    notices_and_forget().await;

    owner_runs(
        "delete from role_assignments where role_id = $1 and user_id in ($2, $3, $4)",
        &[admin, user::MARC, user::HUGO, user::LEA],
    )
    .await;
    owner_runs(
        "update users set status = 'active' where id = $1",
        &[user::HUGO],
    )
    .await;
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_a_deny_policy_keeps_from_the_policies_is_not_told() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.refusee").await;
    let id = propose(&paul, completed_interventions()).await;
    // A deny on the policies' administration, for Claire: the
    // administration refuses to write one, but the check honours it, as an
    // administration change would.
    let policy = Uuid::new_v4();
    owner_runs(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition) values ($1, $2, 'Refus de test', 'deny', array['policy:manage'], \
         jsonb_build_object('op', 'eq', 'left', jsonb_build_object('attr', 'subject.userId'), \
         'right', jsonb_build_object('value', $3::text)))",
        &[policy, tenant::ID, claire_id],
    )
    .await;

    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert!(!announced(&api, &id).await);
    assert!(notices_and_forget().await.is_empty());
    // The deny gone: told then.
    owner_runs("delete from access_policies where id = $1", &[policy]).await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), ["claire.refusee@neo-maintenance.example"]);
    notices_and_forget().await;
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_whose_author_no_longer_administers_the_policies_waits() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.partie").await;
    let hers = propose(&claire, completed_interventions()).await;
    retire(&paul, claire_id).await;

    // Nobody may approve it: Paul is not told.
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    assert!(!announced(&api, &hers).await);
    // Claire administers the policies again: Paul is told then.
    let granted = paul
        .post(
            &format!("/admin/users/{claire_id}/assignments"),
            json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), [support::email(user::PAUL)]);
    notices_and_forget().await;
    withdraw(&claire, &hers).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_expired_proposal_is_not_announced() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.expiree").await;
    let expired = propose(&paul, completed_interventions()).await;
    let expired_id: Uuid = expired.parse().unwrap();
    owner_runs(
        "update policy_proposals set proposed_at = now() - interval '8 days', \
         expires_at = now() - interval '1 day' where id = $1",
        &[expired_id],
    )
    .await;

    // Nothing waits in the company: not even listed.
    let pool = api.app.services.db.pool();
    let listed = companies_with_unannounced_policy_proposals(pool).await;
    assert!(!listed.unwrap().contains(&tenant::ID));
    assert_eq!(announce(&api).await, 0);
    assert!(told(&api).is_empty());
    // Beside a proposal still waiting: that one alone is told.
    let waiting = propose(&paul, completed_interventions()).await;
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), ["claire.expiree@neo-maintenance.example"]);
    assert_eq!(
        notices_and_forget().await,
        [(
            Some(claire_id),
            "success".to_string(),
            json!({ "proposals": [waiting] })
        )]
    );
    assert!(!announced(&api, &expired).await);

    owner_runs("delete from policy_proposals where id = $1", &[expired_id]).await;
    withdraw(&paul, &waiting).await;
    retire(&paul, claire_id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_s_administrators_are_not_told() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.suspendue").await;
    let id = propose(&paul, completed_interventions()).await;
    let suspend = "update tenants set status = 'suspended' where id = $1";
    owner_runs(suspend, &[tenant::ID]).await;
    // Its people cannot sign in to decide: not even listed. Read while
    // suspended, asserted once active again: a failed assertion leaves the
    // company active for the next tests.
    let pool = api.app.services.db.pool();
    let listed = companies_with_unannounced_policy_proposals(pool).await;
    let suspended = (announce(&api).await, told(&api), announced(&api, &id).await);
    let resume = "update tenants set status = 'active' where id = $1";
    owner_runs(resume, &[tenant::ID]).await;
    assert!(!listed.unwrap().contains(&tenant::ID));
    assert_eq!(suspended, (0, Vec::new(), false));

    // Active again: told then.
    assert_eq!(announce(&api).await, 1);
    assert_eq!(told(&api), ["claire.suspendue@neo-maintenance.example"]);
    notices_and_forget().await;
    withdraw(&paul, &id).await;
    retire(&paul, claire_id).await;
}
