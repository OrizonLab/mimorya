//! The verification of its company's audit chain an administrator asks for
//! (ADR-179): `audit:read` on the whole company, the chain verified from its
//! first place under this server's key, against the anchors the
//! administrator kept outside the platform. An anchor must name their
//! company, by its identifier or its slug: one that names another is refused
//! without that company being looked up, so that nothing tells whether it
//! exists. A few verifications at once in a process, one for a company
//! (`places.rs`), each traced in the audit log with what it found, as a
//! failure when the chain does not hold.
//! A server without a master key seals nothing, and verifies nothing.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::audit_chain::ChainHead;
use mimorya_db::repositories::tenants::tenant_slug;
use serde_json::json;
use uuid::Uuid;

use super::{Anchor, AnchorFound, ChainChecks, ChainKey, ChainReport, verify_company};
use crate::config::AppConfig;
use crate::services::AccessContext;

/// Anchors one verification takes.
pub const MAX_ANCHORS: usize = 20;
/// The longest anchor read: one as written is about a hundred characters.
const ANCHOR_LENGTH: usize = 200;
const PERMISSION: &str = "audit:read";

const UNSEALED: &str = "Ce serveur ne scelle pas le journal d’audit : aucune clé maître \
     (MIMORYA_MASTER_KEY) n’y est configurée. Adressez-vous à l’exploitant de la plateforme.";
const TOO_LONG: &str = "Une ancre compte au plus 200 caractères.";

/// What the verification of a company's chain found, and the anchors it
/// was given, in their order.
#[derive(Debug)]
pub struct OwnChainCheck {
    pub tenant_id: Uuid,
    pub report: ChainReport,
    pub anchors: Vec<Anchor>,
}

/// Verifies the chain of the company of `access`, against `anchors` as the
/// administrator wrote them.
pub async fn verify_own_chain(
    db: &Database,
    config: &AppConfig,
    checks: &ChainChecks,
    access: &AccessContext,
    anchors: &[String],
) -> anyhow::Result<OwnChainCheck> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let Some(cipher) = config.secrets.as_deref() else {
        return Err(DomainError::unavailable(UNSEALED).into());
    };
    let key = ChainKey::of(cipher)?;
    let anchors = own_anchors(db, tenant_id, anchors).await?;
    let _place = checks.take(tenant_id)?;
    let heads: Vec<ChainHead> = anchors.iter().map(Anchor::head).collect();
    let report = verify_company(db, &key, tenant_id, &heads).await?;
    if !report.holds() {
        tracing::error!(
            tenant_id = %tenant_id,
            broken = report.broken,
            anchors_missed = missed(&report),
            "Audit chain verified on demand does not hold"
        );
    }
    record(db, access, &report).await?;
    Ok(OwnChainCheck {
        tenant_id,
        report,
        anchors,
    })
}

/// The anchors written, read, each of the company `tenant_id`.
async fn own_anchors(
    db: &Database,
    tenant_id: Uuid,
    written: &[String],
) -> anyhow::Result<Vec<Anchor>> {
    if written.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = db.begin(tenant_id).await?;
    let slug = tenant_slug(&mut tx, tenant_id).await?.unwrap_or_default();
    tx.commit().await?;
    written
        .iter()
        .map(|text| own_anchor(text, tenant_id, &slug).map_err(anyhow::Error::from))
        .collect()
}

/// The anchor `text` writes, if it names the company `tenant_id`, of slug
/// `slug`.
fn own_anchor(text: &str, tenant_id: Uuid, slug: &str) -> Result<Anchor, DomainError> {
    if text.chars().count() > ANCHOR_LENGTH {
        return Err(DomainError::validation(TOO_LONG));
    }
    let anchor = Anchor::parse(text).map_err(DomainError::validation)?;
    if !anchor.names(tenant_id, slug) {
        return Err(DomainError::validation(format!(
            "L’ancre de la place {} nomme « {} », qui n’est pas votre entreprise : une ancre la \
             nomme par son identifiant ({tenant_id}) ou par son sigle ({slug}).",
            anchor.seq, anchor.company
        )));
    }
    Ok(anchor)
}

fn missed(report: &ChainReport) -> usize {
    report
        .anchors
        .iter()
        .filter(|found| **found != AnchorFound::Held)
        .count()
}

/// The verification traced in the company's audit log: who asked, what it
/// found; a failure when the chain does not hold.
async fn record(db: &Database, access: &AccessContext, report: &ChainReport) -> anyhow::Result<()> {
    let holds = report.holds();
    let outcome = if holds { "success" } else { "failure" };
    let mut entry = AuditEntry::new(
        access.subject.tenant_id,
        "user",
        "audit.chain_verified",
        outcome,
    )
    .by_user(access.subject.user_id)
    .metadata(json!({
        "verified": report.verified,
        "unsealed": report.unsealed,
        "lastPlace": report.head.as_ref().map(|head| head.chain_seq),
        "broken": report.broken,
        "anchors": report.anchors.len(),
        "anchorsMissed": missed(report),
    }));
    if !holds {
        entry = entry.reason(if report.broken > 0 {
            "La chaîne du journal d’audit est rompue"
        } else {
            "Une ancre n’est plus tenue par la chaîne du journal d’audit"
        });
    }
    let mut tx = db.begin(access.subject.tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_names_the_company_or_is_refused() {
        let acme = Uuid::from_u128(1);
        let seal = "ab".repeat(32);
        assert!(own_anchor(&format!("{acme}:3:{seal}"), acme, "acme").is_ok());
        assert!(own_anchor(&format!(" acme:3:{seal} "), acme, "acme").is_ok());
        let other = own_anchor(&format!("{}:3:{seal}", Uuid::from_u128(2)), acme, "acme");
        let refused = other.unwrap_err();
        assert!(refused.message.contains("n’est pas votre entreprise"));
        assert!(refused.message.contains(&acme.to_string()));
        assert!(own_anchor(&format!("beta:3:{seal}"), acme, "acme").is_err());
        let malformed = own_anchor("acme:3", acme, "acme").unwrap_err();
        assert!(malformed.message.contains("<entreprise>:<place>:<sceau>"));
        let long = format!("acme:3:{}", "a".repeat(ANCHOR_LENGTH));
        assert_eq!(
            own_anchor(&long, acme, "acme").unwrap_err().message,
            TOO_LONG
        );
    }
}
