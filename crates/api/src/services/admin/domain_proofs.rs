//! Proving the company's e-mail domains (ADR-069). An administrator of its
//! sign-in publishes, in a domain's DNS, the record the company is given for
//! it; the domain must be one a provider of the company covers. Read here and
//! found, the domain is proved for the company, and a generic OpenID Connect
//! or SAML provider may then create accounts in it.

use mimorya_core::DomainError;
use mimorya_core::identity_providers::{normalise_domains, proof_record_name, proves_domain};
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::verified_domains::{is_domain_declared, record_verified_domain};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::sso::SsoService;

const PERMISSION: &str = "settings:manage";

fn not_covered() -> DomainError {
    DomainError::validation("Domaine : aucun fournisseur de l’entreprise ne le couvre")
}

/// The domain proved, as normalised.
pub async fn prove_domain(
    db: &Database,
    sso: &SsoService,
    access: &AccessContext,
    domain: &str,
) -> anyhow::Result<String> {
    access.require_tenant_wide(PERMISSION)?;
    let domain = normalise_domains(&[domain.to_string()])
        .map_err(DomainError::validation)?
        .into_iter()
        .next()
        .ok_or_else(not_covered)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = db.begin(tenant_id).await?;
    let declared = is_domain_declared(&mut tx, &domain).await?;
    tx.commit().await?;
    if !declared {
        return Err(not_covered().into());
    }
    // Read outside the administration lock: a lookup may take seconds.
    let name = proof_record_name(&domain);
    let found = sso.dns().txt(&name).await.map_err(|error| {
        tracing::warn!(%error, domain, "A domain's proof could not be read");
        DomainError::validation(format!(
            "DNS : l’enregistrement {name} n’a pas pu être lu ; réessayez plus tard"
        ))
    })?;
    if !proves_domain(tenant_id, &domain, &found) {
        return Err(DomainError::validation(format!(
            "DNS : aucun enregistrement TXT {name} ne porte la valeur attendue ; publiez-le, \
             puis vérifiez de nouveau une fois propagé"
        ))
        .into());
    }
    let mut tx = begin_change(db, access, PERMISSION).await?;
    // Dropped by its provider meanwhile: nothing to prove any more.
    if !is_domain_declared(&mut tx, &domain).await? {
        return Err(not_covered().into());
    }
    record_verified_domain(&mut tx, tenant_id, &domain, access.subject.user_id).await?;
    append_audit(
        &mut tx,
        audit_entry(
            access,
            "identity_provider.domain_verified",
            "tenant",
            tenant_id,
        )
        .metadata(json!({ "domain": domain })),
    )
    .await?;
    tx.commit().await?;
    Ok(domain)
}
