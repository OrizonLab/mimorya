//! The company's model escalation (ADR-207), chosen by those who manage its
//! settings (`settings:manage`, company-wide), under the administration
//! lock that its conversations' model is changed under too: a model of that
//! provider, with its key and endpoint, the mechanisms that call it, their
//! monthly budget and the escalations a run may have. Read with why it does
//! not serve, if so, and what the mechanisms spent this month. Each change
//! is audited, with the choice alone.

use chrono::{DateTime, Utc};
use mimorya_agent::llm::{
    EscalationChoice, EscalationRequest, TenantLlmChoice, check_escalation, escalation_problem,
};
use mimorya_core::DomainError;
use mimorya_core::time_zone::month_start;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::llm_settings::{ModelPurpose, get_llm_settings};
use mimorya_db::repositories::model_escalation::{
    ModelEscalationRow, NewModelEscalation, escalation_spent, model_escalation,
    remove_model_escalation, set_model_escalation,
};
use serde_json::json;
use sqlx::PgConnection;

use super::{audit_entry, begin_change};
use crate::contract::admin::ModelEscalationUpdate;
use crate::services::AccessContext;

const PERMISSION: &str = "settings:manage";

/// What an administrator reads of the company's escalation.
pub struct EscalationOverview {
    /// The model of its conversations, if it has its own provider.
    pub main: Option<TenantLlmChoice>,
    pub stored: Option<StoredEscalation>,
    /// Why the escalation stored does not serve now.
    pub problem: Option<String>,
    pub spending: MonthSpending,
}

pub struct StoredEscalation {
    pub choice: EscalationChoice,
    pub updated_at: DateTime<Utc>,
}

/// What the mechanisms cost since the start of the month.
pub struct MonthSpending {
    pub spent_micros: i64,
    pub month_start: DateTime<Utc>,
}

pub async fn company_model_escalation(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<EscalationOverview> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let main = conversations_choice(&mut tx).await?;
    let stored = model_escalation(&mut tx).await?.map(stored_escalation);
    let spending = month_spending(&mut tx).await?;
    tx.commit().await?;
    let problem = stored
        .as_ref()
        .and_then(|stored| escalation_problem(&stored.choice, main.as_ref()));
    Ok(EscalationOverview {
        main,
        stored,
        problem,
        spending,
    })
}

/// Sets the company's escalation, if it has its own provider and the
/// choice keeps to the rules.
pub async fn set_company_model_escalation(
    db: &Database,
    access: &AccessContext,
    update: &ModelEscalationUpdate,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let main = conversations_choice(&mut tx).await?;
    let request = EscalationRequest {
        model: &update.model,
        after_failure: update.after_failure,
        router: update.router,
        advisor: update.advisor,
        monthly_budget_micros: update.monthly_budget_micros,
        max_per_run: update.max_per_run,
    };
    let choice = check_escalation(main.as_ref(), &request).map_err(DomainError::validation)?;
    let previous = model_escalation(&mut tx).await?;
    let max_per_run = i32::try_from(choice.max_per_run)?;
    set_model_escalation(
        &mut tx,
        NewModelEscalation {
            tenant_id,
            provider: choice.provider.clone(),
            base_url: choice.base_url.clone(),
            model: choice.model.clone(),
            after_failure: choice.after_failure,
            router: choice.router,
            advisor: choice.advisor,
            monthly_budget_micros: choice.monthly_budget_micros,
            max_per_run,
            updated_by_user_id: access.subject.user_id,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "model_escalation.configured", "tenant", tenant_id).metadata(json!({
            "model": choice.model,
            "afterFailure": choice.after_failure,
            "router": choice.router,
            "advisor": choice.advisor,
            "monthlyBudgetMicros": choice.monthly_budget_micros,
            "maxPerRun": choice.max_per_run,
            "previousModel": previous.map(|row| row.model),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Removes the company's escalation: its runs keep their main model.
pub async fn reset_company_model_escalation(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if !remove_model_escalation(&mut tx).await? {
        return Err(DomainError::not_found("Aucune escalade de modèle n’est réglée").into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, "model_escalation.reset", "tenant", tenant_id),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The budget of the company's escalation and what it spent this month,
/// if it chose one: shown with its usage.
pub async fn escalation_budget(
    conn: &mut PgConnection,
) -> anyhow::Result<Option<(i64, MonthSpending)>> {
    let Some(row) = model_escalation(conn).await? else {
        return Ok(None);
    };
    Ok(Some((
        row.monthly_budget_micros,
        month_spending(conn).await?,
    )))
}

async fn month_spending(conn: &mut PgConnection) -> anyhow::Result<MonthSpending> {
    let month_start = month_start(Utc::now());
    Ok(MonthSpending {
        spent_micros: escalation_spent(conn, month_start).await?,
        month_start,
    })
}

/// The model of the company's conversations, if it has its own provider.
async fn conversations_choice(conn: &mut PgConnection) -> anyhow::Result<Option<TenantLlmChoice>> {
    Ok(get_llm_settings(conn, ModelPurpose::Conversations)
        .await?
        .map(|row| TenantLlmChoice {
            provider: row.provider,
            model: row.model,
            base_url: row.base_url,
        }))
}

fn stored_escalation(row: ModelEscalationRow) -> StoredEscalation {
    StoredEscalation {
        choice: EscalationChoice {
            provider: row.provider,
            base_url: row.base_url,
            model: row.model,
            after_failure: row.after_failure,
            router: row.router,
            advisor: row.advisor,
            monthly_budget_micros: row.monthly_budget_micros,
            // Kept between 1 and 50 by the table.
            max_per_run: u32::try_from(row.max_per_run).unwrap_or(1),
        },
        updated_at: row.updated_at,
    }
}
