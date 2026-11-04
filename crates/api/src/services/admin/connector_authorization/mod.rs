//! The OAuth authorization of an MCP server by a company-wide connector
//! administrator (ADR-171): its start sends the administrator's browser to
//! the authorization server the declaration names, an attempt sealed in
//! their cookie (`attempt`, `start`); the return, in their session,
//! exchanges the code and keeps the tokens sealed for the connector
//! (`finish`). An authorization is withdrawn here, and goes with the
//! declaration it was given under (`connector_oauth`, `connectors`) or with
//! the connector; its tokens are then revoked at the authorization server
//! (`revocation`, ADR-172).

mod attempt;
mod finish;
mod revocation;
mod start;

pub use attempt::{AUTHORIZATION_COOKIE, authorization_cookie, cleared_authorization_cookie};
pub use finish::{AuthorizationFailure, OAuthReturn, finish_authorization};
pub(super) use revocation::{revoke, take_authorization};
pub use start::{AuthorizationStart, start_authorization};

use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::connector_oauth::McpServer;
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::AUTHORIZATION_REMOVED;

/// Where the authorization server sends the browser back, under the web
/// origin: the address to register for the client.
pub const CALLBACK_PATH: &str = "/api/admin/connectors/oauth/callback";

pub fn redirect_uri(web_origin: &str) -> String {
    format!("{web_origin}{CALLBACK_PATH}")
}

/// Withdraws the connector's authorization: its tokens go with it, then
/// are revoked at the authorization server if it allows it (ADR-172).
pub async fn remove_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let server = McpServer::of(get_connector_instance(&mut tx, id).await?)?;
    let declared = server.declared.as_ref();
    let Some(taken) = take_authorization(&mut tx, admin, tenant_id, id, declared).await? else {
        return Err(DomainError::validation("Ce serveur n’est pas autorisé").into());
    };
    append_audit(
        &mut tx,
        audit_entry(access, AUTHORIZATION_REMOVED, "connector", id),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    revoke(admin, access, taken).await;
    Ok(())
}
