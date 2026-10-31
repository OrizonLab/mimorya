//! Long conversations whose earlier messages are summarised (ADR-145), for
//! the end-to-end tests to read them so (`conversation-summary.spec.ts`),
//! as passes at the end of runs would have left them: Marc's nine exchanges
//! on the Lumière site, the first six summarised and all shown above the
//! note; and two of 105 exchanges, more than a conversation shows (200
//! messages), whose summary stops before the messages shown (Horizon) or
//! among them, the first ones it covers no longer shown (Azur); both lag
//! behind, more messages after them than are reread (ADR-149). Written at
//! start, so that the scripted model plays no pass.

use mimorya_api::contract::MessageMetadata;
use mimorya_core::demo::{tenant, user};
use mimorya_db::TenantTx;
use mimorya_db::repositories::conversation_summaries::{FoldedSummary, store_summary};
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};

/// A conversation as the test reads it: its site, how many exchanges, how
/// many messages its summary covers, and the summary.
struct Summarised {
    site: &'static str,
    exchanges: usize,
    folded: usize,
    summary: &'static str,
}

const CONVERSATIONS: [Summarised; 3] = [
    Summarised {
        site: "Lumière",
        exchanges: 9,
        folded: 12,
        summary: "Marc suit le chantier Lumière : INT-2049 reste en cours, \
                  la mise en service est prévue lundi avec Léa.",
    },
    Summarised {
        site: "Horizon",
        exchanges: 105,
        folded: 8,
        summary: "Marc prépare la réception du chantier Horizon : réserves levées, \
                  procès-verbal à signer vendredi.",
    },
    Summarised {
        site: "Azur",
        exchanges: 105,
        folded: 20,
        summary: "Marc suit les retards du chantier Azur : livraison des panneaux \
                  décalée, Léa replanifie la pose.",
    },
];

/// Leaves Marc's summarised conversations.
pub async fn leave(app: &mimorya_api::app::App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    for conversation in &CONVERSATIONS {
        summarised(&mut tx, conversation).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// « Suivi du chantier … », its « Question n » and « Réponse n », and its
/// summary up to the message `folded`.
async fn summarised(tx: &mut TenantTx, written: &Summarised) -> anyhow::Result<()> {
    let title = format!("Suivi du chantier {}", written.site);
    let conversation = create_conversation(tx, tenant::ID, user::MARC, Some(&title)).await?;
    let mut through = None;
    let mut count = 0;
    for n in 1..=written.exchanges {
        for (role, text, author) in [
            ("user", "Question", Some(user::MARC)),
            ("assistant", "Réponse", None),
        ] {
            let message = add_message(
                tx,
                NewMessage {
                    tenant_id: tenant::ID,
                    conversation_id: conversation.id,
                    role,
                    content: format!("{text} {n} sur le chantier {}", written.site),
                    author_user_id: author,
                    run_id: None,
                    metadata: serde_json::to_value(MessageMetadata::default())?,
                },
            )
            .await?;
            count += 1;
            if count == written.folded {
                through = Some(message.id);
            }
        }
    }
    let through = through.ok_or_else(|| anyhow::anyhow!("no message folded"))?;
    let summary = FoldedSummary {
        tenant_id: tenant::ID,
        conversation_id: conversation.id,
        content: written.summary.into(),
        through_message_id: through,
        folded: i32::try_from(written.folded)?,
    };
    anyhow::ensure!(
        store_summary(tx, summary, None).await?,
        "summary not stored"
    );
    Ok(())
}
