//! What every Microsoft 365 tool does before and after Graph (ADR-117):
//! take the access token of the person the run serves, from the platform,
//! once they granted the permissions the tool needs (ADR-119), and tell, in
//! French, why there is none or why Graph refused. A token Graph refuses is
//! reported, so that the next call does not use it again. A write Graph
//! did not answer may have happened: the model is told to check it first.

use mimorya_core::account_links::{
    CALENDARS_READ, CALENDARS_WRITE, MAIL_READ, MAIL_SEND, MICROSOFT365,
};
use mimorya_core::tools::{AccountAccessError, ToolContext};
use serde_json::Value;
use url::Url;

use super::graph::{Graph, GraphFailure};

pub const NOT_LINKED: &str = "Aucun compte Microsoft 365 relié pour la personne servie : \
     elle le relie dans « Mon compte », puis redemande";
pub const REVOKED: &str = "Microsoft 365 n’honore plus la liaison du compte de la personne \
     servie : elle a été retirée ; la personne relie de nouveau son compte dans « Mon compte »";
const UNSETTLED: &str = "l’action a peut-être eu lieu : vérifiez dans Microsoft 365 avant de \
     la redemander";

/// Graph's answer to a `GET` as the run's person, who granted `needs`, or
/// why there is none, in French, as the model reads it.
pub async fn read_as_person(
    graph: &Graph,
    ctx: &ToolContext,
    needs: &[&str],
    url: Url,
    prefer: Option<&str>,
) -> Result<Value, String> {
    let token = token(ctx, needs).await?;
    let answer = graph.get(&token, url, prefer).await;
    answered(ctx, answer).await
}

/// Graph's answer to a `POST` of `body` as the run's person, who granted
/// `needs`, sent once: done, with Graph's answer if it gave one; or why
/// not, saying when it may have happened all the same.
pub async fn write_as_person(
    graph: &Graph,
    ctx: &ToolContext,
    needs: &[&str],
    url: Url,
    body: &Value,
    prefer: Option<&str>,
) -> Result<Option<Value>, String> {
    let token = token(ctx, needs).await?;
    let answer = graph.post(&token, url, body, prefer).await;
    let unsettled = answer.as_ref().is_err_and(|failure| !failure.settled());
    answered(ctx, answer).await.map_err(|why| match unsettled {
        true => format!("{why} : {UNSETTLED}"),
        false => why,
    })
}

async fn token(ctx: &ToolContext, needs: &[&str]) -> Result<String, String> {
    ctx.accounts
        .access_token(MICROSOFT365, needs)
        .await
        .map_err(unreachable)
}

async fn answered<T>(ctx: &ToolContext, answer: Result<T, GraphFailure>) -> Result<T, String> {
    match answer {
        Ok(answer) => Ok(answer),
        Err(failure) => {
            if failure == GraphFailure::Unauthorized {
                ctx.accounts.rejected(MICROSOFT365).await;
            }
            Err(failure.message())
        }
    }
}

fn unreachable(error: AccountAccessError) -> String {
    match error {
        AccountAccessError::NotLinked => NOT_LINKED.into(),
        AccountAccessError::Revoked => REVOKED.into(),
        AccountAccessError::NotGranted(missing) => not_granted(&missing),
        AccountAccessError::Unavailable(why) => why,
    }
}

/// What the person did not grant, in words, and how they grant it.
fn not_granted(missing: &[String]) -> String {
    let what: Vec<&str> = missing.iter().map(|scope| permission(scope)).collect();
    format!(
        "La personne servie n’a pas accordé à l’application Microsoft 365 de l’entreprise {} : \
         elle relie de nouveau son compte dans « Mon compte » pour l’accorder, puis redemande",
        what.join(" ni ")
    )
}

fn permission(scope: &str) -> &str {
    match scope {
        MAIL_SEND => "l’envoi d’e-mails en son nom",
        CALENDARS_WRITE => "l’écriture dans son agenda",
        MAIL_READ => "la lecture de ses e-mails",
        CALENDARS_READ => "la lecture de son agenda",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_not_granted_is_said_in_words() {
        let said = not_granted(&[MAIL_SEND.into(), CALENDARS_WRITE.into()]);
        assert!(said.contains("l’envoi d’e-mails en son nom ni l’écriture dans son agenda"));
        assert!(said.contains("« Mon compte »"));
    }
}
