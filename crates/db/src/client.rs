//! Connection pools and tenant-bound transactions. Row-level security policies
//! read `app.tenant_id`, which is transaction-local: it cannot leak to another
//! request through the connection pool. Tenant data is only reached through
//! `Database::begin`.

use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct PoolOptions<'a> {
    pub max_connections: u32,
    pub application_name: &'a str,
}

impl Default for PoolOptions<'_> {
    fn default() -> Self {
        Self {
            max_connections: 10,
            application_name: "mimorya",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(url: &str, options: PoolOptions<'_>) -> sqlx::Result<Self> {
        let connect = PgConnectOptions::from_str(url)?.application_name(options.application_name);
        let pool = PgPoolOptions::new()
            .max_connections(options.max_connections)
            .connect_with(connect)
            .await?;
        Ok(Self { pool })
    }

    /// The raw pool, for pre-tenant lookups (`auth_*`, `system_*` functions).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Starts a transaction bound to a tenant. Dropping it without `commit`
    /// rolls everything back.
    pub async fn begin(&self, tenant_id: Uuid) -> sqlx::Result<TenantTx> {
        bound(self.pool.begin().await?, tenant_id).await
    }

    /// A read-only transaction whose reads all see the same snapshot: rows
    /// read together (a configuration and its secret) match each other.
    pub async fn begin_snapshot(&self, tenant_id: Uuid) -> sqlx::Result<TenantTx> {
        let tx = self
            .pool
            .begin_with("begin isolation level repeatable read read only")
            .await?;
        bound(tx, tenant_id).await
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

async fn bound(mut tx: Transaction<'static, Postgres>, tenant_id: Uuid) -> sqlx::Result<TenantTx> {
    sqlx::query("select set_config('app.tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(TenantTx { tx, tenant_id })
}

/// A transaction in which row-level security sees exactly one tenant.
#[derive(Debug)]
pub struct TenantTx {
    tx: Transaction<'static, Postgres>,
    tenant_id: Uuid,
}

impl TenantTx {
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        self.tx.commit().await
    }

    /// Serialises the administration changes of the tenant (accounts, rights,
    /// organisation) until the end of the transaction: what a change checked
    /// on the current data stays true until it commits.
    pub async fn lock_administration(&mut self) -> sqlx::Result<()> {
        sqlx::query(
            "select pg_advisory_xact_lock(hashtextextended('mimorya.administration:' || $1, 0))",
        )
        .bind(self.tenant_id.to_string())
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }
}

impl Deref for TenantTx {
    type Target = PgConnection;

    fn deref(&self) -> &PgConnection {
        &self.tx
    }
}

impl DerefMut for TenantTx {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self.tx
    }
}
