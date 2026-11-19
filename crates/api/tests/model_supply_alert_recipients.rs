//! Who is told of the operator's supply near its end (ADR-211), and when:
//! each account that manages the company's settings on the whole company,
//! as its rules say now (`company_wide_administrators`). Not an
//! administrator of a unit only, nor a disabled account, nor a role whose
//! assignment expired; with nobody to tell, nothing is recorded, and the
//! alert waits for an administrator. The pass runs in a process that
//! executes runs, one at a time; an e-mail the relay refuses is audited,
//! without the address, and not sent again. What is told:
//! `model_supply_alerts.rs`.

mod support;

use std::time::Duration;

use mimorya_api::services::provisioning::CapChoice;
use mimorya_api::services::supply_alerts::{SUPPLY_ALERT_SWEEP, send_supply_alerts};
use mimorya_core::demo::{tenant, user};
use mimorya_core::model_supply::SupplyFormula;
use sqlx::PgPool;
use support::model_supply::{CAP, capped, spent, supply};
use support::owner::owner_runs;
use support::refusals::{entries_since, latest_entry};
use support::supply_alerts::{leave, pass, recorded, told};
use support::{TestApi, email, owner_url};
use uuid::Uuid;

const EIGHTY: &str = "Consommation du modèle : 80 % du plafond mensuel atteints";

/// The company's administrators' role.
async fn admin_role(owner: &PgPool) -> Uuid {
    sqlx::query_scalar("select id from roles where tenant_id = $1 and key = 'tenant_admin'")
        .bind(tenant::ID)
        .fetch_one(owner)
        .await
        .unwrap()
}

/// The company at 80 % of a 1 USD cap.
async fn at_eighty(owner: &PgPool) {
    leave(owner).await;
    supply(owner, capped(CapChoice::Amount(CAP))).await;
    spent(owner, SupplyFormula::Capped, CAP * 80 / 100).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_manages_the_settings_of_the_whole_company_is_told() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    at_eighty(&owner).await;
    let role = admin_role(&owner).await;
    // Marc administers his unit only; Hugo's account is disabled; Lea's
    // role expired yesterday.
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

    let before = api.mail.sent().len();
    assert_eq!(pass(&api).await, [EIGHTY]);
    assert_eq!(told(&api, before), [email(user::PAUL)]);

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
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn with_nobody_to_tell_the_alert_waits_for_an_administrator() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    at_eighty(&owner).await;
    // A deny on the settings' administration, for every person.
    let policy = Uuid::new_v4();
    owner_runs(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition) values ($1, $2, 'Refus de test', 'deny', array['settings:manage'], \
         '{\"op\": \"eq\", \"left\": {\"attr\": \"context.actorType\"}, \
         \"right\": {\"value\": \"user\"}}'::jsonb)",
        &[policy, tenant::ID],
    )
    .await;
    assert!(pass(&api).await.is_empty());
    assert_eq!(recorded(&api).await, 0);

    // An administrator again: told then, once.
    owner_runs("delete from access_policies where id = $1", &[policy]).await;
    assert_eq!(pass(&api).await, [EIGHTY]);
    assert!(pass(&api).await.is_empty());
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_process_that_executes_runs_alerts_at_a_time() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    at_eighty(&owner).await;
    // A process that executes no runs alerts nobody.
    assert_eq!(send_supply_alerts(&api.app.services).await.unwrap(), 0);
    // Under way elsewhere: skipped, nothing sent nor recorded.
    let db = &api.app.services.db;
    let held = db.try_sweep_lock(SUPPLY_ALERT_SWEEP).await.unwrap();
    let held = held.expect("free");
    assert!(pass(&api).await.is_empty());
    assert_eq!(recorded(&api).await, 0);
    held.release().await;

    pass(&api).await;
    // A maintenance pass may have alerted meanwhile: once it is done.
    let wait = Duration::from_secs(5);
    db.sweep_lock(SUPPLY_ALERT_SWEEP, wait)
        .await
        .unwrap()
        .release()
        .await;
    let to_paul = api
        .mail
        .sent()
        .iter()
        .filter(|mail| mail.to == email(user::PAUL) && mail.subject == EIGHTY)
        .count();
    assert_eq!(to_paul, 1);
    leave(&owner).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_alert_the_relay_refuses_is_audited_and_not_sent_again() {
    let api = TestApi::start_with_mailer().await;
    let owner = PgPool::connect(owner_url()).await.unwrap();
    at_eighty(&owner).await;
    let since = latest_entry(&api).await;
    api.mail.fail();

    assert!(pass(&api).await.is_empty());
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("model_supply.alerted"), "{audit}");
    assert!(audit.contains("\"failure\""), "{audit}");
    assert!(
        audit.contains("\"percent\": 80") || audit.contains("\"percent\":80"),
        "{audit}"
    );
    assert!(!audit.contains(&email(user::PAUL)), "{audit}");
    // Recorded: not sent again.
    assert!(recorded(&api).await > 0);
    let again = latest_entry(&api).await;
    pass(&api).await;
    assert!(
        !entries_since(&api, again)
            .await
            .contains("model_supply.alerted")
    );
    leave(&owner).await;
}
