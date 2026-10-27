//! The application a connector declares for its people's links (ADR-116,
//! ADR-121), read from its stored configuration: at which provider, its
//! client, where its people sign in, and whether its tools may write.

use anyhow::bail;
use mimorya_connectors::google_workspace::GoogleWorkspaceConfig;
use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_core::account_links::LinkKind;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;

pub(super) struct LinkApp {
    pub kind: LinkKind,
    pub client_id: String,
    pub issuer: String,
    pub write_tools: bool,
}

impl LinkApp {
    /// The connector's application, or why it cannot be read (in French).
    pub fn read(instance: &ConnectorInstanceRow) -> anyhow::Result<Self> {
        match LinkKind::of(&instance.connector_type) {
            Some(kind @ LinkKind::Microsoft365) => {
                let config = Microsoft365Config::read(&instance.config)?;
                Ok(Self {
                    kind,
                    client_id: config.client_id,
                    issuer: config.issuer,
                    write_tools: config.write_tools,
                })
            }
            Some(kind @ LinkKind::GoogleWorkspace) => {
                let config = GoogleWorkspaceConfig::read(&instance.config)?;
                Ok(Self {
                    kind,
                    client_id: config.client_id,
                    issuer: config.issuer,
                    write_tools: config.write_tools,
                })
            }
            None => bail!("Ce connecteur ne relie aucun compte"),
        }
    }
}
