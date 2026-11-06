//! The tools of a company's connectors for one run (ADR-048, ADR-049): each
//! enabled instance started, or taken from the cache, with the credential
//! its run may use. On a server each person authorizes (ADR-174), that is
//! the authorization of the person the run serves, never another's nor the
//! company's: their own instance, versioned by their authorization too.
//! Without it, the connector is named as one they must authorize in their
//! account; an evaluation of a skill serves no one, and such a connector is
//! withheld from it, named too. How a person's instance went says as much
//! of their authorization as of the server: it is not recorded on the
//! instance, unless its policies cannot be read.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use futures::future::join_all;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, list_connector_instances, report_connector_status,
};
use uuid::Uuid;

use mimorya_connectors::mcp::{OAUTH_EXPIRED_PERSON, OAUTH_REFUSED_PERSON, OAUTH_UNUSABLE_PERSON};

use super::cache::{Origin, STILL_STARTING, SessionKey, Unavailable, Version};
use super::{
    ConnectorIssue, ConnectorService, Credential, CredentialFor, Policies, TenantTools,
    UNOPENED_PERSON, credential_for, credential_of, each_person, failure, read_policies,
    scoped_overrides, start_instance,
};

/// Whom a run's connectors are started for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    /// The person a run serves, and whether they may use connectors: on a
    /// server each person authorizes, their own authorization, and nothing
    /// sent without that right (ADR-174).
    Person { user: Uuid, uses_connectors: bool },
    /// A skill's evaluation: no one's authorization.
    Evaluation,
}

/// Why a server each person authorizes is missing from a run: its person
/// has not authorized it (ADR-174).
pub const TO_AUTHORIZE: &str = "Connecteur que chaque personne autorise pour elle-même : à \
     autoriser dans « Mon compte »";

/// Why one is not started for a person who may not use connectors: their
/// token is not sent for tools they could not be given.
pub const NOT_FOR_THEM: &str = "Connecteur que chaque personne autorise pour elle-même : la \
     personne servie n’utilise pas les connecteurs";

/// Why one is missing from an evaluation, which serves no one.
pub const WITHHELD_PERSONAL: &str = "Connecteur que chaque personne autorise pour elle-même : \
     retenu pendant l’évaluation d’une compétence";

/// What an instance starts with for this run.
enum Plan {
    /// The company's: a typed token or the company's authorization.
    Shared(anyhow::Result<Credential>),
    /// The person's own authorization, given or renewed `since`.
    Own {
        person: Uuid,
        credential: anyhow::Result<Credential>,
        since: DateTime<Utc>,
    },
    /// Not started: why, and whether the person is to authorize it.
    Missing {
        message: &'static str,
        to_authorize: bool,
    },
}

/// An instance's tools under their policies, ready for a run.
struct Ready {
    runtime: std::sync::Arc<mimorya_connectors::ConnectorRuntime>,
    names: Vec<String>,
    overrides: Policies,
}

/// How an instance went for this run.
enum Prepared {
    /// Started, or not; `shared`: what it says is the server's own status.
    Started {
        outcome: Result<Ready, Unavailable>,
        shared: bool,
    },
    Missing {
        message: &'static str,
        to_authorize: bool,
    },
}

impl ConnectorService {
    pub async fn tools_for(&self, tenant_id: Uuid, served: Served) -> anyhow::Result<TenantTools> {
        // Before anything is read: a renewal makes a new version.
        self.renew_before_run(tenant_id, served).await?;
        let ticket = self.cache.ticket().await;
        // Instances and tokens read in one snapshot: a token always goes
        // with the address it was saved, or authorized, for.
        let mut tx = self.db.begin_snapshot(tenant_id).await?;
        let mut instances = list_connector_instances(&mut tx, true).await?;
        // The oldest keeps its tools when two connectors name the same one.
        instances.sort_by_key(|instance| instance.created_at);
        let mut plans = Vec::with_capacity(instances.len());
        for instance in &instances {
            plans.push(self.plan(&mut tx, instance, served).await?);
        }
        tx.commit().await?;

        let prepared = join_all(
            instances
                .iter()
                .zip(plans)
                .map(|(instance, plan)| self.prepare(instance, plan, ticket)),
        )
        .await;
        let mut result = TenantTools::default();
        let mut names = HashSet::new();
        let mut reports = Vec::new();
        for (instance, prepared) in instances.iter().zip(prepared) {
            let issue = |message: &str, to_authorize: bool| ConnectorIssue {
                connector: instance.name.clone(),
                connector_type: instance.connector_type.clone(),
                message: message.into(),
                to_authorize,
            };
            let (outcome, shared) = match prepared {
                Prepared::Started { outcome, shared } => (outcome, shared),
                Prepared::Missing {
                    message,
                    to_authorize,
                } => {
                    result.issues.push(issue(message, to_authorize));
                    continue;
                }
            };
            let outcome = outcome.and_then(|ready| {
                if ready.names.iter().any(|name| names.contains(name)) {
                    return Err(Unavailable::new(
                        "Des outils portent le même nom que ceux d’un connecteur plus ancien",
                    ));
                }
                Ok(ready)
            });
            let error = match outcome {
                Ok(ready) => {
                    names.extend(ready.names);
                    result.give(
                        &instance.name,
                        &instance.connector_type,
                        &ready.runtime.tools,
                        ready.overrides,
                    );
                    None
                }
                Err(unavailable) => {
                    // A person's token that no longer serves is theirs to
                    // authorize anew, in their account.
                    let theirs = !shared && theirs_to_authorize(&unavailable.message);
                    let said = if theirs {
                        TO_AUTHORIZE
                    } else if unavailable.origin == Origin::Starting {
                        STILL_STARTING
                    } else {
                        "Connecteur indisponible"
                    };
                    result.issues.push(issue(said, theirs));
                    match unavailable.origin {
                        Origin::Changed | Origin::Starting => continue,
                        Origin::Remembered => {}
                        Origin::Fresh => {
                            tracing::warn!(connector = %instance.id, error = %unavailable.message, "Connector unavailable");
                        }
                    }
                    Some(unavailable.message)
                }
            };
            let status = if error.is_some() { "error" } else { "ok" };
            if shared && (instance.status != status || instance.last_error != error) {
                reports.push((instance.id, instance.updated_at, error));
            }
        }
        if !reports.is_empty() {
            let mut tx = self.db.begin(tenant_id).await?;
            for (id, version, error) in &reports {
                report_connector_status(&mut tx, *id, *version, error.as_deref()).await?;
            }
            tx.commit().await?;
        }
        Ok(result)
    }

    /// What the instance starts with for this run, read in the caller's
    /// transaction; the error is the database's.
    async fn plan(
        &self,
        tx: &mut mimorya_db::TenantTx,
        instance: &ConnectorInstanceRow,
        served: Served,
    ) -> anyhow::Result<Plan> {
        let person = match served {
            Served::Person {
                uses_connectors: false,
                ..
            } if each_person(instance) => {
                return Ok(Plan::Missing {
                    message: NOT_FOR_THEM,
                    to_authorize: false,
                });
            }
            Served::Person { user, .. } => user,
            Served::Evaluation if each_person(instance) => {
                return Ok(Plan::Missing {
                    message: WITHHELD_PERSONAL,
                    to_authorize: false,
                });
            }
            Served::Evaluation => {
                return Ok(Plan::Shared(
                    credential_of(tx, &self.secrets, instance).await?,
                ));
            }
        };
        Ok(
            match credential_for(tx, &self.secrets, instance, person).await? {
                CredentialFor::Shared(credential) => Plan::Shared(credential),
                CredentialFor::Own { credential, since } => Plan::Own {
                    person,
                    credential,
                    since,
                },
                CredentialFor::NotAuthorized => Plan::Missing {
                    message: TO_AUTHORIZE,
                    to_authorize: true,
                },
            },
        )
    }

    /// The instance's tools and policies for this run, or why not.
    async fn prepare(&self, instance: &ConnectorInstanceRow, plan: Plan, ticket: u64) -> Prepared {
        let (key, version, credential) = match plan {
            Plan::Missing {
                message,
                to_authorize,
            } => {
                return Prepared::Missing {
                    message,
                    to_authorize,
                };
            }
            Plan::Shared(credential) => (
                SessionKey::from(instance.id),
                Version::from(instance.updated_at),
                credential,
            ),
            // Started again when the instance or the authorization changes.
            Plan::Own {
                person,
                credential,
                since,
            } => (
                SessionKey::person(instance.id, person),
                Version {
                    connector: instance.updated_at,
                    authorization: Some(since),
                },
                credential,
            ),
        };
        let policies = match read_policies(instance) {
            Ok(policies) => policies,
            // The server's own, whoever's instance it is.
            Err(error) => {
                return Prepared::Started {
                    outcome: Err(Unavailable::new(error)),
                    shared: true,
                };
            }
        };
        let shared = key.person.is_none();
        let outcome = self
            .started(instance, key, version, credential, ticket)
            .await
            .map(|runtime| {
                let names: Vec<String> = runtime
                    .tools
                    .iter()
                    .map(|tool| tool.info().name.clone())
                    .collect();
                let overrides = scoped_overrides(instance, policies, &names);
                Ready {
                    runtime,
                    names,
                    overrides,
                }
            });
        Prepared::Started { outcome, shared }
    }

    async fn started(
        &self,
        instance: &ConnectorInstanceRow,
        key: SessionKey,
        version: Version,
        credential: anyhow::Result<Credential>,
        ticket: u64,
    ) -> Result<std::sync::Arc<mimorya_connectors::ConnectorRuntime>, Unavailable> {
        let credential = credential.map_err(|error| Unavailable::new(error.to_string()))?;
        // Started in the background, owning what it needs (ADR-177).
        let settings = self.settings;
        let connector = instance.connector_type.clone();
        let config = instance.config.clone();
        let start = move || async move {
            start_instance(settings, &connector, &config, &credential)
                .await
                .map_err(|error| failure(&error, credential.token.as_deref()))
        };
        let patience = self.settings.start_patience;
        self.cache
            .get_or_start(key, version, ticket, patience, start)
            .await
    }
}

/// Whether a person's instance failed for their authorization's token,
/// expired, refused by the server, unusable or unopenable: theirs to give
/// anew (ADR-174).
fn theirs_to_authorize(message: &str) -> bool {
    [
        OAUTH_EXPIRED_PERSON,
        OAUTH_REFUSED_PERSON,
        OAUTH_UNUSABLE_PERSON,
        UNOPENED_PERSON,
    ]
    .iter()
    .any(|said| message.starts_with(said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_s_own_token_failing_is_theirs_to_authorize_again() {
        for said in [
            OAUTH_EXPIRED_PERSON,
            OAUTH_REFUSED_PERSON,
            OAUTH_UNUSABLE_PERSON,
            UNOPENED_PERSON,
        ] {
            assert!(theirs_to_authorize(said), "{said}");
            let detailed = format!("{said} (détail : HTTP 401)");
            assert!(theirs_to_authorize(&detailed), "{detailed}");
        }
        // The server's own failures stay unavailable.
        for said in [
            "Le serveur MCP a renvoyé une erreur",
            "Délai dépassé",
            mimorya_connectors::mcp::OAUTH_REFUSED,
        ] {
            assert!(!theirs_to_authorize(said), "{said}");
        }
    }
}
