//! The SAML request a sign-in awaits the answer to (ADR-068), sealed in the
//! browser's attempt like OpenID Connect's state: its identifier, which the
//! provider's response must name, and where that response must arrive.

use saml_rs::{AuthnRequest, PendingAuthnRequest, PendingSnapshot, RelayStateParam, SamlInstant};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamlPending {
    id: String,
    peer_entity_id: String,
    expected_binding: String,
    request_binding: Option<String>,
    acs_url: String,
    acs_binding: String,
    acs_index: Option<u16>,
    acs_is_default: bool,
    issued_at: Option<String>,
    expires_at: Option<String>,
}

impl SamlPending {
    pub fn keep(pending: &PendingAuthnRequest) -> Self {
        let snapshot = pending.snapshot();
        Self {
            id: snapshot.id,
            peer_entity_id: snapshot.peer_entity_id,
            expected_binding: snapshot.expected_binding,
            request_binding: snapshot.request_binding,
            acs_url: snapshot.acs_url,
            acs_binding: snapshot.acs_binding,
            acs_index: snapshot.acs_index,
            acs_is_default: snapshot.acs_is_default,
            issued_at: snapshot.issued_at.map(|i| i.as_str().to_string()),
            expires_at: snapshot.expires_at.map(|i| i.as_str().to_string()),
        }
    }

    pub fn restore(&self) -> Result<PendingAuthnRequest, String> {
        let instant = |value: &Option<String>| {
            value
                .as_deref()
                .map(SamlInstant::try_new)
                .transpose()
                .map_err(|e| format!("demande SAML illisible : {e}"))
        };
        let mut snapshot = PendingSnapshot::<AuthnRequest>::authn_request(
            self.id.clone(),
            RelayStateParam::absent(),
            self.peer_entity_id.clone(),
            self.expected_binding.clone(),
            self.acs_url.clone(),
            self.acs_binding.clone(),
        );
        snapshot.request_binding = self.request_binding.clone();
        snapshot.acs_index = self.acs_index;
        snapshot.acs_is_default = self.acs_is_default;
        snapshot.issued_at = instant(&self.issued_at)?;
        snapshot.expires_at = instant(&self.expires_at)?;
        PendingAuthnRequest::from_snapshot(snapshot)
            .map_err(|e| format!("demande SAML illisible : {e}"))
    }
}
