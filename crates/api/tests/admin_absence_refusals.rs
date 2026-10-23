//! Who may not declare a collaborator's absence (ADR-078), and which absence
//! cannot hold: only `user:manage` on the whole company, never on one's own
//! account nor naming oneself to stand in, never for another company's or an
//! inactive account. Nothing is recorded nor told; refusals of rights,
//! foreign or unknown accounts are audited (ADR-038, ADR-047, ADR-054).

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_db::repositories::role_assignments::{
    NewRoleAssignment, create_role_assignment, delete_role_assignment,
};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::delegations::{ABSENCE, a_week, absence_changes, absence_of};
use support::{Client, TestApi};
use uuid::Uuid;

/// How `client`'s `method` on the absence of `absent` is answered.
async fn answer(client: &Client<'_>, method: &str, absent: Uuid) -> (StatusCode, Value) {
    let path = absence_of(absent);
    let response = match method {
        "GET" => client.get(&path).await,
        "PUT" => client.put(&path, a_week(user::NADIA)).await,
        _ => client.delete(&path).await,
    };
    let message = response.json()["error"]["message"].clone();
    (response.status, message)
}

/// The colleagues offered to stand in, as `path` answers `client`.
async fn named(client: &Client<'_>, path: &str) -> String {
    client.get(path).await.json()["candidates"].to_string()
}

/// The refusals the administration audited on this route: action, by whom.
async fn refusals(api: &TestApi) -> Vec<(String, Option<Uuid>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, actor_user_id from audit_log where action like 'admin.%' \
         and metadata->>'route' = '/admin/users/{id}/absence' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_account_administrators_declare_another_s_absence() {
    // Without a relay, the team administrator's invitation link is shown.
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut team_admin = create_partial_admin(&api, &paul, "absence.partielle").await;
    let team_admin_id = team_admin.id;
    let team_admin = activate(&api, &mut team_admin).await;
    let company_wide = json!("Permission requise sur toute l’entreprise : user:manage");
    for method in ["GET", "PUT", "DELETE"] {
        // A manager without `user:manage` neither reads nor declares Lucas's.
        assert_eq!(
            answer(&marc, method, user::LUCAS).await.0,
            StatusCode::FORBIDDEN
        );
        // Nor an administrator of his team only.
        let refused = answer(&team_admin, method, user::LUCAS).await;
        assert_eq!(refused, (StatusCode::FORBIDDEN, company_wide.clone()));
        // Nor the administrator his own: it is declared in « Validations ».
        assert_eq!(
            answer(&paul, method, user::PAUL).await.0,
            StatusCode::FORBIDDEN
        );
        // Another company's account is as unknown as an absent one.
        let foreign = answer(&paul, method, OTHER_USER_ID).await;
        assert_eq!(foreign.0, StatusCode::NOT_FOUND);
        assert_eq!(answer(&paul, method, Uuid::new_v4()).await, foreign);
    }

    // Nothing recorded; the refused changes audited.
    let lucas = api.sign_in(user::LUCAS).await;
    assert_eq!(lucas.get(ABSENCE).await.json()["own"], Value::Null);
    assert_eq!(paul.get(ABSENCE).await.json()["own"], Value::Null);
    assert!(absence_changes(&api, user::LUCAS).await.is_empty());
    let audited = refusals(&api).await;
    for actor in [user::MARC, team_admin_id, user::PAUL] {
        assert!(
            audited.contains(&("admin.refused".into(), Some(actor))),
            "{audited:?}"
        );
    }
    assert!(audited.contains(&("admin.not_found".into(), Some(user::PAUL))));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_absence_that_cannot_hold_is_refused() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let mails = api.mail.sent().len();
    let refused = |delegate: Uuid, ends_in: i64, expected: StatusCode, message: &'static str| {
        let paul = &paul;
        async move {
            let ends = (Utc::now() + Duration::days(ends_in)).to_rfc3339();
            let body = json!({ "delegateId": delegate, "endsAt": ends });
            let answer = paul.put(&absence_of(user::JULIEN), body).await;
            assert_eq!(answer.status, expected, "{}", answer.text());
            assert_eq!(answer.json()["error"]["message"], message);
        }
    };
    let invalid = StatusCode::BAD_REQUEST;
    let unknown = "Ce collègue n’existe pas ou son compte n’est pas actif.";
    let absent = "Désignez comme remplaçant un autre collaborateur que l’absent.";
    refused(user::JULIEN, 3, invalid, absent).await;
    refused(OTHER_USER_ID, 3, invalid, unknown).await;
    refused(Uuid::new_v4(), 3, invalid, unknown).await;
    let past = "La fin de l’absence doit être à venir.";
    refused(user::MARC, -1, invalid, past).await;
    let too_long = "Une absence déclarée dure au plus 90 jours : déclarez la suite plus tard.";
    refused(user::MARC, 91, invalid, too_long).await;
    // Standing in, the administrator would take over the notices of another:
    // never himself, nor offered to.
    let himself = "Vous ne pouvez pas vous désigner vous-même comme remplaçant depuis \
                   l’administration";
    refused(user::PAUL, 3, StatusCode::FORBIDDEN, himself).await;
    // Paul validating too, Marc may name him, not Paul himself.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let lead = NewRoleAssignment {
        tenant_id: tenant::ID,
        user_id: user::PAUL,
        role_id: role::TEAM_LEAD,
        scope: AssignmentScope::subtree(unit::TEAM_SOUTH),
        granted_by_user_id: None,
        expires_at: None,
    };
    let lead = create_role_assignment(&mut tx, lead).await.unwrap();
    tx.commit().await.unwrap();
    let marc = api.sign_in(user::MARC).await;
    let paul_id = user::PAUL.to_string();
    assert!(named(&marc, ABSENCE).await.contains(&paul_id));
    assert!(
        !named(&paul, &absence_of(user::JULIEN))
            .await
            .contains(&paul_id)
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    assert!(delete_role_assignment(&mut tx, lead).await.unwrap());
    tx.commit().await.unwrap();

    // A disabled account: an absence would change nothing.
    let account = format!("/admin/users/{}", user::JULIEN);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let inactive = "Ce compte n’est pas actif : une absence serait sans effet.";
    refused(user::MARC, 3, StatusCode::CONFLICT, inactive).await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    // Nothing recorded nor told. Audited as refusals: the administrator
    // himself, and a delegate who designates nothing the company has; not
    // the invalid requests (ADR-038, ADR-054).
    let julien = api.sign_in(user::JULIEN).await;
    let own = julien.get(ABSENCE).await;
    assert_eq!(own.status, StatusCode::OK, "{}", own.text());
    assert_eq!(own.json()["own"], Value::Null);
    assert_eq!(api.mail.sent().len(), mails);
    assert!(absence_changes(&api, user::JULIEN).await.is_empty());
    let audited = refusals(&api).await;
    assert!(audited.contains(&("admin.refused".into(), Some(user::PAUL))));
    assert!(audited.contains(&("admin.unknown_reference".into(), Some(user::PAUL))));
}
