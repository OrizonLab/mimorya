//! The company's accounts as its identity provider changes and removes them
//! through SCIM (ADR-183), once the token is read: a replacement (`PUT`) or
//! a patch (`PATCH`) changes the account's name, job title, the provider's
//! identifier of it and whether it is active; never its address, which
//! another account could then use to take it over. Disabled, the account
//! loses at once what a disabling by the administration takes
//! (`services::admin::account_access`). A removal (`DELETE`) disables it and
//! takes it out of SCIM's sight until an administrator enables it again.
//! Each change is audited by the platform (`system`), naming the token.

use mimorya_core::scim::{ChangeRefused, ScimChanges, patch, replacement};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::scim_users::{
    ScimUserRow, external_id_taken, mark_scim_removed, scim_user, set_external_id,
};
use mimorya_db::repositories::users::{UserPatch, update_user};
use serde_json::{Value, json};
use uuid::Uuid;

use super::caller::{ScimCaller, open, refuse};
use super::users::audited;
use crate::contract::{ScimUser, scim_operations, scim_user_fields};
use crate::http::scim::{ScimError, ScimResult};
use crate::services::admin::{ConnectorAdmin, TakenAccess, let_go, take_access};

/// Replaces the account `id` by what `body` sends, as read at `base`.
pub async fn replace_scim_user(
    admin: &ConnectorAdmin<'_>,
    secret: Option<&str>,
    id: Option<Uuid>,
    body: ScimResult<Value>,
    base: &str,
) -> ScimResult<ScimUser> {
    let (caller, tx) = open(admin.db, secret).await?;
    let read = body.and_then(|body| {
        let fields = scim_user_fields(&body).map_err(ScimError::invalid_value)?;
        replacement(fields).map_err(refused)
    });
    match read {
        Ok(changes) => change(admin, caller, tx, id, changes, base).await,
        Err(error) => refuse(tx, error).await,
    }
}

/// Applies to the account `id` the operations `body` sends, as read at
/// `base`.
pub async fn patch_scim_user(
    admin: &ConnectorAdmin<'_>,
    secret: Option<&str>,
    id: Option<Uuid>,
    body: ScimResult<Value>,
    base: &str,
) -> ScimResult<ScimUser> {
    let (caller, tx) = open(admin.db, secret).await?;
    let read = body.and_then(|body| {
        let operations = scim_operations(&body).map_err(ScimError::invalid_syntax)?;
        patch(operations).map_err(refused)
    });
    match read {
        Ok(changes) => change(admin, caller, tx, id, changes, base).await,
        Err(error) => refuse(tx, error).await,
    }
}

/// Removes the account `id`: disabled, out of SCIM's sight.
pub async fn remove_scim_user(
    admin: &ConnectorAdmin<'_>,
    secret: Option<&str>,
    id: Option<Uuid>,
) -> ScimResult<()> {
    let (caller, mut tx) = open(admin.db, secret).await?;
    // After the token's row, as every change of the company's (ADR-183).
    tx.lock_administration().await?;
    let Some(row) = found(&mut tx, id).await? else {
        return refuse(tx, ScimError::not_found()).await;
    };
    let tenant_id = caller.tenant_id;
    let disabling = row.status != "disabled";
    let mut revocations = TakenAccess::default();
    if disabling {
        let status = UserPatch {
            status: Some("disabled"),
            ..UserPatch::default()
        };
        update_user(&mut tx, row.id, status).await?;
        revocations = take_access(&mut tx, admin, tenant_id, row.id).await?;
    }
    mark_scim_removed(&mut tx, tenant_id, row.id).await?;
    let details = json!({ "removed": true, "wasDisabled": !disabling });
    append_audit(&mut tx, audited(&caller, "user.disabled", row.id, details)).await?;
    tx.commit().await?;
    let_go(admin, tenant_id, None, row.id, revocations).await;
    Ok(())
}

/// Applies `changes` to the account `id`, in the transaction `tx` the
/// caller's token opened.
async fn change(
    admin: &ConnectorAdmin<'_>,
    caller: ScimCaller,
    mut tx: TenantTx,
    id: Option<Uuid>,
    changes: ScimChanges,
    base: &str,
) -> ScimResult<ScimUser> {
    tx.lock_administration().await?;
    let Some(row) = found(&mut tx, id).await? else {
        return refuse(tx, ScimError::not_found()).await;
    };
    if let Some(problem) = refusal(&mut tx, &row, &changes).await? {
        return refuse(tx, problem).await;
    }
    let changed = changed(&changes);
    if changed.is_empty() {
        // Only attributes no one keeps: the account is as it was, its last
        // change and the audit untouched; the token's use is noted.
        tx.commit().await?;
        return Ok(ScimUser::of(&row, base));
    }
    let tenant_id = caller.tenant_id;
    let was_disabled = row.status == "disabled";
    let status = match changes.active {
        Some(false) if !was_disabled => Some("disabled"),
        Some(true) if was_disabled => Some("active"),
        _ => None,
    };
    let patch = UserPatch {
        display_name: changes.display_name.clone(),
        job_title: changes.title.clone(),
        status,
        ..UserPatch::default()
    };
    update_user(&mut tx, row.id, patch).await?;
    if let Some(external_id) = &changes.external_id {
        set_external_id(&mut tx, tenant_id, row.id, external_id.as_deref()).await?;
    }
    let revocations = match status {
        Some("disabled") => take_access(&mut tx, admin, tenant_id, row.id).await?,
        _ => TakenAccess::default(),
    };
    let action = match status {
        Some("disabled") => "user.disabled",
        Some(_) => "user.enabled",
        None => "user.updated",
    };
    let details = json!({ "changed": changed });
    append_audit(&mut tx, audited(&caller, action, row.id, details)).await?;
    let row = scim_user(&mut tx, row.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("account changed but not read back"))?;
    tx.commit().await?;
    let_go(admin, tenant_id, None, row.id, revocations).await;
    Ok(ScimUser::of(&row, base))
}

/// The company's account `id`, as SCIM sees it.
async fn found(tx: &mut TenantTx, id: Option<Uuid>) -> ScimResult<Option<ScimUserRow>> {
    Ok(match id {
        Some(id) => scim_user(tx, id).await?,
        None => None,
    })
}

/// Why `changes` cannot be made to the account `row`, if they cannot: its
/// address is the account's for good, the provider's identifier one
/// account's only.
async fn refusal(
    tx: &mut TenantTx,
    row: &ScimUserRow,
    changes: &ScimChanges,
) -> ScimResult<Option<ScimError>> {
    if let Some(user_name) = &changes.user_name
        && !user_name.eq_ignore_ascii_case(&row.email)
    {
        return Ok(Some(ScimError::mutability(
            "userName : l’adresse d’un compte ne change pas par SCIM",
        )));
    }
    if let Some(Some(external_id)) = &changes.external_id
        && external_id_taken(tx, Some(row.id), external_id).await?
    {
        return Ok(Some(ScimError::uniqueness(
            "Cet externalId est déjà celui d’un autre compte",
        )));
    }
    Ok(None)
}

/// The attributes `changes` set, as SCIM names them; never their values.
fn changed(changes: &ScimChanges) -> Vec<&'static str> {
    [
        ("displayName", changes.display_name.is_some()),
        ("title", changes.title.is_some()),
        ("externalId", changes.external_id.is_some()),
        ("active", changes.active.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, set)| set.then_some(name))
    .collect()
}

/// A change refused, as SCIM says it.
fn refused(refusal: ChangeRefused) -> ScimError {
    match refusal {
        ChangeRefused::Syntax(detail) => ScimError::invalid_syntax(detail),
        ChangeRefused::Value(detail) => ScimError::invalid_value(detail),
        ChangeRefused::Mutability(detail) => ScimError::mutability(detail),
    }
}
