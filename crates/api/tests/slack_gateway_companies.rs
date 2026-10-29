//! Two companies' Slack gateways (ADR-125): an event one company's
//! application signed reaches nothing of another's gateway; the events the
//! gateways took are forgotten a day later, in every company, the recent ones
//! kept; each company's people are answered in their own threads, with their
//! own company's application (ADR-127).

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::AccessContext;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::admin::{NewSlackGatewayInput, declare_slack_gateway};
use mimorya_connectors::testing::{BOT_TOKEN, FakeSlack, SIGNING_SECRET, WORKSPACE, signed};
use mimorya_core::demo::{tenant, user};
use mimorya_core::gateways::slack::signature as sign;
use mimorya_db::repositories::gateways::purge_gateway_events;
use serde_json::json;
use support::TestApi;
use support::companies;
use support::email;
use support::gateways::{declare_slack, envelope, offered_code, post_signed};
use support::queue::as_owner;
use support::slack_conversations::{
    MARC_DM, MARC_SLACK, answer, event_id, linked, settle, write, written,
};
use support::sso::SSO_DEPLOYMENT;
use uuid::Uuid;

/// Another company's own signing secret.
const OTHER_SECRET: &str = "0ther5ecret0ther5ecret0ther5ecr";

/// Another company, and its Slack gateway.
async fn other_company(api: &TestApi, slug: &str) -> (Uuid, Uuid) {
    let (tenant_id, gateway, _) = other_company_with_admin(api, slug).await;
    (tenant_id, gateway)
}

/// Another company, its Slack gateway, and its administrator's rights.
async fn other_company_with_admin(api: &TestApi, slug: &str) -> (Uuid, Uuid, AccessContext) {
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
    (acme.tenant_id, gateway, jeanne)
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
    // Acme's own signature reaches Acme's gateway, not Néo Maintenance's.
    let theirs = sign(OTHER_SECRET, &timestamp, check.as_bytes());
    let acme_headers = Some((timestamp.as_str(), theirs.as_str()));
    let path = format!("/gateways/slack/{other}");
    let answered = post_signed(&api, &path, acme_headers, &check).await;
    assert_eq!(answered.status, StatusCode::OK, "{}", answered.text());
    assert_eq!(answered.json()["challenge"], "defi");
    let path = format!("/gateways/slack/{own}");
    let refused = post_signed(&api, &path, acme_headers, &check).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
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

/// Jeanne's Slack user, and her direct conversation with Acme's application.
const JEANNE_SLACK: &str = "U0JEANNE";
const JEANNE_DM: &str = "D0JEANNE";

/// What Jeanne writes at `ts` to Acme's gateway `gateway`, signed with its
/// own secret, as Slack sends it.
async fn jeanne_writes(api: &TestApi, gateway: Uuid, text: &str, ts: &str) {
    let event = json!({
        "type": "message", "channel": JEANNE_DM, "user": JEANNE_SLACK, "text": text,
        "ts": ts, "channel_type": "im", "event_ts": ts
    });
    let body = envelope(WORKSPACE, &event_id(), event);
    let timestamp = Utc::now().timestamp().to_string();
    let signature = sign(OTHER_SECRET, &timestamp, body.as_bytes());
    let headers = Some((timestamp.as_str(), signature.as_str()));
    let path = format!("/gateways/slack/{gateway}");
    let sent = post_signed(api, &path, headers, &body).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_is_answered_in_its_own_threads() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let neo = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let slug = "acme-slack-replies";
    let (_, acme, jeanne) = other_company_with_admin(&api, slug).await;
    // Jeanne links her Slack user to her account, at Acme.
    slack.add_person(JEANNE_SLACK, &format!("jeanne.martin@{slug}.example"));
    jeanne_writes(&api, acme, "Bonjour", "1700000100.000100").await;
    let code = offered_code(&slack);
    (api.app.services.gateways)
        .confirm_link(&jeanne, &code, Utc::now())
        .await
        .unwrap();

    // Each writes; one pass answers both, each in its own thread.
    let thread = "1700000200.000100";
    api.script(vec![ScriptedStep::text("Réponse Néo.")]);
    write(&api, neo, &written(MARC_SLACK, "Question", thread, None)).await;
    api.script(vec![ScriptedStep::text("Réponse Acme.")]);
    jeanne_writes(&api, acme, "Question", thread).await;
    assert_eq!(answer(&api).await, 2);
    let said_in = |channel: &str| -> Vec<String> {
        (slack.posts().into_iter())
            .filter(|p| p.channel == channel && p.thread_ts.as_deref() == Some(thread))
            .map(|p| p.text)
            .collect()
    };
    let marcs = said_in(MARC_DM);
    assert_eq!(marcs.len(), 1);
    assert!(marcs[0].starts_with("Réponse Néo."), "{marcs:?}");
    let jeannes = said_in(JEANNE_DM);
    assert_eq!(jeannes.len(), 1);
    assert!(jeannes[0].starts_with("Réponse Acme."), "{jeannes:?}");
    assert!(!jeannes[0].contains("Néo"), "{jeannes:?}");
}
