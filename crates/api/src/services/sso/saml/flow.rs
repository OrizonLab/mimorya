//! A SAML sign-in through `saml-rs` (ADR-068): the application as service
//! provider (its entity ID, its assertion consumer address), the request
//! the browser carries to the provider (HTTP-Redirect, unsigned), and the
//! verification of the response it posts back. The assertion must be
//! signed by a certificate of the declared metadata (RSA with SHA-2,
//! exclusive canonicalisation), name this application as its audience and
//! this address as its recipient, answer the request awaited, and be
//! within its validity; the assertion it carries is reported for replay.

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

/// The assertion it carried: its identifier and when it expires.
#[derive(Debug, Clone)]
pub struct Carried {
    pub assertion_id: String,
    pub expires_at: SystemTime,
}

/// Notes the assertion instead of judging it: the server records it in the
/// company's tenant, where every server sees it.
#[derive(Default)]
struct Noted(Option<Carried>);

impl ReplayCache for Noted {
    fn check_and_store(&mut self, key: ReplayKey, expires_at: SystemTime) -> Result<(), SamlError> {
        if let ReplayKey::AssertionId(id) = &key {
            self.0 = Some(Carried {
                assertion_id: id.as_str().to_string(),
                expires_at,
            });
        }
        Ok(())
    }
}

/// The verified session and the assertion it came in, or why the response
/// is refused (in French, for the audit: never the response itself).
pub fn verify_response(
    application: &ServiceProvider<'_>,
    metadata: &str,
    pending: &SamlPending,
    fields: Vec<(String, String)>,
    now: SystemTime,
) -> Result<(SsoSession, Carried), String> {
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
    let carried = noted.0.ok_or("SAML : réponse sans assertion")?;
    Ok((session, carried))
}

/// The library's verdict, bounded: its words name the check that failed,
/// never the response's content.
fn refusal(error: SamlError) -> String {
    let text: String = error.to_string().chars().take(200).collect();
    format!("SAML : réponse refusée ({text})")
}
