//! The company's identity providers (ADR-063), declared, changed or
//! removed by an administrator of its settings, under the administration
//! lock. The provider is reached before it is declared; the client secret
//! is sealed in the secret store, never returned nor audited.

use mimorya_connectors::mcp::url_problem;
use mimorya_core::DomainError;
use mimorya_core::identity_providers::{
    ProviderKind, client_id_problem, client_secret_problem, declared_issuer, declared_name,
    normalise_domains,
};
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::identity_providers::{
    IdentityProviderPatch, NewIdentityProvider, create_identity_provider, delete_identity_provider,
    get_identity_provider, replace_provider_domains, update_identity_provider,
};
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::sso::{SIGN_IN_ADMINISTRATOR, SsoService, client_secret_name};

// Who declares the providers is who keeps a password under them (ADR-066).
const PERMISSION: &str = SIGN_IN_ADMINISTRATOR;
const ABSENT: &str = "Fournisseur d’identité introuvable";

pub struct ProviderDeclaration {
    pub kind: String,
    pub name: Option<String>,
    pub directory_id: Option<String>,
    pub issuer: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub domains: Vec<String>,
    pub enabled: bool,
    pub required: bool,
}

#[derive(Default)]
pub struct ProviderChanges {
    pub name: Option<String>,
    pub client_secret: Option<String>,
    pub domains: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub required: Option<bool>,
}

fn invalid(problem: impl Into<String>) -> DomainError {
    DomainError::validation(problem.into())
}

pub async fn declare_identity_provider(
    db: &Database,
    sso: &SsoService,
    secrets: &SecretStore,
    access: &AccessContext,
    input: ProviderDeclaration,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let kind = ProviderKind::parse(&input.kind)
        .ok_or_else(|| invalid("Type de fournisseur : microsoft, google ou oidc"))?;
    let issuer = declared_issuer(kind, input.directory_id.as_deref(), input.issuer.as_deref())
        .map_err(invalid)?;
    if let Some(problem) = url_problem(&issuer, sso.network()) {
        return Err(invalid(format!("Adresse de l’émetteur : {problem}")).into());
    }
    let name = declared_name(kind, input.name.as_deref()).map_err(invalid)?;
    if let Some(problem) =
        client_id_problem(&input.client_id).or_else(|| client_secret_problem(&input.client_secret))
    {
        return Err(invalid(problem).into());
    }
    let domains = normalise_domains(&input.domains).map_err(invalid)?;
    secrets.require()?;
    // Reached before anything is written, outside the lock.
    if let Some(problem) = sso.issuer_problem(&issuer).await {
        return Err(invalid(problem).into());
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let id = create_identity_provider(
        &mut tx,
        NewIdentityProvider {
            tenant_id,
            kind: kind.as_str(),
            name: &name,
            issuer: &issuer,
            client_id: &input.client_id,
            enabled: input.enabled,
            required: input.required,
            created_by_user_id: access.subject.user_id,
        },
    )
    .await?
    .ok_or_else(|| {
        DomainError::conflict("Ce client de cet émetteur est déjà déclaré dans l’entreprise")
    })?;
    if let Some(taken) = replace_provider_domains(&mut tx, tenant_id, id, &domains).await? {
        return Err(taken_domain(&taken).into());
    }
    secrets
        .put(
            &mut tx,
            tenant_id,
            &client_secret_name(id),
            &input.client_secret,
        )
        .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "identity_provider.created", "identity_provider", id).metadata(json!({
            "kind": kind.as_str(),
            "name": name,
            "issuer": issuer,
            "clientId": input.client_id,
            "domains": domains,
            "enabled": input.enabled,
            "required": input.required,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

fn taken_domain(domain: &str) -> DomainError {
    DomainError::conflict(format!(
        "Le domaine {domain} relève déjà d’un autre fournisseur de l’entreprise"
    ))
}

pub async fn change_identity_provider(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    id: Uuid,
    changes: ProviderChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let domains = changes
        .domains
        .as_deref()
        .map(normalise_domains)
        .transpose()
        .map_err(invalid)?;
    if let Some(problem) = changes
        .client_secret
        .as_deref()
        .and_then(client_secret_problem)
    {
        return Err(invalid(problem).into());
    }
    if changes.client_secret.is_some() {
        secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let current = get_identity_provider(&mut tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found(ABSENT))?;
    let kind = ProviderKind::parse(&current.kind).ok_or_else(|| anyhow::anyhow!("stored kind"))?;
    let name = changes
        .name
        .as_deref()
        .map(|name| declared_name(kind, Some(name)))
        .transpose()
        .map_err(invalid)?;
    update_identity_provider(
        &mut tx,
        id,
        IdentityProviderPatch {
            name: name.clone(),
            enabled: changes.enabled,
            required: changes.required,
        },
    )
    .await?;
    if let Some(domains) = &domains
        && let Some(taken) = replace_provider_domains(&mut tx, tenant_id, id, domains).await?
    {
        return Err(taken_domain(&taken).into());
    }
    if let Some(secret) = &changes.client_secret {
        secrets
            .put(&mut tx, tenant_id, &client_secret_name(id), secret)
            .await?;
    }
    append_audit(
        &mut tx,
        audit_entry(access, "identity_provider.updated", "identity_provider", id).metadata(json!({
            "name": name,
            "domains": domains,
            "enabled": changes.enabled,
            "required": changes.required,
            "secretReplaced": changes.client_secret.is_some(),
            "previousDomains": current.domains,
            "previousEnabled": current.enabled,
            "previousRequired": current.required,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// With its domains, its secret and the identities tied to it.
pub async fn remove_identity_provider(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let current = get_identity_provider(&mut tx, id)
        .await?
        .ok_or_else(|| DomainError::not_found(ABSENT))?;
    delete_identity_provider(&mut tx, id).await?;
    secrets.delete(&mut tx, &client_secret_name(id)).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "identity_provider.deleted", "identity_provider", id).metadata(json!({
            "kind": current.kind,
            "name": current.name,
            "issuer": current.issuer,
            "clientId": current.client_id,
            "domains": current.domains,
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
