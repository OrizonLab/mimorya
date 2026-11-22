use std::collections::HashMap;

use mimorya_agent::llm::anthropic::DEFAULT_BASE_URL as OFFICIAL;

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
    // The tests' simulated servers are never read from the environment.
    assert!(config.mcp_network.test_servers.is_empty());
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
fn bounds_the_mcp_start_patience() {
    let url = ("DATABASE_APP_URL", "postgres://app@localhost/mimorya");
    let key = "MIMORYA_MCP_START_PATIENCE_SECS";
    assert_eq!(
        load(&[url]).unwrap().mcp_start_patience,
        Duration::from_secs(5)
    );
    // 0: a run never waits for a connector still starting.
    let none = load(&[url, (key, "0")]).unwrap();
    assert_eq!(none.mcp_start_patience, Duration::ZERO);
    for refused in ["121", "-1", "x"] {
        assert!(load(&[url, (key, refused)]).is_err(), "{refused}");
    }
}

#[test]
fn finds_the_document_reader_and_bounds_the_quota() {
    let config = load(&[DB]).unwrap();
    assert_eq!(
        config.document_extractor.file_name().unwrap(),
        "mimorya-extract"
    );
    assert_eq!(config.document_quota_bytes, 1024 * 1024 * 1024);
    let set = load(&[
        DB,
        ("MIMORYA_EXTRACTOR", "/opt/mimorya/mimorya-extract"),
        ("MIMORYA_DOCUMENT_QUOTA_MB", "5"),
    ])
    .unwrap();
    assert_eq!(
        set.document_extractor,
        PathBuf::from("/opt/mimorya/mimorya-extract")
    );
    assert_eq!(set.document_quota_bytes, 5 * 1024 * 1024);
    for refused in ["0", "1048577", "x"] {
        let quota = ("MIMORYA_DOCUMENT_QUOTA_MB", refused);
        assert!(load(&[DB, quota]).is_err(), "{refused}");
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
        ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
        (
            "MIMORYA_OUTBOUND_PROXY",
            "http://proxy:s3cret-proxy@proxy.exemple.fr:3128",
        ),
    ])
    .unwrap();
    assert!(config.mcp_network.proxy.is_some());
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

#[test]
fn reads_what_the_process_does() {
    assert_eq!(load(&[DB]).unwrap().role, ProcessRole::All);
    for (value, role) in [
        ("all", ProcessRole::All),
        ("api", ProcessRole::Api),
        ("worker", ProcessRole::Worker),
    ] {
        assert_eq!(load(&[DB, ("MIMORYA_ROLE", value)]).unwrap().role, role);
    }
    let refused = load(&[DB, ("MIMORYA_ROLE", "both")]).unwrap_err();
    assert!(refused.to_string().contains("MIMORYA_ROLE"));
    assert!(ProcessRole::All.serves_http() && ProcessRole::All.executes_runs());
    assert!(ProcessRole::Api.serves_http() && !ProcessRole::Api.executes_runs());
    assert!(!ProcessRole::Worker.serves_http() && ProcessRole::Worker.executes_runs());
}

#[test]
fn a_proxy_needs_private_addresses_allowed() {
    let proxy = (
        "MIMORYA_OUTBOUND_PROXY",
        "http://compte:secret@proxy.example:3128",
    );
    let private = ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true");
    // None by default, or empty.
    assert!(load(&[DB]).unwrap().mcp_network.proxy.is_none());
    let empty = ("MIMORYA_OUTBOUND_PROXY", " ");
    assert!(load(&[DB, empty]).unwrap().mcp_network.proxy.is_none());
    // Where the platform keeps to public servers, the proxy would resolve
    // the names for it: refused, its address never repeated.
    let error = load(&[DB, proxy]).unwrap_err().to_string();
    assert!(error.contains("MIMORYA_OUTBOUND_PROXY"), "{error}");
    assert!(
        error.contains("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK"),
        "{error}"
    );
    assert!(!error.contains("secret"), "{error}");
    let bypass = ("MIMORYA_OUTBOUND_NO_PROXY", "intranet.example");
    let config = load(&[DB, proxy, private, bypass]).unwrap();
    let kept = config.mcp_network.proxy.expect("a proxy");
    assert_eq!(kept.shown(), "http://proxy.example:3128");
    assert_eq!(kept.bypass(), Some("intranet.example"));
    // An environment file written with Windows line ends reads the same.
    let crlf = ("MIMORYA_OUTBOUND_NO_PROXY", "intranet.example\r");
    let config = load(&[DB, proxy, private, crlf]).unwrap();
    let kept = config.mcp_network.proxy.expect("a proxy");
    assert_eq!(kept.bypass(), Some("intranet.example"));
}

#[test]
fn a_proxy_is_an_http_s_address_and_its_bypass_needs_it() {
    let private = ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true");
    for refused in [
        "proxy.example:3128",
        "socks5://proxy.example",
        "http://p.example/x",
    ] {
        let proxy = ("MIMORYA_OUTBOUND_PROXY", refused);
        let error = load(&[DB, private, proxy]).unwrap_err().to_string();
        assert!(error.contains("MIMORYA_OUTBOUND_PROXY"), "{error}");
        assert!(!error.contains(refused), "{error}");
    }
    let bypass = ("MIMORYA_OUTBOUND_NO_PROXY", "intranet.example");
    let error = load(&[DB, private, bypass]).unwrap_err().to_string();
    assert!(error.contains("MIMORYA_OUTBOUND_NO_PROXY"), "{error}");
    // A bypass that cannot be read is said of its own variable.
    let proxy = ("MIMORYA_OUTBOUND_PROXY", "http://proxy.example:3128");
    let unreadable = ("MIMORYA_OUTBOUND_NO_PROXY", "intranet\n.example");
    let error = load(&[DB, private, proxy, unreadable])
        .unwrap_err()
        .to_string();
    assert!(error.contains("- MIMORYA_OUTBOUND_NO_PROXY:"), "{error}");
    assert!(!error.contains("- MIMORYA_OUTBOUND_PROXY:"), "{error}");
}

/// Whether the platform's model is at Anthropic's official address decides
/// its advisor (ADR-207): its endpoint is written out, the official one when
/// none is set, its provider reading neither key nor address from the
/// environment (ADR-221).
#[test]
fn the_platform_s_anthropic_endpoint_and_its_advisor_are_read() {
    let key = ("ANTHROPIC_API_KEY", "sk-test");
    let plain = load(&[DB, key]).unwrap();
    assert_eq!(plain.llm.unwrap().base_url.as_deref(), Some(OFFICIAL));
    assert!(!plain.anthropic_advisor);
    // As the stack passes it when its operator sets nothing.
    let empty = load(&[DB, key, ("ANTHROPIC_BASE_URL", " ")]).unwrap();
    assert_eq!(empty.llm.unwrap().base_url.as_deref(), Some(OFFICIAL));
    let gateway = ("ANTHROPIC_BASE_URL", "https://passerelle.exemple");
    let config = load(&[DB, key, gateway, ("MIMORYA_ANTHROPIC_ADVISOR", "true")]).unwrap();
    assert_eq!(
        config.llm.unwrap().base_url.as_deref(),
        Some("https://passerelle.exemple")
    );
    assert!(config.anthropic_advisor);
    let first = ("LLM_BASE_URL", "https://autre.exemple");
    let config = load(&[DB, key, gateway, first]).unwrap();
    assert_eq!(
        config.llm.unwrap().base_url.as_deref(),
        Some("https://autre.exemple")
    );
    // Another provider does not read Anthropic's.
    let other = load(&[
        DB,
        ("LLM_PROVIDER", "openai"),
        ("LLM_API_KEY", "k"),
        gateway,
    ])
    .unwrap();
    assert_eq!(other.llm.unwrap().base_url, None);
    // A wrong address is said under the variable read.
    let error = load(&[DB, key, ("ANTHROPIC_BASE_URL", "ftp://x")])
        .unwrap_err()
        .to_string();
    assert!(error.contains("ANTHROPIC_BASE_URL"), "{error}");
}

/// Jev is closed unless the operator opens it, a version pinned, never an
/// alias, and an engine given up on within bounds (ADR-213).
#[test]
fn jev_is_closed_by_default_its_version_pinned_and_its_delay_bounded() {
    let plain = load(&[DB]).unwrap();
    assert!(!plain.typesafe_jev);
    assert_eq!(plain.jev_model, "jev-1.14.0");
    assert_eq!(plain.decision_timeout.as_millis(), 2_000);
    let opened = load(&[
        DB,
        ("MIMORYA_TYPESAFE_JEV", "true"),
        ("MIMORYA_JEV_MODEL", "jev-1.13.0"),
        ("MIMORYA_DECISION_TIMEOUT_MS", "500"),
    ])
    .unwrap();
    assert!(opened.typesafe_jev);
    assert_eq!(opened.jev_model, "jev-1.13.0");
    assert_eq!(opened.decision_timeout.as_millis(), 500);
    for (variable, value) in [
        ("MIMORYA_JEV_MODEL", "jev-latest"),
        ("MIMORYA_JEV_MODEL", "jev-preview"),
        ("MIMORYA_DECISION_TIMEOUT_MS", "99"),
        ("MIMORYA_DECISION_TIMEOUT_MS", "30001"),
    ] {
        let error = load(&[DB, (variable, value)]).unwrap_err().to_string();
        assert!(error.contains(variable), "{value}: {error}");
    }
}
