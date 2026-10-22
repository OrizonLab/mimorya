//! A SAML provider's metadata as a company declares it (ADR-068): read
//! strictly and bounded, it must name the provider (its entity ID), where
//! the browser signs in (HTTP-Redirect) and at least one signing
//! certificate, which alone verify its assertions. The company declares it,
//! as it would type it: it needs no signature of its own.

use mimorya_core::identity_providers::SAML_METADATA_MAX_BYTES;
use saml_rs::raw::Binding;
use saml_rs::{IdpDescriptor, MetadataTrustPolicy};

/// What the declared metadata says, once read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdpSummary {
    pub entity_id: String,
    /// Where the browser goes to sign in (HTTP-Redirect binding).
    pub sign_in_url: String,
}

/// The provider's descriptor, or why the metadata cannot serve (in French).
pub fn idp_descriptor(metadata: &str) -> Result<IdpDescriptor, String> {
    if metadata.len() > SAML_METADATA_MAX_BYTES {
        return Err(format!(
            "SAML : métadonnées de plus de {} Kio",
            SAML_METADATA_MAX_BYTES / 1024
        ));
    }
    IdpDescriptor::from_metadata_xml(metadata, MetadataTrustPolicy::UnsignedForCompatibility)
        .map_err(|e| format!("SAML : métadonnées illisibles ({e})"))
}

/// What the metadata declares, or why it cannot serve (in French).
pub fn summary(metadata: &str) -> Result<IdpSummary, String> {
    let descriptor = idp_descriptor(metadata)?;
    let read = descriptor.metadata();
    let sign_in_url = read
        .get_single_sign_on_service(Binding::Redirect)
        .ok_or("SAML : métadonnées sans adresse de connexion (liaison HTTP-Redirect)")?;
    if read
        .x509_certificates(saml_rs::constants::CertUse::Signing)
        .is_empty()
    {
        return Err("SAML : métadonnées sans certificat de signature".into());
    }
    Ok(IdpSummary {
        entity_id: descriptor.entity_id().as_str().to_string(),
        sign_in_url,
    })
}
