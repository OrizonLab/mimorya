//! A pass of the classifier (ADR-213, D4): the companies with documents
//! awaiting their classification, each a batch of its oldest at a time, so
//! that one company's backlog never holds the others'. The usage is read
//! again for each company: off since the deposit, its requests are dropped
//! without asking anything. One pass at a time, whatever the number of
//! workers (ADR-086): a pass waits for the one under way elsewhere, then
//! classifies what that one left.

use std::time::Duration;

use anyhow::Context;
use mimorya_agent::decision::{DecisionMode, DecisionUsage, UsagePolicy};
use mimorya_db::repositories::decision_settings::decision_usages;
use mimorya_db::repositories::document_labels::{
    drop_label_requests, label_requests, tenants_awaiting_document_labels,
};
use uuid::Uuid;

use super::DocumentLabelService;

/// Documents classified per company and pass.
const BATCH: i64 = 10;
/// The passes' lock, across processes (ADR-086).
const LABELS_SWEEP: &str = "document-labels";
/// How long a pass waits for the one under way elsewhere.
const LABELS_WAIT: Duration = Duration::from_secs(300);

impl DocumentLabelService {
    /// Classifies the documents awaiting it in every company; how many were
    /// classified. One pass at a time, in every process.
    pub async fn classify_pending(&self) -> anyhow::Result<usize> {
        let _pass = self.pass.lock().await;
        let sweep = self
            .db
            .sweep_lock(LABELS_SWEEP, LABELS_WAIT)
            .await
            .context("document classification held elsewhere")?;
        let classified = self.classify_all().await;
        sweep.release().await;
        classified
    }

    async fn classify_all(&self) -> anyhow::Result<usize> {
        let awaiting = tenants_awaiting_document_labels(self.db.pool()).await?;
        let mut classified = 0;
        for tenant_id in awaiting {
            match self.classify_company(tenant_id).await {
                Ok(count) => classified += count,
                // Its requests stay: tried again at the next pass.
                Err(error) => tracing::warn!(
                    %tenant_id,
                    error = format!("{error:#}"),
                    "Document classification failed"
                ),
            }
        }
        Ok(classified)
    }

    async fn classify_company(&self, tenant_id: Uuid) -> anyhow::Result<usize> {
        let mut tx = self.db.begin(tenant_id).await?;
        let policy = usage_policy(&mut tx).await?;
        if policy.mode == DecisionMode::Off {
            drop_label_requests(&mut tx).await?;
            tx.commit().await?;
            return Ok(0);
        }
        let requests = label_requests(&mut tx, BATCH).await?;
        tx.commit().await?;
        let mut classified = 0;
        for request in requests {
            match self.classify(tenant_id, policy, request).await {
                Ok(()) => classified += 1,
                // Its request stays: tried again at the next pass, the
                // company's others meanwhile.
                Err(error) => tracing::warn!(
                    %tenant_id,
                    document_id = %request.document_id,
                    error = format!("{error:#}"),
                    "Document classification failed"
                ),
            }
        }
        Ok(classified)
    }
}

/// How the company set the usage: off without a row.
async fn usage_policy(conn: &mut sqlx::PgConnection) -> anyhow::Result<UsagePolicy> {
    let usage = DecisionUsage::DocumentLabels;
    let row = (decision_usages(conn).await?)
        .into_iter()
        .find(|row| row.usage == usage.as_str());
    Ok(row
        .and_then(|row| {
            Some(UsagePolicy {
                mode: DecisionMode::parse(&row.mode)?,
                threshold: row.threshold,
            })
        })
        .unwrap_or(UsagePolicy {
            mode: DecisionMode::Off,
            threshold: 0.5,
        }))
}
