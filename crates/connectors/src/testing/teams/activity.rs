//! The activity a person's message to a Teams bot is, as the Bot Framework
//! sends it, and the claims of the token it comes with (ADR-138), from the
//! test Teams service: the API's tests write them in process
//! (`FakeTeams::message`, `FakeTeams::claims`), the end-to-end tests ask
//! for them (`outside`).

use mimorya_core::gateways::teams::BOT_FRAMEWORK_ISSUER;
use serde_json::{Value, json};

use super::{TEAMS_DIRECTORY, conversation_of, member_id};

/// The test Teams service of the test Microsoft at `base`.
pub(super) fn service_url(base: &str) -> String {
    format!("{base}/teams/")
}

/// The claims of a token the Bot Framework sends the bot `app` with an
/// activity of the service at `service_url`, valid now.
pub(super) fn claims(service_url: &str, app: &str) -> Value {
    let now = chrono::Utc::now().timestamp();
    json!({
        "iss": BOT_FRAMEWORK_ISSUER,
        "aud": app,
        "exp": now + 3600,
        "nbf": now - 60,
        "serviceurl": service_url,
    })
}

/// What the Bot Framework sends the bot `app` of the test directory when
/// the person `aad`, named `name`, writes `text` in their personal
/// conversation with it, as its activity `id`, from the service at
/// `service_url`.
pub(super) fn message(
    service_url: &str,
    app: &str,
    id: &str,
    (aad, name): (&str, &str),
    text: &str,
) -> Value {
    json!({
        "type": "message",
        "id": id,
        "timestamp": "2026-10-30T09:00:00.000Z",
        "serviceUrl": service_url,
        "channelId": "msteams",
        "from": { "id": member_id(aad), "name": name, "aadObjectId": aad },
        "conversation": {
            "conversationType": "personal",
            "tenantId": TEAMS_DIRECTORY,
            "id": conversation_of(aad),
        },
        "recipient": { "id": format!("28:{app}"), "name": "Assistant" },
        "text": text,
        "textFormat": "plain",
        "locale": "fr-FR",
        "channelData": { "tenant": { "id": TEAMS_DIRECTORY } },
    })
}
