//! The skills a run's request needs, asked of the decision service
//! (ADR-213, D2): one yes or no per skill the person may read, on the
//! request alone. Said useful at the company's threshold, a skill is listed
//! with the request; said useless as surely, it is left out; any doubt, a
//! failure, or more skills than are asked about at once, and they are all
//! listed, as without a decision. Listing fewer only shortens the hint:
//! `skill_read` still loads any active skill, and checks the reader's
//! right, so a decision restricts nothing.

use crate::decision::{Decided, DecisionQuestion, DecisionRequest, DecisionState, DecisionUsage};

use super::ports::SkillSummary;

/// At most so many skills are asked about at once; more, and all are listed.
pub const MAX_ASKED_SKILLS: usize = 20;
/// What the decision reads of the request, at most.
const MAX_REQUEST_CHARS: usize = 2_000;

/// Which skills a run lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillsShown {
    /// Every one: no decision, or a doubt.
    All,
    /// Those said useful, perhaps none.
    Chosen(Vec<SkillSummary>),
}

fn question_name(at: usize) -> String {
    format!("competence_{}", at + 1)
}

/// What the decision service is asked: `None` when there is nothing to
/// choose from, or too much.
pub fn skill_choice_request(objective: &str, skills: &[SkillSummary]) -> Option<DecisionRequest> {
    if skills.is_empty() || skills.len() > MAX_ASKED_SKILLS {
        return None;
    }
    let questions = skills
        .iter()
        .enumerate()
        .map(|(at, skill)| {
            DecisionQuestion::noul(
                &question_name(at),
                &format!(
                    "La compétence « {} » de l’entreprise sert-elle à traiter cette demande ? \
                     Elle sert à : {}",
                    skill.name, skill.description
                ),
            )
        })
        .collect();
    Some(DecisionRequest {
        usage: DecisionUsage::SkillChoice.as_str(),
        state: DecisionState::new().field("demande", objective, MAX_REQUEST_CHARS),
        questions,
    })
}

/// The skills said useful at `threshold`; all of them on a doubt: one said
/// neither useful nor useless as surely, or unanswered.
pub fn chosen_skills(decided: &Decided, skills: &[SkillSummary], threshold: f64) -> SkillsShown {
    let mut chosen = Vec::new();
    for (at, skill) in skills.iter().enumerate() {
        let Some(yes) = decided.yes(&question_name(at)) else {
            return SkillsShown::All;
        };
        if yes >= threshold {
            chosen.push(skill.clone());
        } else if yes + threshold > 1.0 {
            return SkillsShown::All;
        }
    }
    SkillsShown::Chosen(chosen)
}

/// The index of `skills`, under `title`, as the agent reads it.
pub fn skills_section(title: &str, skills: &[SkillSummary]) -> String {
    let list: Vec<String> = skills
        .iter()
        .map(|s| format!("- {} : {}", s.name, s.description))
        .collect();
    format!("## {title}\n{}", list.join("\n"))
}

/// The run's first turn with the skills it lists, before the request.
pub fn with_skills(turn: String, shown: &[SkillSummary]) -> String {
    if shown.is_empty() {
        return turn;
    }
    let section = skills_section("Compétences de l’entreprise utiles à cette demande", shown);
    format!("{section}\n\n{turn}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::decision::{DecisionAnswer, EngineKind};

    fn skills(count: usize) -> Vec<SkillSummary> {
        (0..count)
            .map(|at| SkillSummary {
                name: format!("competence-{at}"),
                description: format!("Tâche {at}"),
            })
            .collect()
    }

    fn decided(yes: &[f64]) -> Decided {
        let answers = yes
            .iter()
            .enumerate()
            .map(|(at, yes)| (question_name(at), DecisionAnswer::Noul { yes: *yes }))
            .collect::<BTreeMap<_, _>>();
        Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers,
        }
    }

    #[test]
    fn one_question_per_skill_on_the_request_alone_within_bounds() {
        let request = skill_choice_request(&"é".repeat(5_000), &skills(3)).unwrap();
        assert!(request.problem().is_none());
        assert_eq!(request.questions.len(), 3);
        assert!(request.questions[1].instructions.contains("competence-1"));
        let [(name, text)] = request.state.fields() else {
            panic!("the request alone")
        };
        assert_eq!(name, "demande");
        assert!(text.chars().count() <= MAX_REQUEST_CHARS);
        assert!(skill_choice_request("x", &[]).is_none());
        assert!(skill_choice_request("x", &skills(MAX_ASKED_SKILLS + 1)).is_none());
    }

    #[test]
    fn the_useful_skills_are_chosen_and_any_doubt_shows_them_all() {
        let all = skills(3);
        let shown = chosen_skills(&decided(&[0.9, 0.1, 0.05]), &all, 0.8);
        assert_eq!(shown, SkillsShown::Chosen(vec![all[0].clone()]));
        assert_eq!(
            chosen_skills(&decided(&[0.1, 0.0, 0.2]), &all, 0.8),
            SkillsShown::Chosen(Vec::new())
        );
        // Neither useful nor useless as surely: a doubt.
        assert_eq!(
            chosen_skills(&decided(&[0.9, 0.5, 0.1]), &all, 0.8),
            SkillsShown::All
        );
        // A skill unanswered: a doubt.
        assert_eq!(
            chosen_skills(&decided(&[0.9, 0.1]), &all, 0.8),
            SkillsShown::All
        );
    }

    #[test]
    fn the_skills_shown_come_before_the_request() {
        let turn = with_skills("Synthèse des interventions".into(), &skills(1));
        assert!(turn.starts_with(
            "## Compétences de l’entreprise utiles à cette demande\n- competence-0 : Tâche 0"
        ));
        assert!(turn.ends_with("Synthèse des interventions"));
        assert_eq!(with_skills("Bonjour".into(), &[]), "Bonjour");
    }
}
