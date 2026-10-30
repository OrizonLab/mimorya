//! How an activity the Bot Framework sends a company's Teams bot proves it
//! comes from Microsoft's channel (ADR-138): a JSON Web Token in its
//! `Authorization` header, signed (RS256) with one of the keys the Bot
//! Framework publishes, endorsed for Teams (`msteams`), issued by the Bot
//! Framework, for the bot's application (its audience), valid now within
//! five minutes, and naming the service the activity came from, the one
//! the bot writes back to (`serviceurl`). Anything else proves nothing.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;
use serde_json::Value;

use super::{BOT_FRAMEWORK_ISSUER, CHANNEL};

/// The clock difference allowed with Microsoft's.
pub const MAX_SKEW_SECONDS: i64 = 300;
/// The longest token read: the Bot Framework's are about a kibibyte.
const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// A key the Bot Framework signs with, as its key set publishes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BotKey {
    pub kid: String,
    pub kty: String,
    /// Its modulus and exponent, base64url-encoded.
    pub n: String,
    pub e: String,
    /// The channels it signs for.
    #[serde(default)]
    pub endorsements: Vec<String>,
}

/// What a token must say of an activity.
#[derive(Debug, Clone, Copy)]
pub struct Expected<'a> {
    /// The bot's application (its Microsoft App ID): the token's audience.
    pub app_id: &'a str,
    /// The service the activity names: the token's `serviceurl`.
    pub service_url: &'a str,
    /// Now, in seconds since the epoch.
    pub now: i64,
}

/// Why a token proves nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenProblem {
    /// Not a bearer token, not a JWT, too long, or not RS256.
    Malformed,
    /// Signed with a key the Bot Framework does not publish (now).
    UnknownKey,
    /// A key that does not sign for Teams.
    NotEndorsed,
    BadSignature,
    WrongIssuer,
    WrongAudience,
    Expired,
    NotYetValid,
    /// Issued for another service than the activity names.
    OtherService,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// The token of an `Authorization` header (`Bearer …`), if it carries one.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    let value = header?.trim();
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && token.len() <= MAX_TOKEN_BYTES)
        .then_some(token)
}

/// The key a token says it is signed with, to know whether the published
/// keys must be read again.
pub fn key_id(token: &str) -> Option<String> {
    let header = token.split('.').next()?;
    let header: Header = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).ok()?).ok()?;
    header.kid
}

/// Whether `token` proves an activity of the Teams channel for `expected`,
/// signed with one of `keys`; why not otherwise.
pub fn verify_token(token: &str, keys: &[BotKey], expected: Expected) -> Result<(), TokenProblem> {
    if token.len() > MAX_TOKEN_BYTES {
        return Err(TokenProblem::Malformed);
    }
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(TokenProblem::Malformed);
    };
    let decoded = |part: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|_| TokenProblem::Malformed)
    };
    let header: Header =
        serde_json::from_slice(&decoded(header)?).map_err(|_| TokenProblem::Malformed)?;
    if header.alg != "RS256" {
        return Err(TokenProblem::Malformed);
    }
    let key = (keys.iter())
        .find(|key| key.kty == "RSA" && Some(&key.kid) == header.kid.as_ref())
        .ok_or(TokenProblem::UnknownKey)?;
    if !key.endorsements.iter().any(|channel| channel == CHANNEL) {
        return Err(TokenProblem::NotEndorsed);
    }
    let public = RsaPublicKeyComponents {
        n: decoded(&key.n)?,
        e: decoded(&key.e)?,
    };
    let signed = &token[..token.len() - signature.len() - 1];
    public
        .verify(
            &RSA_PKCS1_2048_8192_SHA256,
            signed.as_bytes(),
            &decoded(signature)?,
        )
        .map_err(|_| TokenProblem::BadSignature)?;
    let claims: Value =
        serde_json::from_slice(&decoded(payload)?).map_err(|_| TokenProblem::Malformed)?;
    claims_hold(&claims, expected)
}

/// Whether the signed `claims` say what an activity's token must.
fn claims_hold(claims: &Value, expected: Expected) -> Result<(), TokenProblem> {
    if claims.get("iss").and_then(Value::as_str) != Some(BOT_FRAMEWORK_ISSUER) {
        return Err(TokenProblem::WrongIssuer);
    }
    let audience = match claims.get("aud") {
        Some(Value::String(audience)) => audience == expected.app_id,
        Some(Value::Array(audiences)) => audiences
            .iter()
            .any(|audience| audience.as_str() == Some(expected.app_id)),
        _ => false,
    };
    if !audience {
        return Err(TokenProblem::WrongAudience);
    }
    let time = |name: &str| claims.get(name).and_then(Value::as_i64);
    match time("exp") {
        Some(expires) if expected.now <= expires + MAX_SKEW_SECONDS => {}
        _ => return Err(TokenProblem::Expired),
    }
    if time("nbf").is_some_and(|starts| expected.now < starts - MAX_SKEW_SECONDS) {
        return Err(TokenProblem::NotYetValid);
    }
    let service = claims.get("serviceurl").and_then(Value::as_str);
    if !service.is_some_and(|service| service.eq_ignore_ascii_case(expected.service_url)) {
        return Err(TokenProblem::OtherService);
    }
    Ok(())
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
