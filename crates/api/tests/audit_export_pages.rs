//! The pages a SIEM reads of its company's audit log (ADR-181), past the
//! token: a page asked amiss refused, a place past the start without its
//! seal among them; whether more follow said exactly; a place the chain no
//! longer counts, or whose seal is no longer the one the SIEM gives, refused
//! as a chain restored behind it, with the chain's last place; a token
//! removed while its read waits reading nothing; a server without a master
//! key exporting nothing, which only a valid token is told.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::audit_chain::{append, chain, seal};
use support::audit_exports::{TOKENS, keyed, made, read, removed, seal_at, waiting_on_locks};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_page_asked_amiss_or_past_the_chain_is_refused() {
    let api = keyed().await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "SIEM").await;
    for action in ["test.page.a", "test.page.b", "test.page.c"] {
        append(db, tenant::ID, action).await;
    }
    seal(db, &SealedHeads::default(), tenant::ID).await.unwrap();
    let head = chain(db, tenant::ID).await.last().unwrap().1;
    let some_seal = "ab".repeat(32);
    for query in [
        "?limit=0".to_string(),
        "?limit=1001".to_string(),
        "?limit=beaucoup".to_string(),
        "?after=-1".to_string(),
        "?depuis=3".to_string(),
        // A seal comes with the place it is at, as the export writes it, and
        // a place past the start with its seal.
        format!("?seal={some_seal}"),
        format!("?after={head}"),
        format!("?after={head}&seal={}", &some_seal[1..]),
        format!("?after={head}&seal={}", "zz".repeat(32)),
    ] {
        let refused = read(&api, Some(&secret), &query).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{query}");
    }
    let most = read(&api, Some(&secret), "?limit=1000").await;
    assert_eq!(most.status, StatusCode::OK, "{}", most.text());

    // Whether more follow, exactly: two left, two asked, none after.
    let page_after = |place: i64, seal: String| format!("?after={place}&seal={seal}&limit=2");
    let query = page_after(head - 2, seal_at(db, tenant::ID, head - 2).await);
    let last_two = read(&api, Some(&secret), &query).await;
    let last_two = last_two.json();
    assert_eq!(last_two["entries"].as_array().map(Vec::len), Some(2));
    assert_eq!(last_two["more"], false, "{last_two}");
    let query = page_after(head - 3, seal_at(db, tenant::ID, head - 3).await);
    let before = read(&api, Some(&secret), &query).await;
    assert_eq!(before.json()["more"], true);

    // The seal held at the last place, in either case, resumes the reading.
    let held = last_two["entries"][1]["seal"].as_str().unwrap().to_string();
    for given in [held.clone(), held.to_uppercase()] {
        let resumed = read(&api, Some(&secret), &format!("?after={head}&seal={given}")).await;
        assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());
    }
    // Another seal there, or a place past the chain: restored behind the SIEM.
    let changed = read(
        &api,
        Some(&secret),
        &format!("?after={head}&seal={some_seal}"),
    )
    .await;
    assert_eq!(changed.status, StatusCode::CONFLICT, "{}", changed.text());
    assert!(changed.text().contains("ne porte plus ce sceau"));
    let past = read(
        &api,
        Some(&secret),
        &format!("?after={}&seal={held}", head + 1000),
    )
    .await;
    assert_eq!(past.status, StatusCode::CONFLICT, "{}", past.text());
    assert!(past.text().contains("ne compte plus la place"));
    // Each refusal names the chain's last place, to resume from one shared.
    for refused in [changed.text(), past.text()] {
        assert!(
            refused.contains(&format!("{head} places scellées")),
            "{refused}"
        );
    }
    removed(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_removed_while_its_read_waits_reads_nothing() {
    let api = keyed().await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "SIEM").await;
    // An administrator's removal, not yet committed when the read finds the
    // token: the read waits for it, and sees it.
    let mut removal = db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from audit_export_tokens where id = $1::uuid")
        .bind(&id)
        .execute(&mut *removal)
        .await
        .unwrap();
    let (waited, ()) = tokio::join!(read(&api, Some(&secret), ""), async {
        waiting_on_locks(db, 1).await;
        removal.commit().await.unwrap();
    });
    assert_eq!(waited.status, StatusCode::UNAUTHORIZED, "{}", waited.text());
    assert!(!paul.get(TOKENS).await.text().contains(&id));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_without_a_master_key_exports_nothing() {
    // Tokens are made where nothing is sealed, but read nothing there.
    let unsealed = TestApi::start().await;
    let paul = unsealed.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "SIEM").await;
    assert_eq!(paul.get(TOKENS).await.json()["sealing"], false);
    let refused = read(&unsealed, Some(&secret), "").await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    let said = refused.text();
    assert!(said.contains("MIMORYA_MASTER_KEY"), "{said}");
    // An unknown token is refused first.
    let unknown = read(&unsealed, Some("mae_inconnu"), "").await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    removed(&paul, &id).await;
}
