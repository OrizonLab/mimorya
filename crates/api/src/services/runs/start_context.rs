//! What a run starts with besides its request (ADR-213, D2): its system
//! prompt, and the skills its first turn lists when the company's decision
//! service chooses them. Off, the system prompt lists every skill the person
//! may read, as before. Observed, the decision is asked and what it would
//! leave out journaled, nothing else changed. Active, the system prompt
//! never lists them, so that it stays the same from one run to the next,
//! and the first turn lists those said useful, or every one on a doubt or a
//! failure. `skill_read` loads any active skill whatever was listed.

use chrono::{DateTime, Utc};
use mimorya_agent::decision::{
    Counted, DecisionMode, DecisionOutcome, DecisionUsage, RunDecisions, UsagePolicy,
};
use mimorya_agent::runtime::{
    MAX_ASKED_SKILLS, SkillSummary, SkillsShown, build_system_prompt, chosen_skills,
    skill_choice_request, skills_section,
};
use mimorya_core::authz::Actor;
use mimorya_db::Database;

use super::context::{readable_skills, user_profile};
use crate::services::access::AccessContext;

pub struct StartContext {
    pub system: String,
    /// The skills the first turn lists: some only when the choice is active.
    pub shown: Vec<SkillSummary>,
    /// What the decision cost, counted in the run.
    pub decided: Vec<Counted>,
}

/// Whom a run starts for, and what it asks.
pub struct Starting<'a> {
    pub access: &'a AccessContext,
    pub actor: &'a Actor,
    pub assistant_name: &'a str,
    pub request: &'a str,
}

pub async fn start_context(
    db: &Database,
    starting: Starting<'_>,
    decisions: Option<&RunDecisions>,
    now: DateTime<Utc>,
) -> anyhow::Result<StartContext> {
    let skills = readable_skills(db, starting.access, starting.actor).await?;
    let profile = user_profile(starting.access, starting.assistant_name);
    let set = decisions
        .map(|decisions| (decisions, decisions.policy(DecisionUsage::SkillChoice)))
        .filter(|(_, policy)| policy.mode != DecisionMode::Off);
    let Some((decisions, policy)) = set else {
        return Ok(StartContext {
            system: build_system_prompt(&profile, &skills, now),
            shown: Vec::new(),
            decided: Vec::new(),
        });
    };
    let (chosen, why, decided) = choose(decisions, policy, starting.request, &skills).await;
    let shown = match chosen {
        SkillsShown::All => skills.clone(),
        SkillsShown::Chosen(chosen) => chosen,
    };
    journal(policy.mode, why, &skills, &shown);
    Ok(match policy.mode {
        DecisionMode::Active => StartContext {
            system: build_system_prompt(&profile, &[], now),
            shown,
            decided,
        },
        _ => StartContext {
            system: build_system_prompt(&profile, &skills, now),
            shown: Vec::new(),
            decided,
        },
    })
}

/// The skills said useful; all of them when nothing is to be asked, or
/// nothing decided; and why, as journaled.
async fn choose(
    decisions: &RunDecisions,
    policy: UsagePolicy,
    request: &str,
    skills: &[SkillSummary],
) -> (SkillsShown, &'static str, Vec<Counted>) {
    let Some(asked) = skill_choice_request(request, skills) else {
        let why = if skills.len() > MAX_ASKED_SKILLS {
            "too_many"
        } else {
            "none"
        };
        return (SkillsShown::All, why, Vec::new());
    };
    let (outcome, counted) = decisions.service.decide_counted(&asked).await;
    let (chosen, why) = match outcome {
        DecisionOutcome::Decided(decided) => {
            match chosen_skills(&decided, skills, policy.threshold) {
                SkillsShown::All => (SkillsShown::All, "doubt"),
                chosen => (chosen, "decided"),
            }
        }
        DecisionOutcome::Failed => (SkillsShown::All, "failed"),
    };
    (chosen, why, counted)
}

/// What the choice leaves out of the context, measured: counts and sizes,
/// never the request.
fn journal(mode: DecisionMode, why: &str, skills: &[SkillSummary], shown: &[SkillSummary]) {
    if skills.is_empty() {
        return;
    }
    let chars = |listed: &[SkillSummary]| skills_section("", listed).chars().count();
    tracing::info!(
        target: "mimorya::decision",
        usage = DecisionUsage::SkillChoice.as_str(),
        mode = mode.as_str(),
        reason = why,
        skills = skills.len(),
        shown = shown.len(),
        index_chars = chars(skills),
        shown_chars = if shown.is_empty() { 0 } else { chars(shown) },
        "Skills chosen for the run"
    );
}
