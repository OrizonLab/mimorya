//! The user directory shows each field only to those entitled to it, on
//! every entry: readers see who is who; account managers also see sign-ins,
//! units, clearance, attributes and pending invitations; role managers also
//! see sign-ins, units, clearance and roles. A withheld field is absent, not
//! `null`, and rights limited to one team count as none.

mod support;

use std::collections::BTreeSet;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::accounts::{activate, create_partial_admin, holder_of, invite_account};
use support::{Client, TestApi};

const READER: &[&str] = &["id", "displayName", "jobTitle", "status", "orgUnit"];
const MANAGER: &[&str] = &["email", "lastLoginAt", "orgUnitId", "clearance"];
const ACCOUNTS: &[&str] = &["attributes", "invitationExpiresAt"];
const ROLES: &[&str] = &["grants", "assignments"];

fn fields(parts: &[&[&str]]) -> BTreeSet<String> {
    parts
        .iter()
        .flat_map(|p| p.iter())
        .map(|f| f.to_string())
        .collect()
}

/// Every entry of the directory as `client` reads it has exactly `expected`.
async fn every_entry_shows(client: &Client<'_>, expected: BTreeSet<String>) {
    let directory = client.get("/admin/users").await;
    assert_eq!(directory.status, StatusCode::OK, "{}", directory.text());
    let users = directory.json()["users"].as_array().unwrap().clone();
    assert!(users.len() > 5);
    for entry in users {
        let shown: BTreeSet<String> = entry.as_object().unwrap().keys().cloned().collect();
        assert_eq!(shown, expected, "{entry}");
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_directory_field_reaches_only_those_entitled_to_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Values to withhold: a pending invitation, an attribute, sign-ins.
    let (invited, _) = invite_account(&paul, "invitation.en.attente").await;
    let set = paul
        .patch(
            &format!("/admin/users/{}", user::EMMA),
            json!({ "attributes": { "contractType": "CDI" } }),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK);
    let reader = holder_of(&api, &paul, "lecteur.annuaire", &["user:read"]).await;
    let accounts = holder_of(
        &api,
        &paul,
        "gestion.comptes",
        &["user:read", "user:manage"],
    )
    .await;
    let roles = holder_of(&api, &paul, "gestion.roles", &["user:read", "role:manage"]).await;
    let mut team_admin = create_partial_admin(&api, &paul, "admin.equipe").await;
    let team_admin = activate(&api, &mut team_admin).await;

    every_entry_shows(&reader, fields(&[READER])).await;
    every_entry_shows(&team_admin, fields(&[READER])).await;
    every_entry_shows(&accounts, fields(&[READER, MANAGER, ACCOUNTS])).await;
    every_entry_shows(&roles, fields(&[READER, MANAGER, ROLES])).await;
    every_entry_shows(&paul, fields(&[READER, MANAGER, ACCOUNTS, ROLES])).await;

    // The withheld values exist: they are not merely empty.
    let listed = paul.get("/admin/users").await.json()["users"]
        .as_array()
        .unwrap()
        .clone();
    let entry = |id: String| {
        listed
            .iter()
            .find(|u| u["id"] == json!(id))
            .expect("listed")
            .clone()
    };
    assert!(entry(invited.id.to_string())["invitationExpiresAt"].is_string());
    assert_eq!(entry(invited.id.to_string())["status"], "invited");
    assert_eq!(
        entry(user::EMMA.to_string())["attributes"]["contractType"],
        "CDI"
    );
    assert!(entry(user::PAUL.to_string())["lastLoginAt"].is_string());
    assert!(
        !entry(user::PAUL.to_string())["grants"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
