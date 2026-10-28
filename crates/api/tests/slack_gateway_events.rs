//! What Slack sends to the company's gateway (ADR-125), before any session:
//! only a request its application signed, within five minutes, is read; an
//! unknown gateway and a wrong, missing or old signature answer alike. The
//! address check gets its challenge back. A person who has not linked their
//! Slack identity is answered, in their direct conversation, with a link to
//! do so; an event Slack sends again is taken once; an event of another
//! workspace, of a paused gateway, the application's own message and
//! anything but a direct message are taken and ignored. A gateway whose
//! secret this server cannot open answers as an unknown one. Only the exact
//! path goes without the header the browser's requests carry.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_connectors::testing::{
    BOT_USER, FakeSlack, OTHER_WORKSPACE, WORKSPACE, signed, signed_at,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::gateways::{GATEWAYS, declare_slack, direct_message, envelope, post_signed, send};
use support::llm::OTHER_MASTER_KEY;
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, WEB_ORIGIN};
use uuid::Uuid;

const LEA_SLACK: &str = "U0LEAFONT";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_what_the_application_signed_is_read() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let path = format!("/gateways/slack/{id}");
    let check = json!({
        "token": "x",
        "challenge": "3eZbrw1aBm2rZgRNFdxV2595E9CY3gmd",
        "type": "url_verification"
    })
    .to_string();

    // Signed: the challenge, sent back.
    let answered = send(&api, id, &check).await;
    assert_eq!(answered.status, StatusCode::OK, "{}", answered.text());
    assert_eq!(
        answered.json()["challenge"],
        "3eZbrw1aBm2rZgRNFdxV2595E9CY3gmd"
    );

    // Unsigned, signed otherwise, or long ago: as an unknown gateway.
    let now = Utc::now().timestamp();
    let (timestamp, signature) = signed(&check);
    let (old_timestamp, old_signature) = signed_at(&check, now - 301);
    let wrong = format!("v0={}", "0".repeat(64));
    let other_body = check.replace("3eZ", "4eZ");
    for (headers, body) in [
        (None, &check),
        (Some((timestamp.as_str(), wrong.as_str())), &check),
        (
            Some((old_timestamp.as_str(), old_signature.as_str())),
            &check,
        ),
        // Another body under this signature.
        (Some((timestamp.as_str(), signature.as_str())), &other_body),
    ] {
        let refused = post_signed(&api, &path, headers, body).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{headers:?}");
        assert_eq!(refused.error_message(), "Passerelle introuvable");
    }
    let unknown = send(&api, Uuid::new_v4(), &check).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.error_message(), "Passerelle introuvable");

    // Signed, yet not what Slack sends.
    let unreadable = send(&api, id, "pas du json").await;
    assert_eq!(unreadable.status, StatusCode::BAD_REQUEST);
    // Bounded.
    let large = format!(
        "{{\"type\": \"x\", \"pad\": \"{}\"}}",
        "a".repeat(300 * 1024)
    );
    let refused = send(&api, id, &large).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(slack.posts().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_not_linked_is_sent_a_link_once_per_event() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;

    let message = direct_message("Ev0MESSAGE01", LEA_SLACK, "Bonjour !");
    let taken = send(&api, id, &message).await;
    assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    // Slack sends it again: taken once.
    assert_eq!(send(&api, id, &message).await.status, StatusCode::OK);
    api.idle().await;
    let posts = slack.posts();
    assert_eq!(posts.len(), 1, "{posts:?}");
    let post = &posts[0];
    assert_eq!(post.channel, "D0LEA");
    // The code in the fragment: never sent to a server with the page.
    let link = format!("<{WEB_ORIGIN}/account/gateway-link#");
    assert!(post.text.contains(&link), "{}", post.text);
    assert!(post.text.contains("|relier mon compte>"), "{}", post.text);
    assert!(post.text.contains("15 minutes"), "{}", post.text);
    // Never what the person wrote.
    assert!(!post.text.contains("Bonjour"), "{}", post.text);

    // In a thread, the answer goes in it.
    let mut threaded: serde_json::Value =
        serde_json::from_str(&direct_message("Ev0MESSAGE02", LEA_SLACK, "Et ici ?")).unwrap();
    threaded["event"]["thread_ts"] = json!("1700000000.000001");
    assert_eq!(
        send(&api, id, &threaded.to_string()).await.status,
        StatusCode::OK
    );
    api.idle().await;
    let posts = slack.posts();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[1].thread_ts.as_deref(), Some("1700000000.000001"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_but_a_person_s_direct_message_of_the_workspace_is_answered() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let message = |event_id: &str, change: &dyn Fn(&mut serde_json::Value)| {
        let mut event: serde_json::Value =
            serde_json::from_str(&direct_message(event_id, LEA_SLACK, "Bonjour")).unwrap();
        change(&mut event);
        event.to_string()
    };
    let ignored = [
        // The application's own message.
        message("Ev0IGNORED01", &|e| e["event"]["user"] = json!(BOT_USER)),
        // A bot's, an edit.
        message("Ev0IGNORED02", &|e| e["event"]["bot_id"] = json!("B0AUTRE")),
        message("Ev0IGNORED03", &|e| {
            e["event"]["subtype"] = json!("message_changed")
        }),
        // Elsewhere than a direct conversation.
        message("Ev0IGNORED04", &|e| {
            e["event"]["channel_type"] = json!("channel")
        }),
        // Of another workspace, though signed by the application.
        message("Ev0IGNORED05", &|e| e["team_id"] = json!(OTHER_WORKSPACE)),
        // Not named as Slack names events.
        message("pas-un-evenement", &|_| {}),
        // Another kind of envelope.
        envelope(
            WORKSPACE,
            "Ev0IGNORED06",
            json!({ "type": "app_home_opened", "user": LEA_SLACK }),
        ),
        json!({ "type": "app_rate_limited", "minute_rate_limited": 1 }).to_string(),
    ];
    for body in &ignored {
        let taken = send(&api, id, body).await;
        assert_eq!(taken.status, StatusCode::OK, "{body}");
    }
    // A paused gateway answers, and does nothing.
    let paused = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK);
    let taken = send(&api, id, &message("Ev0PAUSED01", &|_| {})).await;
    assert_eq!(taken.status, StatusCode::OK);
    api.idle().await;
    assert!(slack.posts().is_empty(), "{:?}", slack.posts());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_gateway_event_goes_without_the_header() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let id = Uuid::new_v4();
    // Reached: it answers as to an unknown gateway.
    let reached = post_signed(&api, &format!("/gateways/slack/{id}"), None, "{}").await;
    assert_eq!(reached.status, StatusCode::NOT_FOUND);
    for path in [
        format!("/gateways/slack/{id}/"),
        format!("/gateways/slack/{}", id.to_string().to_uppercase()),
        format!("/gateways/slack/{}", id.simple()),
        format!("/gateways/slack/{}", id.to_string().replacen('-', "%2D", 1)),
        "/gateways/slack/pas-un-identifiant".to_owned(),
        format!("/gateways/slack//{id}"),
        format!("/gateways/teams/{id}"),
        "/admin/gateways/slack".to_owned(),
        "/account/gateways/link".to_owned(),
        "/account/gateways/link/preview".to_owned(),
    ] {
        let refused = post_signed(&api, &path, None, "{}").await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            refused.error_message(),
            "En-tête de requête manquant",
            "{path}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_gateway_whose_secret_this_server_cannot_open_answers_as_unknown() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let check = json!({ "type": "url_verification", "challenge": "defi" }).to_string();
    assert_eq!(send(&api, id, &check).await.status, StatusCode::OK);

    // Another server, another master key: the stored secret does not open.
    let deployment = [
        ("MIMORYA_MASTER_KEY", OTHER_MASTER_KEY),
        ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
    ];
    let other = TestApi::start_with_slack(&deployment, &slack.url).await;
    let refused = send(&other, id, &check).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    assert_eq!(refused.error_message(), "Passerelle introuvable");
}
