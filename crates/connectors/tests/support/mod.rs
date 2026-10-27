//! Shared fixtures: the demo connector behind the policy enforcement point,
//! and the reference scenario's scripted run.
#![allow(dead_code)] // not every test binary uses every fixture

pub mod mcp;
pub mod microsoft365;
pub mod rest;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, TimeZone, Utc};
use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::llm::{LlmMessage, ModelRouter, PriceTable};
use mimorya_agent::runtime::{
    AgentRuntime, DEFAULT_BUDGET, MemoryStore, RunConfig, RunOutcome, RuntimeDeps, SkillStore,
    StartInput, UserProfileContext, VerificationConfig, build_system_prompt, build_user_turn,
};
use mimorya_agent::testing::{
    InMemoryPorts, RecordedRequest, ScriptedProvider, ScriptedStep, SkillFixture, TestAccess,
    memory_fixture,
};
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor, ToolRegistry};
use mimorya_connectors::demo_interventions::{DemoInterventionsStore, demo_intervention_tools};
use mimorya_core::authz::{Actor, AuthorizationEngine};
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_core::demo::{self, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, memory_read_filter};
use mimorya_core::tools::{ToolRef, ToolResult, ToolServices};
use serde_json::{Value, json};
use uuid::Uuid;

/// Wednesday 14 October 2026: the dataset's week runs from 12 to 16 October.
pub fn reference() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()
}

pub fn executor(tools: Vec<ToolRef>, user_id: Uuid, ports: &Arc<InMemoryPorts>) -> ToolExecutor {
    executor_for(
        ToolRegistry::new(tools, &HashMap::new()).unwrap(),
        user_id,
        ports,
    )
}

pub fn executor_for(
    registry: ToolRegistry,
    user_id: Uuid,
    ports: &Arc<InMemoryPorts>,
) -> ToolExecutor {
    executor_with_engine(registry, user_id, ports, demo::authorization_engine())
}

pub fn executor_with_engine(
    registry: ToolRegistry,
    user_id: Uuid,
    ports: &Arc<InMemoryPorts>,
    engine: AuthorizationEngine,
) -> ToolExecutor {
    let run_id = Uuid::new_v4();
    ToolExecutor::new(
        Arc::new(engine),
        Arc::new(registry),
        Arc::new(demo::subject(user_id)),
        Actor::agent(Some(run_id)),
        AutonomyPolicy::new(AutonomyLevel::ExecuteWithApproval),
        run_id,
        ToolServices {
            deliverables: ports.ports().deliverables,
            accounts: ports.ports().accounts,
        },
        Duration::from_secs(30),
    )
}

pub struct ConnectorHarness {
    pub store: Arc<DemoInterventionsStore>,
    pub ports: Arc<InMemoryPorts>,
    pub executor: ToolExecutor,
}

impl ConnectorHarness {
    pub fn new(user_id: Uuid) -> Self {
        Self::with_engine(user_id, demo::authorization_engine())
    }

    /// The same, under another authorization (extra policies, for instance).
    pub fn with_engine(user_id: Uuid, engine: AuthorizationEngine) -> Self {
        let store = Arc::new(DemoInterventionsStore::new(reference()));
        let ports = InMemoryPorts::new(user_id, Uuid::new_v4());
        let registry =
            ToolRegistry::new(demo_intervention_tools(store.clone()), &HashMap::new()).unwrap();
        let executor = executor_with_engine(registry, user_id, &ports, engine);
        Self {
            store,
            ports,
            executor,
        }
    }

    /// Evaluates a call; executes it when allowed.
    pub async fn call(&self, name: &str, input: Value) -> (ToolCallEvaluation, Option<ToolResult>) {
        let evaluation = self.executor.evaluate(&LlmToolCall {
            id: "c".into(),
            name: name.into(),
            input,
        });
        let result = match &evaluation {
            ToolCallEvaluation::Allow { tool, input } => {
                Some(self.executor.execute(tool, input.clone()).await.0)
            }
            _ => None,
        };
        (evaluation, result)
    }
}

pub fn ids(result: &ToolResult) -> Vec<String> {
    result.content["interventions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

pub const WEEK: (&str, &str) = ("2026-10-12", "2026-10-16");

pub fn week() -> Value {
    json!({ "from": WEEK.0, "to": WEEK.1 })
}

pub const REQUEST: &str = "Analyse les comptes rendus des interventions de cette semaine, identifie les interventions non terminées, prépare une synthèse et propose un planning de suivi pour mon équipe.";

pub struct Scenario {
    pub runtime: AgentRuntime,
    pub ports: Arc<InMemoryPorts>,
    pub provider: Arc<ScriptedProvider>,
    pub store: Arc<DemoInterventionsStore>,
    user_id: Uuid,
}

impl Scenario {
    pub fn new(user_id: Uuid, steps: Vec<ScriptedStep>) -> Self {
        let subject = Arc::new(demo::subject(user_id));
        let run_id = Uuid::new_v4();
        let mut technical = memory_fixture(
            MemoryScope::OrgUnit,
            MemoryKind::Procedure,
            "Clôture d’une intervention",
            "Une intervention n’est terminée que si le compte rendu est signé et qu’aucune pièce n’est en attente.",
        );
        technical.org_unit_id = Some(unit::TECHNICAL);
        let mut personal = memory_fixture(
            MemoryScope::User,
            MemoryKind::Preference,
            "Déplacements",
            "Lucas souhaite être prévenu la veille de ses interventions.",
        );
        personal.owner_user_id = Some(user::LUCAS);
        let ports = InMemoryPorts::with_fixtures(
            user_id,
            run_id,
            vec![technical, personal],
            vec![SkillFixture {
                name: "synthese-hebdo-interventions".into(),
                description: "Synthèse hebdomadaire des interventions et planning de suivi".into(),
                content: "# Synthèse hebdomadaire\n1. Rechercher… 6. Présenter…".into(),
                quality_criteria: Vec::new(),
            }],
        );
        let engine = Arc::new(demo::authorization_engine());
        let store = Arc::new(DemoInterventionsStore::new(reference()));
        let engine_ports = ports.ports();
        let mut tools = mimorya_agent::tools::platform::platform_tools(
            engine_ports.memory.clone(),
            engine_ports.skills.clone(),
            engine_ports.documents.clone(),
        );
        tools.extend(demo_intervention_tools(store.clone()));
        let provider = Arc::new(ScriptedProvider::new(steps));
        let mut deps = RuntimeDeps::new(
            ModelRouter::new(provider.clone()),
            TestAccess::fixed(engine, subject),
            Arc::new(ToolRegistry::new(tools, &HashMap::new()).unwrap()),
            engine_ports,
            Arc::new(PriceTable::default()),
        );
        deps.retry_delay = Duration::from_millis(1);
        let config = RunConfig {
            run_id,
            actor: Actor::agent(Some(run_id)),
            objective: REQUEST.into(),
            autonomy: AutonomyPolicy::new(AutonomyLevel::ExecuteWithApproval),
            budget: DEFAULT_BUDGET,
            success_criteria: Vec::new(),
            verification: VerificationConfig {
                llm_judge: false,
                skill_criteria: true,
                max_correction_attempts: 2,
            },
            effort: None,
            handoff: false,
        };
        Self {
            runtime: AgentRuntime::new(deps, config),
            ports,
            provider,
            store,
            user_id,
        }
    }

    pub async fn start(&self) -> RunOutcome {
        let engine = demo::authorization_engine();
        let subject = demo::subject(self.user_id);
        let filter = memory_read_filter(&engine, &subject, &Actor::user());
        let memories = MemoryStore::search(
            &*self.ports,
            &filter,
            "interventions terminée compte rendu",
            5,
            subject.clearance(),
        )
        .await
        .unwrap();
        let system = build_system_prompt(
            &UserProfileContext {
                assistant_name: "Mimorya".into(),
                tenant_name: "Néo Maintenance (démo)".into(),
                display_name: "Marc Lefèvre".into(),
                job_title: Some("Responsable technique".into()),
                org_unit_path: Some(engine.org_tree().path_label(unit::TECHNICAL)),
                roles: vec!["Responsable d’équipe — Service technique et sous-unités".into()],
            },
            &SkillStore::list_active(&*self.ports).await.unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 14, 10, 0, 0).unwrap(),
        );
        self.runtime
            .start(StartInput {
                system,
                history: Vec::new(),
                user_turn: build_user_turn(REQUEST, &memories),
                unavailable_connectors: Vec::new(),
                evidence: Vec::new(),
            })
            .await
            .unwrap()
    }
}

pub fn tool_names(request: &RecordedRequest) -> Vec<String> {
    let mut names = request.tool_names();
    names.sort();
    names
}

/// The JSON payload inside the untrusted envelope of a tool result.
pub fn parse_result(content: &str) -> Value {
    let start = content.find('\n').unwrap() + 1;
    let end = content.rfind('\n').unwrap();
    serde_json::from_str(&content[start..end]).unwrap()
}

pub fn first_user_turn(request: &RecordedRequest) -> String {
    match &request.messages[0] {
        LlmMessage::User { content } => content.clone(),
        other => panic!("expected a user turn, got {other:?}"),
    }
}

/// Valid follow-up planning for the technical manager (week after the reference date).
pub fn marc_plan() -> Vec<Value> {
    let slot = |intervention: &str,
                technician: Uuid,
                date: &str,
                start: &str,
                hours: f64,
                reason: &str| {
        json!({
            "interventionId": intervention,
            "technicianId": technician,
            "date": date,
            "startTime": start,
            "durationHours": hours,
            "reason": reason,
        })
    };
    vec![
        slot(
            "INT-2042",
            user::EMMA,
            "2026-10-19",
            "09:00",
            2.0,
            "Remplacement de la vanne Schrader",
        ),
        slot(
            "INT-2043",
            user::LUCAS,
            "2026-10-19",
            "13:00",
            2.0,
            "Pose du circulateur Grundfos",
        ),
        slot(
            "INT-2046",
            user::LUCAS,
            "2026-10-20",
            "09:00",
            2.0,
            "Diagnostic et compte rendu manquant",
        ),
        slot(
            "INT-2049",
            user::LEA,
            "2026-10-19",
            "10:00",
            2.0,
            "Nouveau passage avec le gardien",
        ),
        slot(
            "INT-2051",
            user::NADIA,
            "2026-10-20",
            "09:00",
            3.0,
            "Remplacement de la pompe après validation du devis",
        ),
        slot(
            "INT-2052",
            user::LEA,
            "2026-10-20",
            "14:00",
            2.0,
            "Entretien à reprogrammer (client absent)",
        ),
    ]
}
