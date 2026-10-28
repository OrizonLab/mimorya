//! Two companies' Slack gateways (ADR-125): an event one company's
//! application signed reaches nothing of another's gateway; the events the
//! gateways took are forgotten a day later, in every company, the recent ones
//! kept.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_api::services::AccessContext;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::admin::{NewSlackGatewayInput, declare_slack_gateway};
use mimorya_connectors::testing::{BOT_TOKEN, FakeSlack, SIGNING_SECRET, signed};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::gateways::purge_gateway_events;
use serde_json::json;
use support::TestApi;
use support::companies;
use support::gateways::{declare_slack, post_signed};
use support::queue::as_owner;
use support::sso::SSO_DEPLOYMENT;
use uuid::Uuid;

/// Another company's own signing secret.
const OTHER_SECRET: &str = "0ther5ecret0ther5ecret0ther5ecr";

/// Another company, its administrator's rights, and its Slack gateway.
async fn other_company(api: &TestApi, slug: &str) -> (Uuid, Uuid) {
    let acme = companies::create(api, slug, &format!("jeanne.martin@{slug}.example")).await;
    // Her invitation accepted.
    as_owner(
        "update users set status = 'active' where id = $1",
        acme.admin_id,
    )
    .await;
    let jeanne: AccessContext = api
        .app
        .services
        .access
        .context(AuthIdentity {
            tenant_id: acme.tenant_id,
            user_id: acme.admin_id,
            session_id: Uuid::nil(),
            must_change_password: false,
        })
        .await
        .unwrap();
    let input = NewSlackGatewayInput {
        name: "Slack Acme".into(),
        bot_token: BOT_TOKEN.into(),
        signing_secret: OTHER_SECRET.into(),
        enabled: true,
    };
    let gateway = declare_slack_gateway(&api.app.services.gateways, &jeanne, input)
        .await
        .unwrap();
    (acme.tenant_id, gateway)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_event_one_company_signed_reaches_nothing_of_another_s() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let own = declare_slack(&paul).await;
    let (_, other) = other_company(&api, "acme-slack-events").await;
    let check = json!({ "type": "url_verification", "challenge": "defi" }).to_string();
    let (timestamp, signature) = signed(&check);
    let headers = Some((timestamp.as_str(), signature.as_str()));

    let answered = post_signed(&api, &format!("/gateways/slack/{own}"), headers, &check).await;
    assert_eq!(answered.status, StatusCode::OK, "{}", answered.text());
    // Signed with Néo Maintenance's secret, sent to Acme's gateway.
    let refused = post_signed(&api, &format!("/gateways/slack/{other}"), headers, &check).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    assert_eq!(refused.error_message(), "Passerelle introuvable");
    assert_ne!(SIGNING_SECRET, OTHER_SECRET);
}

/// Keeps that `gateway` of `tenant_id` took `event_id` at `at`.
async fn taken(api: &TestApi, tenant_id: Uuid, gateway: Uuid, event_id: &str, at: DateTime<Utc>) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into gateway_events (tenant_id, gateway_id, event_id, received_at) \
         values ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(gateway)
    .bind(event_id)
    .bind(at)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The events `tenant_id`'s gateways still keep.
async fn kept(api: &TestApi, tenant_id: Uuid) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_scalar("select event_id from gateway_events order by event_id")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_events_taken_are_forgotten_a_day_later_in_every_company() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let own = declare_slack(&paul).await;
    let (acme, other) = other_company(&api, "acme-slack-purge").await;
    let now = Utc::now();
    let old = now - Duration::hours(25);
    taken(&api, tenant::ID, own, "Ev0NEOANCIEN", old).await;
    taken(&api, tenant::ID, own, "Ev0NEORECENT", now).await;
    taken(&api, acme, other, "Ev0ACMEANCIEN", old).await;
    taken(&api, acme, other, "Ev0ACMERECENT", now).await;

    let pool = api.app.services.db.pool();
    let purged = purge_gateway_events(pool, now - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(purged, 2);
    assert_eq!(kept(&api, tenant::ID).await, ["Ev0NEORECENT"]);
    assert_eq!(kept(&api, acme).await, ["Ev0ACMERECENT"]);
    // Nothing more a second time.
    let again = purge_gateway_events(pool, now - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(again, 0);
}
