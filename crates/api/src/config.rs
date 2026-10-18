//! Server configuration, read once from the environment at startup. Invalid
//! values stop the server with the list of problems instead of falling back
//! silently.

use mimorya_agent::llm::{PROVIDER_KINDS, ProviderConfig};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Environment {
    #[default]
    Development,
    Test,
    Production,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub env: Environment,
    pub host: String,
    pub port: u16,
    /// `tracing` filter directive (`info`, `warn`, `mimorya_api=debug`…).
    pub log_level: String,
    /// Runtime connection (role `mimorya_app`, subject to row-level security).
    pub database_app_url: String,
    /// Public origin of the web application, compared with the `Origin` header.
    pub web_origin: String,
    pub secure_cookies: bool,
    /// The API sits behind the web relay: the client address is read from
    /// `X-Forwarded-For`. Disable when the API is exposed directly.
    pub trust_proxy: bool,
    pub assistant_name: String,
    pub llm: Option<ProviderConfig>,
    /// Model-based review of answers when success criteria are given.
    pub llm_judge: bool,
    /// Login attempts allowed per client address and per minute.
    pub login_rate_limit_per_minute: u32,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_vars(|key| std::env::var(key).ok())
    }

    /// Reads the configuration through `get` (tests pass a map). Empty values
    /// count as unset.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let mut reader = Reader {
            get: &|key| get(key).filter(|v| !v.trim().is_empty()),
            issues: Vec::new(),
        };
        let env = match reader.value("MIMORYA_ENV").as_deref() {
            None | Some("development") => Environment::Development,
            Some("test") => Environment::Test,
            Some("production") => Environment::Production,
            Some(_) => reader.invalid("MIMORYA_ENV", "development, test ou production attendu"),
        };
        let host = reader
            .value("HOST")
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = reader.parsed("PORT", 4000_u16);
        let log_level = match reader.value("LOG_LEVEL").as_deref() {
            None => "info".to_string(),
            Some("silent") => "off".to_string(),
            Some("fatal") => "error".to_string(),
            Some(level) => level.to_string(),
        };
        let database_app_url = match reader.value("DATABASE_APP_URL") {
            Some(url) if url.starts_with("postgres://") || url.starts_with("postgresql://") => url,
            Some(_) => reader.invalid("DATABASE_APP_URL", "URL postgres:// attendue"),
            None => reader.invalid("DATABASE_APP_URL", "obligatoire"),
        };
        let web_origin = match reader.value("WEB_ORIGIN") {
            None => "http://localhost:3000".to_string(),
            Some(url) if is_http_url(&url) => url.trim_end_matches('/').to_string(),
            Some(_) => reader.invalid("WEB_ORIGIN", "URL http(s) attendue"),
        };
        let secure_cookies =
            reader.boolean("SESSION_SECURE_COOKIE", env == Environment::Production);
        let trust_proxy = reader.boolean("TRUST_PROXY", true);
        let assistant_name = reader
            .value("MIMORYA_ASSISTANT_NAME")
            .unwrap_or_else(|| "Mimorya".to_string());
        let llm = reader.llm();
        let llm_judge = reader.boolean("MIMORYA_LLM_JUDGE", false);
        let login_rate_limit_per_minute = reader.parsed("LOGIN_RATE_LIMIT_PER_MINUTE", 10_u32);
        if login_rate_limit_per_minute == 0 {
            reader.issue("LOGIN_RATE_LIMIT_PER_MINUTE", "entier positif attendu");
        }

        if !reader.issues.is_empty() {
            anyhow::bail!("Invalid configuration:\n{}", reader.issues.join("\n"));
        }
        Ok(Self {
            env,
            host,
            port,
            log_level,
            database_app_url,
            web_origin,
            secure_cookies,
            trust_proxy,
            assistant_name,
            llm,
            llm_judge,
            login_rate_limit_per_minute,
        })
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

struct Reader<'a> {
    get: &'a dyn Fn(&str) -> Option<String>,
    issues: Vec<String>,
}

impl Reader<'_> {
    fn value(&self, key: &str) -> Option<String> {
        (self.get)(key)
    }

    fn issue(&mut self, key: &str, message: &str) {
        self.issues.push(format!("- {key}: {message}"));
    }

    /// Records the issue and returns a placeholder (the configuration is rejected anyway).
    fn invalid<T: Default>(&mut self, key: &str, message: &str) -> T {
        self.issue(key, message);
        T::default()
    }

    fn parsed<T: std::str::FromStr + Copy>(&mut self, key: &str, default: T) -> T {
        match self.value(key) {
            None => default,
            Some(raw) => raw.trim().parse().unwrap_or_else(|_| {
                self.issue(key, "nombre invalide");
                default
            }),
        }
    }

    fn boolean(&mut self, key: &str, default: bool) -> bool {
        match self.value(key).as_deref() {
            None => default,
            Some("true" | "1") => true,
            Some("false" | "0") => false,
            Some(_) => self.invalid(key, "true, false, 1 ou 0 attendu"),
        }
    }

    fn llm(&mut self) -> Option<ProviderConfig> {
        let anthropic_key = self.value("ANTHROPIC_API_KEY");
        let kind = self.value("LLM_PROVIDER").unwrap_or_else(|| {
            if anthropic_key.is_some() {
                "anthropic".into()
            } else {
                "none".into()
            }
        });
        if kind == "none" {
            return None;
        }
        if !PROVIDER_KINDS.contains(&kind.as_str()) {
            let expected = format!("{} ou none attendu", PROVIDER_KINDS.join(", "));
            self.issue("LLM_PROVIDER", &expected);
            return None;
        }
        let base_url = self.value("LLM_BASE_URL");
        if base_url.as_deref().is_some_and(|url| !is_http_url(url)) {
            self.issue("LLM_BASE_URL", "URL http(s) attendue");
        }
        let api_key = self
            .value("LLM_API_KEY")
            .or_else(|| (kind == "anthropic").then_some(anthropic_key).flatten());
        Some(ProviderConfig {
            kind,
            model: self.value("LLM_MODEL"),
            api_key,
            base_url,
        })
    }
}

#[cfg(test)]
mod tests {
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
    fn secures_cookies_in_production() {
        let config = load(&[DB, ("MIMORYA_ENV", "production")]).unwrap();
        assert!(config.secure_cookies);
    }
}
