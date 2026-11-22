//! The company's record of processing activities (ADR-217), as who holds
//! `compliance:read` company-wide reads it: drawn from its settings as they
//! stand, in one transaction of its own, each time it is asked for, and
//! stored nowhere. A model is named where the company's data go now: the
//! one its runs resolve to, the operator's on its key when its formula
//! supplies it (ADR-208), at the server the operator set for it if any,
//! nothing for a choice that cannot be used.

mod reading;

#[cfg(test)]
mod tests;

use mimorya_core::gateways::GatewayKind;
use mimorya_core::identity_providers::ProviderKind;
use mimorya_core::processing_register::{
    IdentityFact, MailRelay, Register, RegisterFacts, register,
};
use mimorya_core::retention::RetentionKind;
use mimorya_core::second_factor::Requirement;
use mimorya_db::Database;
use mimorya_db::repositories::bearer_tokens::{TokenTable, count_tokens};
use mimorya_db::repositories::connectors::list_connector_instances;
use mimorya_db::repositories::decision_settings::decision_usages;
use mimorya_db::repositories::embedding_settings::get_embedding_settings;
use mimorya_db::repositories::gateways::list_gateways;
use mimorya_db::repositories::identity_providers::list_identity_providers;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use mimorya_db::repositories::retention_durations::retention_durations;
use mimorya_db::repositories::second_factor_settings::second_factor_requirement;

use self::reading::{
    chosen_model, connector, decision_uses, embedding_model, host, platform_model, retention,
};
use crate::mail::MailSettings;
use crate::services::embeddings::EmbeddingResolution;
use crate::services::llm::{Resolution, Review};
use crate::services::{AccessContext, EmbeddingService, LlmService};

const PERMISSION: &str = "compliance:read";

/// What the register reads besides the database.
pub struct RegisterSources<'a> {
    pub db: &'a Database,
    pub llm: &'a LlmService,
    pub embeddings: &'a EmbeddingService,
    /// The platform sends e-mails.
    pub mailer: bool,
    /// Through this relay, when the configuration names it (`SMTP_URL`).
    pub mail: Option<&'a MailSettings>,
}

pub async fn processing_register(
    sources: RegisterSources<'_>,
    access: &AccessContext,
) -> anyhow::Result<Register> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let models = sources.llm.resolve(tenant_id).await?;
    let embedder = sources.embeddings.resolve(tenant_id).await?;
    let jev = sources.llm.jev_opens(tenant_id).await?;

    let mut tx = sources.db.begin(tenant_id).await?;
    let runs_row = get_llm_settings(&mut tx, ModelPurpose::Conversations).await?;
    let review_row = get_llm_settings(&mut tx, ModelPurpose::Verification).await?;
    let usages = decision_usages(&mut tx).await?;
    // Disabled ones too: what they keep stays stored (ADR-220).
    let connectors = list_connector_instances(&mut tx, false).await?;
    let gateways = list_gateways(&mut tx).await?;
    let providers = list_identity_providers(&mut tx).await?;
    let scim = count_tokens(&mut tx, TokenTable::Scim).await? > 0;
    let audit_export = count_tokens(&mut tx, TokenTable::AuditExport).await? > 0;
    let stored = retention_durations(&mut tx).await?;
    let second_factor = second_factor_requirement(&mut tx).await?;
    // Read with the rest: the model the embedder serves must be the one
    // stored now.
    let embedding_row = get_embedding_settings(&mut tx).await?;
    tx.commit().await?;

    let runs_model = match &models.runs {
        Resolution::Company(_) => runs_row.as_ref().map(chosen_model),
        Resolution::Platform(Some(_)) => sources
            .llm
            .platform_endpoint()
            .map(|(kind, base_url)| platform_model(kind, base_url)),
        Resolution::Platform(None) | Resolution::Refused(_) => None,
    };
    let review_model = match &models.review {
        Review::Company(_) => review_row.as_ref().map(chosen_model),
        Review::Runs | Review::Refused(_) => None,
    };
    let embeddings = match (embedder.as_ref(), embedding_row) {
        (EmbeddingResolution::Ready(embedder), Some(row)) if embedder.serves(&row) => {
            Some(embedding_model(&row))
        }
        _ => None,
    };
    let decisions = decision_uses(
        usages
            .iter()
            .map(|row| (row.usage.as_str(), row.mode.as_str())),
    );
    let facts = RegisterFacts {
        company: access.tenant.name.clone(),
        runs_model,
        review_model,
        embeddings,
        decisions,
        jev,
        connectors: connectors.iter().map(connector).collect(),
        gateways: gateways
            .iter()
            .filter(|gateway| gateway.enabled)
            .filter_map(|gateway| GatewayKind::of(&gateway.kind))
            .collect(),
        identity_providers: providers
            .iter()
            .filter(|provider| provider.enabled)
            .filter_map(|provider| {
                Some(IdentityFact {
                    kind: ProviderKind::parse(&provider.kind)?,
                    name: provider.name.clone(),
                    host: host(&provider.issuer),
                })
            })
            .collect(),
        scim,
        audit_export,
        mail_relay: sources.mailer.then(|| MailRelay {
            host: sources.mail.and_then(|mail| host(&mail.smtp_url)),
        }),
        retention: RetentionKind::ALL
            .into_iter()
            .map(|kind| retention(kind, &stored))
            .collect(),
        second_factor: second_factor
            .as_deref()
            .and_then(Requirement::parse)
            .unwrap_or_default(),
    };
    Ok(register(&facts))
}
