//! An account erased (ADR-189). Lucas has something of every kind the
//! platform keeps on a person, and decided a validation of Marc's. Paul
//! disables him, then erases him, typing his address again: nothing in any
//! table names Lucas any more, his session is gone, and the counts given
//! are those that went. Marc's data is as it was, his validation kept
//! without who decided it, the company's retention duration he chose
//! without who chose it. The audit log keeps every entry naming Lucas,
//! unchanged: sealed before and after, the chain verifies intact. The
//! erasure itself is traced by its counts alone.

mod support;

use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::retention_durations::set_retention_duration;
use serde_json::{Value, json};
use support::audit_chain::{seal, verify};
use support::audit_exports::audit_log;
use support::erasure::{entries_naming, rows_naming};
use support::personal_belongings::{belongings, company_parts, decided_for};
use support::queue::API;
use support::retention::{clear_retention, owner};
use support::{TestApi, email};
use uuid::Uuid;

/// What the platform keeps of `person` that an erasure takes, counted as
/// the erasure counts it.
async fn kept_of(person: Uuid) -> Value {
    let owner = owner().await;
    let (conversations, messages, memories, runs): (i64, i64, i64, i64) = sqlx::query_as(
        "select \
         (select count(*) from conversations where owner_user_id = $1), \
         (select count(*) from messages m join conversations c on c.id = m.conversation_id \
           where c.owner_user_id = $1), \
         (select count(*) from memory_items where scope = 'user' and owner_user_id = $1), \
         (select count(*) from agent_runs where on_behalf_of_user_id = $1)",
    )
    .bind(person)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    json!({ "conversations": conversations, "messages": messages,
            "memories": memories, "runs": runs })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_erases_a_disabled_account_and_all_it_held() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let neo = company_parts(tenant::ID, "neo", 1001).await;
    belongings(&neo, user::LUCAS, "de-lucas", None, 31, 10).await;
    let marcs = belongings(&neo, user::MARC, "de-marc", None, 11, 20).await;
    decided_for(tenant::ID, marcs.run, user::MARC, user::LUCAS, "par-lucas").await;
    // A retention duration he chose for the company (ADR-195).
    let mut tx = db.begin(tenant::ID).await.unwrap();
    set_retention_duration(&mut tx, tenant::ID, "episodes", 6, user::LUCAS)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::LUCAS),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status.as_u16(), 200, "{}", disabled.text());
    let named = entries_naming(user::LUCAS).await;
    assert!(!named.is_empty());
    assert!(seal(db, &heads, tenant::ID).await.unwrap() > 0);
    let marc_before = rows_naming(user::MARC).await;
    let expected = kept_of(user::LUCAS).await;
    assert!(expected["conversations"].as_i64() >= Some(1), "{expected}");

    // His address typed again, in another case and with spaces.
    let typed = format!("  {}  ", email(user::LUCAS).to_uppercase());
    let erased = paul
        .post(
            &format!("/admin/users/{}/erasure", user::LUCAS),
            json!({ "confirmEmail": typed }),
        )
        .await;
    assert_eq!(erased.status.as_u16(), 200, "{}", erased.text());
    assert_eq!(erased.json(), expected);

    // Nothing names Lucas any more; his session is gone.
    let left: Vec<_> = (rows_naming(user::LUCAS).await.into_iter())
        .filter(|(_, count)| *count > 0)
        .collect();
    assert!(left.is_empty(), "rows still naming Lucas: {left:?}");
    assert_eq!(lucas.get("/auth/me").await.status.as_u16(), 401);
    let directory = paul.get("/admin/users").await.json();
    assert!(
        !(directory["users"].as_array().unwrap().iter()).any(|u| u["id"] == json!(user::LUCAS))
    );

    // Marc's data as it was, his validation without who decided it.
    assert_eq!(rows_naming(user::MARC).await, marc_before);
    let owner = owner().await;
    let decided: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "select status, decided_by_user_id from approvals \
         where run_id = $1 and tool_name = 'outil-décidé'",
    )
    .bind(marcs.run)
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(decided, [("rejected".to_owned(), None)]);
    // The company's duration stays as it was, without who chose it.
    let chosen: Vec<(String, i32, Option<Uuid>)> = sqlx::query_as(
        "select kind, duration, updated_by_user_id from tenant_retention_durations \
         where tenant_id = $1",
    )
    .bind(tenant::ID)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    assert_eq!(chosen, [("episodes".to_owned(), 6, None)]);
    clear_retention(tenant::ID).await;

    // The log keeps what named him, unchanged, and traces the erasure.
    let still: Vec<i64> = entries_naming(user::LUCAS).await;
    assert!(
        named.iter().all(|id| still.contains(id)),
        "{named:?} {still:?}"
    );
    let traced: Vec<_> = (audit_log(&paul).await.into_iter())
        .filter(|entry| entry["action"] == "user.erased")
        .collect();
    assert_eq!(traced.len(), 1, "{traced:?}");
    assert_eq!(traced[0]["actorUserId"], json!(user::PAUL));
    assert_eq!(traced[0]["resourceId"], json!(user::LUCAS.to_string()));
    assert_eq!(traced[0]["metadata"], expected);
    assert!(seal(db, &heads, tenant::ID).await.unwrap() > 0);
    let report = verify(db, tenant::ID).await;
    assert_eq!(report.broken, 0, "{report:?}");
    assert!(report.verified > named.len() as i64, "{report:?}");
}
