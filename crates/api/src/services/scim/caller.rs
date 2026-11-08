//! Who a SCIM request comes from (ADR-183): the company its token names,
//! before any session, in a transaction of that company in which the
//! token's use is noted. An absent, unknown or removed token, or a suspended
//! company's, reaches nothing, and is refused alike.

use chrono::Utc;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::tokens::SCIM_TOKENS;
use crate::http::scim::{ScimError, ScimResult};

/// The company a request comes from, and the token it came with.
#[derive(Debug, Clone, Copy)]
pub struct ScimCaller {
    pub tenant_id: Uuid,
    pub token_id: Uuid,
}

/// The caller `secret` names, and a transaction of its company in which its
/// use is noted: removed meanwhile, the token reaches nothing.
pub async fn open(db: &Database, secret: Option<&str>) -> ScimResult<(ScimCaller, TenantTx)> {
    let (tenant_id, token_id) = SCIM_TOKENS
        .company(db, secret)
        .await?
        .ok_or_else(ScimError::unauthorized)?;
    let mut tx = db.begin(tenant_id).await?;
    if !SCIM_TOKENS.touch(&mut tx, token_id, Utc::now()).await? {
        return Err(ScimError::unauthorized());
    }
    Ok((
        ScimCaller {
            tenant_id,
            token_id,
        },
        tx,
    ))
}

/// Whether `secret` names a caller, its use noted: what is said only to it.
pub async fn authenticate(db: &Database, secret: Option<&str>) -> ScimResult<()> {
    let (_, tx) = open(db, secret).await?;
    tx.commit().await?;
    Ok(())
}

/// Refuses with `error`, the token's use noted all the same: a provider
/// refused still calls.
pub async fn refuse<T>(tx: TenantTx, error: ScimError) -> ScimResult<T> {
    tx.commit().await?;
    Err(error)
}
