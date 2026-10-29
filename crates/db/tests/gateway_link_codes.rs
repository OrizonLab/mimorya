//! The links offered to a Telegram user (ADR-133): one awaits their consent
//! per bot, the last one opened, even when two are opened at once; the
//! bot's offers wait their turn instead of failing on one another.

mod support;

use std::time::Duration;

use chrono::{Duration as Lifetime, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::gateway_link_codes::{offer_link_code, put_link_code};
use mimorya_db::repositories::gateways::{NewGateway, create_gateway};
use uuid::Uuid;

/// The Telegram user who opens both links.
const TELEGRAM_USER: &str = "4242";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_links_opened_at_once_leave_the_last_one_offered() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let gateway = bot(&db).await;
    let now = Utc::now();
    let (lea_code, marc_code) = (format!("{:064x}", 1), format!("{:064x}", 2));
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let expires_at = now + Lifetime::minutes(15);
    for (person, code) in [(user::LEA, &lea_code), (user::MARC, &marc_code)] {
        put_link_code(&mut tx, (tenant::ID, gateway, person), code, expires_at)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();

    // Léa's link opened, not yet committed, when Marc's is opened too.
    let mut first = db.begin(tenant::ID).await.unwrap();
    let offered = offer_link_code(&mut first, gateway, &lea_code, now, TELEGRAM_USER).await;
    assert_eq!(offered.unwrap(), Some(user::LEA));
    let second = tokio::spawn({
        let db = db.clone();
        async move {
            let mut second = db.begin(tenant::ID).await.unwrap();
            let offered =
                offer_link_code(&mut second, gateway, &marc_code, now, TELEGRAM_USER).await;
            second.commit().await.unwrap();
            offered
        }
    });
    // Marc's offer queued behind Léa's before hers is committed.
    wait_for_a_waiter(&db).await;
    first.commit().await.unwrap();
    // Marc's waited its turn, and is the one offered now.
    assert_eq!(second.await.unwrap().unwrap(), Some(user::MARC));
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let offered: Vec<Uuid> =
        sqlx::query_scalar("select user_id from gateway_link_codes where offered_external_id = $1")
            .bind(TELEGRAM_USER)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(offered, [user::MARC]);
}

/// Waits until a connection to this test database waits for a lock.
async fn wait_for_a_waiter(db: &Database) {
    for _ in 0..600 {
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks l join pg_stat_activity a on a.pid = l.pid \
             where not l.granted and a.datname = current_database()",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the second offer never waited for the first");
}

/// The demo company's Telegram bot.
async fn bot(db: &Database) -> Uuid {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let id = create_gateway(
        &mut tx,
        NewGateway {
            tenant_id: tenant::ID,
            kind: "telegram",
            name: "Telegram Néo",
            enabled: true,
            workspace_id: "123456789",
            workspace_name: "NeoMaintenanceBot",
            bot_user_id: "123456789",
            granted_scopes: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}
