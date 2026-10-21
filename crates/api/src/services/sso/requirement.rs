//! A provider required for its domains (ADR-066): their accounts sign in
//! through it, never with a password, nor choose one when accepting their
//! invitation, nor get a reset link. The company's administrators of its
//! sign-in (`settings:manage` on the whole company) keep their password, so
//! that a provider out of order never locks the company out.

use mimorya_db::Database;
use mimorya_db::repositories::identity_providers::required_provider_for;
use uuid::Uuid;

use crate::services::AccessService;

/// Who keeps their password under a required provider.
pub const SIGN_IN_ADMINISTRATOR: &str = "settings:manage";

/// The name of the provider this account must sign in with, if a password
/// is refused to it.
pub async fn required_provider(
    db: &Database,
    access: &AccessService,
    tenant_id: Uuid,
    user_id: Uuid,
    email: &str,
) -> anyhow::Result<Option<String>> {
    let mut tx = db.begin(tenant_id).await?;
    let required = required_provider_for(&mut tx, email).await?;
    tx.commit().await?;
    let Some(provider) = required else {
        return Ok(None);
    };
    // An invited account has no rights yet: it is not exempt.
    let exempt = access
        .active_context(tenant_id, user_id)
        .await?
        .is_some_and(|context| context.require_tenant_wide(SIGN_IN_ADMINISTRATOR).is_ok());
    Ok((!exempt).then_some(provider))
}
