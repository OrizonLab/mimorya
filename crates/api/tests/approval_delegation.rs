//! Declaring an absence (ADR-071): each person names a colleague who stands
//! in for them, for at most 90 days ending in the future; declared again,
//! it is replaced; withdrawn, it is gone, and audited either way. The
//! delegate is an active account of the company, never oneself. Ended or
//! of a disabled account, an absence stands in for nothing; ended, it is
//! purged. Whom one may name is shown to those who validate only
//! (ADR-073).

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, user};
use mimorya_db::repositories::approval_delegations::purge_ended_approval_delegations;
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::{Client, TestApi, owner_url};
use uuid::Uuid;

const ABSENCE: &str = "/approval-delegation";

fn in_days(days: i64) -> String {
    (Utc::now() + Duration::days(days)).to_rfc3339()
}

/// Names `delegate` from now until `days` from now.
async fn declare(client: &Client<'_>, delegate: Uuid, days: i64) {
    let body = json!({ "delegateId": delegate, "endsAt": in_days(days) });
    let declared = client.put(ABSENCE, body).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
}

/// Who the reader's absence names, and whom they stand in for.
async fn read(client: &Client<'_>) -> (Option<Uuid>, Vec<Uuid>) {
    let read = client.get(ABSENCE).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    let body = read.json();
    let id = |v: &Value| v.as_str().unwrap().parse().unwrap();
    let own = body["own"].as_object().map(|o| id(&o["delegate"]["id"]));
    let covers = body["standingInFor"].as_array().unwrap();
    (own, covers.iter().map(|d| id(&d["absent"]["id"])).collect())
}

/// Each absence change audited for `user_id`: action and metadata.
async fn audited(api: &TestApi, user_id: Uuid) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "select action, metadata from audit_log where action like 'approval.delegation%' \
         and actor_user_id = $1 and resource_id = $1::text order by id",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_absence_is_declared_replaced_and_withdrawn() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let marc = api.sign_in(user::MARC).await;
    let nadia = api.sign_in(user::NADIA).await;
    assert_eq!(read(&julien).await, (None, vec![]));

    // Ninety days, and the hour a clock going back adds (ADR-073).
    let starts = Utc::now() + Duration::days(1);
    let ends = starts + Duration::days(90) + Duration::minutes(59);
    let body = json!({ "delegateId": user::MARC, "startsAt": starts.to_rfc3339(), "endsAt": ends.to_rfc3339() });
    let declared = julien.put(ABSENCE, body).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let own = julien.get(ABSENCE).await.json()["own"].clone();
    assert_eq!(own["absent"]["displayName"], "Julien Moreau");
    assert_eq!(own["delegate"]["displayName"], "Marc Lefèvre");
    let read_start: chrono::DateTime<Utc> = own["startsAt"].as_str().unwrap().parse().unwrap();
    assert!((read_start - starts).num_seconds().abs() < 1);
    // To come, it is listed to whom it names.
    assert_eq!(read(&marc).await, (None, vec![user::JULIEN]));

    // Declared again, it is replaced.
    declare(&julien, user::NADIA, 3).await;
    assert_eq!(read(&julien).await, (Some(user::NADIA), vec![]));
    assert_eq!(read(&marc).await, (None, vec![]));
    assert_eq!(read(&nadia).await, (None, vec![user::JULIEN]));

    let withdrawn = julien.delete(ABSENCE).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    assert_eq!(read(&julien).await, (None, vec![]));
    assert_eq!(read(&nadia).await, (None, vec![]));
    let again = julien.delete(ABSENCE).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.text());

    let actions: Vec<(String, Option<Value>)> = audited(&api, user::JULIEN)
        .await
        .into_iter()
        .map(|(action, metadata)| (action, metadata.get("delegateId").cloned()))
        .collect();
    assert_eq!(
        actions,
        [
            ("approval.delegation_set".into(), Some(json!(user::MARC))),
            ("approval.delegation_set".into(), Some(json!(user::NADIA))),
            ("approval.delegation_ended".into(), None),
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_absence_that_cannot_hold_is_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    let before = audited(&api, user::JULIEN).await.len();
    let refused = |delegate: Uuid, starts: Option<String>, ends: String, message: &'static str| {
        let julien = &julien;
        async move {
            let body = match starts {
                Some(starts) => {
                    json!({ "delegateId": delegate, "startsAt": starts, "endsAt": ends })
                }
                None => json!({ "delegateId": delegate, "endsAt": ends }),
            };
            let answer = julien.put(ABSENCE, body).await;
            assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
            assert_eq!(answer.json()["error"]["message"], message);
        }
    };
    let unknown = "Ce collègue n’existe pas ou son compte n’est pas actif.";
    refused(
        user::JULIEN,
        None,
        in_days(3),
        "Désignez un collègue : vous ne pouvez pas vous remplacer vous-même.",
    )
    .await;
    refused(Uuid::new_v4(), None, in_days(3), unknown).await;
    // Another company's account is unknown here, as an absent one.
    refused(OTHER_USER_ID, None, in_days(3), unknown).await;
    refused(
        user::MARC,
        None,
        in_days(-1),
        "La fin de l’absence doit être à venir.",
    )
    .await;
    refused(
        user::MARC,
        Some(in_days(5)),
        in_days(4),
        "La fin de l’absence doit suivre son début.",
    )
    .await;
    let too_long = "Une absence déclarée dure au plus 90 jours : déclarez la suite plus tard.";
    refused(user::MARC, None, in_days(91), too_long).await;
    let starts = Utc::now() + Duration::days(1);
    let beyond = (starts + Duration::days(90) + Duration::minutes(61)).to_rfc3339();
    refused(user::MARC, Some(starts.to_rfc3339()), beyond, too_long).await;
    refused(user::MARC, Some(in_days(10)), in_days(101), too_long).await;

    // A disabled colleague stands in for nobody.
    let account = format!("/admin/users/{}", user::LUCAS);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    refused(user::LUCAS, None, in_days(3), unknown).await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    // Nothing declared, nothing audited.
    assert_eq!(audited(&api, user::JULIEN).await.len(), before);
    assert_eq!(read(&julien).await, (None, vec![]));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_ended_absence_or_one_of_a_disabled_account_covers_nothing_and_ended_is_purged() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    let marc = api.sign_in(user::MARC).await;
    declare(&julien, user::MARC, 2).await;

    // Julien disabled: Marc covers nothing.
    let account = format!("/admin/users/{}", user::JULIEN);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(read(&marc).await, (None, vec![]));
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert_eq!(read(&marc).await, (None, vec![user::JULIEN]));

    // Ended: read as none at once, deleted by the maintenance.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update approval_delegations set starts_at = now() - interval '2 days', \
         ends_at = now() - interval '1 second' where user_id = $1",
    )
    .bind(user::JULIEN)
    .execute(&owner)
    .await
    .unwrap();
    let julien = api.sign_in(user::JULIEN).await;
    assert_eq!(read(&julien).await, (None, vec![]));
    assert_eq!(read(&marc).await, (None, vec![]));
    let pool = api.app.services.db.pool();
    assert_eq!(purge_ended_approval_delegations(pool).await.unwrap(), 1);
    let left: i64 = sqlx::query_scalar("select count(*) from approval_delegations")
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(left, 0);
    let gone = julien.delete(ABSENCE).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND, "{}", gone.text());
}

/// The names `client` is offered to stand in for them (ADR-073).
async fn candidates(client: &Client<'_>) -> Vec<String> {
    let read = client.get(ABSENCE).await.json();
    read["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["displayName"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn whom_one_may_name_is_shown_to_those_who_validate_only() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    // Active colleagues holding `approval:decide` over others, not oneself:
    // neither through an assignment ended, nor over oneself only.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for (user_id, scope, expires_at) in [
        (
            user::LUCAS,
            AssignmentScope::Tenant,
            Some(Utc::now() - Duration::days(1)),
        ),
        (user::HUGO, AssignmentScope::Own, None),
    ] {
        let assignment = NewRoleAssignment {
            tenant_id: tenant::ID,
            user_id,
            role_id: role::TEAM_LEAD,
            scope,
            granted_by_user_id: None,
            expires_at,
        };
        create_role_assignment(&mut tx, assignment).await.unwrap();
    }
    tx.commit().await.unwrap();
    assert_eq!(candidates(&marc).await, ["Julien Moreau", "Nadia Benali"]);
    // Whoever does not validate learns nothing of who does.
    assert!(candidates(&lucas).await.is_empty());

    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(candidates(&marc).await, ["Julien Moreau"]);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
}
