//! What the indexer gives vectors to (ADR-074, ADR-096): a company's active
//! memories, then its documents' passages; for each, the texts awaiting the
//! model the company chose, and where their vectors are stored.

use mimorya_db::repositories::document_embeddings::{passages_to_embed, store_passage_embedding};
use mimorya_db::repositories::memory_embeddings::{memories_to_embed, store_embedding};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Memories,
    Passages,
}

/// In the order a pass indexes them: the memory a turn recalls first.
pub(super) const SOURCES: [Source; 2] = [Source::Memories, Source::Passages];

/// A text awaiting its vector.
pub(super) struct Pending {
    pub id: Uuid,
    pub text: String,
}

impl Source {
    /// At most `limit` texts of the source not yet given to `model`.
    pub(super) async fn pending(
        self,
        conn: &mut PgConnection,
        model: &str,
        limit: i64,
    ) -> sqlx::Result<Vec<Pending>> {
        Ok(match self {
            Self::Memories => memories_to_embed(conn, model, limit)
                .await?
                .into_iter()
                .map(|memory| Pending {
                    id: memory.id,
                    text: format!("{}\n\n{}", memory.title, memory.content),
                })
                .collect(),
            // A passage with its document's title and its section's.
            Self::Passages => passages_to_embed(conn, model, limit)
                .await?
                .into_iter()
                .map(|passage| Pending {
                    id: passage.id,
                    text: match passage.heading {
                        Some(heading) => {
                            format!("{}\n\n{heading}\n\n{}", passage.title, passage.content)
                        }
                        None => format!("{}\n\n{}", passage.title, passage.content),
                    },
                })
                .collect(),
        })
    }

    /// Stores the vector `model` gave the text `id`, `None` when refused.
    pub(super) async fn store(
        self,
        conn: &mut PgConnection,
        id: Uuid,
        vector: Option<&[f32]>,
        model: &str,
    ) -> sqlx::Result<()> {
        match self {
            Self::Memories => store_embedding(conn, id, vector, model).await,
            Self::Passages => store_passage_embedding(conn, id, vector, model).await,
        }
    }
}

/// What a pass gave a vector to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Indexed {
    pub memories: usize,
    pub passages: usize,
}

impl Indexed {
    pub(super) fn add(&mut self, source: Source, count: usize) {
        match source {
            Source::Memories => self.memories += count,
            Source::Passages => self.passages += count,
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.memories += other.memories;
        self.passages += other.passages;
    }

    pub fn total(self) -> usize {
        self.memories + self.passages
    }
}
