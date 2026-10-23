//! Recording a run event: appended to the run's journal in its company's
//! transaction (replayed by the event stream) and announced to the other
//! processes in the same transaction (ADR-083), then broadcast to the
//! clients following the run here. Shared by the run's own ports, its
//! takeover and the stale-run sweep.

use mimorya_agent::runtime::RunEvent;
use mimorya_db::Database;
use mimorya_db::notifications::notify_run_events;
use mimorya_db::repositories::run_events::append_run_event;
use uuid::Uuid;

use crate::services::event_bus::{BusEvent, Notice, RunEventBus};

/// Records the event; its number in the run's journal.
pub async fn record_run_event(
    db: &Database,
    bus: &RunEventBus,
    tenant_id: Uuid,
    run_id: Uuid,
    event: RunEvent,
) -> anyhow::Result<i32> {
    let (event_type, data) = event.into_parts();
    let mut tx = db.begin(tenant_id).await?;
    let row = append_run_event(&mut tx, tenant_id, run_id, &event_type, &data).await?;
    let notice = Notice::event(bus.origin(), run_id, row.seq);
    notify_run_events(&mut *tx, &notice).await?;
    tx.commit().await?;
    bus.publish(
        run_id,
        BusEvent {
            seq: Some(row.seq),
            event_type,
            data,
        },
    );
    Ok(row.seq)
}
