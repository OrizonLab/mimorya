//! Demo connector instance backed by the fictional interventions dataset.

use mimorya_core::demo::{demo_id, tenant};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub const DEMO_CONNECTOR_ID: Uuid = demo_id(500);

pub(super) async fn insert(conn: &mut PgConnection) -> sqlx::Result<()> {
    sqlx::query("insert into connector_instances (id, tenant_id, type, name, config) values ($1, $2, $3, $4, $5)")
        .bind(DEMO_CONNECTOR_ID)
        .bind(tenant::ID)
        .bind("demo_interventions")
        .bind("Interventions (démonstration)")
        .bind(json!({ "dataset": "neo-maintenance", "note": "Données fictives" }))
        .execute(conn)
        .await?;
    Ok(())
}
