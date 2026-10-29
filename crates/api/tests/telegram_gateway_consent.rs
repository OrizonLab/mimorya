//! No one's Telegram account is linked to another's account unknowingly
//! (ADR-133): whoever opens a link is told whose account it links, and
//! only accepting it in Telegram proposes the link, the last one the bot
//! named and no earlier one; an account linked to
//! someone is never taken from them by another's code, which only they
//! remove. A code is its own company's bot's only, and every company's
//! expired codes are forgotten.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::testing::{FakeTelegram, OTHER_BOT, TELEGRAM_BOT, update};
use mimorya_core::demo::{tenant, user};
use mimorya_core::gateways::telegram::link_code_hash;
use mimorya_db::repositories::gateway_link_codes::purge_link_codes;
use serde_json::Value;
use support::TestApi;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{
    LEA_TELEGRAM, LEA_USERNAME, MARC_TELEGRAM, MARC_USERNAME, ask_code, confirm, declare_telegram,
    of_person, open_and_accept, other_company_bot, post_update, write,
};
use uuid::Uuid;

const LEA: (i64, &str) = (LEA_TELEGRAM, LEA_USERNAME);

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_one_s_telegram_account_is_linked_to_another_s_account_unknowingly() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let marc = api.sign_in(user::MARC).await;
    let id = declare_telegram(&paul).await;

    // Marc has Léa open his link: she is told it links Marc's account, and
    // ignores it. Marc has nothing to confirm.
    let code = ask_code(&marc, id).await;
    let offered = write(&api, &telegram, id, 1, LEA, &format!("/start {code}")).await;
    assert!(offered.contains("au compte de Marc Lefèvre"), "{offered}");
    assert_eq!(
        of_person(&marc, id).await["pending"]["claimedBy"],
        Value::Null
    );
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);

    // She links her own, which Marc's offered code does not take.
    let hers = ask_code(&lea, id).await;
    open_and_accept(&api, &telegram, id, 2, LEA, &hers).await;
    assert_eq!(confirm(&lea, id).await.status, StatusCode::OK);
    let told = write(&api, &telegram, id, 4, LEA, "/oui").await;
    assert!(told.starts_with("Aucune liaison"), "{told}");
    // Linked to her, Marc's next link is refused to her.
    let code = ask_code(&marc, id).await;
    let refused = write(&api, &telegram, id, 5, LEA, &format!("/start {code}")).await;
    assert!(refused.contains("déjà relié"), "{refused}");
    assert!(!refused.contains("Marc"), "{refused}");
    let told = write(&api, &telegram, id, 6, LEA, "/oui").await;
    assert!(told.starts_with("Aucune liaison"), "{told}");
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        of_person(&lea, id).await["linked"]["externalId"],
        LEA_TELEGRAM.to_string()
    );
    assert_eq!(of_person(&marc, id).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_consent_accepts_the_last_link_named_only() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let marc = api.sign_in(user::MARC).await;
    let id = declare_telegram(&paul).await;
    let start = |code: &str| format!("/start {code}");

    // Marc's link, ignored; then her own, accepted, whose account named.
    write(
        &api,
        &telegram,
        id,
        1,
        LEA,
        &start(&ask_code(&marc, id).await),
    )
    .await;
    let hers = ask_code(&lea, id).await;
    let told = open_and_accept(&api, &telegram, id, 2, LEA, &hers).await;
    assert!(told.contains("au compte de Léa Fontaine"), "{told}");
    // Accepting again accepts nothing: not Marc's link she ignored.
    let again = write(&api, &telegram, id, 4, LEA, "/oui").await;
    assert!(again.starts_with("Aucune liaison"), "{again}");
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);

    // Marc's link, then hers, which someone else opens after her: her
    // consent accepts nothing, Marc's least of all.
    write(
        &api,
        &telegram,
        id,
        5,
        LEA,
        &start(&ask_code(&marc, id).await),
    )
    .await;
    let hers = ask_code(&lea, id).await;
    write(&api, &telegram, id, 6, LEA, &start(&hers)).await;
    let marc_telegram = (MARC_TELEGRAM, MARC_USERNAME);
    write(&api, &telegram, id, 7, marc_telegram, &start(&hers)).await;
    let told = write(&api, &telegram, id, 8, LEA, "/oui").await;
    assert!(told.starts_with("Aucune liaison"), "{told}");
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(confirm(&lea, id).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_accepted_never_takes_someone_else_s() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let marc = api.sign_in(user::MARC).await;
    let id = declare_telegram(&paul).await;

    // Léa accepts Marc's link knowingly, then links her own first.
    let code = ask_code(&marc, id).await;
    open_and_accept(&api, &telegram, id, 1, LEA, &code).await;
    let hers = ask_code(&lea, id).await;
    open_and_accept(&api, &telegram, id, 3, LEA, &hers).await;
    assert_eq!(confirm(&lea, id).await.status, StatusCode::OK);
    // Marc's confirmation then takes nothing from her.
    let refused = confirm(&marc, id).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert_eq!(
        of_person(&lea, id).await["linked"]["externalId"],
        LEA_TELEGRAM.to_string()
    );
    assert_eq!(of_person(&marc, id).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_code_and_a_secret_are_their_own_company_s_bot_s_only() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let id = declare_telegram(&paul).await;
    let (_, theirs, _) = other_company_bot(&api, "acme-telegram-consent").await;
    let (_, ours) = telegram.webhook(TELEGRAM_BOT).unwrap();
    let (_, acme) = telegram.webhook(OTHER_BOT).unwrap();
    let to = |gateway: Uuid| format!("/gateways/telegram/{gateway}");

    // Each bot's address answers its own secret only.
    let body = update(1, LEA_TELEGRAM, LEA_USERNAME, "Bonjour");
    for (gateway, secret) in [(theirs, &ours), (id, &acme)] {
        let refused = post_update(&api, &to(gateway), Some(secret), &body).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND);
    }
    // Léa's code, opened with Acme's bot, links nothing there.
    let code = ask_code(&lea, id).await;
    let start = update(2, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    let taken = post_update(&api, &to(theirs), Some(&acme), &start).await;
    assert_eq!(taken.status, StatusCode::OK);
    api.idle().await;
    let told = telegram.messages().pop().unwrap();
    assert_eq!((told.bot, told.chat), (OTHER_BOT, LEA_TELEGRAM));
    assert!(told.text.contains("n’est plus valable"), "{}", told.text);
    // Still hers to open with her own company's bot.
    let marc = (MARC_TELEGRAM, MARC_USERNAME);
    let offered = write(&api, &telegram, id, 3, marc, &format!("/start {code}")).await;
    assert!(offered.contains("au compte de Léa Fontaine"), "{offered}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn expired_codes_are_forgotten_in_every_company() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let own = declare_telegram(&paul).await;
    let (acme, theirs, jeanne) = other_company_bot(&api, "acme-telegram-purge").await;
    let now = Utc::now();
    let past = now - Duration::minutes(1);
    let later = now + Duration::minutes(10);
    // Expired, one of them accepted; and still valid, in both companies.
    put_code(&api, (tenant::ID, own, user::LEA), "neo-ancien", past, true).await;
    put_code(
        &api,
        (tenant::ID, own, user::MARC),
        "neo-recent",
        later,
        false,
    )
    .await;
    let jeanne = jeanne.subject.user_id;
    put_code(&api, (acme, theirs, jeanne), "acme-ancien", past, false).await;

    let pool = api.app.services.db.pool();
    assert_eq!(purge_link_codes(pool, now).await.unwrap(), 2);
    assert_eq!(kept(&api, tenant::ID).await, [user::MARC]);
    assert_eq!(kept(&api, acme).await, Vec::<Uuid>::new());
    // Nothing more a second time.
    assert_eq!(purge_link_codes(pool, now).await.unwrap(), 0);
}

/// Keeps a code for the person `user_id` on the gateway, valid until
/// `expires_at`, accepted by Léa's Telegram account or not.
async fn put_code(
    api: &TestApi,
    (tenant_id, gateway_id, user_id): (Uuid, Uuid, Uuid),
    code: &str,
    expires_at: DateTime<Utc>,
    accepted: bool,
) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let telegram_user = accepted.then(|| LEA_TELEGRAM.to_string());
    sqlx::query(
        "insert into gateway_link_codes (tenant_id, gateway_id, user_id, code_hash, expires_at, \
         offered_external_id, offered_at, claimed_external_id, claimed_name, claimed_at) \
         values ($1, $2, $3, $4, $5, $6, case when $6 is null then null else now() end, \
         $6, case when $6 is null then null else '@lea_font' end, \
         case when $6 is null then null else now() end)",
    )
    .bind(tenant_id)
    .bind(gateway_id)
    .bind(user_id)
    .bind(link_code_hash(code))
    .bind(expires_at)
    .bind(telegram_user)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Whose codes the company keeps.
async fn kept(api: &TestApi, tenant_id: Uuid) -> Vec<Uuid> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let kept = sqlx::query_scalar("select user_id from gateway_link_codes order by user_id")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    kept
}
