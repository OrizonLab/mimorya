//! The skills' administrators told by e-mail of the agent's proposals
//! (ADR-102), by the periodic pass: one e-mail to each account that may
//! decide them on the whole company, naming their skills, never the
//! procedure nor whom it was proposed for; each proposal announced once,
//! audited, not when decided before the pass; nothing without a relay, and
//! an administrator who chose not to receive these is not sent one,
//! audited as refused. Each company's proposals are read and told in it
//! alone.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::skill_proposal_notice::{PROPOSALS_ANNOUNCED, announce_skill_proposals};
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::notification_choices::{choose, chosen};
use support::skill_proposals::{
    SKILL, SKILLS, proposed, read_proposed, restore_the_report, validate, version_of_the_report,
    waiting,
};
use support::{TestApi, WEB_ORIGIN, email, owner_url};
use uuid::Uuid;

const ONE: &str = "Une proposition de compétence attend votre décision";

/// Announces the proposals, as a process that executes runs does every
/// minute; how many.
async fn announce(api: &TestApi) -> usize {
    api.app.services.runs.start_executing().unwrap();
    announce_skill_proposals(&api.app.services).await.unwrap()
}

/// A proposal of Lucas's for the weekly report, validated: the run.
async fn one_proposal(api: &TestApi) -> Uuid {
    let lucas = api.sign_in(user::LUCAS).await;
    let (run, approval) = proposed(api, &lucas, version_of_the_report(), read_proposed()).await;
    validate(api, &lucas, &approval).await;
    run
}

/// The notices audited in `tenant_id`: recipient, outcome and metadata.
async fn notices(tenant_id: Uuid) -> Vec<(Option<Uuid>, String, Value)> {
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

/// The versions of the weekly report announced, by number.
async fn announced(api: &TestApi) -> Vec<(i32, bool)> {
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

/// Forgets the notices this file's tests audited, for the next ones.
async fn forget_notices() {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from audit_log where action = $1")
        .bind(PROPOSALS_ANNOUNCED)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_is_announced_once_to_the_skills_administrator() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));

    assert_eq!(announce(&api).await, 1);
    let sent: Vec<_> = api
        .mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::PAUL));
    assert!(
        sent[0].text.contains(&format!("« {SKILL} »")),
        "{}",
        sent[0].text
    );
    assert!(sent[0].text.contains(&format!("{WEB_ORIGIN}/admin\n")));
    // Neither the procedure nor whom it was proposed for.
    assert!(!sent[0].text.contains("Relire les retards"));
    assert!(!sent[0].text.contains("Lucas"));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "success".to_string(),
            json!({ "proposals": 1, "skills": [SKILL] })
        )]
    );
    assert_eq!(announced(&api).await, [(2, true)]);

    // Once.
    assert_eq!(announce(&api).await, 0);
    assert_eq!(
        api.mail.sent().iter().filter(|m| m.subject == ONE).count(),
        1
    );
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn proposals_are_told_together_and_a_decided_one_is_not_announced() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let run = one_proposal(&api).await;
    waiting(&api, 2, user::LUCAS, run).await;
    // Decided before the pass: nothing to tell of it.
    let rejected = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/rejection"),
            json!({ "version": 4 }),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());

    // Another company's proposal, told to its own administrator alone.
    let other = sqlx::PgPool::connect(owner_url()).await.unwrap();
    for statement in [
        "insert into roles (id, tenant_id, key, name, description, permissions) values \
         ('00000000-0000-4000-8000-0000000f0101', $1, 'admin', 'Admin', 'Rôle', \
          array['skill:manage'])",
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type) values \
         ($1, $2, '00000000-0000-4000-8000-0000000f0101', 'tenant')",
        "insert into skills (id, tenant_id, name, description) values \
         ('00000000-0000-4000-8000-0000000f0102', $1, 'procedure-ailleurs', \
          'Une procédure d’une autre entreprise')",
        "insert into skill_versions (tenant_id, skill_id, version, content, status, proposed) \
         values ($1, '00000000-0000-4000-8000-0000000f0102', 1, '1. Ailleurs.', 'candidate', \
          true)",
    ] {
        sqlx::query(statement)
            .bind(OTHER_TENANT_ID)
            .bind(OTHER_USER_ID)
            .execute(&other)
            .await
            .unwrap();
    }

    assert_eq!(announce(&api).await, 3);
    let sent = api.mail.sent();
    let to_paul: Vec<_> = sent.iter().filter(|m| m.to == email(user::PAUL)).collect();
    assert_eq!(to_paul.len(), 1, "{to_paul:?}");
    assert_eq!(
        to_paul[0].subject,
        "2 propositions de compétences attendent votre décision"
    );
    assert!(to_paul[0].text.contains(&format!(": « {SKILL} ».")));
    assert!(!to_paul[0].text.contains("procedure-ailleurs"));
    let elsewhere: Vec<_> = sent
        .iter()
        .filter(|m| m.text.contains("procedure-ailleurs"))
        .collect();
    assert_eq!(elsewhere.len(), 1, "{elsewhere:?}");
    assert_ne!(elsewhere[0].to, email(user::PAUL));
    assert!(!elsewhere[0].text.contains(SKILL));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "success".to_string(),
            json!({ "proposals": 2, "skills": [SKILL] })
        )]
    );
    assert_eq!(notices(OTHER_TENANT_ID).await.len(), 1);
    assert_eq!(announced(&api).await, [(2, true), (3, true), (4, false)]);

    for statement in [
        "delete from skills where id = '00000000-0000-4000-8000-0000000f0102'",
        "delete from role_assignments where role_id = '00000000-0000-4000-8000-0000000f0101'",
        "delete from roles where id = '00000000-0000-4000-8000-0000000f0101'",
    ] {
        sqlx::query(statement).execute(&other).await.unwrap();
    }
    other.close().await;
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_may_decide_on_the_whole_company_is_told() {
    let api = TestApi::start_with_mailer().await;
    one_proposal(&api).await;
    // Marc administers the skills of a unit only, Hugo's account is
    // disabled: neither may decide on the whole company.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let unit: Uuid =
        sqlx::query_scalar("select org_unit_id from org_memberships where user_id = $1 limit 1")
            .bind(user::MARC)
            .fetch_one(&owner)
            .await
            .unwrap();
    let admin_role: Uuid =
        sqlx::query_scalar("select id from roles where tenant_id = $1 and key = 'tenant_admin'")
            .bind(tenant::ID)
            .fetch_one(&owner)
            .await
            .unwrap();
    sqlx::query(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type, \
         scope_org_unit_id, include_descendants) values ($1, $2, $3, 'org_unit', $4, true)",
    )
    .bind(tenant::ID)
    .bind(user::MARC)
    .bind(admin_role)
    .bind(unit)
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into role_assignments (tenant_id, user_id, role_id, scope_type) \
         values ($1, $2, $3, 'tenant')",
    )
    .bind(tenant::ID)
    .bind(user::HUGO)
    .bind(admin_role)
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query("update users set status = 'disabled' where id = $1")
        .bind(user::HUGO)
        .execute(&owner)
        .await
        .unwrap();

    assert_eq!(announce(&api).await, 1);
    let told: Vec<String> = api
        .mail
        .sent()
        .into_iter()
        .filter(|m| m.subject == ONE)
        .map(|m| m.to)
        .collect();
    assert_eq!(told, [email(user::PAUL)]);

    sqlx::query("delete from role_assignments where role_id = $1 and user_id in ($2, $3)")
        .bind(admin_role)
        .bind(user::MARC)
        .bind(user::HUGO)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("update users set status = 'active' where id = $1")
        .bind(user::HUGO)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_who_chose_not_to_receive_them_is_not_sent_one() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    choose(&paul, chosen(false, true, true)).await;
    one_proposal(&api).await;

    assert_eq!(announce(&api).await, 1);
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));
    assert_eq!(
        notices(tenant::ID).await,
        [(
            Some(user::PAUL),
            "denied".to_string(),
            json!({ "proposals": 1, "skills": [SKILL], "optedOut": true })
        )]
    );
    // Counted as told: not sent once they choose otherwise.
    choose(&paul, chosen(true, true, true)).await;
    assert_eq!(announce(&api).await, 0);
    assert!(api.mail.sent().iter().all(|m| m.subject != ONE));
    restore_the_report(&api).await;
    forget_notices().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_relay_nothing_is_sent_nor_recorded() {
    let api = TestApi::start().await;
    one_proposal(&api).await;
    assert_eq!(announce(&api).await, 0);
    assert_eq!(announced(&api).await, [(2, false)]);
    assert!(notices(tenant::ID).await.is_empty());
    restore_the_report(&api).await;
}
