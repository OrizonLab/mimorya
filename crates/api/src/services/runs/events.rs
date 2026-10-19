//! Recording a run event: appended to the run's journal in its company's
//! transaction (replayed by the event stream), then broadcast to the clients
//! following the run. Shared by the run's own ports and the stale-run sweep.

use mimorya_agent::runtime::RunEvent;
use mimorya_db::Database;
use mimorya_db::repositories::run_events::append_run_event;
use uuid::Uuid;

use crate::services::event_bus::{BusEvent, RunEventBus};

pub async fn record_run_event(
    db: &Database,
    bus: &RunEventBus,
    tenant_id: Uuid,
    run_id: Uuid,
    event: RunEvent,
) -> anyhow::Result<()> {
    let (event_type, data) = event.into_parts();
    let mut tx = db.begin(tenant_id).await?;
    let row = append_run_event(&mut tx, tenant_id, run_id, &event_type, &data).await?;
    tx.commit().await?;
    bus.publish(
        run_id,
        BusEvent {
            seq: Some(row.seq),
            event_type,
            data,
        },
    );
    Ok(())
}
