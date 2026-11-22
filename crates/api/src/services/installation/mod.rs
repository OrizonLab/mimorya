//! What an installation has ready and what it lacks (ADR-223), checked by
//! `mimorya-admin check-install` in the api service, with its environment,
//! and told in French: the database and its migrations, the master key, the
//! domain and its certificate, the mail relay (a test e-mail to an address
//! given), the archive of the WAL, the workers listening, the brand, the
//! platform's model. The model is called only when the operator asks
//! (`--call-model`): it costs a few tokens, and no test ever does.

mod archive;
mod database;
mod mail;
mod model;
mod settings;
mod site;
mod told;

use std::sync::Arc;

use mimorya_agent::llm::LlmProvider;

use crate::config::AppConfig;
use crate::mail::Mailer;
pub use told::told;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    /// Works, but something an installation should have is missing or
    /// unverified.
    ToComplete,
    /// To fix before the platform is opened: it cannot serve, or no longer
    /// protects its data (the archive of the WAL failing).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub topic: &'static str,
    pub state: State,
    pub said: String,
}

impl Finding {
    fn new(topic: &'static str, state: State, said: impl Into<String>) -> Self {
        Self {
            topic,
            state,
            said: said.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The product's name, as people read it.
    pub product: String,
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether something is to fix before the platform is opened.
    pub fn failed(&self) -> bool {
        self.findings.iter().any(|f| f.state == State::Failed)
    }
}

/// What reaches out of the check, given by its caller.
pub struct Probes {
    /// Reaches the platform's own address, as anyone does.
    pub web: reqwest_mcp::Client,
    /// Sends the test e-mail; none without a relay.
    pub mailer: Option<Arc<dyn Mailer>>,
    /// Where the test e-mail goes, if one is asked for.
    pub mail_to: Option<String>,
    /// The platform's model, given only to be called.
    pub model: Option<Arc<dyn LlmProvider>>,
}

pub async fn check(config: &AppConfig, probes: Probes) -> Report {
    let mut findings = Vec::new();
    let db = database::connect(config).await;
    match &db {
        Ok(db) => {
            findings.push(database::reached());
            findings.push(database::migrations(db).await);
        }
        Err(why) => findings.push(Finding::new(database::TOPIC, State::Failed, why.clone())),
    }
    findings.push(settings::master_key(config));
    findings.push(site::reachable(config, &probes.web).await);
    let product = &config.brand.names.product;
    let mail_to = probes.mail_to.as_deref();
    findings.push(mail::relay(config, probes.mailer, mail_to, product).await);
    match &db {
        Ok(db) => {
            findings.push(archive::state(db).await);
            findings.push(database::workers(db).await);
        }
        Err(_) => {
            for topic in [archive::TOPIC, database::WORKERS] {
                let said = "non vérifié sans la base";
                findings.push(Finding::new(topic, State::Failed, said));
            }
        }
    }
    findings.push(settings::brand(config));
    findings.push(model::platform(config, probes.model).await);
    if let Ok(db) = db {
        db.close().await;
    }
    Report {
        product: product.clone(),
        findings,
    }
}
