//! A SAML 2.0 provider for the tests (ADR-068), through `saml-rs`'s own
//! identity provider: its metadata (entity ID, HTTP-Redirect sign-in
//! address, signing certificate), and the browser's visit, played
//! in-process: it reads the request the address carries and signs in the
//! person the test chose, answering with the response the browser posts
//! back. Its keys are test keys, public; an impostor signs with others
//! under the same entity ID, and a lax one signs with an inclusive
//! canonicalisation, which the application's strict profile refuses. The
//! end-to-end server serves it at an address of its own (`at`).

use saml_rs::constants::{Binding, transform_algorithm};
use saml_rs::metadata::{Endpoint, IdpMetadataConfig};
use saml_rs::raw::{
    BindingContext, EntitySetting, HttpRequest, IdentityProvider, LoginResponseOptions,
    ServiceProvider, User,
};
use saml_rs::template::{LoginResponseAttribute, LoginResponseTemplate};
use url::Url;

pub const SAML_ENTITY_ID: &str = "https://idp.saml.example/metadata";
pub const SAML_SIGN_IN_URL: &str = "https://idp.saml.example/sso";
const KEY: &str = include_str!("saml_idp_key.pem");
const CERT: &str = include_str!("saml_idp_cert.pem");
const IMPOSTOR_KEY: &str = include_str!("saml_impostor_key.pem");
const IMPOSTOR_CERT: &str = include_str!("saml_impostor_cert.pem");

/// Attributes the provider may send, under Microsoft's names.
pub const OBJECT_ID: &str = "http://schemas.microsoft.com/identity/claims/objectidentifier";
pub const EMAIL: &str = "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress";
pub const DISPLAY_NAME: &str = "http://schemas.microsoft.com/identity/claims/displayname";

/// Who signs in at the provider.
#[derive(Debug, Clone)]
pub struct SamlPerson {
    pub name_id: String,
    /// Attribute name and value.
    pub attributes: Vec<(&'static str, String)>,
}

impl SamlPerson {
    /// Its address as its NameID, as Entra ID sends its sign-in name.
    pub fn named(email: &str) -> Self {
        Self {
            name_id: email.into(),
            attributes: Vec::new(),
        }
    }

    pub fn with(mut self, name: &'static str, value: &str) -> Self {
        self.attributes.push((name, value.into()));
        self
    }
}

/// Inclusive canonicalisation, which the strict profile refuses.
const INCLUSIVE_C14N: &str = "http://www.w3.org/TR/2001/REC-xml-c14n-20010315";

pub struct MockSamlProvider {
    entity_id: String,
    sign_in_url: String,
    key: &'static str,
    cert: &'static str,
    canonicalisation: &'static str,
}

impl MockSamlProvider {
    pub fn new() -> Self {
        Self {
            entity_id: SAML_ENTITY_ID.into(),
            sign_in_url: SAML_SIGN_IN_URL.into(),
            key: KEY,
            cert: CERT,
            canonicalisation: transform_algorithm::EXC_C14N,
        }
    }

    /// Served under `base`: its metadata at `/metadata`, its sign-in at
    /// `/sso`.
    pub fn at(base: &str) -> Self {
        Self {
            entity_id: format!("{base}/metadata"),
            sign_in_url: format!("{base}/sso"),
            ..Self::new()
        }
    }

    /// The same entity ID, other keys.
    pub fn impostor() -> Self {
        Self {
            key: IMPOSTOR_KEY,
            cert: IMPOSTOR_CERT,
            ..Self::new()
        }
    }

    /// The same provider, its signatures canonicalised inclusively.
    pub fn lax() -> Self {
        Self {
            canonicalisation: INCLUSIVE_C14N,
            ..Self::new()
        }
    }

    fn idp(&self, person: Option<&SamlPerson>) -> IdentityProvider {
        let mut setting = EntitySetting::default();
        setting.private_key = Some(self.key.into());
        setting.signing_cert = Some(self.cert.into());
        setting.request_signature_algorithm =
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256".into();
        if let Some(person) = person {
            setting.login_response_template = Some(LoginResponseTemplate {
                context: None,
                attributes: person
                    .attributes
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| LoginResponseAttribute {
                        name: (*name).into(),
                        name_format: "urn:oasis:names:tc:SAML:2.0:attrname-format:uri".into(),
                        value_xsi_type: "xs:string".into(),
                        value_tag: format!("value{i}"),
                        value_xmlns_xs: None,
                        value_xmlns_xsi: None,
                    })
                    .collect(),
            });
        }
        IdentityProvider::from_config(
            &IdpMetadataConfig {
                entity_id: self.entity_id.clone(),
                signing_certs: vec![self.cert.into()],
                single_sign_on_service: vec![Endpoint::new(Binding::Redirect, &self.sign_in_url)],
                ..Default::default()
            },
            setting,
        )
        .expect("test provider")
    }

    /// The metadata a company declares.
    pub fn metadata(&self) -> String {
        self.idp(None).metadata.get_metadata().to_string()
    }

    /// The browser's visit to `address`: the request it carries answered
    /// for `person`, as the `SAMLResponse` the browser posts back to the
    /// application whose metadata is `sp_metadata`.
    pub fn respond(&self, address: &str, sp_metadata: &str, person: &SamlPerson) -> String {
        self.respond_to(address, sp_metadata, sp_metadata, person)
    }

    /// The request read as the application's, the response made for the
    /// application whose metadata is `audience`.
    pub fn respond_to(
        &self,
        address: &str,
        sp_metadata: &str,
        audience: &str,
        person: &SamlPerson,
    ) -> String {
        self.answer(address, sp_metadata, audience, person).context
    }

    /// The response, and where the browser posts it (its destination).
    pub fn answer(
        &self,
        address: &str,
        sp_metadata: &str,
        audience: &str,
        person: &SamlPerson,
    ) -> BindingContext {
        let idp = self.idp(Some(person));
        // The library signs with the transforms it keeps for the
        // application it answers.
        let application = |metadata: &str| {
            let mut setting = EntitySetting::default();
            setting.transformation_algorithms = vec![
                transform_algorithm::ENVELOPED_SIGNATURE.into(),
                self.canonicalisation.into(),
            ];
            ServiceProvider::from_metadata(metadata, setting).expect("application metadata")
        };
        let sp = application(sp_metadata);
        let url = Url::parse(address).expect("sign-in address");
        assert!(address.starts_with(&self.sign_in_url), "{address}");
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let request = idp
            .parse_login_request(&sp, Binding::Redirect, &HttpRequest::redirect(query))
            .expect("request read");
        let request_id = request
            .extract
            .get_str("request.id")
            .expect("request id")
            .to_string();
        let user = User {
            name_id: person.name_id.clone(),
            attributes: person
                .attributes
                .iter()
                .enumerate()
                .map(|(i, (_, value))| (format!("value{i}"), value.clone()))
                .collect(),
            session_index: None,
        };
        idp.create_login_response(
            &application(audience),
            Binding::Post,
            &user,
            &LoginResponseOptions {
                in_response_to: Some(&request_id),
                ..Default::default()
            },
        )
        .expect("response")
    }
}
