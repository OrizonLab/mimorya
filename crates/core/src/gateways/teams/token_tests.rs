use base64::engine::general_purpose::STANDARD;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::json;

use super::*;

include!("test_keys.in");

const APP: &str = "6f1c2b7a-0d3e-4c5b-9a8f-7e6d5c4b3a21";
const SERVICE: &str = "https://smba.trafficmanager.net/emea/";
const NOW: i64 = 1_800_000_000;

fn pair(key: &str) -> RsaKeyPair {
    RsaKeyPair::from_pkcs8(&STANDARD.decode(key).unwrap()).unwrap()
}

/// The key `key` as the Bot Framework publishes it, as `kid`.
fn published(key: &str, kid: &str, endorsements: &[&str]) -> BotKey {
    let public: RsaPublicKeyComponents<Vec<u8>> = pair(key).public().into();
    BotKey {
        kid: kid.into(),
        kty: "RSA".into(),
        n: URL_SAFE_NO_PAD.encode(&public.n),
        e: URL_SAFE_NO_PAD.encode(&public.e),
        endorsements: endorsements.iter().map(|e| e.to_string()).collect(),
    }
}

/// `claims` signed with `key` as `kid`.
fn signed(key: &str, kid: &str, claims: &Value) -> String {
    let header =
        URL_SAFE_NO_PAD.encode(json!({ "alg": "RS256", "kid": kid, "typ": "JWT" }).to_string());
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    let input = format!("{header}.{payload}");
    let pair = pair(key);
    let mut signature = vec![0; pair.public().modulus_len()];
    pair.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .unwrap();
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature))
}

fn claims() -> Value {
    json!({
        "iss": BOT_FRAMEWORK_ISSUER,
        "aud": APP,
        "exp": NOW + 3600,
        "nbf": NOW - 60,
        "serviceurl": SERVICE,
    })
}

fn expected() -> Expected<'static> {
    Expected {
        app_id: APP,
        service_url: SERVICE,
        now: NOW,
    }
}

fn keys() -> Vec<BotKey> {
    vec![published(KEY, "k1", &["msteams", "skype"])]
}

#[test]
fn a_token_of_the_bot_framework_for_the_bot_and_its_service_proves_the_activity() {
    let token = signed(KEY, "k1", &claims());
    assert_eq!(verify_token(&token, &keys(), expected()), Ok(()));
    assert_eq!(key_id(&token).as_deref(), Some("k1"));
    // Its audience among others, the service written otherwise.
    let mut other = claims();
    other["aud"] = json!(["autre", APP]);
    other["serviceurl"] = json!("https://SMBA.trafficmanager.net/emea/");
    assert_eq!(
        verify_token(&signed(KEY, "k1", &other), &keys(), expected()),
        Ok(())
    );
    // Within five minutes of its validity.
    let mut late = claims();
    late["exp"] = json!(NOW - MAX_SKEW_SECONDS);
    assert_eq!(
        verify_token(&signed(KEY, "k1", &late), &keys(), expected()),
        Ok(())
    );
}

#[test]
fn a_token_signed_otherwise_proves_nothing() {
    // Another key, under a published key's name.
    let forged = signed(FORGER_KEY, "k1", &claims());
    assert_eq!(
        verify_token(&forged, &keys(), expected()),
        Err(TokenProblem::BadSignature)
    );
    // A key the Bot Framework does not publish.
    let unknown = signed(KEY, "k2", &claims());
    assert_eq!(
        verify_token(&unknown, &keys(), expected()),
        Err(TokenProblem::UnknownKey)
    );
    // A key that does not sign for Teams.
    let skype = vec![published(KEY, "k1", &["skype"])];
    let token = signed(KEY, "k1", &claims());
    assert_eq!(
        verify_token(&token, &skype, expected()),
        Err(TokenProblem::NotEndorsed)
    );
    // Its claims changed once signed.
    let (head, rest) = token.split_once('.').unwrap();
    let (_, signature) = rest.split_once('.').unwrap();
    let mut claims = claims();
    claims["aud"] = json!("autre");
    let tampered = format!(
        "{head}.{}.{signature}",
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    assert_eq!(
        verify_token(&tampered, &keys(), expected()),
        Err(TokenProblem::BadSignature)
    );
    // Not RS256, or not a token.
    let none = format!(
        "{}.{}.",
        URL_SAFE_NO_PAD.encode(json!({ "alg": "none", "kid": "k1" }).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    assert_eq!(
        verify_token(&none, &keys(), expected()),
        Err(TokenProblem::Malformed)
    );
    for token in ["", "a.b", "a.b.c.d", "é.é.é"] {
        assert_eq!(
            verify_token(token, &keys(), expected()),
            Err(TokenProblem::Malformed)
        );
    }
}

#[test]
fn a_token_for_another_bot_service_or_time_proves_nothing() {
    let refused = |change: &dyn Fn(&mut Value)| {
        let mut claims = claims();
        change(&mut claims);
        verify_token(&signed(KEY, "k1", &claims), &keys(), expected())
    };
    assert_eq!(
        refused(&|c| c["iss"] = json!("https://sts.windows.net/autre/")),
        Err(TokenProblem::WrongIssuer)
    );
    assert_eq!(
        refused(&|c| c["aud"] = json!("autre")),
        Err(TokenProblem::WrongAudience)
    );
    assert_eq!(
        refused(&|c| c["aud"] = json!([])),
        Err(TokenProblem::WrongAudience)
    );
    assert_eq!(
        refused(&|c| c["exp"] = json!(NOW - MAX_SKEW_SECONDS - 1)),
        Err(TokenProblem::Expired)
    );
    assert_eq!(
        refused(&|c| {
            c.as_object_mut().unwrap().remove("exp");
        }),
        Err(TokenProblem::Expired)
    );
    assert_eq!(
        refused(&|c| c["nbf"] = json!(NOW + MAX_SKEW_SECONDS + 1)),
        Err(TokenProblem::NotYetValid)
    );
    assert_eq!(
        refused(&|c| c["serviceurl"] = json!("https://ailleurs.example/")),
        Err(TokenProblem::OtherService)
    );
    assert_eq!(
        refused(&|c| {
            c.as_object_mut().unwrap().remove("serviceurl");
        }),
        Err(TokenProblem::OtherService)
    );
}

#[test]
fn only_a_bearer_token_is_read() {
    assert_eq!(bearer(Some("Bearer abc.def.ghi")), Some("abc.def.ghi"));
    assert_eq!(bearer(Some("bearer  abc ")), Some("abc"));
    for header in [
        None,
        Some(""),
        Some("Basic abc"),
        Some("Bearer "),
        Some("abc"),
    ] {
        assert_eq!(bearer(header), None, "{header:?}");
    }
    let long = format!("Bearer {}", "a".repeat(MAX_TOKEN_BYTES + 1));
    assert_eq!(bearer(Some(&long)), None);
}
