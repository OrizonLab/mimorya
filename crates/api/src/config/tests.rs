use std::collections::HashMap;

use super::*;

fn load(vars: &[(&str, &str)]) -> anyhow::Result<AppConfig> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    AppConfig::from_vars(|key| map.get(key).cloned())
}

const DB: (&str, &str) = ("DATABASE_APP_URL", "postgres://app:app@localhost/mimorya");

#[test]
fn applies_defaults_and_infers_the_anthropic_provider() {
    let config = load(&[DB, ("ANTHROPIC_API_KEY", "sk-test")]).unwrap();
    assert_eq!(config.port, 4000);
    assert_eq!(config.web_origin, "http://localhost:3000");
    assert!(!config.secure_cookies);
    let llm = config.llm.unwrap();
    assert_eq!(llm.kind, "anthropic");
    assert_eq!(llm.api_key.as_deref(), Some("sk-test"));

    let without = load(&[DB, ("LLM_PROVIDER", "")]).unwrap();
    assert!(without.llm.is_none());
}

#[test]
fn rejects_invalid_values_with_every_issue() {
    let error = load(&[("PORT", "abc"), ("LLM_PROVIDER", "unknown")])
        .unwrap_err()
        .to_string();
    assert!(error.contains("DATABASE_APP_URL"));
    assert!(error.contains("PORT"));
    assert!(error.contains("LLM_PROVIDER"));
}

#[test]
fn bounds_the_mcp_start_timeout() {
    let url = ("DATABASE_APP_URL", "postgres://app@localhost/mimorya");
    let key = "MIMORYA_MCP_START_TIMEOUT_SECS";
    assert_eq!(
        load(&[url]).unwrap().mcp_start_timeout,
        Duration::from_secs(20)
    );
    let five = load(&[url, (key, "5")]).unwrap();
    assert_eq!(five.mcp_start_timeout, Duration::from_secs(5));
    for refused in ["0", "121", "x"] {
        assert!(load(&[url, (key, refused)]).is_err(), "{refused}");
    }
}

#[test]
fn reads_the_mail_relay_in_full_or_not_at_all() {
    let url = ("DATABASE_APP_URL", "postgres://app@localhost/mimorya");
    assert!(load(&[url]).unwrap().mail.is_none());
    let relay = ("SMTP_URL", "smtp://localhost:1025");
    let from = ("MAIL_FROM", "Mimorya <no-reply@exemple.fr>");
    assert!(load(&[url, relay, from]).unwrap().mail.is_some());
    for partial in [relay, from] {
        assert!(load(&[url, partial]).is_err());
    }
}

#[test]
fn never_prints_credentials_in_debug_output() {
    let config = load(&[
        (
            "DATABASE_APP_URL",
            "postgres://app:s3cret-db@localhost/mimorya",
        ),
        ("ANTHROPIC_API_KEY", "sk-s3cret"),
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("SMTP_URL", "smtps://mailer:s3cret-smtp@smtp.exemple.fr"),
        ("MAIL_FROM", "Mimorya <no-reply@exemple.fr>"),
    ])
    .unwrap();
    let printed = format!("{config:?}");
    assert!(!printed.contains("s3cret"), "{printed}");
    assert!(!printed.contains(MASTER_KEY), "{printed}");
}

/// Base64 of 32 bytes.
const MASTER_KEY: &str = crate::secrets::PUBLIC_TEST_MASTER_KEYS[0];

#[test]
fn reads_the_master_key_and_the_listed_endpoints() {
    let config = load(&[
        DB,
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        (
            "MIMORYA_TENANT_LLM_ENDPOINTS",
            " http://ollama.interne:11434/v1, https://llm.interne/v1 ",
        ),
    ])
    .unwrap();
    assert!(config.secrets.is_some());
    assert_eq!(
        config.tenant_llm_endpoints,
        ["http://ollama.interne:11434/v1", "https://llm.interne/v1"]
    );
    let without = load(&[DB]).unwrap();
    assert!(without.secrets.is_none() && without.tenant_llm_endpoints.is_empty());
    let error = load(&[
        DB,
        ("MIMORYA_MASTER_KEY", "trop-court"),
        ("MIMORYA_TENANT_LLM_ENDPOINTS", "ftp://x"),
    ])
    .unwrap_err()
    .to_string();
    assert!(error.contains("MIMORYA_MASTER_KEY"));
    assert!(error.contains("MIMORYA_TENANT_LLM_ENDPOINTS"));
}

#[test]
fn secures_cookies_in_production() {
    let config = load(&[DB, ("MIMORYA_ENV", "production")]).unwrap();
    assert!(config.secure_cookies);
}

#[test]
fn refuses_the_tests_public_master_keys_in_production() {
    for key in crate::secrets::PUBLIC_TEST_MASTER_KEYS {
        let error = load(&[
            DB,
            ("MIMORYA_ENV", "production"),
            ("MIMORYA_MASTER_KEY", key),
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("MIMORYA_MASTER_KEY"), "{error}");
        assert!(!error.contains(key), "{error}");
        // Surrounding spaces, which the cipher ignores, do not hide them.
        let padded = format!(" {key}\n");
        let padded = [
            DB,
            ("MIMORYA_ENV", "production"),
            ("MIMORYA_MASTER_KEY", &padded),
        ];
        assert!(load(&padded).is_err());
        // Outside production, the tests use them.
        assert!(
            load(&[DB, ("MIMORYA_MASTER_KEY", key)])
                .unwrap()
                .secrets
                .is_some()
        );
    }
}

/// The end-to-end tests' key is one of the refused ones: a key copied from
/// their configuration does not start a production server.
#[test]
fn the_end_to_end_tests_use_a_refused_key() {
    let config = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/playwright.config.ts");
    let config = std::fs::read_to_string(config).expect("playwright.config.ts");
    let line = config
        .lines()
        .find(|line| line.contains("MIMORYA_MASTER_KEY:"))
        .expect("the e2e server's master key");
    assert!(
        crate::secrets::PUBLIC_TEST_MASTER_KEYS
            .iter()
            .any(|key| line.contains(key)),
        "{line}"
    );
}
