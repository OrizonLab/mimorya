//! What every Microsoft 365 tool does before and after Graph (ADR-117):
//! take the access token of the person the run serves, from the platform,
//! and tell, in French, why there is none or why Graph refused. A token
//! Graph refuses is reported, so that the next call does not use it again.

use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::tools::{AccountAccessError, ToolContext, ToolResult};
use serde_json::Value;
use url::Url;

use super::graph::{Graph, GraphFailure};

pub const NOT_LINKED: &str = "Aucun compte Microsoft 365 relié pour la personne servie : \
     elle le relie dans « Mon compte », puis redemande";
pub const REVOKED: &str = "Microsoft 365 n’honore plus la liaison du compte de la personne \
     servie : elle a été retirée ; la personne relie de nouveau son compte dans « Mon compte »";

/// Graph's answer to a `GET` as the run's person, or the failed result the
/// model reads.
pub async fn read_as_person(
    graph: &Graph,
    ctx: &ToolContext,
    url: Url,
    prefer: Option<&str>,
) -> Result<Value, ToolResult> {
    let token = ctx
        .accounts
        .access_token(MICROSOFT365)
        .await
        .map_err(|error| ToolResult::error(unreachable(error)))?;
    match graph.get(&token, url, prefer).await {
        Ok(answer) => Ok(answer),
        Err(failure) => {
            if failure == GraphFailure::Unauthorized {
                ctx.accounts.rejected(MICROSOFT365).await;
            }
            Err(ToolResult::error(failure.message()))
        }
    }
}

fn unreachable(error: AccountAccessError) -> String {
    match error {
        AccountAccessError::NotLinked => NOT_LINKED.into(),
        AccountAccessError::Revoked => REVOKED.into(),
        AccountAccessError::Unavailable(why) => why,
    }
}
