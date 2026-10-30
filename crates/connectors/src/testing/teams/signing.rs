//! How the test Microsoft signs what the Bot Framework sends a bot
//! (ADR-138): a JSON Web Token (RS256) with its test key, published in its
//! key set, endorsed for Teams; or with another key, never published, as a
//! forger would. Test keys only, never anything else's.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair, RsaPublicKeyComponents};
use serde_json::{Value, json};

/// The name of the key the test Bot Framework signs with.
pub const TEAMS_KEY_ID: &str = "cle-test-bot-framework";

include!("../../../../core/src/gateways/teams/test_keys.in");

/// Which key signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signer {
    /// The test Bot Framework, with its published key.
    BotFramework,
    /// Someone else, naming the published key.
    Forger,
}

fn pair(signer: Signer) -> RsaKeyPair {
    let key = match signer {
        Signer::BotFramework => KEY,
        Signer::Forger => FORGER_KEY,
    };
    let der = STANDARD.decode(key).expect("a test key");
    RsaKeyPair::from_pkcs8(&der).expect("a test key")
}

/// The key set the test Bot Framework publishes: its key, endorsed for
/// `endorsements`.
pub(super) fn key_set(endorsements: &[String]) -> Value {
    let public: RsaPublicKeyComponents<Vec<u8>> = pair(Signer::BotFramework).public().into();
    json!({ "keys": [{
        "kty": "RSA",
        "use": "sig",
        "kid": TEAMS_KEY_ID,
        "n": URL_SAFE_NO_PAD.encode(&public.n),
        "e": URL_SAFE_NO_PAD.encode(&public.e),
        "endorsements": endorsements,
    }] })
}

/// `claims` signed by `signer`, naming the published key.
pub fn signed_token(signer: Signer, claims: &Value) -> String {
    let header = json!({ "alg": "RS256", "kid": TEAMS_KEY_ID, "typ": "JWT" });
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let pair = pair(signer);
    let mut signature = vec![0; pair.public().modulus_len()];
    pair.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .expect("signed");
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature))
}
