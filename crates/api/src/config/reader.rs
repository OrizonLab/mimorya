//! Reads the environment variables one by one, collecting every problem
//! instead of stopping at the first.

use std::sync::Arc;

use mimorya_agent::llm::{PROVIDER_KINDS, ProviderConfig};

use crate::mail::MailSettings;
use crate::secrets::SecretCipher;

pub(super) fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

pub(super) struct Reader<'a> {
    pub(super) get: &'a dyn Fn(&str) -> Option<String>,
    pub(super) issues: Vec<String>,
}

impl Reader<'_> {
    pub(super) fn value(&self, key: &str) -> Option<String> {
        (self.get)(key)
    }

    pub(super) fn issue(&mut self, key: &str, message: &str) {
        self.issues.push(format!("- {key}: {message}"));
    }

    /// Records the issue and returns a placeholder (the configuration is rejected anyway).
    pub(super) fn invalid<T: Default>(&mut self, key: &str, message: &str) -> T {
        self.issue(key, message);
        T::default()
    }

    pub(super) fn parsed<T: std::str::FromStr + Copy>(&mut self, key: &str, default: T) -> T {
        match self.value(key) {
            None => default,
            Some(raw) => raw.trim().parse().unwrap_or_else(|_| {
                self.issue(key, "nombre invalide");
                default
            }),
        }
    }

    pub(super) fn boolean(&mut self, key: &str, default: bool) -> bool {
        match self.value(key).as_deref() {
            None => default,
            Some("true" | "1") => true,
            Some("false" | "0") => false,
            Some(_) => self.invalid(key, "true, false, 1 ou 0 attendu"),
        }
    }

    pub(super) fn master_key(&mut self) -> Option<Arc<SecretCipher>> {
        let encoded = self.value("MIMORYA_MASTER_KEY")?;
        match SecretCipher::from_base64(&encoded) {
            Ok(cipher) => Some(Arc::new(cipher)),
            Err(_) => self.invalid(
                "MIMORYA_MASTER_KEY",
                "base64 de 32 octets attendu (openssl rand -base64 32)",
            ),
        }
    }

    /// Comma-separated http(s) URLs.
    pub(super) fn endpoints(&mut self, key: &str) -> Vec<String> {
        let endpoints: Vec<String> = self
            .value(key)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            .collect();
        if endpoints.iter().any(|url| !is_http_url(url)) {
            self.issue(key, "URL http(s) séparées par des virgules attendues");
        }
        endpoints
    }

    /// Both or neither of `SMTP_URL` and `MAIL_FROM`.
    pub(super) fn mail(&mut self) -> Option<MailSettings> {
        match (self.value("SMTP_URL"), self.value("MAIL_FROM")) {
            (None, None) => None,
            (Some(url), Some(from)) => match MailSettings::new(&url, &from) {
                Ok(settings) => Some(settings),
                Err((key, problem)) => self.invalid(key, &problem),
            },
            (Some(_), None) => self.invalid("MAIL_FROM", "obligatoire avec SMTP_URL"),
            (None, Some(_)) => self.invalid("SMTP_URL", "obligatoire avec MAIL_FROM"),
        }
    }

    pub(super) fn llm(&mut self) -> Option<ProviderConfig> {
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
