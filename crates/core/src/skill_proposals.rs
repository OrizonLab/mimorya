//! What the agent proposes of the company's skills (ADR-100): a new version
//! of an active skill, or a new skill, written with the person it serves,
//! who validates its exact text before it reaches the skills'
//! administrators. Nothing reaches a run before one of them makes it
//! active. Pure rules; the ports apply them where the skills are kept.

/// Proposals awaiting the administrators in one company: beyond, the agent
/// proposes nothing more until they decide on some.
pub const MAX_PENDING_PROPOSALS: i64 = 20;

/// A procedure the agent proposes, as the person validated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillProposal {
    /// The active skill a version is proposed for, or the new skill's name.
    pub name: String,
    /// What a new skill covers; none for a version of an active one.
    pub description: Option<String>,
    /// The whole procedure, normalized (`skill_rules::normalized_content`).
    pub content: String,
    /// Why, in one line: the version's note.
    pub rationale: String,
}

/// The company's skill bearing a proposal's name, as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedSkill {
    /// No skill bears it.
    Unknown,
    /// An active skill: runs read it.
    Active,
    /// A draft or a retired skill.
    Inactive,
}

/// Where a proposal is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalTarget {
    /// A version of the active skill it names.
    NewVersion,
    /// A new skill, a draft, whose first version it is.
    NewSkill,
}

/// What became of a proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalOutcome {
    /// Recorded for the administrators: where, and its version's number.
    Proposed {
        target: ProposalTarget,
        version: i32,
    },
    /// Refused: why, in French.
    Refused(String),
}

/// Where `proposal` goes, the skill it names and the proposals pending as
/// they stand; or why it is refused, in French. Skill names are no secret
/// within the company: a refusal may say one is taken.
pub fn proposal_target(
    proposal: &SkillProposal,
    named: NamedSkill,
    pending: i64,
) -> Result<ProposalTarget, String> {
    if pending >= MAX_PENDING_PROPOSALS {
        return Err(format!(
            "{MAX_PENDING_PROPOSALS} propositions attendent déjà les administrateurs des \
             compétences : proposez de nouveau une fois qu’ils en auront décidé."
        ));
    }
    let name = &proposal.name;
    match (named, proposal.description.is_some()) {
        (NamedSkill::Active, false) => Ok(ProposalTarget::NewVersion),
        (NamedSkill::Unknown, true) => Ok(ProposalTarget::NewSkill),
        (NamedSkill::Active, true) => Err(format!(
            "La compétence « {name} » existe : proposez-en une nouvelle version, sans description."
        )),
        (NamedSkill::Inactive, _) => Err(format!(
            "Une compétence de l’entreprise porte déjà le nom « {name} » sans être active : \
             proposez un autre nom."
        )),
        (NamedSkill::Unknown, false) => Err(format!(
            "Aucune compétence active ne s’appelle « {name} » : pour en proposer une nouvelle, \
             donnez aussi sa description."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(description: Option<&str>) -> SkillProposal {
        SkillProposal {
            name: "synthese-hebdo".into(),
            description: description.map(Into::into),
            content: "1. Lister les interventions.".into(),
            rationale: "Ajoute l’étape de relecture.".into(),
        }
    }

    #[test]
    fn a_version_goes_to_an_active_skill_and_a_new_skill_to_a_free_name() {
        let version = proposal(None);
        let new = proposal(Some("Synthèse hebdomadaire des interventions"));
        assert_eq!(
            proposal_target(&version, NamedSkill::Active, 0),
            Ok(ProposalTarget::NewVersion)
        );
        assert_eq!(
            proposal_target(&new, NamedSkill::Unknown, 0),
            Ok(ProposalTarget::NewSkill)
        );
    }

    #[test]
    fn a_name_taken_otherwise_or_unknown_without_description_is_refused() {
        let version = proposal(None);
        let new = proposal(Some("Synthèse hebdomadaire des interventions"));
        let refused = |p: &SkillProposal, named| proposal_target(p, named, 0).unwrap_err();
        assert!(refused(&new, NamedSkill::Active).contains("proposez-en une nouvelle version"));
        assert!(refused(&new, NamedSkill::Inactive).contains("sans être active"));
        assert!(refused(&version, NamedSkill::Inactive).contains("sans être active"));
        assert!(refused(&version, NamedSkill::Unknown).contains("donnez aussi sa description"));
    }

    #[test]
    fn nothing_more_is_proposed_while_too_many_wait() {
        let version = proposal(None);
        let full = MAX_PENDING_PROPOSALS;
        assert!(proposal_target(&version, NamedSkill::Active, full - 1).is_ok());
        let refused = proposal_target(&version, NamedSkill::Active, full).unwrap_err();
        assert!(refused.contains("attendent déjà"), "{refused}");
    }
}
