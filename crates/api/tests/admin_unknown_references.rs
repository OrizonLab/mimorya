//! Administration changes sending in their body an identifier that
//! designates nothing the company has (a role, a unit, a lead) are answered
//! as invalid, and leave a trace (ADR-054): the identifier and what it was
//! meant to designate, alike for another company's resource and for one
//! that exists nowhere, in the administrator's company only. Only an
//! administrator holding the permission reaches that answer; an invalid
//! request of another kind leaves none, nor does anything the request sent
//! but the identifiers.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, unit, user};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::refusals::{
    entries_of_since, entries_since, latest_entry, latest_entry_of, not_found_since,
    refusals_since, unknown_references_since,
};
use support::{Client, TestApi, TestResponse};
use uuid::Uuid;

/// Sent in the requests: it must reach no entry.
const MARKER: &str = "Marqueur 7d2a";

/// Another company's custom role and organisation unit.
async fn foreign_role_and_unit(api: &TestApi) -> (Uuid, Uuid) {
    let (role_id, unit_id) = (Uuid::new_v4(), Uuid::new_v4());
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into roles (id, tenant_id, key, name, permissions) \
         values ($1, $2, 'custom_unknown', 'Étranger', '{intervention:read}')",
    )
    .bind(role_id)
    .bind(OTHER_TENANT_ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into org_units (id, tenant_id, name, type) values ($1, $2, 'Autre siège', 'company')",
    )
    .bind(unit_id)
    .bind(OTHER_TENANT_ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (role_id, unit_id)
}

/// One change sending `id` where a resource of `kind` is expected.
struct Change {
    method: &'static str,
    route: &'static str,
    /// The resource the path designates, if any.
    resource: Option<(&'static str, Uuid)>,
    kind: &'static str,
    message: &'static str,
}

impl Change {
    async fn send(&self, admin: &Client<'_>, id: Uuid) -> TestResponse {
        let lucas = format!("/admin/users/{}", user::LUCAS);
        let north = format!("/admin/org-units/{}", unit::TEAM_NORTH);
        match (self.method, self.route, self.message) {
            ("POST", "/admin/users/{id}/assignments", "Rôle inconnu") => {
                let grant = json!({ "roleId": id, "scope": { "type": "tenant" } });
                admin.post(&format!("{lucas}/assignments"), grant).await
            }
            ("POST", "/admin/users/{id}/assignments", _) => {
                let scope =
                    json!({ "type": "org_unit", "orgUnitId": id, "includeDescendants": true });
                let grant = json!({ "roleId": role::TECHNICIAN, "scope": scope });
                admin.post(&format!("{lucas}/assignments"), grant).await
            }
            ("POST", "/admin/org-units", _) => {
                let unit = json!({ "parentId": id, "name": MARKER, "type": "team" });
                admin.post("/admin/org-units", unit).await
            }
            ("PATCH", "/admin/org-units/{id}", "Unité parente inconnue") => {
                admin
                    .patch(&north, json!({ "parentId": id, "name": MARKER }))
                    .await
            }
            ("PATCH", "/admin/org-units/{id}", _) => {
                admin
                    .patch(&north, json!({ "leadUserId": id, "name": MARKER }))
                    .await
            }
            ("POST", "/admin/users", _) => {
                let account = json!({
                    "email": "unite.inconnue@neo-maintenance.example",
                    "displayName": MARKER,
                    "orgUnitId": id,
                });
                admin.post("/admin/users", account).await
            }
            _ => {
                admin
                    .patch(&lucas, json!({ "orgUnitId": id, "displayName": MARKER }))
                    .await
            }
        }
    }
}

fn changes() -> Vec<Change> {
    let lucas = Some(("user", user::LUCAS));
    let north = Some(("org_unit", unit::TEAM_NORTH));
    let change = |method, route, resource, kind, message| Change {
        method,
        route,
        resource,
        kind,
        message,
    };
    vec![
        change(
            "POST",
            "/admin/users/{id}/assignments",
            lucas,
            "role",
            "Rôle inconnu",
        ),
        change(
            "POST",
            "/admin/users/{id}/assignments",
            lucas,
            "org_unit",
            "Unité inconnue",
        ),
        change(
            "POST",
            "/admin/org-units",
            None,
            "org_unit",
            "Unité parente inconnue",
        ),
        change(
            "PATCH",
            "/admin/org-units/{id}",
            north,
            "org_unit",
            "Unité parente inconnue",
        ),
        change(
            "PATCH",
            "/admin/org-units/{id}",
            north,
            "user",
            "Responsable inconnu",
        ),
        change("POST", "/admin/users", None, "org_unit", "Unité inconnue"),
        change(
            "PATCH",
            "/admin/users/{id}",
            lucas,
            "org_unit",
            "Unité inconnue",
        ),
    ]
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn unknown_identifiers_sent_in_the_body_are_audited_alike() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (foreign_role, foreign_unit) = foreign_role_and_unit(&api).await;
    let since = latest_entry(&api).await;
    let elsewhere = latest_entry_of(&api, OTHER_TENANT_ID).await;

    // Each change twice: with another company's resource, then with one that
    // exists nowhere. Both are answered alike, as invalid.
    let mut sent = Vec::new();
    for change in changes() {
        let foreign = match change.kind {
            "role" => foreign_role,
            "user" => OTHER_USER_ID,
            _ => foreign_unit,
        };
        let mut answers = Vec::new();
        for id in [foreign, Uuid::new_v4()] {
            let response = change.send(&paul, id).await;
            assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", change.route);
            assert_eq!(response.error_message(), change.message, "{}", change.route);
            answers.push(response.json());
            sent.push((
                id,
                change.method,
                change.route,
                change.resource,
                change.kind,
            ));
        }
        assert_eq!(answers[0], answers[1], "{}", change.route);
    }

    let traced = unknown_references_since(&api, since).await;
    assert_eq!(traced.len(), sent.len(), "{traced:?}");
    for ((actor, entry), (id, method, route, resource, kind)) in traced.iter().zip(&sent) {
        assert_eq!(*actor, user::PAUL);
        let resource = resource.map(|(kind, id)| (kind.to_string(), id.to_string()));
        assert_eq!(entry.resource, resource, "{route}");
        let expected: Value = json!({
            "method": method,
            "route": route,
            "reference": { "type": kind, "id": id },
        });
        assert_eq!(entry.metadata, expected);
        assert!(entry.reason.is_some() && entry.request_id.is_some());
    }
    // Apart from the identifier, another company's resource and an absent
    // one leave the same entry.
    for pair in traced.chunks(2) {
        let (foreign, absent) = (&pair[0].1, &pair[1].1);
        assert_eq!(foreign.reason, absent.reason);
        assert_eq!(foreign.metadata["route"], absent.metadata["route"]);
        assert_eq!(
            foreign.metadata["reference"]["type"],
            absent.metadata["reference"]["type"]
        );
    }
    assert!(refusals_since(&api, since).await.is_empty());
    assert!(not_found_since(&api, since).await.is_empty());
    assert!(!entries_since(&api, since).await.contains(MARKER));
    assert_eq!(
        entries_of_since(&api, OTHER_TENANT_ID, elsewhere).await,
        "[]"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_authorised_administrator_s_unknown_identifiers_are_traced() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut account = create_partial_admin(&api, &paul, "admin.unknown").await;
    let partial = activate(&api, &mut account).await;
    let since = latest_entry(&api).await;

    // Refused before anything is read: traced as refusals.
    let grant = json!({ "roleId": Uuid::new_v4(), "scope": { "type": "tenant" } });
    let url = format!("/admin/users/{}/assignments", user::LUCAS);
    for client in [&partial, &marc] {
        let granted = client.post(&url, grant.clone()).await;
        assert_eq!(granted.status, StatusCode::FORBIDDEN);
    }
    // Invalid for another reason: not traced.
    let past = json!({
        "roleId": role::TECHNICIAN,
        "scope": { "type": "tenant" },
        "expiresAt": "2020-01-01T00:00:00Z",
    });
    assert_eq!(paul.post(&url, past).await.status, StatusCode::BAD_REQUEST);
    let typed = json!({ "parentId": unit::TECHNICAL, "name": MARKER, "type": "galaxie" });
    let created = paul.post("/admin/org-units", typed).await;
    assert_eq!(created.status, StatusCode::BAD_REQUEST);

    assert_eq!(refusals_since(&api, since).await.len(), 2);
    assert!(unknown_references_since(&api, since).await.is_empty());
}
