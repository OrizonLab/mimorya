//! The company's Microsoft Teams bot in the tests (ADR-138): declared by an
//! administrator with the local Microsoft's test application, activities
//! sent as the Bot Framework sends them, with its token (or another), the
//! link the bot offers a person, and their Teams identity linked with it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_connectors::testing::{
    FakeTeams, Signer, TEAMS_APP, TEAMS_DIRECTORY, TEAMS_SECRET, signed_token,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::gateways::remove_gateways;
use super::{Client, TestApi, TestResponse};

pub const DECLARE_TEAMS: &str = "/admin/gateways/teams";
/// Marc's and Léa's Microsoft Entra objects, as Teams names them.
pub const MARC_TEAMS: (&str, &str) = ("5a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d", "Marc Lefèvre");
pub const LEA_TEAMS: (&str, &str) = ("6b2c3d4e-5f6a-4b7c-9d8e-0f1a2b3c4d5e", "Léa Font");

/// The company's Teams bot, declared by `admin` with the local Microsoft's
/// test application; the gateways a previous test declared go first.
pub async fn declare_teams(admin: &Client<'_>) -> Uuid {
    remove_gateways(admin).await;
    let declared = admin
        .post(
            DECLARE_TEAMS,
            json!({
                "name": "Teams Néo",
                "appId": TEAMS_APP,
                "directoryId": TEAMS_DIRECTORY,
                "clientSecret": TEAMS_SECRET,
            }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::CREATED, "{}", declared.text());
    declared.json()["id"].as_str().unwrap().parse().unwrap()
}

/// What posting `activity` to the bot `id`, with `authorization` if any,
/// without the browser's header, gets; once answered, what it started is
/// done.
pub async fn post_activity(
    api: &TestApi,
    id: Uuid,
    authorization: Option<&str>,
    activity: &Value,
) -> TestResponse {
    let mut request =
        Request::post(format!("/gateways/teams/{id}")).header("content-type", "application/json");
    if let Some(authorization) = authorization {
        request = request.header("authorization", authorization);
    }
    let sent = api
        .send(request.body(Body::from(activity.to_string())).unwrap())
        .await;
    api.idle().await;
    sent
}

/// What the Bot Framework sending `activity` to the bot `id`, with its
/// token for the test application, gets.
pub async fn deliver(api: &TestApi, teams: &FakeTeams, id: Uuid, activity: &Value) -> TestResponse {
    let token = format!("Bearer {}", teams.token(TEAMS_APP));
    post_activity(api, id, Some(&token), activity).await
}

/// The same, with a token of `claims` signed by `signer`.
pub async fn deliver_signed(
    api: &TestApi,
    id: Uuid,
    (signer, claims): (Signer, &Value),
    activity: &Value,
) -> TestResponse {
    let token = format!("Bearer {}", signed_token(signer, claims));
    post_activity(api, id, Some(&token), activity).await
}

/// What the person `from` writes to the bot `id` as the activity
/// `activity_id`, delivered.
pub async fn write(
    api: &TestApi,
    teams: &FakeTeams,
    id: Uuid,
    activity_id: &str,
    from: (&str, &str),
    text: &str,
) {
    let activity = teams.message(TEAMS_APP, activity_id, from, text);
    let sent = deliver(api, teams, id, &activity).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
}

/// The code of the link the bot last wrote.
pub fn offered_code(teams: &FakeTeams) -> String {
    let sent = teams.sent();
    let text = &sent.last().expect("a message written").text;
    let start = text.find("gateway-link#").expect("a link") + "gateway-link#".len();
    let end = start + text[start..].find(')').expect("its end");
    text[start..end].to_owned()
}

/// The code of the link the bot sends the person `from`, not yet linked,
/// when they write to it as the activity `activity_id`.
pub async fn link_code(
    api: &TestApi,
    teams: &FakeTeams,
    id: Uuid,
    activity_id: &str,
    from: (&str, &str),
) -> String {
    write(api, teams, id, activity_id, from, "Bonjour").await;
    offered_code(teams)
}
