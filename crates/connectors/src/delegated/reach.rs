//! What every tool of a linked account does before and after its
//! provider's API (ADR-117, ADR-122): take the access token of the person
//! the run serves, from the platform, once they granted the permissions the
//! tool needs (ADR-119), and tell, in French, why there is none or why the
//! provider refused. A token the provider refuses is reported, so that the
//! next call does not use it again. A write the provider did not answer
//! may have happened: the model is told to check it first.

use mimorya_core::account_links::{
    CALENDAR_EVENTS, CALENDAR_READONLY, CALENDARS_READ, CALENDARS_WRITE, GMAIL_READONLY,
    GMAIL_SEND, LinkKind, MAIL_READ, MAIL_SEND,
};
use mimorya_core::tools::{AccountAccessError, ToolContext};
use serde_json::Value;
use url::Url;

use super::api::{ApiFailure, ProviderApi};

/// The provider's answer to a `GET` as the run's person, who granted
/// `needs`, or why there is none, in French, as the model reads it.
pub async fn read_as_person(
    api: &ProviderApi,
    ctx: &ToolContext,
    needs: &[&str],
    url: Url,
    prefer: Option<&str>,
) -> Result<Value, String> {
    AsPerson::open(api, ctx, needs)
        .await?
        .get(url, prefer)
        .await
}

/// The run's person at the provider, once they granted `needs`: their
/// token taken once, for a tool that reads several times.
pub struct AsPerson<'a> {
    api: &'a ProviderApi,
    ctx: &'a ToolContext,
    token: String,
}

impl<'a> AsPerson<'a> {
    pub async fn open(
        api: &'a ProviderApi,
        ctx: &'a ToolContext,
        needs: &[&str],
    ) -> Result<Self, String> {
        let token = token(api.kind(), ctx, needs).await?;
        Ok(Self { api, ctx, token })
    }

    /// The provider's answer to a `GET`, or why there is none.
    pub async fn get(&self, url: Url, prefer: Option<&str>) -> Result<Value, String> {
        let answer = self.api.get(&self.token, url, prefer).await;
        answered(self.api.kind(), self.ctx, answer).await
    }
}

/// The provider's answer to a `POST` of `body` as the run's person, who
/// granted `needs`, sent once: done, with its answer if it gave one; or why
/// not, saying when it may have happened all the same.
pub async fn write_as_person(
    api: &ProviderApi,
    ctx: &ToolContext,
    needs: &[&str],
    url: Url,
    body: &Value,
    prefer: Option<&str>,
) -> Result<Option<Value>, String> {
    let kind = api.kind();
    let token = token(kind, ctx, needs).await?;
    let answer = api.post(&token, url, body, prefer).await;
    let unsettled = answer.as_ref().is_err_and(|failure| !failure.settled());
    answered(kind, ctx, answer)
        .await
        .map_err(|why| match unsettled {
            true => format!(
                "{why} : l’action a peut-être eu lieu : vérifiez dans {} avant de la redemander",
                kind.product()
            ),
            false => why,
        })
}

async fn token(kind: LinkKind, ctx: &ToolContext, needs: &[&str]) -> Result<String, String> {
    ctx.accounts
        .access_token(kind.connector_type(), needs)
        .await
        .map_err(|error| unreachable(kind, error))
}

async fn answered<T>(
    kind: LinkKind,
    ctx: &ToolContext,
    answer: Result<T, ApiFailure>,
) -> Result<T, String> {
    match answer {
        Ok(answer) => Ok(answer),
        Err(failure) => {
            if failure == ApiFailure::Unauthorized {
                ctx.accounts.rejected(kind.connector_type()).await;
            }
            Err(failure.message(kind))
        }
    }
}

fn unreachable(kind: LinkKind, error: AccountAccessError) -> String {
    let product = kind.product();
    match error {
        AccountAccessError::NotLinked => format!(
            "Aucun compte {product} relié pour la personne servie : elle le relie dans « Mon \
             compte », puis redemande"
        ),
        AccountAccessError::Revoked => format!(
            "{product} n’honore plus la liaison du compte de la personne servie : elle a été \
             retirée ; la personne relie de nouveau son compte dans « Mon compte »"
        ),
        AccountAccessError::NotGranted(missing) => not_granted(kind, &missing),
        AccountAccessError::Unavailable(why) => why,
    }
}

/// What the person did not grant, in words, and how they grant it.
fn not_granted(kind: LinkKind, missing: &[String]) -> String {
    let what: Vec<&str> = missing.iter().map(|scope| permission(scope)).collect();
    format!(
        "La personne servie n’a pas accordé à l’application {} de l’entreprise {} : elle relie \
         de nouveau son compte dans « Mon compte » pour l’accorder, puis redemande",
        kind.product(),
        what.join(" ni ")
    )
}

fn permission(scope: &str) -> &str {
    match scope {
        MAIL_SEND | GMAIL_SEND => "l’envoi d’e-mails en son nom",
        CALENDARS_WRITE | CALENDAR_EVENTS => "l’écriture dans son agenda",
        MAIL_READ | GMAIL_READONLY => "la lecture de ses e-mails",
        CALENDARS_READ | CALENDAR_READONLY => "la lecture de son agenda",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_not_granted_is_said_in_words() {
        let said = not_granted(
            LinkKind::Microsoft365,
            &[MAIL_SEND.into(), CALENDARS_WRITE.into()],
        );
        assert!(said.contains("l’envoi d’e-mails en son nom ni l’écriture dans son agenda"));
        assert!(said.contains("application Microsoft 365"));
        assert!(said.contains("« Mon compte »"));
        let said = not_granted(LinkKind::GoogleWorkspace, &[GMAIL_READONLY.into()]);
        assert!(
            said.contains("application Google Workspace de l’entreprise la lecture de ses e-mails")
        );
    }

    #[test]
    fn an_account_not_reached_is_said_of_its_provider() {
        let said = unreachable(LinkKind::GoogleWorkspace, AccountAccessError::NotLinked);
        assert!(
            said.starts_with("Aucun compte Google Workspace relié"),
            "{said}"
        );
        let said = unreachable(LinkKind::Microsoft365, AccountAccessError::Revoked);
        assert!(said.starts_with("Microsoft 365 n’honore plus"), "{said}");
    }
}
