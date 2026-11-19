//! The alerts of the operator's supply (ADR-211). Every minute, a process
//! that executes runs reads, one process at a time (ADR-086), each active
//! company on the operator's key: what its month was re-invoiced against
//! the shares of its cap, or its credit's balance against the shares of its
//! last payment, as the operator set them. The shares reached and never
//! alerted for that month and cap, or that payment, are told once (a cap
//! changed or a new payment re-arms them), the most pressing alone, in one
//! e-mail to each account that manages the company's settings
//! on the whole company (`settings:manage`, as its rules say now,
//! `company_wide_administrators`). It is always sent, as an invitation is:
//! it says what would stop its runs. The shares are then recorded, even
//! those whose e-mail the relay refused, and are not told again; a company
//! without such an administrator records nothing, for the next one. Without
//! a relay, nothing is sent nor recorded. Each e-mail is audited, without
//! the address nor the relay's answer.

mod mail;

use chrono::{DateTime, Utc};
use mimorya_core::model_supply::{
    AlertKind, SupplyFormula, SupplyState, cap_alerts_reached, credit_alerts_reached, most_pressing,
};
use mimorya_core::time_zone::{PLATFORM_TIME_ZONE, month_start};
use mimorya_db::TenantTx;
use mimorya_db::repositories::administrators::Recipient;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::model_supply::{
    ModelSupplyRow, credit_entries, model_supply, supply_state,
};
use mimorya_db::repositories::org::load_authorization_data;
use mimorya_db::repositories::supply_alerts::{
    record_supply_alerts, sent_supply_alerts, supplied_companies,
};
use serde_json::json;
use uuid::Uuid;

use crate::app::Services;
use crate::mail::Mailer;
use crate::services::admin::{company_wide_administrators, engine_of};
use mail::{SupplyAlertMail, supply_alert_mail};

/// The audited action, once per administrator told.
pub const SUPPLY_ALERTED: &str = "model_supply.alerted";
/// The sweep's lock (ADR-086).
pub const SUPPLY_ALERT_SWEEP: &str = "supply-alerts";
const PERMISSION: &str = "settings:manage";

/// What a company's supply is worth an alert for now.
struct Due {
    kind: AlertKind,
    /// The month of the cap, or the reference of the last payment.
    period: String,
    /// The shares reached, never alerted for `period`.
    reached: Vec<u8>,
    /// What the month was re-invoiced, or the credit's balance.
    amount_micros: i64,
    /// The cap, or the last payment.
    of_micros: i64,
}

/// Tells the administrators of each company on the operator's key of the
/// shares its supply reached; for how many companies. Only in a process
/// that executes runs, and one at a time.
pub async fn send_supply_alerts(services: &Services) -> anyhow::Result<usize> {
    let Some(mailer) = services.mailer.clone() else {
        return Ok(0);
    };
    if !services.runs.executes() {
        return Ok(0);
    }
    let Some(sweep) = services.db.try_sweep_lock(SUPPLY_ALERT_SWEEP).await? else {
        return Ok(0);
    };
    let alerted = alert_all(services, mailer.as_ref()).await;
    sweep.release().await;
    alerted
}

async fn alert_all(services: &Services, mailer: &dyn Mailer) -> anyhow::Result<usize> {
    let mut alerted = 0;
    for tenant_id in supplied_companies(services.db.pool()).await? {
        // One company failing leaves the others to be told.
        match alert(services, mailer, tenant_id).await {
            Ok(told) => alerted += usize::from(told),
            Err(error) => tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Supply alert not sent to the company's administrators"
            ),
        }
    }
    Ok(alerted)
}

/// Tells the company's administrators of the shares its supply reached and
/// never alerted; whether it did.
async fn alert(services: &Services, mailer: &dyn Mailer, tenant_id: Uuid) -> anyhow::Result<bool> {
    let mut tx = services.db.begin(tenant_id).await?;
    let Some(row) = model_supply(&mut tx, tenant_id).await? else {
        return Ok(false);
    };
    let month = month_start(Utc::now());
    let state = supply_state(&mut tx, tenant_id, month).await?;
    let due = match state.formula {
        SupplyFormula::Capped => cap_due(&state, &row, month),
        SupplyFormula::Prepaid => credit_due(&mut tx, tenant_id, &state, &row).await?,
        SupplyFormula::ClientKey => None,
    };
    let Some(mut due) = due else {
        return Ok(false);
    };
    let sent = sent_supply_alerts(&mut tx, due.kind.as_str(), &due.period).await?;
    due.reached
        .retain(|percent| !sent.contains(&i16::from(*percent)));
    let Some(percent) = most_pressing(due.kind, &due.reached) else {
        return Ok(false);
    };
    let rules = load_authorization_data(&mut tx).await?;
    let company = rules.tenant.name.clone();
    let engine = engine_of(rules)?;
    let administrators = company_wide_administrators(&mut tx, &engine, PERMISSION).await?;
    tx.commit().await?;
    if administrators.is_empty() {
        return Ok(false);
    }
    for administrator in &administrators {
        let told = Told {
            tenant_id,
            company: &company,
            due: &due,
            percent,
        };
        if let Err(error) = tell(services, mailer, &told, administrator).await {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Supply alert not told to an administrator"
            );
        }
    }
    let reached: Vec<i16> = due.reached.iter().map(|p| i16::from(*p)).collect();
    let mut tx = services.db.begin(tenant_id).await?;
    record_supply_alerts(&mut tx, tenant_id, due.kind.as_str(), &due.period, &reached).await?;
    tx.commit().await?;
    Ok(true)
}

/// The shares of the cap the month came to, under a cap.
fn cap_due(state: &SupplyState, row: &ModelSupplyRow, month: DateTime<Utc>) -> Option<Due> {
    let cap = state.monthly_cap_micros.filter(|_| !state.unlimited)?;
    let month_billed = state.capped_month_billed_micros;
    // The month and its cap: a cap changed re-arms its shares; one held
    // before in the month keeps those already told.
    let month = month.with_timezone(&PLATFORM_TIME_ZONE).format("%Y-%m");
    Some(Due {
        kind: AlertKind::Cap,
        period: format!("{month}:{cap}"),
        reached: cap_alerts_reached(month_billed, cap, &row.cap_alerts()),
        amount_micros: month_billed,
        of_micros: cap,
    })
}

/// The shares of the last payment the credit's balance fell to, once one
/// was paid.
async fn credit_due(
    tx: &mut TenantTx,
    tenant_id: Uuid,
    state: &SupplyState,
    row: &ModelSupplyRow,
) -> anyhow::Result<Option<Due>> {
    let entries = credit_entries(tx, tenant_id).await?;
    // The latest payment: a correction is none.
    let Some(last) = entries.iter().rev().find(|entry| entry.amount_micros > 0) else {
        return Ok(None);
    };
    let balance = state.prepaid_balance_micros;
    Ok(Some(Due {
        kind: AlertKind::Credit,
        period: last.reference.clone(),
        reached: credit_alerts_reached(balance, last.amount_micros, &row.credit_alerts()),
        amount_micros: balance,
        of_micros: last.amount_micros,
    }))
}

/// One alert, as each administrator is told it.
struct Told<'a> {
    tenant_id: Uuid,
    company: &'a str,
    due: &'a Due,
    percent: u8,
}

/// Sends `administrator` the alert, and audits it.
async fn tell(
    services: &Services,
    mailer: &dyn Mailer,
    told: &Told<'_>,
    administrator: &Recipient,
) -> anyhow::Result<()> {
    let config = &services.config;
    let link = format!("{}/admin", config.web_origin);
    let mail = supply_alert_mail(&SupplyAlertMail {
        product: &config.assistant_name,
        company: told.company,
        administrator: &administrator.display_name,
        email: &administrator.email,
        kind: told.due.kind,
        percent: told.percent,
        amount_micros: told.due.amount_micros,
        of_micros: told.due.of_micros,
        link: &link,
    });
    let outcome = match mailer.send(&mail).await {
        Ok(()) => "success",
        Err(_) => {
            tracing::warn!(tenant_id = %told.tenant_id, "Supply alert e-mail refused by the relay");
            "failure"
        }
    };
    let metadata = json!({ "kind": told.due.kind.as_str(), "percent": told.percent });
    let entry = AuditEntry::new(told.tenant_id, "system", SUPPLY_ALERTED, outcome)
        .on_behalf_of(administrator.user_id)
        .metadata(metadata);
    let mut tx = services.db.begin(told.tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}
