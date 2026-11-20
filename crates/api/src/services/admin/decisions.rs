//! The company's decision service (ADR-213), set by those who manage its
//! settings (`settings:manage`, company-wide), under the administration
//! lock: its engine, its main model by default or Jev where the operator
//! opens it, with its key to TypeSafe sealed and never journaled; and the
//! mode and threshold of each usage, one not delivered left off. Each change
//! is audited with the choice alone; the engine's test too, with its outcome.

use chrono::{DateTime, Utc};
use mimorya_agent::decision::{
    DecisionMode, DecisionOutcome, DecisionQuestion, DecisionRequest, DecisionState, DecisionUsage,
    ENGINE_TEST, EngineKind, UsagePolicy, usage_problem,
};
use mimorya_agent::llm::{ModelRole, api_key_problem};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::decision_settings::{
    DecisionUsageRow, decision_settings, decision_usages, remove_decision_settings,
    set_decision_settings,
};
use serde_json::json;

use super::{audit_entry, begin_change};
use crate::contract::admin::{DecisionEngineChoice, DecisionSettingsUpdate};
use crate::secrets::TYPESAFE_API_KEY;
use crate::services::llm::{
    DecisionEngines, DecisionScope, JEV_CLOSED, supply_stopped, trace_held_back,
};
use crate::services::{AccessContext, LlmService};

const PERMISSION: &str = "settings:manage";
/// A usage's threshold until it is set.
pub const DEFAULT_THRESHOLD: f64 = 0.5;

/// What an administrator reads of the company's decision service.
pub struct DecisionsOverview {
    pub jev_chosen: bool,
    pub engines: DecisionEngines,
    pub has_jev_key: bool,
    /// Each usage, as set or by default.
    pub usages: Vec<(DecisionUsage, UsagePolicy)>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub async fn company_decisions(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
) -> anyhow::Result<DecisionsOverview> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = db.begin(tenant_id).await?;
    let settings = decision_settings(&mut tx).await?;
    let stored = decision_usages(&mut tx).await?;
    let has_jev_key = llm.secrets().exists(&mut tx, TYPESAFE_API_KEY).await?;
    tx.commit().await?;
    let engines = llm.decision_engines(tenant_id).await?;
    let usages = DecisionUsage::ALL
        .into_iter()
        .map(|usage| {
            let row = stored.iter().find(|row| row.usage == usage.as_str());
            let policy = UsagePolicy {
                mode: row
                    .and_then(|row| DecisionMode::parse(&row.mode))
                    .unwrap_or_default(),
                threshold: row.map_or(DEFAULT_THRESHOLD, |row| row.threshold),
            };
            (usage, policy)
        })
        .collect();
    Ok(DecisionsOverview {
        jev_chosen: settings.as_ref().is_some_and(|row| row.engine == "jev"),
        engines,
        has_jev_key,
        usages,
        updated_at: settings.map(|row| row.updated_at),
    })
}

/// Sets the company's engine and usages, as they keep to the rules.
pub async fn set_company_decisions(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
    update: &DecisionSettingsUpdate,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let jev = update.engine == DecisionEngineChoice::Jev;
    if (jev || update.jev_api_key.is_some()) && !llm.decision_config().jev_allowed {
        return Err(DomainError::validation(JEV_CLOSED).into());
    }
    if let Some(problem) = update.jev_api_key.as_deref().and_then(api_key_problem) {
        return Err(DomainError::validation(problem).into());
    }
    if update.jev_api_key.is_some() {
        llm.secrets().require()?;
    }
    let mut usages = Vec::new();
    for asked in &update.usages {
        let usage = DecisionUsage::from(asked.key);
        let policy = UsagePolicy {
            mode: asked.mode.into(),
            threshold: asked.threshold,
        };
        if let Some(problem) = usage_problem(usage, &policy) {
            return Err(DomainError::validation(problem).into());
        }
        if usages
            .iter()
            .any(|row: &DecisionUsageRow| row.usage == usage.as_str())
        {
            return Err(DomainError::validation("Un usage est réglé deux fois").into());
        }
        usages.push(DecisionUsageRow {
            usage: usage.as_str().into(),
            mode: policy.mode.as_str().into(),
            threshold: policy.threshold,
        });
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    if let Some(key) = &update.jev_api_key {
        llm.secrets()
            .put(&mut tx, tenant_id, TYPESAFE_API_KEY, key)
            .await?;
    }
    if jev && !llm.secrets().exists(&mut tx, TYPESAFE_API_KEY).await? {
        return Err(
            DomainError::validation("Saisissez la clé d’API TypeSafe de votre entreprise").into(),
        );
    }
    let engine = if jev { "jev" } else { "main_model" };
    set_decision_settings(&mut tx, tenant_id, engine, &usages, access.subject.user_id).await?;
    let audited: Vec<_> = usages
        .iter()
        .map(|row| json!({ "usage": row.usage, "mode": row.mode, "threshold": row.threshold }))
        .collect();
    append_audit(
        &mut tx,
        audit_entry(access, "decision_service.configured", "tenant", tenant_id).metadata(json!({
            "engine": engine,
            "usages": audited,
            "keyReplaced": update.jev_api_key.is_some(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Back to the main model, every usage off, the key to TypeSafe removed.
pub async fn reset_company_decisions(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(db, access, PERMISSION).await?;
    let removed = remove_decision_settings(&mut tx).await?;
    let key_removed = llm.secrets().delete(&mut tx, TYPESAFE_API_KEY).await?;
    if !removed && !key_removed {
        return Err(DomainError::not_found("Aucun réglage du service de décision").into());
    }
    append_audit(
        &mut tx,
        audit_entry(access, "decision_service.reset", "tenant", tenant_id),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// What the engine's test asks.
fn test_request() -> DecisionRequest {
    DecisionRequest {
        usage: ENGINE_TEST,
        state: DecisionState::new().field("message", "Bonjour, comment allez-vous ?", 100),
        questions: vec![DecisionQuestion::noul(
            "salutation",
            "Ce message est-il une salutation ?",
        )],
    }
}

/// What the engine chosen answered to the test.
pub struct DecisionTested {
    pub engine: EngineKind,
    pub model: String,
    pub latency_ms: u64,
    /// The probability of yes.
    pub yes: f64,
}

/// The engine chosen, asked a fixed question on a fixed state: what it
/// answered, or why it does not (in French).
pub async fn test_company_decisions(
    db: &Database,
    llm: &LlmService,
    access: &AccessContext,
) -> anyhow::Result<DecisionTested> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let scope = DecisionScope {
        run_id: None,
        user_id: Some(access.subject.user_id),
    };
    let service = llm
        .chosen_decision_engine(tenant_id, scope)
        .await?
        .map_err(DomainError::validation)?;
    // The main model asks nothing on the operator's key past its supply: the
    // test is refused and traced as a run's start would be (ADR-208).
    if service.engines() == [EngineKind::MainModel] {
        let models = llm.resolve(tenant_id).await?;
        if let Some(stop) = supply_stopped(db, &models, tenant_id, ModelRole::Decision).await? {
            trace_held_back(db, tenant_id, access.subject.user_id, stop).await?;
            return Err(DomainError::unavailable(stop.message()).into());
        }
    }
    let outcome = service.decide(&test_request()).await;
    let decided = match &outcome {
        DecisionOutcome::Decided(decided) => decided.yes("salutation").map(|yes| (decided, yes)),
        DecisionOutcome::Failed => None,
    };
    let mut tx = db.begin(tenant_id).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "decision_service.tested", "tenant", tenant_id).metadata(json!({
            "engines": service.engines().iter().map(|engine| engine.as_str()).collect::<Vec<_>>(),
            "ok": decided.is_some(),
        })),
    )
    .await?;
    tx.commit().await?;
    let (decided, yes) = decided.ok_or_else(|| {
        DomainError::unavailable(
            "Le moteur n’a pas répondu : vérifiez sa clé ou réessayez dans un instant",
        )
    })?;
    Ok(DecisionTested {
        engine: decided.engine,
        model: decided.model.clone(),
        latency_ms: decided.latency_ms,
        yes,
    })
}
