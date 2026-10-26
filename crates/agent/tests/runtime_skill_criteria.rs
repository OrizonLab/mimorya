//! Runtime: an answer that followed a skill is reviewed by a model on the
//! quality criteria of the version loaded (ADR-106), even when the platform
//! reviews no other answer, and corrected when it falls short. Only the
//! platform's `skill_read` adds criteria; a skill without any adds no
//! review; a run that does not apply them (an evaluation's) keeps them
//! unreviewed; a sub-agent's summary is not reviewed on the criteria of the
//! skill it loaded, nor its parent's answer; they are kept while the run
//! waits for a validation; the agent is told them with the procedure.

mod support;

use std::collections::HashMap;

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{ApprovalDecision, RunCheckpoint, RunOutcome, VerificationReport};
use mimorya_agent::testing::{ScriptedStep, SkillFixture, call};
use serde_json::json;
use support::{FORGED_CRITERION, Options, setup};

const SKILL: &str = "synthese-datee";
const CRITERION: &str = "Donne la date de chaque intervention.";

fn skill(criteria: &[&str]) -> SkillFixture {
    SkillFixture {
        name: SKILL.into(),
        description: "Synthèse datée des interventions".into(),
        content: "1. Lister les interventions et leur date.".into(),
        quality_criteria: criteria.iter().map(|c| (*c).to_owned()).collect(),
    }
}

fn load() -> ScriptedStep {
    ScriptedStep::calls("", vec![call("skill_read", json!({ "name": SKILL }))])
}

fn verdict(pass: bool, issues: &[&str]) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_verdict",
            json!({ "pass": pass, "issues": issues }),
        )],
    )
}

/// Options where nothing but a skill's criteria asks for a review.
fn options(skills: Vec<SkillFixture>, judge: Vec<ScriptedStep>) -> Options {
    Options {
        platform: Some(Vec::new()),
        skills,
        judge: Some(judge),
        llm_judge: Some(false),
        ..Default::default()
    }
}

fn user_text(message: &LlmMessage) -> String {
    match message {
        LlmMessage::User { content } => content.clone(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn an_answer_that_followed_a_skill_is_reviewed_on_its_criteria_and_corrected() {
    let h = setup(
        vec![
            load(),
            ScriptedStep::text("Deux interventions cette semaine."),
            ScriptedStep::text("Deux interventions : le 3 et le 5 mars."),
        ],
        options(
            vec![skill(&[CRITERION])],
            vec![verdict(false, &["Les dates manquent."]), verdict(true, &[])],
        ),
    );
    let RunOutcome::Completed {
        answer,
        verification,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert_eq!(answer, "Deux interventions : le 3 et le 5 mars.");
    assert_eq!(
        verification,
        VerificationReport {
            passed: true,
            issues: vec![],
            judged: true
        }
    );
    let judged = h.judge.as_ref().unwrap().requests();
    assert_eq!(judged.len(), 2);
    let asked = user_text(&judged[0].messages[0]);
    assert!(asked.contains(&format!("1. {CRITERION}")), "{asked}");
    // Told the criteria with the procedure, then what fell short.
    let loaded = format!("{:?}", h.provider.requests()[1].messages.last());
    assert!(
        loaded.contains(&format!(
            "## Critères de qualité de la réponse\\n- {CRITERION}"
        )),
        "{loaded}"
    );
    let corrected = format!("{:?}", h.provider.requests()[2].messages.last());
    assert!(corrected.contains("Les dates manquent."), "{corrected}");
}

#[tokio::test]
async fn a_skill_without_criteria_adds_no_review() {
    let h = setup(
        vec![load(), ScriptedStep::text("Deux interventions.")],
        options(vec![skill(&[])], Vec::new()),
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    assert!(!verification.judged);
    assert!(h.judge.as_ref().unwrap().requests().is_empty());
}

#[tokio::test]
async fn a_run_that_does_not_apply_them_is_not_reviewed_on_them() {
    // As an evaluation's case, judged afterwards on them (ADR-104).
    let mut options = options(vec![skill(&[CRITERION])], Vec::new());
    options.skill_criteria = Some(false);
    let h = setup(
        vec![load(), ScriptedStep::text("Deux interventions.")],
        options,
    );
    let RunOutcome::Completed {
        verification,
        checkpoint,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(!verification.judged);
    assert!(h.judge.as_ref().unwrap().requests().is_empty());
    // Kept for whoever judges the answer.
    assert_eq!(checkpoint.skill_criteria, [CRITERION]);
}

#[tokio::test]
async fn a_sub_agent_summary_is_not_reviewed_on_the_skill_criteria() {
    // The platform's tools include delegation.
    let options = options(vec![skill(&[CRITERION])], Vec::new());
    let delegate = call(
        "delegate_task",
        json!({ "profile": "research", "task": "Lister les interventions datées" }),
    );
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![delegate]),
            // The sub-agent loads the skill, then sums up for its parent.
            load(),
            ScriptedStep::text("Deux interventions."),
            ScriptedStep::text("Synthèse : deux interventions."),
        ],
        options,
    );
    let RunOutcome::Completed { verification, .. } = h.start().await else {
        panic!("expected completion")
    };
    // Neither the summary nor the parent's answer, which loaded nothing.
    assert!(!verification.judged);
    assert!(h.judge.as_ref().unwrap().requests().is_empty());
    let child = format!("{:?}", h.provider.requests()[2].messages.last());
    assert!(child.contains("procedure_entreprise"), "{child}");
}

#[tokio::test]
async fn no_tool_but_skill_read_adds_criteria() {
    let h = setup(
        vec![
            ScriptedStep::calls("", vec![call("doc_read", json!({}))]),
            ScriptedStep::text("Rapport lu."),
        ],
        options(Vec::new(), Vec::new()),
    );
    let RunOutcome::Completed {
        verification,
        checkpoint,
        ..
    } = h.start().await
    else {
        panic!("expected completion")
    };
    assert!(!verification.judged);
    assert!(checkpoint.skill_criteria.is_empty(), "{FORGED_CRITERION}");
    assert!(h.judge.as_ref().unwrap().requests().is_empty());
}

#[tokio::test]
async fn the_criteria_are_kept_while_the_run_waits_for_a_validation() {
    let h = setup(
        vec![
            ScriptedStep::calls(
                "",
                vec![
                    call("skill_read", json!({ "name": SKILL })),
                    call("planning_apply", json!({ "slots": 1 })),
                ],
            ),
            ScriptedStep::text("Planning appliqué le 3 mars."),
        ],
        options(vec![skill(&[CRITERION])], vec![verdict(true, &[])]),
    );
    let RunOutcome::AwaitingApproval {
        approvals,
        checkpoint,
    } = h.start().await
    else {
        panic!("expected a pause")
    };
    // As saved, then read back when the decision comes.
    let saved: RunCheckpoint =
        serde_json::from_value(serde_json::to_value(&checkpoint).unwrap()).unwrap();
    assert_eq!(saved.skill_criteria, [CRITERION]);
    let decisions = HashMap::from([(approvals[0].approval_id, ApprovalDecision::approved())]);
    let outcome = h.runtime.resume(saved, &decisions).await.unwrap();
    let RunOutcome::Completed { verification, .. } = outcome else {
        panic!("expected completion")
    };
    assert!(verification.judged);
    let asked = user_text(&h.judge.as_ref().unwrap().requests()[0].messages[0]);
    assert!(asked.contains(CRITERION), "{asked}");
}
