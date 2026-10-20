//! What a run is given to work with: the tools in reach of the user (and
//! the connectors in reach that could not start), the system prompt, and the
//! memories relevant to the request.

use chrono::{DateTime, Utc};
use mimorya_agent::runtime::{
    EnginePorts, MemoryHit, SkillSummary, UserProfileContext, build_system_prompt,
};
use mimorya_agent::tools::ToolRegistry;
use mimorya_agent::tools::platform::platform_tools;
use mimorya_core::authz::Actor;
use mimorya_core::memory::{MemoryKind, MemoryScope, memory_read_filter};
use mimorya_db::Database;
use mimorya_db::repositories::memory::{MemorySearchHit, mark_memory_used, search_memory};
use mimorya_db::repositories::skills::list_active_skills;

use crate::services::access::AccessContext;
use crate::services::connectors::{ConnectorService, unavailable_in_reach};

const MEMORY_RECALL_LIMIT: i64 = 5;

/// The tools of a run, and the connectors in reach of its user that could
/// not start (ADR-048).
pub struct RunTools {
    pub registry: ToolRegistry,
    pub unavailable_connectors: Vec<String>,
}

pub async fn build_run_tools(
    access: &AccessContext,
    actor: &Actor,
    ports: &EnginePorts,
    connectors: &ConnectorService,
) -> anyhow::Result<RunTools> {
    let connector_tools = connectors.tools_for(access.subject.tenant_id).await?;
    let holds = |permission: &str| access.engine.has(&access.subject, permission, actor);
    let unavailable_connectors = unavailable_in_reach(&connector_tools.issues, holds);
    let mut tools = platform_tools(ports.memory.clone(), ports.skills.clone());
    tools.extend(connector_tools.tools);
    let registry = ToolRegistry::new(tools, &connector_tools.overrides)?;
    Ok(RunTools {
        registry,
        unavailable_connectors,
    })
}

/// The system prompt; the skills index only when the agent may read skills.
pub async fn build_run_system_prompt(
    db: &Database,
    access: &AccessContext,
    actor: &Actor,
    assistant_name: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<String> {
    let skills = if access.engine.has(&access.subject, "skill:read", actor) {
        let mut tx = db.begin(access.subject.tenant_id).await?;
        let skills = list_active_skills(&mut tx).await?;
        tx.commit().await?;
        skills
    } else {
        Vec::new()
    };
    let skills: Vec<SkillSummary> = skills
        .into_iter()
        .map(|s| SkillSummary {
            name: s.name,
            description: s.description,
        })
        .collect();
    let profile = UserProfileContext {
        assistant_name: assistant_name.to_string(),
        tenant_name: access.tenant.name.clone(),
        display_name: access.profile.display_name.clone(),
        job_title: access.profile.job_title.clone(),
        org_unit_path: access
            .profile
            .primary_unit_id
            .map(|unit| access.org_tree().path_label(unit)),
        roles: access.grants.clone(),
    };
    Ok(build_system_prompt(&profile, &skills, now))
}

/// Memories the agent may read for this user, ranked against the request.
pub async fn recall_memories(
    db: &Database,
    access: &AccessContext,
    actor: &Actor,
    text: &str,
) -> anyhow::Result<Vec<MemoryHit>> {
    let filter = memory_read_filter(&access.engine, &access.subject, actor);
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let hits = search_memory(
        &mut tx,
        &filter,
        text,
        MEMORY_RECALL_LIMIT,
        access.subject.clearance(),
    )
    .await?;
    let hits: Vec<MemoryHit> = hits.into_iter().filter_map(memory_hit).collect();
    let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
    mark_memory_used(&mut tx, &ids).await?;
    tx.commit().await?;
    Ok(hits)
}

/// A stored search hit as the engine sees it (rows with unknown values are skipped).
pub fn memory_hit(row: MemorySearchHit) -> Option<MemoryHit> {
    Some(MemoryHit {
        id: row.id,
        scope: MemoryScope::parse(&row.scope)?,
        kind: MemoryKind::parse(&row.kind)?,
        title: row.title,
        content: row.content,
        org_unit_id: row.org_unit_id,
        updated_at: row.updated_at,
        score: f64::from(row.score),
    })
}
