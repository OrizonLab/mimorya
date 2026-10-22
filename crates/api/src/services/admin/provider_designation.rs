//! What designates a declared identity provider and how it is reached,
//! checked before anything is written: an OpenID Connect provider by its
//! issuer, reached at once, with the company's client and its secret
//! (ADR-063); a SAML provider by its metadata, read here, whose entity ID
//! is its issuer, and the company's own entity ID as its client (ADR-068).

use mimorya_connectors::mcp::url_problem;
use mimorya_core::DomainError;
use mimorya_core::identity_providers::{
    ProviderKind, client_id_problem, client_secret_problem, declared_issuer,
};
use uuid::Uuid;

use crate::services::sso::{SsoService, saml_summary};

fn invalid(problem: impl Into<String>) -> DomainError {
    DomainError::validation(problem.into())
}

/// What a declaration gives, beyond its name, domains and settings.
pub struct Given {
    pub directory_id: Option<String>,
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub metadata: Option<String>,
}

/// The provider as stored.
pub struct Designation {
    pub issuer: String,
    pub client_id: String,
    /// OpenID Connect: sealed in the company's secret store.
    pub client_secret: Option<String>,
    /// SAML: kept with the provider.
    pub saml_metadata: Option<String>,
}

pub async fn designation(
    sso: &SsoService,
    tenant_id: Uuid,
    kind: ProviderKind,
    given: Given,
) -> anyhow::Result<Designation> {
    if kind == ProviderKind::Saml {
        return Ok(saml(sso, tenant_id, given)?);
    }
    if given.metadata.is_some() {
        return Err(invalid("Métadonnées : pour un fournisseur SAML seulement").into());
    }
    let issuer = declared_issuer(kind, given.directory_id.as_deref(), given.issuer.as_deref())
        .map_err(invalid)?;
    if let Some(problem) = url_problem(&issuer, sso.network()) {
        return Err(invalid(format!("Adresse de l’émetteur : {problem}")).into());
    }
    let client_id = given.client_id.unwrap_or_default();
    let client_secret = given.client_secret.unwrap_or_default();
    if let Some(problem) =
        client_id_problem(&client_id).or_else(|| client_secret_problem(&client_secret))
    {
        return Err(invalid(problem).into());
    }
    // Reached before anything is written, outside the lock.
    if let Some(problem) = sso.issuer_problem(&issuer).await {
        return Err(invalid(problem).into());
    }
    Ok(Designation {
        issuer,
        client_id,
        client_secret: Some(client_secret),
        saml_metadata: None,
    })
}

fn saml(sso: &SsoService, tenant_id: Uuid, given: Given) -> Result<Designation, DomainError> {
    if given.issuer.is_some() || given.client_id.is_some() || given.client_secret.is_some() {
        return Err(invalid(
            "SAML : les métadonnées du fournisseur suffisent, sans émetteur, client ni secret",
        ));
    }
    let metadata = given
        .metadata
        .ok_or_else(|| invalid("SAML : collez les métadonnées du fournisseur (XML)"))?;
    let entity_id = saml_entity_of(sso, &metadata)?;
    let issuer = declared_issuer(
        ProviderKind::Saml,
        given.directory_id.as_deref(),
        Some(&entity_id),
    )
    .map_err(invalid)?;
    Ok(Designation {
        issuer,
        client_id: sso.saml_entity_id(tenant_id),
        client_secret: None,
        saml_metadata: Some(metadata),
    })
}

/// The entity ID of metadata a SAML provider can serve with, its sign-in
/// address one the browser may be sent to.
pub fn saml_entity_of(sso: &SsoService, metadata: &str) -> Result<String, DomainError> {
    let summary = saml_summary(metadata).map_err(invalid)?;
    if let Some(problem) = sso.browser_address_problem(&summary.sign_in_url) {
        return Err(invalid(format!("SAML : adresse de connexion : {problem}")));
    }
    Ok(summary.entity_id)
}
