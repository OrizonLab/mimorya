//! The demo company's audit log sealed from the start (ADR-178): an entry
//! written then sealed at start, so that `audit-chain.spec.ts` finds a
//! place and an anchor to keep however soon it runs, without waiting for
//! the maintenance's next pass (ADR-179).

use mimorya_api::app::App;
use mimorya_api::services::audit_chain::{ChainKey, SealedHeads, seal_company};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};

pub async fn leave(app: &App) -> anyhow::Result<()> {
    let services = &app.services;
    let cipher = services
        .config
        .secrets
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("no master key"))?;
    let mut tx = services.db.begin(tenant::ID).await?;
    let started = AuditEntry::new(tenant::ID, "system", "e2e.chain_started", "success");
    append_audit(&mut tx, started).await?;
    tx.commit().await?;
    let key = ChainKey::of(cipher)?;
    seal_company(&services.db, &key, &SealedHeads::default(), tenant::ID).await?;
    Ok(())
}
