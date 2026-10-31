//! A conversation's summary (ADR-145): stored once from none, then moved
//! forward only from the summary it was folded from, so that of two passes
//! from the same summary the second changes nothing; the messages it does
//! not cover read in their order, the latest or the earliest.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::conversation_summaries::{
    FoldedSummary, conversation_summary, earliest_messages_after, latest_messages_after,
    store_summary,
};
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};
use serde_json::json;
use uuid::Uuid;

fn folded(conversation_id: Uuid, content: &str, through: Uuid, folded: i32) -> FoldedSummary {
    FoldedSummary {
        tenant_id: tenant::ID,
        conversation_id,
        content: content.into(),
        through_message_id: through,
        folded,
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_summary_moves_forward_only_from_the_one_it_was_folded_from() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.app).await;
    // Rolled back when dropped: nothing is left behind.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let conversation = create_conversation(&mut tx, tenant::ID, user::MARC, None)
        .await
        .unwrap()
        .id;
    let mut ids = Vec::new();
    for n in 1..=8 {
        let message = NewMessage {
            tenant_id: tenant::ID,
            conversation_id: conversation,
            role: if n % 2 == 1 { "user" } else { "assistant" },
            content: format!("Message {n}"),
            author_user_id: None,
            run_id: None,
            metadata: json!({}),
        };
        ids.push(add_message(&mut tx, message).await.unwrap().id);
    }

    // The messages after none, or after one, in their order.
    let texts = |rows: Vec<mimorya_db::repositories::conversations::MessageRow>| {
        rows.into_iter().map(|m| m.content).collect::<Vec<_>>()
    };
    let earliest = earliest_messages_after(&mut tx, conversation, None, 3)
        .await
        .unwrap();
    assert_eq!(texts(earliest), ["Message 1", "Message 2", "Message 3"]);
    let latest = latest_messages_after(&mut tx, conversation, Some(ids[3]), 3)
        .await
        .unwrap();
    assert_eq!(texts(latest), ["Message 6", "Message 7", "Message 8"]);
    let after = earliest_messages_after(&mut tx, conversation, Some(ids[3]), 10)
        .await
        .unwrap();
    assert_eq!(
        texts(after),
        ["Message 5", "Message 6", "Message 7", "Message 8"]
    );

    // Two passes from none: the first is stored, the second changes nothing.
    let first = folded(conversation, "Premier résumé", ids[1], 2);
    assert!(store_summary(&mut tx, first, None).await.unwrap());
    let late = folded(conversation, "Résumé tardif", ids[3], 4);
    assert!(!store_summary(&mut tx, late, None).await.unwrap());
    let held = conversation_summary(&mut tx, conversation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.content, "Premier résumé");
    assert_eq!((held.through_message_id, held.folded_messages), (ids[1], 2));

    // Two passes from that summary: the first moves it forward, the second,
    // folded from it too, neither moves it back nor counts twice.
    let next = folded(conversation, "Deuxième résumé", ids[5], 4);
    assert!(store_summary(&mut tx, next, Some(ids[1])).await.unwrap());
    let stale = folded(conversation, "Résumé dépassé", ids[3], 2);
    assert!(!store_summary(&mut tx, stale, Some(ids[1])).await.unwrap());
    let held = conversation_summary(&mut tx, conversation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.content, "Deuxième résumé");
    assert_eq!((held.through_message_id, held.folded_messages), (ids[5], 6));
}
