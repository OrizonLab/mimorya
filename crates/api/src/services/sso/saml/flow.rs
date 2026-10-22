//! A SAML sign-in through `saml-rs` (ADR-068): the application as service
//! provider (its entity ID, its assertion consumer address), the request
//! the browser carries to the provider (HTTP-Redirect, unsigned), and the
//! verification of the response it posts back. The assertion must be
//! signed by a certificate of the declared metadata (RSA with SHA-2,
//! exclusive canonicalisation), name this application as its audience and
//! this address as its recipient, answer the request awaited, and be
//! within its validity; every assertion it carries is reported for replay.

use std::time::SystemTime;

use saml_rs::{
    AcsEndpoint, AssertionSignaturePolicy, AudienceValidationPolicy, AuthnRequestSigningPolicy,
    BrowserInput, EntityId, FormField, ReplayCache, ReplayKey, ReplayPolicy,
    ResponseSignaturePolicy, Saml, SamlError, SamlValidationContext, SpConfig, SpValidationPolicy,
    SsoResponse, SsoSession, StartSso, XmlSignatureProfile,
};

use super::descriptor::idp_descriptor;
use super::pending::SamlPending;

/// This application at a SAML provider.
pub struct ServiceProvider<'a> {
    /// The audience the provider's assertions name (stored with the
    /// provider as its client).
    pub entity_id: &'a str,
    /// Where the provider posts its response.
    pub acs_url: &'a str,
}

/// A request to send the browser with, and what its answer must match.
pub struct SamlRequest {
    pub url: String,
    pub pending: SamlPending,
}

fn sp(application: &ServiceProvider<'_>) -> Result<Saml<saml_rs::Sp>, String> {
    let fail = |e: SamlError| format!("SAML : application mal décrite ({e})");
    let policy = SpValidationPolicy {
        assertions: AssertionSignaturePolicy::RequireSigned,
        responses: ResponseSignaturePolicy::RequireForEncryptedCbc,
        xml_signatures: XmlSignatureProfile::StrictRsaSha2,
        authn_requests: AuthnRequestSigningPolicy::DoNotSignForCompatibility,
        audience: AudienceValidationPolicy::Validate,
        ..SpValidationPolicy::recommended()
    };
    let config = SpConfig::builder(EntityId::try_new(application.entity_id).map_err(fail)?)
        .acs_endpoint(AcsEndpoint::post(application.acs_url).map_err(fail)?)
        .validation(policy)
        .build()
        .map_err(fail)?;
    Saml::sp(config).map_err(fail)
}

/// This application's metadata, for the provider to import: its entity ID
/// and where responses are posted (failure in French).
pub fn sp_metadata(application: &ServiceProvider<'_>) -> Result<String, String> {
    Ok(sp(application)?.metadata_xml().to_string())
}

/// The request to the provider of this metadata (failure in French).
pub fn authn_request(
    application: &ServiceProvider<'_>,
    metadata: &str,
) -> Result<SamlRequest, String> {
    let idp = idp_descriptor(metadata)?;
    let started = sp(application)?
        .start_sso(&idp, StartSso::redirect())
        .map_err(|e| format!("SAML : demande impossible ({e})"))?;
    let url = started
        .outbound
        .redirect_url()
        .map_err(|e| format!("SAML : demande impossible ({e})"))?
        .to_string();
    Ok(SamlRequest {
        url,
        pending: SamlPending::keep(&started.pending),
    })
}

/// An assertion it carried: its identifier and when it expires.
#[derive(Debug, Clone)]
pub struct Carried {
    pub assertion_id: String,
    pub expires_at: SystemTime,
}

/// Notes the assertions instead of judging them: the server records them
/// all in the company's tenant, where every server sees them.
#[derive(Default)]
struct Noted(Vec<Carried>);

impl ReplayCache for Noted {
    fn check_and_store(&mut self, key: ReplayKey, expires_at: SystemTime) -> Result<(), SamlError> {
        if let ReplayKey::AssertionId(id) = &key {
            self.0.push(Carried {
                assertion_id: id.as_str().to_string(),
                expires_at,
            });
        }
        Ok(())
    }
}

/// The verified session and every assertion it came with, or why the
/// response is refused (in French, for the audit: never the response
/// itself).
pub fn verify_response(
    application: &ServiceProvider<'_>,
    metadata: &str,
    pending: &SamlPending,
    fields: Vec<(String, String)>,
    now: SystemTime,
) -> Result<(SsoSession, Vec<Carried>), String> {
    let idp = idp_descriptor(metadata)?;
    let pending = pending.restore()?;
    let input = BrowserInput::<SsoResponse>::post(
        fields
            .into_iter()
            .map(|(name, value)| FormField::new(name, value))
            .collect(),
    );
    let mut noted = Noted::default();
    let validation = SamlValidationContext::new(now, ReplayPolicy::RequireCache(&mut noted));
    let session = sp(application)?
        .finish_sso(&idp, &pending, input, validation)
        .map_err(refusal)?;
    if noted.0.is_empty() {
        return Err("SAML : réponse sans assertion".into());
    }
    Ok((session, noted.0))
}

/// The library's verdict, bounded: its words name the check that failed,
/// never the response's content.
fn refusal(error: SamlError) -> String {
    let text: String = error.to_string().chars().take(200).collect();
    format!("SAML : réponse refusée ({text})")
}

#[cfg(test)]
mod tests {
    use saml_rs::{AssertionId, MessageId};

    use super::*;

    #[test]
    fn every_assertion_of_a_response_is_noted() {
        let mut noted = Noted::default();
        let at = SystemTime::UNIX_EPOCH;
        let assertion = |id: &str| ReplayKey::AssertionId(AssertionId::try_new(id).unwrap());
        noted.check_and_store(assertion("_a1"), at).unwrap();
        let response = ReplayKey::ResponseId(MessageId::try_new("_r1").unwrap());
        noted.check_and_store(response, at).unwrap();
        noted.check_and_store(assertion("_a2"), at).unwrap();
        let ids: Vec<&str> = noted.0.iter().map(|c| c.assertion_id.as_str()).collect();
        assert_eq!(ids, ["_a1", "_a2"]);
    }
}
