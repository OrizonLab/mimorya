//! The codes the test provider issues (`/authorize`): to a client it
//! knows, for the person the test chose, else the one the address hints at,
//! named as Microsoft (a link's directory account, ADR-116) or Google (its
//! Workspace domain, ADR-121) name them; and what each code is redeemed for.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use super::{APPLICATION_ID, CLIENT_ID, GOOGLE_CLIENT_ID, Person, Provider};

/// The clients it knows.
pub(super) fn known_client(id: &str) -> bool {
    [CLIENT_ID, APPLICATION_ID, GOOGLE_CLIENT_ID].contains(&id)
}

pub(super) struct Grant {
    pub person: Person,
    /// Its token's audience.
    pub client_id: String,
    pub nonce: Option<String>,
    pub challenge: String,
    pub redirect_uri: String,
    /// The permissions asked.
    pub scope: String,
    /// Offline access asked, as Microsoft (a scope) or Google (a
    /// parameter) ask it: it brings a refresh token.
    pub offline: bool,
}

impl Provider {
    /// A code for this authorization request, signed in as the person
    /// chosen, else as the one hinted at: the return's query and address.
    pub(super) fn grant(
        &mut self,
        params: &HashMap<String, String>,
    ) -> Result<(String, String), String> {
        let param = |name: &str| {
            params
                .get(name)
                .cloned()
                .ok_or_else(|| format!("missing {name}"))
        };
        let client_id = param("client_id")?;
        if param("response_type")? != "code"
            || !known_client(&client_id)
            || param("code_challenge_method")? != "S256"
        {
            return Err("unexpected request".into());
        }
        let person = match (&self.signed_in, params.get("login_hint")) {
            (Some(person), _) => person.clone(),
            (None, Some(email)) => {
                let mut person = Person::verified(&format!("sujet-{email}"), email);
                // As the provider's own sign-in page names them.
                if let Some(name) = params.get("name").filter(|n| !n.is_empty()) {
                    person
                        .extra
                        .insert("name".into(), Value::String(name.clone()));
                }
                // A link at Google (ADR-121): the account's Workspace
                // domain, its address's.
                if client_id == GOOGLE_CLIENT_ID {
                    let domain = email.rsplit_once('@').map_or("", |(_, d)| d);
                    person
                        .extra
                        .insert("hd".into(), Value::String(domain.into()));
                }
                // A link at Microsoft (ADR-116): the directory's account,
                // named as Microsoft names it.
                else if params
                    .get("scope")
                    .is_some_and(|scope| scope.split(' ').any(|s| s == "offline_access"))
                {
                    let oid = Value::String(format!("objet-{email}"));
                    person.extra.insert("oid".into(), oid);
                    person.preferred_username = Some(email.clone());
                }
                person
            }
            (None, None) => return Err("nobody signs in".into()),
        };
        let scope = param("scope")?;
        let offline = scope.split(' ').any(|s| s == "offline_access")
            || params.get("access_type").map(String::as_str) == Some("offline");
        let code = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
        let redirect_uri = param("redirect_uri")?;
        self.grants.insert(
            code.clone(),
            Grant {
                person,
                client_id,
                nonce: params.get("nonce").cloned(),
                challenge: param("code_challenge")?,
                redirect_uri: redirect_uri.clone(),
                scope,
                offline,
            },
        );
        let back = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &code)
            .append_pair("state", &param("state")?)
            .finish();
        Ok((back, redirect_uri))
    }
}
