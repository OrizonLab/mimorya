//! Recording a run event: appended to the run's journal in its company's
//! transaction (replayed by the event stream) and announced to the other
//! processes in the same transaction (ADR-084), then broadcast to the
//! clients following the run here. Shared by the run's own ports, its
//! takeover, the stale-run sweep and the queue.

use mimorya_agent::runtime::RunEvent;
use mimorya_db::notifications::notify_run_events;
use mimorya_db::repositories::run_events::append_run_event;
use mimorya_db::{Database, TenantTx};
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
    let mut tx = db.begin(tenant_id).await?;
    let recorded = append_in(&mut tx, bus, run_id, event).await?;
    tx.commit().await?;
    let seq = recorded.seq.unwrap_or_default();
    bus.publish(run_id, recorded);
    Ok(seq)
}

/// Appends the event within `tx`, announced to the other processes once it
/// commits; what to broadcast here then.
pub async fn append_in(
    tx: &mut TenantTx,
    bus: &RunEventBus,
    run_id: Uuid,
    event: RunEvent,
) -> anyhow::Result<BusEvent> {
    let (event_type, data) = event.into_parts();
    let tenant_id = tx.tenant_id();
    let row = append_run_event(tx, tenant_id, run_id, &event_type, &data).await?;
    let notice = Notice::event(bus.origin(), run_id, row.seq);
    notify_run_events(&mut **tx, &notice).await?;
    Ok(BusEvent {
        seq: Some(row.seq),
        event_type,
        data,
    })
}
