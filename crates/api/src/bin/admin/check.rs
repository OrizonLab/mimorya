//! `check-install` (ADR-223): what the installation has ready and what it
//! lacks, said in French; the command fails when a point stops the platform
//! from serving. Run in the api service, with its environment and its
//! brand folder.

use std::sync::Arc;

use mimorya_agent::llm::create_provider;
use mimorya_api::AppConfig;
use mimorya_api::mail::{Mailer, SmtpMailer};
use mimorya_api::services::installation::{Probes, check, told};
use mimorya_connectors::outbound::{NetworkPolicy, http_client};

/// The platform's own address is reached as anyone reaches it: through no
/// proxy, wherever its domain resolves.
const DIRECT: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
    proxy: None,
    test_servers: &[],
    test_roots: &[],
};

pub async fn check_install(mail_to: Option<String>, call_model: bool) -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    let mailer: Option<Arc<dyn Mailer>> = match (&config.mail, &mail_to) {
        (Some(settings), Some(_)) => Some(Arc::new(SmtpMailer::new(settings)?)),
        _ => None,
    };
    let model = match (&config.llm, call_model) {
        (Some(llm), true) => Some(create_provider(llm)?),
        _ => None,
    };
    let probes = Probes {
        web: http_client(DIRECT)?,
        mailer,
        mail_to,
        model,
    };
    let report = check(&config, probes).await;
    print!("{}", told(&report));
    if report.failed() {
        anyhow::bail!("l’installation ne peut pas servir en l’état : voir les points en ÉCHEC");
    }
    Ok(())
}
