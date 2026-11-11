//! The purges that go a batch at a time (ADR-186, ADR-197): a statement
//! after another, while one comes back full and the pass has statements
//! left, each company counted once with what it lost in all. A batch that
//! comes back short ends it, a backlog left to the next pass; so does a
//! statement that fails, logged, what the batches before deleted kept.

use std::future::Future;

use mimorya_db::repositories::retention::{PurgedConversations, PurgedUsage};
use uuid::Uuid;

/// What a company lost to a purge that goes a batch at a time.
pub(crate) trait Lost {
    fn tenant_id(&self) -> Uuid;
    /// What the batch size counts.
    fn count(&self) -> i32;
    /// Adds what the same company lost to another statement.
    fn add(&mut self, other: &Self);
}

impl Lost for PurgedConversations {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn count(&self) -> i32 {
        self.conversations
    }

    fn add(&mut self, other: &Self) {
        self.conversations += other.conversations;
        self.messages += other.messages;
    }
}

impl Lost for PurgedUsage {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn count(&self) -> i32 {
        self.records
    }

    fn add(&mut self, other: &Self) {
        self.records += other.records;
    }
}

/// What `purge` deleted a batch of `size` at a time, while a batch comes
/// back full and the pass has `batches` statements left: each company
/// once, what it lost in all. A failed statement ends it, logged as `what`.
pub(crate) async fn in_batches<T, F, Fut>(
    what: &str,
    batches: usize,
    size: i32,
    mut purge: F,
) -> Vec<T>
where
    T: Lost,
    F: FnMut() -> Fut,
    Fut: Future<Output = sqlx::Result<Vec<T>>>,
{
    let mut lost: Vec<T> = Vec::new();
    for _ in 0..batches {
        let batch = match purge().await {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(error = %error, "{what} purge failed");
                break;
            }
        };
        let deleted: i32 = batch.iter().map(Lost::count).sum();
        for company in batch {
            match lost
                .iter_mut()
                .find(|known| known.tenant_id() == company.tenant_id())
            {
                Some(known) => known.add(&company),
                None => lost.push(company),
            }
        }
        if deleted < size {
            break;
        }
    }
    lost
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn usage(tenant_id: Uuid, records: i32) -> PurgedUsage {
        PurgedUsage { tenant_id, records }
    }

    /// The batches a purge returns in turn, and how many it was asked for.
    fn purge_of(
        batches: Vec<sqlx::Result<Vec<PurgedUsage>>>,
        asked: &Cell<usize>,
    ) -> impl FnMut() -> std::future::Ready<sqlx::Result<Vec<PurgedUsage>>> + '_ {
        let mut batches = batches.into_iter();
        move || {
            asked.set(asked.get() + 1);
            std::future::ready(batches.next().unwrap_or_else(|| Ok(Vec::new())))
        }
    }

    #[tokio::test]
    async fn full_batches_go_on_each_company_counted_once() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let asked = Cell::new(0);
        let batches = vec![
            Ok(vec![usage(a, 2)]),
            Ok(vec![usage(a, 1), usage(b, 1)]),
            Ok(vec![usage(b, 1)]),
        ];
        let lost = in_batches("Test", 5, 2, purge_of(batches, &asked)).await;
        // The third came back short: no fourth asked.
        assert_eq!(asked.get(), 3);
        assert_eq!(lost, [usage(a, 3), usage(b, 2)]);
    }

    #[tokio::test]
    async fn a_failed_statement_ends_it_what_was_deleted_kept() {
        let a = Uuid::from_u128(1);
        let asked = Cell::new(0);
        let batches = vec![Ok(vec![usage(a, 2)]), Err(sqlx::Error::PoolTimedOut)];
        let lost = in_batches("Test", 5, 2, purge_of(batches, &asked)).await;
        assert_eq!(asked.get(), 2);
        assert_eq!(lost, [usage(a, 2)]);
    }

    #[tokio::test]
    async fn a_pass_asks_its_statements_at_most() {
        let a = Uuid::from_u128(1);
        let asked = Cell::new(0);
        let batches = (0..10).map(|_| Ok(vec![usage(a, 2)])).collect();
        let lost = in_batches("Test", 3, 2, purge_of(batches, &asked)).await;
        assert_eq!(asked.get(), 3);
        assert_eq!(lost, [usage(a, 6)]);
    }
}
