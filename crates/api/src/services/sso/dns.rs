//! The TXT records of a domain's DNS, read to prove the domain (ADR-069):
//! through the system's resolver, each lookup bounded in time; the tests
//! give their own records.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;

/// How long a lookup may take.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

#[async_trait]
pub trait TxtLookup: Send + Sync {
    /// The values of the TXT records at `name`, each record's strings
    /// joined; none when it has none.
    async fn txt(&self, name: &str) -> anyhow::Result<Vec<String>>;
}

/// The system's resolver, as configured for the server (`resolv.conf`).
pub struct SystemDns(Option<TokioResolver>);

impl SystemDns {
    pub fn shared() -> Arc<dyn TxtLookup> {
        Arc::new(Self::default())
    }
}

/// Without a usable configuration, every lookup fails, saying so.
impl Default for SystemDns {
    fn default() -> Self {
        let resolver = TokioResolver::builder_tokio().and_then(|builder| builder.build());
        if let Err(error) = &resolver {
            tracing::warn!(%error, "No DNS resolver: domains cannot be proved");
        }
        Self(resolver.ok())
    }
}

#[async_trait]
impl TxtLookup for SystemDns {
    async fn txt(&self, name: &str) -> anyhow::Result<Vec<String>> {
        let resolver = self.0.as_ref().ok_or_else(|| anyhow!("no DNS resolver"))?;
        let found = tokio::time::timeout(LOOKUP_TIMEOUT, resolver.txt_lookup(name))
            .await
            .context("DNS lookup timed out")?;
        let lookup = match found {
            Ok(lookup) => lookup,
            Err(error) if error.is_no_records_found() || error.is_nx_domain() => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error).context("DNS lookup"),
        };
        Ok(lookup
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::TXT(txt) => Some(
                    txt.txt_data
                        .iter()
                        .map(|part| String::from_utf8_lossy(part))
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect())
    }
}
