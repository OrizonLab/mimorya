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

/// The test Bot Framework's key (PKCS#8).
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
/// A key no one publishes (PKCS#8).
const FORGER_KEY: &str = concat!(
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
