//! How the operator supplies a company's model (ADR-208), named by its
//! slug, as the database's owner: the application's role only reads the
//! formulas and the credits, and nothing it runs can give a company the
//! operator's key. Each change is written in one transaction with its entry
//! in the company's audit log, by the `system` actor; a change of the
//! formula or of the markup is announced to every process, which loads the
//! company's models again. The report reads what the company consumed on
//! the operator's key, really and re-invoiced, and its credit's balance,
//! computed again from the register.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_core::model_supply::{
    DEFAULT_MONTHLY_CAP_MICROS, SupplyFormula, cap_problem, credit_problem, markup_problem,
};
use mimorya_core::time_zone::month_start;
use mimorya_db::notifications::notify_invalidation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::model_supply::{
    CreditEntryRow, NewSupplyFormula, SupplySpending, add_credit_entry, credit_entries,
    lock_supply, model_supply, set_supply_formula, set_supply_markup, supply_spending,
};
use mimorya_db::repositories::tenants::find_tenant_by_slug;
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::system_entry;
use crate::services::invalidations::{Invalidation, announcement};

/// The cap the operator gives with the capped formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapChoice {
    /// None given: the default for the capped formula, none otherwise.
    Default,
    Amount(i64),
    /// Asked explicitly: the capped formula without a cap.
    Unlimited,
}

/// What the operator changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplyChange {
    Formula {
        formula: SupplyFormula,
        cap: CapChoice,
    },
    Markup {
        bps: u32,
    },
    Credit {
        amount_micros: i64,
        reference: String,
    },
}

/// A company's supply as the operator reads it.
#[derive(Debug, Clone)]
pub struct SupplyReport {
    pub tenant_id: Uuid,
    pub formula: SupplyFormula,
    pub monthly_cap_micros: Option<i64>,
    pub unlimited: bool,
    pub markup_bps: u32,
    pub month_start: DateTime<Utc>,
    pub spending: SupplySpending,
    /// The credit's payments, the earliest first.
    pub entries: Vec<CreditEntryRow>,
}

/// The company whose slug is `slug`.
async fn company(owner: &PgPool, slug: &str) -> anyhow::Result<Uuid> {
    find_tenant_by_slug(owner, slug.trim())
        .await?
        .ok_or_else(|| DomainError::not_found("Aucune entreprise ne porte ce sigle").into())
}

pub async fn model_supply_report(owner: &PgPool, slug: &str) -> anyhow::Result<SupplyReport> {
    let tenant_id = company(owner, slug).await?;
    let mut conn = owner.acquire().await?;
    report(&mut conn, tenant_id).await
}

/// Applies `change` to the company whose slug is `slug`: what it then is.
pub async fn change_model_supply(
    owner: &PgPool,
    slug: &str,
    change: SupplyChange,
) -> anyhow::Result<SupplyReport> {
    let tenant_id = company(owner, slug).await?;
    let mut tx = owner.begin().await?;
    // Two changes at once: each traced after the other, with what it found.
    if !lock_supply(&mut tx, tenant_id).await? {
        return Err(DomainError::not_found("Aucune entreprise ne porte ce sigle").into());
    }
    let previous = model_supply(&mut tx, tenant_id).await?;
    let entry = match &change {
        SupplyChange::Formula { formula, cap } => {
            let chosen = chosen_formula(*formula, *cap)?;
            set_supply_formula(&mut tx, tenant_id, chosen).await?;
            json!({
                "formula": formula.as_str(),
                "monthlyCapMicros": chosen.monthly_cap_micros,
                "unlimited": chosen.unlimited,
                "previousFormula": previous.as_ref().map(|row| row.formula.clone()),
            })
        }
        SupplyChange::Markup { bps } => {
            refuse(markup_problem(*bps))?;
            set_supply_markup(&mut tx, tenant_id, i32::try_from(*bps)?).await?;
            json!({
                "markupBps": bps,
                "previousMarkupBps": previous.as_ref().map(|row| row.markup_bps),
            })
        }
        SupplyChange::Credit {
            amount_micros,
            reference,
        } => {
            let reference = reference.trim();
            refuse(credit_problem(*amount_micros, reference))?;
            if !add_credit_entry(&mut tx, tenant_id, *amount_micros, reference).await? {
                return Err(DomainError::conflict(
                    "Un versement porte déjà cette référence dans cette entreprise : une \
                     correction est un versement de plus, avec sa propre référence",
                )
                .into());
            }
            json!({ "amountMicros": amount_micros, "reference": reference })
        }
    };
    let action = match change {
        SupplyChange::Formula { .. } => "model_supply.configured",
        SupplyChange::Markup { .. } => "model_supply.markup_set",
        SupplyChange::Credit { .. } => "model_supply.credit_added",
    };
    append_audit(
        &mut tx,
        system_entry(tenant_id, action, "tenant", tenant_id).metadata(entry),
    )
    .await?;
    if !matches!(change, SupplyChange::Credit { .. }) {
        // Each process loads the company's models again, with this supply.
        let reload = announcement(Uuid::new_v4(), Invalidation::Llm(tenant_id));
        notify_invalidation(&mut *tx, &reload).await?;
    }
    let report = report(&mut tx, tenant_id).await?;
    tx.commit().await?;
    Ok(report)
}

/// The formula as stored: the capped one with its cap (100 USD unless
/// given) or, asked explicitly, without; the others without either.
fn chosen_formula(formula: SupplyFormula, cap: CapChoice) -> anyhow::Result<NewSupplyFormula> {
    let (monthly_cap_micros, unlimited) = match (formula, cap) {
        (SupplyFormula::Capped, CapChoice::Default) => (Some(DEFAULT_MONTHLY_CAP_MICROS), false),
        (SupplyFormula::Capped, CapChoice::Amount(micros)) => {
            refuse(cap_problem(micros))?;
            (Some(micros), false)
        }
        (SupplyFormula::Capped, CapChoice::Unlimited) => (None, true),
        (_, CapChoice::Default) => (None, false),
        (_, _) => {
            return Err(DomainError::validation(
                "Un plafond ne se règle qu’avec la formule capped",
            )
            .into());
        }
    };
    Ok(NewSupplyFormula {
        formula,
        monthly_cap_micros,
        unlimited,
    })
}

async fn report(conn: &mut PgConnection, tenant_id: Uuid) -> anyhow::Result<SupplyReport> {
    let row = model_supply(conn, tenant_id).await?;
    let month_start = month_start(Utc::now());
    let spending = supply_spending(conn, tenant_id, month_start).await?;
    let entries = credit_entries(conn, tenant_id).await?;
    let formula = match &row {
        Some(row) => SupplyFormula::parse(&row.formula)
            .ok_or_else(|| anyhow::anyhow!("unknown supply formula {}", row.formula))?,
        None => SupplyFormula::ClientKey,
    };
    Ok(SupplyReport {
        tenant_id,
        formula,
        monthly_cap_micros: row.as_ref().and_then(|row| row.monthly_cap_micros),
        unlimited: row.as_ref().is_some_and(|row| row.unlimited),
        markup_bps: row
            .as_ref()
            .map_or(0, |row| u32::try_from(row.markup_bps).unwrap_or(0)),
        month_start,
        spending,
        entries,
    })
}

fn refuse(problem: Option<String>) -> Result<(), DomainError> {
    match problem {
        Some(problem) => Err(DomainError::validation(problem)),
        None => Ok(()),
    }
}
