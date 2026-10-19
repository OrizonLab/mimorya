//! Reads and writes a company's secrets through its tenant transaction.

use std::sync::Arc;

use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::secrets::{delete_secret, get_secret, put_secret};
use uuid::Uuid;

use super::SecretCipher;

/// The API key of the company's model provider.
pub const LLM_API_KEY: &str = "llm.api_key";

#[derive(Debug, Clone, Default)]
pub struct SecretStore {
    /// `None` when the server has no master key: nothing can be stored.
    cipher: Option<Arc<SecretCipher>>,
}

impl SecretStore {
    pub fn new(cipher: Option<Arc<SecretCipher>>) -> Self {
        Self { cipher }
    }

    pub fn available(&self) -> bool {
        self.cipher.is_some()
    }

    /// Refuses (in French) when nothing can be stored on this server.
    pub fn require(&self) -> Result<(), DomainError> {
        self.cipher().map(|_| ())
    }

    fn cipher(&self) -> Result<&SecretCipher, DomainError> {
        self.cipher.as_deref().ok_or_else(|| {
            DomainError::unavailable(
                "Ce serveur ne peut pas conserver de clé : l’exploitant n’a pas configuré \
                 MIMORYA_MASTER_KEY",
            )
        })
    }

    pub async fn put(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        name: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        let sealed = self.cipher()?.seal(tenant_id, name, value)?;
        put_secret(tx, tenant_id, name, &sealed).await?;
        Ok(())
    }

    /// The value, if one is stored; an error if it cannot be opened (other
    /// master key, altered row).
    pub async fn get(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        name: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some(sealed) = get_secret(tx, name).await? else {
            return Ok(None);
        };
        Ok(Some(self.cipher()?.open(tenant_id, name, &sealed)?))
    }

    /// Whether a value is stored (without opening it).
    pub async fn exists(&self, tx: &mut TenantTx, name: &str) -> anyhow::Result<bool> {
        Ok(get_secret(tx, name).await?.is_some())
    }

    pub async fn delete(&self, tx: &mut TenantTx, name: &str) -> anyhow::Result<bool> {
        Ok(delete_secret(tx, name).await?)
    }
}
