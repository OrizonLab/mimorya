//! What the agent proposes of the company's skills (ADR-100): a new version
//! of an active skill, or a new skill, written with the person it serves,
//! who validates its exact text before it reaches the skills'
//! administrators. Nothing reaches a run before one of them makes it
//! active. Whoever proposes reads the active skills only: a name borne by
//! an inactive skill is answered as one no skill bears. Pure rules; the
//! ports apply them where the skills are kept.

/// Proposals awaiting the administrators in one company: beyond, the agent
/// proposes nothing more until they decide on some.
pub const MAX_PENDING_PROPOSALS: i64 = 20;
/// Of those, the ones one person's runs proposed: no one fills the company's
/// share alone.
pub const MAX_PENDING_PER_PERSON: i64 = 5;

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
    /// A draft or a retired skill, which whoever proposes cannot read.
    Inactive,
}

/// The proposals awaiting the administrators, as they stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    /// In the whole company.
    pub company: i64,
    /// Proposed for the person served.
    pub person: i64,
}

/// Where a proposal is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalTarget {
    /// A version of the active skill it names.
    NewVersion,
    /// A new skill, a draft, whose first version it is.
    NewSkill,
    /// A version of the inactive skill bearing the new skill's name: for
    /// its administrators to decide, answered as a new skill.
    InactiveSkill,
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
/// they stand; or why it is refused, in French. Nothing said tells an
/// inactive skill from a name no skill bears.
pub fn proposal_target(
    proposal: &SkillProposal,
    named: NamedSkill,
    pending: Pending,
) -> Result<ProposalTarget, String> {
    let wait = "proposez de nouveau une fois qu’ils en auront décidé.";
    if pending.company >= MAX_PENDING_PROPOSALS {
        return Err(format!(
            "{MAX_PENDING_PROPOSALS} propositions attendent déjà les administrateurs des \
             compétences : {wait}"
        ));
    }
    if pending.person >= MAX_PENDING_PER_PERSON {
        return Err(format!(
            "{MAX_PENDING_PER_PERSON} propositions faites pour cette personne attendent déjà les \
             administrateurs des compétences : {wait}"
        ));
    }
    let name = &proposal.name;
    match (named, proposal.description.is_some()) {
        (NamedSkill::Active, false) => Ok(ProposalTarget::NewVersion),
        (NamedSkill::Unknown, true) => Ok(ProposalTarget::NewSkill),
        (NamedSkill::Inactive, true) => Ok(ProposalTarget::InactiveSkill),
        (NamedSkill::Active, true) => Err(format!(
            "La compétence « {name} » existe : proposez-en une nouvelle version, sans description."
        )),
        (NamedSkill::Unknown | NamedSkill::Inactive, false) => Err(format!(
            "Aucune compétence active ne s’appelle « {name} » : pour en proposer une nouvelle, \
             donnez aussi sa description."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Pending = Pending {
        company: 0,
        person: 0,
    };

    fn proposal(description: Option<&str>) -> SkillProposal {
        SkillProposal {
            name: "synthese-hebdo".into(),
            description: description.map(Into::into),
            content: "1. Lister les interventions.".into(),
            rationale: "Ajoute l’étape de relecture.".into(),
        }
    }

    #[test]
    fn a_version_goes_to_an_active_skill_and_a_new_skill_to_its_name() {
        let version = proposal(None);
        let new = proposal(Some("Synthèse hebdomadaire des interventions"));
        let target = |p, named| proposal_target(p, named, NONE);
        assert_eq!(
            target(&version, NamedSkill::Active),
            Ok(ProposalTarget::NewVersion)
        );
        assert_eq!(
            target(&new, NamedSkill::Unknown),
            Ok(ProposalTarget::NewSkill)
        );
        assert_eq!(
            target(&new, NamedSkill::Inactive),
            Ok(ProposalTarget::InactiveSkill)
        );
    }

    #[test]
    fn an_inactive_skill_is_refused_as_a_name_no_skill_bears() {
        let version = proposal(None);
        let refused = |named| proposal_target(&version, named, NONE).unwrap_err();
        assert!(refused(NamedSkill::Unknown).contains("donnez aussi sa description"));
        assert_eq!(refused(NamedSkill::Inactive), refused(NamedSkill::Unknown));
        let new = proposal(Some("Synthèse hebdomadaire des interventions"));
        let active = proposal_target(&new, NamedSkill::Active, NONE).unwrap_err();
        assert!(active.contains("proposez-en une nouvelle version"));
    }

    #[test]
    fn nothing_more_is_proposed_while_too_many_wait() {
        let version = proposal(None);
        let target = |company, person| {
            proposal_target(&version, NamedSkill::Active, Pending { company, person })
        };
        assert!(target(MAX_PENDING_PROPOSALS - 1, MAX_PENDING_PER_PERSON - 1).is_ok());
        let company = target(MAX_PENDING_PROPOSALS, 0).unwrap_err();
        assert!(
            company.contains("attendent déjà les administrateurs"),
            "{company}"
        );
        let person = target(0, MAX_PENDING_PER_PERSON).unwrap_err();
        assert!(person.contains("pour cette personne"), "{person}");
    }
}
