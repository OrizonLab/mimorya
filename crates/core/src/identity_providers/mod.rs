//! What a company may declare as an identity provider (ADR-063), and what a
//! provider's identity token must prove before an account is tied to it,
//! or created with the name it gives (ADR-067): the same rules wherever they
//! are checked, without I/O.

mod display_name;

use std::collections::BTreeSet;

use uuid::Uuid;

pub use display_name::federated_display_name;

/// Google's issuer, the same for every Workspace.
pub const GOOGLE_ISSUER: &str = "https://accounts.google.com";
/// Longest name of a provider, in characters.
pub const PROVIDER_NAME_MAX_CHARS: usize = 80;
/// Longest client identifier and secret, in characters.
pub const CLIENT_ID_MAX_CHARS: usize = 255;
pub const CLIENT_SECRET_MAX_CHARS: usize = 1024;
/// Most e-mail domains one provider covers.
pub const MAX_DOMAINS: usize = 20;
const DOMAIN_MAX_LEN: usize = 253;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Microsoft Entra ID (Microsoft 365), one directory.
    Microsoft,
    /// Google Workspace, its domains.
    Google,
    /// Any OpenID Connect provider (Okta, Keycloak…).
    Oidc,
}

impl ProviderKind {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "microsoft" => Some(Self::Microsoft),
            "google" => Some(Self::Google),
            "oidc" => Some(Self::Oidc),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Microsoft => "microsoft",
            Self::Google => "google",
            Self::Oidc => "oidc",
        }
    }

    /// The name shown when the company gives none.
    fn default_name(self) -> Option<&'static str> {
        match self {
            Self::Microsoft => Some("Microsoft 365"),
            Self::Google => Some("Google Workspace"),
            Self::Oidc => None,
        }
    }
}

/// The issuer of a Microsoft Entra ID directory (its tenant ID).
pub fn microsoft_issuer(directory: Uuid) -> String {
    format!("https://login.microsoftonline.com/{directory}/v2.0")
}

/// The issuer a declaration designates, or why it is refused (in French).
/// A generic issuer's address is then checked against the network policy.
pub fn declared_issuer(
    kind: ProviderKind,
    directory_id: Option<&str>,
    issuer: Option<&str>,
) -> Result<String, String> {
    match kind {
        ProviderKind::Microsoft => {
            if issuer.is_some() {
                return Err("Microsoft : l’annuaire suffit, sans adresse d’émetteur".into());
            }
            let directory = directory_id
                .ok_or("Microsoft : indiquez l’identifiant de l’annuaire (ID de locataire)")?;
            Uuid::parse_str(directory)
                .map(microsoft_issuer)
                .map_err(|_| {
                    "Microsoft : l’identifiant de l’annuaire est un GUID \
                     (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)"
                        .into()
                })
        }
        ProviderKind::Google => match (directory_id, issuer) {
            (None, None) => Ok(GOOGLE_ISSUER.into()),
            _ => Err("Google : ni annuaire ni adresse d’émetteur à indiquer".into()),
        },
        ProviderKind::Oidc => {
            if directory_id.is_some() {
                return Err(
                    "OpenID Connect : l’adresse de l’émetteur suffit, sans annuaire".into(),
                );
            }
            let issuer = issuer.ok_or("OpenID Connect : indiquez l’adresse de l’émetteur")?;
            if let Some(kind) = issuer_own_kind(issuer) {
                return Err(format!(
                    "OpenID Connect : cet émetteur est celui de {kind}, choisissez ce type"
                ));
            }
            Ok(issuer.into())
        }
    }
}

/// The kind whose rules an issuer must follow, if it has one: a generic
/// provider proves an address by `email_verified` alone, which a consumer
/// Google account or another Microsoft directory would satisfy; their own
/// kinds check the Workspace domain, or the directory's sign-in name. A
/// generic provider declared with such an issuer before this rule creates
/// no account (ADR-067).
pub fn issuer_own_kind(issuer: &str) -> Option<&'static str> {
    let issuer = issuer.trim_end_matches('/').to_ascii_lowercase();
    // Microsoft's public cloud, and its national ones.
    let microsoft = [
        "https://login.microsoftonline.com/",
        "https://sts.windows.net/",
        "https://login.microsoftonline.us/",
        "https://login.partner.microsoftonline.cn/",
        "https://login.chinacloudapi.cn/",
    ];
    if issuer == GOOGLE_ISSUER {
        Some("Google Workspace")
    } else if microsoft.iter().any(|host| issuer.starts_with(host)) {
        Some("Microsoft 365")
    } else {
        None
    }
}

/// The name the sign-in page shows: the one given, else the kind's.
pub fn declared_name(kind: ProviderKind, name: Option<&str>) -> Result<String, String> {
    let name = match (name, kind.default_name()) {
        (Some(name), _) => name,
        (None, Some(default)) => default,
        (None, None) => return Err("Donnez un nom au fournisseur".into()),
    };
    let chars = name.chars().count();
    if !(2..=PROVIDER_NAME_MAX_CHARS).contains(&chars) || name.chars().any(char::is_control) {
        return Err(format!(
            "Le nom du fournisseur compte de 2 à {PROVIDER_NAME_MAX_CHARS} caractères, \
             sans caractère de contrôle"
        ));
    }
    Ok(name.into())
}

/// Why a client identifier cannot be used, if it cannot.
pub fn client_id_problem(client_id: &str) -> Option<String> {
    let chars = client_id.chars().count();
    (!(1..=CLIENT_ID_MAX_CHARS).contains(&chars)
        || client_id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control()))
    .then(|| {
        format!("L’identifiant client compte de 1 à {CLIENT_ID_MAX_CHARS} caractères, sans espace")
    })
}

/// Why a client secret cannot be used, if it cannot.
pub fn client_secret_problem(secret: &str) -> Option<String> {
    let chars = secret.chars().count();
    (!(1..=CLIENT_SECRET_MAX_CHARS).contains(&chars)
        || secret.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then(|| {
        format!("Le secret client compte de 1 à {CLIENT_SECRET_MAX_CHARS} caractères, sans espace")
    })
}

/// The domains as stored: trimmed, in lower case, once each, in order; or
/// why they are refused.
pub fn normalise_domains(domains: &[String]) -> Result<Vec<String>, String> {
    let set: BTreeSet<String> = domains
        .iter()
        .map(|d| d.trim().to_ascii_lowercase())
        .collect();
    if set.is_empty() || set.len() > MAX_DOMAINS {
        return Err(format!(
            "Indiquez de 1 à {MAX_DOMAINS} domaines d’e-mail (exemple : entreprise.fr)"
        ));
    }
    if let Some(bad) = set.iter().find(|d| !is_domain(d)) {
        return Err(format!(
            "« {bad} » n’est pas un domaine d’e-mail (exemple : entreprise.fr)"
        ));
    }
    Ok(set.into_iter().collect())
}

/// A host name of two labels or more, letters, digits and hyphens (an
/// internationalised one in its `xn--` form).
fn is_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    domain.len() <= DOMAIN_MAX_LEN
        && labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        && labels
            .last()
            .is_some_and(|tld| !tld.chars().all(|c| c.is_ascii_digit()))
}

/// The domain of an address, in lower case.
pub fn email_domain(email: &str) -> Option<String> {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
}

/// What a verified identity token says, for the rules below.
#[derive(Debug, Default, Clone)]
pub struct TokenClaims<'a> {
    pub subject: &'a str,
    pub email: Option<&'a str>,
    pub email_verified: Option<bool>,
    /// Microsoft: the account's sign-in name in its directory (its UPN,
    /// whose domain the directory verified).
    pub preferred_username: Option<&'a str>,
    /// Microsoft: whether the domain of `email` is verified (`xms_edov`).
    pub email_domain_verified: Option<bool>,
    /// Microsoft: the account's immutable object ID in its directory.
    pub oid: Option<&'a str>,
    /// Google: the account's Workspace domain.
    pub hosted_domain: Option<&'a str>,
    /// The person's full name, and its parts, as the provider gives them:
    /// the name of an account created at the first sign-in.
    pub name: Option<&'a str>,
    pub given_name: Option<&'a str>,
    pub family_name: Option<&'a str>,
}

/// Who the provider proves the person is: its immutable subject, and the
/// address it vouches for, if any (in lower case), which alone may tie an
/// account at the first sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenIdentity {
    pub subject: String,
    pub email: Option<String>,
}

/// Refused (with the reason, in French) when the token cannot identify
/// anyone of the provider's domains.
pub fn proven_identity(
    kind: ProviderKind,
    claims: &TokenClaims<'_>,
    domains: &[String],
) -> Result<ProvenIdentity, &'static str> {
    let verified = claims.email_verified == Some(true);
    let (subject, email) = match kind {
        // The sign-in name first: its domain is one the directory
        // verified, which `email` alone is not. The object ID stays when
        // the application changes.
        ProviderKind::Microsoft => (
            claims
                .oid
                .ok_or("Jeton Microsoft sans identifiant d’objet (oid)")?,
            claims
                .preferred_username
                .filter(|u| u.contains('@'))
                .or(claims
                    .email
                    .filter(|_| claims.email_domain_verified == Some(true))),
        ),
        // A Workspace account of a declared domain, never a consumer one.
        ProviderKind::Google => {
            let hosted = claims
                .hosted_domain
                .map(str::to_ascii_lowercase)
                .ok_or("Compte Google hors Google Workspace")?;
            if !domains.contains(&hosted) {
                return Err("Compte Google d’un domaine Workspace non déclaré");
            }
            (claims.subject, claims.email.filter(|_| verified))
        }
        ProviderKind::Oidc => (claims.subject, claims.email.filter(|_| verified)),
    };
    if subject.is_empty() {
        return Err("Jeton sans identifiant de la personne");
    }
    Ok(ProvenIdentity {
        subject: subject.into(),
        email: email.map(str::to_lowercase),
    })
}

#[cfg(test)]
mod tests;
