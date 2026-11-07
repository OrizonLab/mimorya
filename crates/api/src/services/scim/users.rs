//! The company's accounts as its identity provider reads and creates them
//! through SCIM (ADR-183). It reads every account of the company, page by
//! page in the order they were made, or the one its filter names (its
//! address, or its own identifier of it), or one by its identifier; an
//! account of another company is not there. It creates an account as an
//! administrator would (ADR-027), but active, without a password: the person
//! signs in through the company's identity provider (ADR-063), or chooses a
//! password through « Mot de passe oublié » (ADR-028). The account receives
//! the base role on its own data, nothing more: no unit, no other role, the
//! default clearance. Never at an address another company holds or of a
//! domain it proved (ADR-069), nor with an identifier the provider already
//! gave another account. The creation is audited by the platform
//! (`system`), naming the token, never its secret.

use mimorya_core::identity_providers::email_domain;
use mimorya_core::scim::{UserFilter, parse_user_filter, read_new_user};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::scim_users::{
    ScimUserRow, count_scim_users, create_provisioned_user, scim_user, scim_user_by_email,
    scim_user_by_external_id, scim_users_page, tie_external_id,
};
use mimorya_db::repositories::verified_domains::domain_held_elsewhere;
use serde_json::{Value, json};
use uuid::Uuid;

use super::caller::{ScimCaller, open, refuse};
use crate::contract::{
    SCIM_LIST_SCHEMA, SCIM_MAX_RESULTS, ScimUser, ScimUserList, scim_user_fields,
};
use crate::http::scim::{ScimError, ScimResult};
use crate::services::admin::grant_member_role;

/// The page a provider asks for: a filter, where it starts (from 1) and how
/// many accounts it holds at most.
#[derive(Debug, Default)]
pub struct ScimPage {
    pub filter: Option<String>,
    pub start_index: Option<i64>,
    pub count: Option<i64>,
}

/// The page of the company's accounts `page` asks for, as read at `base`.
pub async fn list_scim_users(
    db: &Database,
    secret: Option<&str>,
    page: ScimPage,
    base: &str,
) -> ScimResult<ScimUserList> {
    let (_, mut tx) = open(db, secret).await?;
    let filter = match page.filter.as_deref().map(parse_user_filter).transpose() {
        Ok(filter) => filter,
        Err(problem) => return refuse(tx, ScimError::invalid_filter(problem)).await,
    };
    // From 1: anything lower reads as 1 (RFC 7644 §3.4.2.4).
    let start = page.start_index.unwrap_or(1).max(1);
    let count = page
        .count
        .unwrap_or(SCIM_MAX_RESULTS)
        .clamp(0, SCIM_MAX_RESULTS);
    let (total, rows) = match filter {
        None => {
            let total = count_scim_users(&mut tx).await?;
            (total, scim_users_page(&mut tx, start - 1, count).await?)
        }
        Some(filter) => {
            let found = match &filter {
                UserFilter::UserName(email) => scim_user_by_email(&mut tx, email).await?,
                UserFilter::ExternalId(id) => scim_user_by_external_id(&mut tx, id).await?,
            };
            let total = i64::from(found.is_some());
            let shown = found.filter(|_| start == 1 && count > 0);
            (total, shown.into_iter().collect())
        }
    };
    tx.commit().await?;
    Ok(ScimUserList {
        schemas: vec![SCIM_LIST_SCHEMA.into()],
        total_results: total,
        start_index: start,
        items_per_page: rows.len() as i64,
        resources: rows.iter().map(|row| ScimUser::of(row, base)).collect(),
    })
}

/// The company's account `id`, as read at `base`.
pub async fn read_scim_user(
    db: &Database,
    secret: Option<&str>,
    id: Option<Uuid>,
    base: &str,
) -> ScimResult<ScimUser> {
    let (_, mut tx) = open(db, secret).await?;
    let found = match id {
        Some(id) => scim_user(&mut tx, id).await?,
        None => None,
    };
    let Some(row) = found else {
        return refuse(tx, ScimError::not_found()).await;
    };
    tx.commit().await?;
    Ok(ScimUser::of(&row, base))
}

/// Creates the account `body` describes, as read at `base`.
pub async fn create_scim_user(
    db: &Database,
    secret: Option<&str>,
    body: &Value,
    base: &str,
) -> ScimResult<ScimUser> {
    let (caller, mut tx) = open(db, secret).await?;
    let read = scim_user_fields(body).and_then(read_new_user);
    let new = match read {
        Ok(new) => new,
        Err(problem) => return refuse(tx, ScimError::invalid_value(problem)).await,
    };
    let tenant_id = caller.tenant_id;
    // As an administrator's change: two creations at once see each other.
    tx.lock_administration().await?;
    if let Some(domain) = email_domain(&new.email)
        && domain_held_elsewhere(&mut tx, tenant_id, &domain).await?
    {
        let refused = ScimError::uniqueness(
            "Cette adresse relève d’un domaine prouvé par une autre entreprise",
        );
        return refuse(tx, refused).await;
    }
    if let Some(external_id) = &new.external_id
        && scim_user_by_external_id(&mut tx, external_id)
            .await?
            .is_some()
    {
        let refused = ScimError::uniqueness("Cet externalId est déjà celui d’un autre compte");
        return refuse(tx, refused).await;
    }
    let Some(user_id) = create_provisioned_user(
        &mut tx,
        tenant_id,
        &new.email,
        &new.display_name,
        new.title.as_deref(),
        new.active,
    )
    .await?
    else {
        let refused = ScimError::uniqueness("Cette adresse e-mail est déjà utilisée");
        return refuse(tx, refused).await;
    };
    grant_member_role(&mut tx, tenant_id, user_id, None).await?;
    if let Some(external_id) = &new.external_id
        && !tie_external_id(&mut tx, tenant_id, user_id, external_id).await?
    {
        // Held by the lock above: never reached but by a defect.
        return Err(anyhow::anyhow!("externalId taken during a locked creation").into());
    }
    let details = json!({ "clearance": "internal", "active": new.active });
    append_audit(&mut tx, audited(&caller, "user.created", user_id, details)).await?;
    let row: ScimUserRow = scim_user(&mut tx, user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("account created but not read back"))?;
    tx.commit().await?;
    Ok(ScimUser::of(&row, base))
}

/// The audit entry of a change the provider made to the account `user_id`
/// through SCIM, naming its token, with its `details`.
fn audited(caller: &ScimCaller, action: &str, user_id: Uuid, details: Value) -> AuditEntry {
    let mut metadata = json!({ "via": "scim", "scimToken": caller.token_id });
    if let (Some(all), Value::Object(more)) = (metadata.as_object_mut(), details) {
        all.extend(more);
    }
    AuditEntry::new(caller.tenant_id, "system", action, "success")
        .resource("user", user_id)
        .metadata(metadata)
}
