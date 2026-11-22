//! The production stack as an operator starts it (ADR-221): from
//! `.env.production.example` with its required values filled, the API and
//! the workers start; without the master key, the stack refuses to; the
//! model proxy is their HTTPS proxy, never the connectors'; and every
//! default the stack writes is the configuration's own.

use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_api::AppConfig;
use regex::Regex;
use ring::rand::{SecureRandom, SystemRandom};

use super::stack::{BACKEND, environment, received, repository, set_in};

/// A master key of this run only: never one a test publishes.
fn master_key() -> String {
    let mut key = [0_u8; 32];
    SystemRandom::new().fill(&mut key).unwrap();
    STANDARD.encode(key)
}

/// `.env.production.example` with its required values filled, the rest
/// left as the example leaves it.
fn filled_example() -> BTreeMap<String, String> {
    let mut env = set_in(&repository(".env.production.example"));
    for (name, value) in [
        ("MIMORYA_DOMAIN", "beta.example.com".to_string()),
        ("MIMORYA_TLS", "exploitation@example.com".to_string()),
        (
            "POSTGRES_PASSWORD",
            "0123456789abcdef0123456789abcdef".to_string(),
        ),
        (
            "MIMORYA_APP_DB_PASSWORD",
            "fedcba9876543210fedcba9876543210".to_string(),
        ),
        ("MIMORYA_MASTER_KEY", master_key()),
    ] {
        assert_eq!(
            env.get(name).map(String::as_str),
            Some(""),
            "{name} left to fill"
        );
        env.insert(name.to_string(), value);
    }
    env
}

fn configuration(given: &BTreeMap<String, String>) -> anyhow::Result<AppConfig> {
    AppConfig::from_vars(|key| given.get(key).cloned())
}

#[test]
fn the_example_once_filled_starts_the_api_and_the_workers() {
    let compose = repository("compose.production.yml");
    let env = filled_example();
    for service in BACKEND {
        let given = received(&compose, service, &env).unwrap();
        let config = configuration(&given).unwrap_or_else(|error| panic!("{service}: {error}"));
        assert_eq!(config.web_origin, "https://beta.example.com");
        assert!(config.secrets.is_some(), "{service}: no master key");
        assert!(config.secure_cookies, "{service}: cookies over HTTP");
        // No relay in the example: no e-mail, and no refusal either.
        assert!(config.mail.is_none());
    }
}

#[test]
fn with_the_operator_s_key_the_platform_s_model_is_the_advised_one() {
    let compose = repository("compose.production.yml");
    let mut env = filled_example();
    env.insert("ANTHROPIC_API_KEY".into(), "cle-exploitant".into());
    let given = received(&compose, "worker", &env).unwrap();
    // The stack passes an empty address: Anthropic's own, never an empty one.
    assert_eq!(given["ANTHROPIC_BASE_URL"], "");
    let model = configuration(&given).unwrap().llm.unwrap();
    assert_eq!(model.kind, "anthropic");
    assert_eq!(model.model.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(model.base_url.as_deref(), Some("https://api.anthropic.com"));
}

#[test]
fn without_the_master_key_the_stack_refuses_to_start() {
    let compose = repository("compose.production.yml");
    let mut env = filled_example();
    env.insert("MIMORYA_MASTER_KEY".into(), String::new());
    for service in BACKEND {
        let refused = received(&compose, service, &env).unwrap_err();
        assert!(
            refused.contains("MIMORYA_MASTER_KEY"),
            "{service}: {refused}"
        );
        assert!(
            refused.contains("openssl rand -base64 32"),
            "{service}: {refused}"
        );
    }
    env.remove("MIMORYA_MASTER_KEY");
    assert!(received(&compose, "api", &env).is_err());
}

#[test]
fn the_model_proxy_is_the_backend_s_https_proxy_never_the_connectors() {
    let compose = repository("compose.production.yml");
    let mut env = filled_example();
    env.insert(
        "MIMORYA_MODEL_PROXY".into(),
        "http://mandataire.interne:3128".into(),
    );
    env.insert("MIMORYA_MODEL_NO_PROXY".into(), "ollama.interne".into());
    for service in BACKEND {
        let given = received(&compose, service, &env).unwrap();
        assert_eq!(given["HTTPS_PROXY"], "http://mandataire.interne:3128");
        assert_eq!(given["NO_PROXY"], "ollama.interne");
        assert_eq!(given["MIMORYA_OUTBOUND_PROXY"], "");
        assert!(configuration(&given).unwrap().mcp_network.proxy.is_none());
    }
}

#[test]
fn every_default_the_stack_writes_is_the_configuration_s_own() {
    let compose = repository("compose.production.yml");
    let default = Regex::new(r"^\$\{([A-Z][A-Z0-9_]*):-([^}]+)\}$").unwrap();
    let bare = BTreeMap::from([(
        "DATABASE_APP_URL".to_string(),
        "postgres://mimorya_app:secret@db:5432/mimorya".to_string(),
    )]);
    let unset = format!("{:?}", configuration(&bare).unwrap());
    let mut compared = 0;
    for (name, value) in environment(&compose, "api") {
        let Some(found) = default.captures(&value) else {
            continue;
        };
        assert_eq!(&found[1], name, "{name} reads another variable");
        let mut given = bare.clone();
        given.insert(name.clone(), found[2].to_string());
        let set = format!("{:?}", configuration(&given).unwrap());
        assert_eq!(set, unset, "{name}: the stack's default is not the code's");
        compared += 1;
    }
    assert!(compared >= 10, "only {compared} defaults compared");
}
