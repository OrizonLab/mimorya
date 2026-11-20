//! One document classified (ADR-213, D4): its title and the text of its
//! first pages read, the decision asked outside any transaction, then,
//! active, what it gives applied to the document as it still is (locked,
//! at the version and level classified): a stricter level raised and
//! audited, a looser one and a type proposed, with the level it held
//! before a raise, to a person who may write it. Its request is dropped
//! whatever the decision: a failure leaves the level its depositor chose,
//! the usage's prudent value, and is not asked again.

use mimorya_agent::decision::{
    Decided, DecisionMode, DecisionOutcome, FIRST_PAGES, MAX_FIRST_TEXT_CHARS, UsagePolicy,
    classified_labels, document_labels_request,
};
use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::{DocumentType, LabelsOutcome, labels_outcome};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::document_labels::{
    LabelProposal, LabelRequest, LabelledDocument, drop_label_request, first_text,
    labelled_document, put_label_proposal, raise_document_sensitivity,
};
use serde_json::json;
use uuid::Uuid;

use super::DocumentLabelService;
use crate::services::llm::DecisionScope;

const RAISED: &str = "document.sensitivity_raised";
const PROPOSED: &str = "document.labels_proposed";

impl DocumentLabelService {
    pub(super) async fn classify(
        &self,
        tenant_id: Uuid,
        policy: UsagePolicy,
        request: LabelRequest,
    ) -> anyhow::Result<()> {
        let id = request.document_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(document) = current(&mut tx, request).await? else {
            // Replaced since without a request, or deleted.
            drop_label_request(&mut tx, id, request.version).await?;
            tx.commit().await?;
            return Ok(());
        };
        let text = first_text(&mut tx, id, FIRST_PAGES, MAX_FIRST_TEXT_CHARS).await?;
        tx.commit().await?;
        let scope = DecisionScope {
            run_id: None,
            user_id: document.depositor,
        };
        let service = self.llm.decision_service(tenant_id, scope).await?;
        let asked = document_labels_request(&document.title, &text);
        let outcome = service.decide(&asked).await;
        let decided = match &outcome {
            DecisionOutcome::Decided(decided) => Some(decided),
            DecisionOutcome::Failed => None,
        };
        let classified = decided.and_then(|decided| classified_labels(decided, policy.threshold));
        let mut tx = self.db.begin(tenant_id).await?;
        let mut effect = LabelsOutcome::default();
        if let (Some(decided), Some(classified)) = (decided, classified)
            && policy.mode == DecisionMode::Active
            && let Some(document) = current(&mut tx, request).await?
        {
            let level = sensitivity(&document.sensitivity);
            let kind = document
                .document_type
                .as_deref()
                .and_then(DocumentType::parse);
            effect = labels_outcome(kind, level, classified);
            apply(&mut tx, tenant_id, request, level, decided, effect).await?;
        }
        drop_label_request(&mut tx, id, request.version).await?;
        tx.commit().await?;
        tracing::info!(
            target: "mimorya::decision",
            usage = "document_labels",
            mode = policy.mode.as_str(),
            document_id = %id,
            classified = classified.is_some(),
            level = classified.map(|c| c.level.as_str()),
            raised = effect.raise.map(Sensitivity::as_str),
            proposed_type = effect.document_type.map(DocumentType::as_str),
            proposed_level = effect.lower.map(Sensitivity::as_str),
            "Document classified"
        );
        Ok(())
    }
}

/// The document, locked, if it is still at the version requested.
async fn current(
    tx: &mut TenantTx,
    request: LabelRequest,
) -> anyhow::Result<Option<LabelledDocument>> {
    let document = labelled_document(tx, request.document_id).await?;
    Ok(document.filter(|document| document.version == request.version))
}

fn sensitivity(stored: &str) -> Sensitivity {
    // Stored under a check constraint: an unknown level reads as the highest.
    stored.parse().unwrap_or(Sensitivity::Restricted)
}

/// Raises a stricter level and records what waits for a person, audited by
/// the platform, never with the document's title nor its text.
async fn apply(
    tx: &mut TenantTx,
    tenant_id: Uuid,
    request: LabelRequest,
    level: Sensitivity,
    decided: &Decided,
    effect: LabelsOutcome,
) -> anyhow::Result<()> {
    let (id, version) = (request.document_id, request.version);
    let engine = decided.engine.as_str();
    if let Some(raise) = effect.raise {
        let raised = raise_document_sensitivity(tx, id, version, level.as_str(), raise.as_str());
        if !raised.await? {
            return Ok(());
        }
        let entry = AuditEntry::new(tenant_id, "system", RAISED, "success")
            .resource("document", id)
            .metadata(json!({
                "from": level.as_str(),
                "to": raise.as_str(),
                "version": version,
                "engine": engine,
                "model": decided.model,
            }));
        append_audit(tx, entry).await?;
    }
    if !effect.proposes() {
        return Ok(());
    }
    let proposal = LabelProposal {
        document_id: id,
        version,
        document_type: effect.document_type.map(|kind| kind.as_str().to_string()),
        sensitivity: effect.lower.map(|lower| lower.as_str().to_string()),
        raised_from: effect.raise.map(|_| level.as_str().to_string()),
        engine: engine.to_string(),
    };
    put_label_proposal(tx, tenant_id, &proposal).await?;
    let entry = AuditEntry::new(tenant_id, "system", PROPOSED, "success")
        .resource("document", id)
        .metadata(json!({
            "documentType": proposal.document_type,
            "sensitivity": proposal.sensitivity,
            "raisedFrom": proposal.raised_from,
            "version": version,
            "engine": engine,
            "model": decided.model,
        }));
    append_audit(tx, entry).await?;
    Ok(())
}
