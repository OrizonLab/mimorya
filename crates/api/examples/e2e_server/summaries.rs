//! A long conversation whose earlier messages are summarised (ADR-145), for
//! the end-to-end tests to read it so (`conversation-summary.spec.ts`):
//! Marc's nine exchanges, the first six folded into a summary, as a pass at
//! the end of a run would have left them. Written at start, so that the
//! scripted model plays no pass.

use mimorya_api::contract::MessageMetadata;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::conversation_summaries::{FoldedSummary, store_summary};
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};

/// The conversation's title, which the test opens.
const TITLE: &str = "Suivi du chantier Lumière";
/// Exchanges written, and how many of them the summary covers.
const EXCHANGES: usize = 9;
const FOLDED: usize = 6;
/// The summary, which the test reads.
const SUMMARY: &str = "Marc suit le chantier Lumière : INT-2049 reste en cours, \
la mise en service est prévue lundi avec Léa.";

/// Leaves Marc's summarised conversation.
pub async fn leave(app: &mimorya_api::app::App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    let conversation = create_conversation(&mut tx, tenant::ID, user::MARC, Some(TITLE)).await?;
    let mut through = None;
    for n in 1..=EXCHANGES {
        for (role, text, author) in [
            ("user", "Question", Some(user::MARC)),
            ("assistant", "Réponse", None),
        ] {
            let message = add_message(
                &mut tx,
                NewMessage {
                    tenant_id: tenant::ID,
                    conversation_id: conversation.id,
                    role,
                    content: format!("{text} {n} sur le chantier Lumière"),
                    author_user_id: author,
                    run_id: None,
                    metadata: serde_json::to_value(MessageMetadata::default())?,
                },
            )
            .await?;
            if n == FOLDED && role == "assistant" {
                through = Some(message.id);
            }
        }
    }
    let through = through.ok_or_else(|| anyhow::anyhow!("no message folded"))?;
    let summary = FoldedSummary {
        tenant_id: tenant::ID,
        conversation_id: conversation.id,
        content: SUMMARY.into(),
        through_message_id: through,
        folded: i32::try_from(2 * FOLDED)?,
    };
    anyhow::ensure!(
        store_summary(&mut tx, summary, None).await?,
        "summary not stored"
    );
    tx.commit().await?;
    Ok(())
}
