//! Server configuration, read once from the environment at startup. Invalid
//! values stop the server with the list of problems instead of falling back
//! silently.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::llm::ProviderConfig;
use mimorya_connectors::outbound::NetworkPolicy;

use crate::mail::MailSettings;
use crate::secrets::SecretCipher;
use crate::services::documents::extractor_beside;
use reader::{Reader, is_http_url};

mod reader;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Environment {
    #[default]
    Development,
    Test,
    Production,
}

/// What a process does (ADR-085): everything, or one side of it. Several
/// `api` processes may serve the same database; workers (or `all`
/// processes), side by side, execute the runs they queue (ADR-086).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProcessRole {
    /// Serves the HTTP API and executes runs.
    #[default]
    All,
    /// Serves the HTTP API; the runs it starts are queued for a worker.
    Api,
    /// Executes runs, queued ones included, and the background work; serves
    /// no HTTP.
    Worker,
}

impl ProcessRole {
    /// Whether this process serves the HTTP API.
    pub fn serves_http(self) -> bool {
        self != Self::Worker
    }

    /// Whether this process executes runs, and the background work that
    /// starts them.
    pub fn executes_runs(self) -> bool {
        self != Self::Api
    }
}

#[derive(Clone)]
pub struct AppConfig {
    pub env: Environment,
    /// What this process does (`MIMORYA_ROLE`: `all`, `api` or `worker`).
    pub role: ProcessRole,
    pub host: String,
    pub port: u16,
    /// `tracing` filter directive (`info`, `warn`, `mimorya_api=debug`…).
    pub log_level: String,
    /// Runtime connection (role `mimorya_app`, subject to row-level security).
    pub database_app_url: String,
    /// Public origin of the web application, compared with the `Origin` header.
    pub web_origin: String,
    pub secure_cookies: bool,
    /// A trusted reverse proxy appends the client address to `X-Forwarded-For`
    /// (the web relay alone does not: it forwards the client's header as is).
    /// Off by default: the client address is then the TCP peer.
    pub trust_proxy: bool,
    pub assistant_name: String,
    pub llm: Option<ProviderConfig>,
    /// Seals the companies' secrets (`MIMORYA_MASTER_KEY`); without it, none
    /// can be stored.
    pub secrets: Option<Arc<SecretCipher>>,
    /// Endpoints a company may choose for a local or private model server
    /// (`MIMORYA_TENANT_LLM_ENDPOINTS`): no other address is ever reached.
    pub tenant_llm_endpoints: Vec<String>,
    /// What MCP connectors may reach (`MIMORYA_MCP_ALLOW_PRIVATE_NETWORK`,
    /// `MIMORYA_MCP_ALLOW_STDIO`): public HTTPS servers only by default; and
    /// the proxy what companies connect is reached through
    /// (`MIMORYA_OUTBOUND_PROXY`, ADR-162).
    pub mcp_network: NetworkPolicy,
    /// How long an MCP server may take to connect and list its tools
    /// (`MIMORYA_MCP_START_TIMEOUT_SECS`, 20 by default).
    pub mcp_start_timeout: Duration,
    /// Sends the invitation e-mails (`SMTP_URL`, `MAIL_FROM`); without it,
    /// the administrator transmits the invitation link (ADR-027).
    pub mail: Option<MailSettings>,
    /// Model-based review of answers when success criteria are given.
    pub llm_judge: bool,
    /// Login attempts allowed per client address and per minute.
    pub login_rate_limit_per_minute: u32,
    /// The program that reads deposited documents (`MIMORYA_EXTRACTOR`;
    /// `mimorya-extract` beside this one by default, ADR-095).
    pub document_extractor: PathBuf,
    /// The bytes of documents a company may keep
    /// (`MIMORYA_DOCUMENT_QUOTA_MB`, 1024 by default).
    pub document_quota_bytes: i64,
}

/// The documents a company keeps, by default, in megabytes.
const DOCUMENT_QUOTA_MB: u32 = 1024;
/// The most a quota may be set to, in megabytes (1 TiB).
const MAX_DOCUMENT_QUOTA_MB: u32 = 1024 * 1024;

/// The database URL carries the role's password: it never appears in debug
/// output (the provider configuration redacts its own key).
impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("env", &self.env)
            .field("role", &self.role)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("database_app_url", &"<redacted>")
            .field("web_origin", &self.web_origin)
            .field("secure_cookies", &self.secure_cookies)
            .field("trust_proxy", &self.trust_proxy)
            .field("assistant_name", &self.assistant_name)
            .field("llm", &self.llm)
            .field("secrets", &self.secrets)
            .field("tenant_llm_endpoints", &self.tenant_llm_endpoints)
            .field("mcp_network", &self.mcp_network)
            .field("mcp_start_timeout", &self.mcp_start_timeout)
            .field("mail", &self.mail)
            .field("llm_judge", &self.llm_judge)
            .field(
                "login_rate_limit_per_minute",
                &self.login_rate_limit_per_minute,
            )
            .field("document_extractor", &self.document_extractor)
            .field("document_quota_bytes", &self.document_quota_bytes)
            .finish()
    }
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
        let role = match reader.value("MIMORYA_ROLE").as_deref() {
            None | Some("all") => ProcessRole::All,
            Some("api") => ProcessRole::Api,
            Some("worker") => ProcessRole::Worker,
            Some(_) => reader.invalid("MIMORYA_ROLE", "all, api ou worker attendu"),
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
        let trust_proxy = reader.boolean("TRUST_PROXY", false);
        let assistant_name = reader
            .value("MIMORYA_ASSISTANT_NAME")
            .unwrap_or_else(|| "Mimorya".to_string());
        let llm = reader.llm();
        let secrets = reader.master_key(env);
        let tenant_llm_endpoints = reader.endpoints("MIMORYA_TENANT_LLM_ENDPOINTS");
        let allow_private_network = reader.boolean("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", false);
        let mcp_network = NetworkPolicy {
            allow_private_network,
            allow_stdio: reader.boolean("MIMORYA_MCP_ALLOW_STDIO", false),
            proxy: reader.outbound_proxy(allow_private_network),
        };
        let start_seconds = reader.parsed("MIMORYA_MCP_START_TIMEOUT_SECS", 20_u64);
        if !(1..=120).contains(&start_seconds) {
            reader.issue("MIMORYA_MCP_START_TIMEOUT_SECS", "entre 1 et 120 secondes");
        }
        let mcp_start_timeout = Duration::from_secs(start_seconds);
        let mail = reader.mail();
        let llm_judge = reader.boolean("MIMORYA_LLM_JUDGE", false);
        let login_rate_limit_per_minute = reader.parsed("LOGIN_RATE_LIMIT_PER_MINUTE", 10_u32);
        if login_rate_limit_per_minute == 0 {
            reader.issue("LOGIN_RATE_LIMIT_PER_MINUTE", "entier positif attendu");
        }
        let document_extractor = match reader.value("MIMORYA_EXTRACTOR") {
            Some(path) => PathBuf::from(path),
            None => std::env::current_exe()
                .map(|running| extractor_beside(&running))
                .unwrap_or_else(|_| PathBuf::from("mimorya-extract")),
        };
        let quota_mb = reader.parsed("MIMORYA_DOCUMENT_QUOTA_MB", DOCUMENT_QUOTA_MB);
        if !(1..=MAX_DOCUMENT_QUOTA_MB).contains(&quota_mb) {
            reader.issue("MIMORYA_DOCUMENT_QUOTA_MB", "entre 1 et 1048576 Mo");
        }
        let document_quota_bytes = i64::from(quota_mb) * 1024 * 1024;

        if !reader.issues.is_empty() {
            anyhow::bail!("Invalid configuration:\n{}", reader.issues.join("\n"));
        }
        Ok(Self {
            env,
            role,
            host,
            port,
            log_level,
            database_app_url,
            web_origin,
            secure_cookies,
            trust_proxy,
            assistant_name,
            llm,
            secrets,
            tenant_llm_endpoints,
            mcp_network,
            mcp_start_timeout,
            mail,
            llm_judge,
            login_rate_limit_per_minute,
            document_extractor,
            document_quota_bytes,
        })
    }
}

#[cfg(test)]
mod tests;
