use base64::engine::general_purpose::STANDARD;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::json;

use super::*;

/// Test keys (PKCS#8), never anything else's.
const KEY: &str = concat!(
    "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCtNU4iGNJqoZJ3HirOIoIPg4mHD2hpOZe9gW5B",
    "qBHRw3nkwvM2cMMrJZu+m5Ad0YASkTNDWLHfGJDNtAODpVS+xpLRreIi7gVeFx4SZHHsWVPn0V8W0go2nDxjaRX+",
    "EL91rdfdqw7mO6ImnSXNk7kDuE4P/IVUMQs9sCcoCY/4hrmiBdCRXUYrDLyPXUmecAgh2soA2OTxnXoG+ygUvUsP",
    "bI2QJkNXkfWfoCrw7xRGY6WvDc0t1AE63Ma/T/SSlgvmEVkwXwj4aiMdSdkzQfCJYPtCr7U8vvE+NVzpp4YOpgFf",
    "Ces0V0cJwDIjfyNDM9MYjdQMZwWyEwTYvAQJ0fi5AgMBAAECggEABE9XACCzfa1DDn087e+sYlPw1GBFBhdFiBTk",
    "KDSVKu3NBuz17dUc+iIfr7zBt4g6wluEFckrdeRv1/qCeuBejTe2OAr2XU2kawyythzzO8EWRiau0cvMrsKw+v1G",
    "q5GBnh1DCZRD1OVHeeP/qI+kMyw/HwU20Rs+TxzmwIeKYlH3VXVVrta17xrfumBtsgyVQ7DopLpAkoiXQzUyDd8u",
    "SW34RdN4BDUDAnZHhiThaPxXvaHY0x8A+3G2yG5/GNwdpW0WZRah0kmjNeiwoX8DbMVUo4bXXzDmt4N9RybBY7CJ",
    "lnOlX+NWmd/8l4q9NzGYXms5sJJaIiimNNij0BqBDQKBgQDufjN6bIpjv/U41gYCFoKUxOAogoiG+PlDVZ/XLzA6",
    "27FAdCgt4YGeQ3qjT0h1aoHf+4Iv++Dn2t6mrnb4FkUdGTjdCXvVr5ZZisbVdSQwij1U0DskUCmAfKuY1Q4Ibv6s",
    "aC28wrvU4Vc5ND5abnypNxOLuOTdY9+fD0xtukRLlQKBgQC57EMvYY262SrFa9hi953PoEwrk+pcCueDSvicbyYi",
    "05QaKUJHRpJyNUmFNdGMALGpImacsJ9fKX0gyeL9SweAyatXWd0NiuxvGoOOzOOjsfEntKqBdM1XQnegvLkdy05v",
    "nSWZJcpOm2Lk/kV+KCR0A4c3cXJFHDJR49VJggNPlQKBgAHSuygDbmdmz0QY9sDLVUI34xLMNKj7fERUgj1N4guB",
    "JHC+spPvNmN0Vpqq/33OS6HftVYSkdyT4sHZcZVh7jNTvwiJiwXOKtMSaYS2OYmZdPCSCvGzHy4ZReKMNmhAGPhO",
    "b5MS1gbBlj4ctSFR2OgeQKr/xdgfbFU187qfAuilAoGAfJa7auvXewd+Ju+fAKroILNNd8JWQTm5bHz33nSJPLtF",
    "usY9OOAs0MWur8qh0kzAwRNhaczd73ow9KD1MEFwFLiPzihU/ndC+yzsTew0+4QJk075F2k7FSdgXOE6t6F+vUK0",
    "aE0vpb9w5P0TenfR/cBDgz7NkUxU2BVx3yF1sIUCgYEAxC2+DiG0vJN/uCvgxIxk2+AFBQp8uYyM8xJphHu+LBa0",
    "z7aKDv+skIMlCnKyhTDQYiK3eYeqZh+IN6MBYrX+TQ3q7901zV50Wkz4LsMP5JErEg6Uv9OcTrXpxYU3ipY/zVgt",
    "Yz1/cQJ2jGWcBoBE4ANmI1T5udAbC8AcrcbUIVo=",
);
const OTHER_KEY: &str = concat!(
    "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCz5TB85aeZYoyqC7hlAWaIavXgHsBbTL3EZjt1",
    "qJBRsBFVz6bZ9bsNays+jLjm9tb+IDGVA379eAtYAWNUpao7eCQofep7h/xfBLI36Mq1PhZhGFWpNdSJ1ZzVGAzJ",
    "xb/ycI0xtFiVKe70CFbcEJDP5OkjmAmvFiu3dKcdXKpmmG27Mq9AeWmXPpEgn1HoT1gkk784ze8eoxtl64/oIn3Y",
    "t/XXxLfKSzUGVBdCf18T97pKdrE/j1euGuYxyeY1cgB7WZLi32my+pY9//eTlC4LcZuC01dEF07NLr3FrS0aV0mr",
    "RaKxNfI4lNhTzufO/Qb0l7atR0meISk0txuQpbDBAgMBAAECggEAVvZnHylhgKt8NkOeKu2nZDDYlTQoZrpNTiDh",
    "g3ot8d88MhBEpz674ru41u7t/3C583Uaot9ixyL90Ngy7Vt8VG06hXyBGqBdJo29nXtiaoUOCnfaCQdlrA/gVb7f",
    "h7aZmJhCGRIGdG0l/z/8xTsY1fbhoPxBSitcjL3Of0opcZhSBvXYTf22+YKxdWGh3zTIYDGKLiTlCPtebwQtvHEh",
    "AZleySHM2I694WwJnFXOfpJw5k+kzy+zvDvO9stcp4dXgflq/6S8ySS2Mif/1AseB3Q2sJFtNdugDETqScmEUto/",
    "4oL2Mcxhbzr3eB2l42hd1f/kHWimUaCMNhhqAXWF1QKBgQD64LNznwncAPBENodT5wR5QkxMT6jVNWcEMfpKRh0g",
    "U7P5GbKn4FWbvGAW349ukjDfNfrBhsj7ZVYxpzo5bCKkvCdadEsItAPRTwid1BXz5/3ZmyaPcyciQY3bTyyD4tPZ",
    "EK3kmQQr8C6pr5cfJ41bZ77FIvbWQnAL1faluKsGCwKBgQC3kXldQ2DCAw5/JH27fs1qVIfdME9L4diI7Oz1gEkm",
    "QkZ2fwV2+nzIa+JQGy/JCdiCCtsVfd0NIsDI2dQtobUNRih93h4bw0LDng3WwPr3TLORgybjNwudx3Dl1bvE9FO8",
    "AWUQTti9uoihy/ko1A4SPCIMH3V8agtDPx6U8Ewf4wKBgE0HFNIZMJPGDf9DpsVSFwOMIZUEmFfuL5UQK0MGyAaW",
    "7Jk9FKeKr35DllUviwBuDmdaUYV99tOJR6+zIqNi2jguNP8G2crwSoPHemPLLHpY7o4TnwFIX+maqw18/7P2vwjU",
    "oC+h86/x7MHY0dOU90frUJ2CoLskZvabWUGEs64FAoGBAIUIaWL8fHe+DVh5EqG54JZJknjrFNySXQHGamf0w451",
    "g2lG3qf+GkaJYOba/QN+HikIQwPNRlDJudsVeh5jzblD+f5V5Y4LMSxhM/hJvNdR/+TZnmoWL1X1/syh28V9p91d",
    "SWfLLGo7b9+J2hULsPLoH2iXt+YlV+oxKTGZACIvAoGBAIRW/Y37Krof5UXqDUO40lfYHBPdoubZnbS+SZAFOkyU",
    "WJro9qJ+Uv0brn2dNZwbkmk5AnP4m0S72CzfOLXfozw50fRZ1H2BG8Ye030LnxKx3QnY58hnctuYeTYWBp3KbYhb",
    "feWzlN7BVcalUVQQA9xCCRjromIXNQbxO5seBI5E",
);
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
    let forged = signed(OTHER_KEY, "k1", &claims());
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
