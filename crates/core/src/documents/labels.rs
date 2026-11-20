//! A deposited document's type and confidentiality as the decision service
//! classifies them (ADR-213, D4), and who may set them. The type is one of
//! a fixed list; the confidentiality, one of the platform's sensitivities,
//! read from the decision at the usage's threshold. Against the document as
//! it is, a stricter level is applied at once; a looser one, like a type,
//! is only proposed, for a person who may write the document at both
//! levels to apply. Nothing here ever lowers a level by itself.

use crate::authz::{Actor, AuthorizationEngine, Sensitivity, Subject};

use super::spaces::{DocumentSpace, can_write_document};

/// A document's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    Contract,
    Invoice,
    Quote,
    Procedure,
    Minutes,
    Report,
    Letter,
    TechnicalSheet,
    Other,
}

/// The confidence from which a type is proposed.
pub const MIN_TYPE_CONFIDENCE: f64 = 0.5;

impl DocumentType {
    pub const ALL: [Self; 9] = [
        Self::Contract,
        Self::Invoice,
        Self::Quote,
        Self::Procedure,
        Self::Minutes,
        Self::Report,
        Self::Letter,
        Self::TechnicalSheet,
        Self::Other,
    ];

    /// As stored (`documents_document_type`) and asked.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Invoice => "invoice",
            Self::Quote => "quote",
            Self::Procedure => "procedure",
            Self::Minutes => "minutes",
            Self::Report => "report",
            Self::Letter => "letter",
            Self::TechnicalSheet => "technical_sheet",
            Self::Other => "other",
        }
    }

    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == stored)
    }

    /// What it covers, in French, as the decision is asked.
    pub fn description(self) -> &'static str {
        match self {
            Self::Contract => "Contrat, avenant, conditions générales ou accord signé",
            Self::Invoice => "Facture, avoir ou relevé de paiement",
            Self::Quote => "Devis, proposition commerciale ou chiffrage",
            Self::Procedure => "Procédure, mode opératoire, consigne ou règlement",
            Self::Minutes => "Compte rendu de réunion, d’intervention ou de visite",
            Self::Report => "Rapport, étude, audit ou bilan",
            Self::Letter => "Courrier, lettre ou note adressée à quelqu’un",
            Self::TechnicalSheet => "Fiche technique, notice, plan ou documentation d’un matériel",
            Self::Other => "Aucun des types précédents",
        }
    }
}

/// What a level covers, in French, from the least strict, as asked.
pub const LEVEL_DESCRIPTIONS: [&str; 4] = [
    "Public : peut être diffusé hors de l’entreprise sans dommage (plaquette, offre ou \
     notice publiées)",
    "Interne : réservé aux membres de l’entreprise, sans donnée sensible (procédure \
     courante, note de service)",
    "Confidentiel : données personnelles, financières ou commerciales dont la diffusion \
     nuirait (contrat, devis, facture nominative, rapport sur un client)",
    "Restreint : données très sensibles réservées à quelques personnes (santé, salaires, \
     dossier disciplinaire ou juridique, secret d’affaires)",
];

/// The strictest level given at least `threshold` of probability, counted
/// from the strictest down: lowering the threshold makes a stricter level
/// likelier. Without probabilities, the level scored, rounded up.
pub fn level_at(probabilities: &[f64], score: f64, threshold: f64) -> Sensitivity {
    let total: f64 = probabilities.iter().sum();
    if probabilities.is_empty() || total.is_nan() || total <= 0.0 {
        return level(score.ceil() as usize);
    }
    let mut above = 0.0;
    for (rank, probability) in probabilities.iter().enumerate().rev() {
        above += probability / total;
        if above > 0.0 && above >= threshold {
            return level(rank);
        }
    }
    Sensitivity::Public
}

fn level(rank: usize) -> Sensitivity {
    let last = Sensitivity::ALL.len() - 1;
    Sensitivity::ALL[rank.min(last)]
}

/// What the decision classified: a type with its confidence, if it said
/// one, and a level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Classified {
    pub document_type: Option<(DocumentType, f64)>,
    pub level: Sensitivity,
}

/// What a classification becomes against the document as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LabelsOutcome {
    /// A stricter level, applied at once.
    pub raise: Option<Sensitivity>,
    /// A type, proposed: confident enough, not « other », not the
    /// document's already.
    pub document_type: Option<DocumentType>,
    /// A looser level, proposed.
    pub lower: Option<Sensitivity>,
}

impl LabelsOutcome {
    /// Something waits for a person: a proposal, or a level raised.
    pub fn proposes(&self) -> bool {
        self.document_type.is_some() || self.lower.is_some() || self.raise.is_some()
    }
}

pub fn labels_outcome(
    current_type: Option<DocumentType>,
    current: Sensitivity,
    classified: Classified,
) -> LabelsOutcome {
    let document_type = classified
        .document_type
        .filter(|(kind, confidence)| {
            *kind != DocumentType::Other
                && *confidence >= MIN_TYPE_CONFIDENCE
                && Some(*kind) != current_type
        })
        .map(|(kind, _)| kind);
    let level = classified.level;
    LabelsOutcome {
        raise: (level > current).then_some(level),
        document_type,
        lower: (level < current).then_some(level),
    }
}

/// Whether the subject may set the type and the confidentiality of a
/// document of `space`: writing it at its level and at the new one, as a
/// deposit there at either would need.
pub fn can_set_labels(
    engine: &AuthorizationEngine,
    subject: &Subject,
    space: DocumentSpace,
    from: Sensitivity,
    to: Sensitivity,
    actor: &Actor,
) -> bool {
    can_write_document(engine, subject, space, from, actor)
        && can_write_document(engine, subject, space, to, actor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Sensitivity::{Confidential, Internal, Public, Restricted};

    #[test]
    fn the_level_is_the_strictest_reaching_the_threshold_from_the_top() {
        let spread = [0.1, 0.2, 0.4, 0.3];
        assert_eq!(level_at(&spread, 1.9, 0.5), Confidential);
        assert_eq!(level_at(&spread, 1.9, 0.3), Restricted);
        assert_eq!(level_at(&spread, 1.9, 0.8), Internal);
        assert_eq!(level_at(&spread, 1.9, 1.0), Public);
        // Unnormalised probabilities read as their shares.
        assert_eq!(level_at(&[2.0, 2.0, 0.0, 0.0], 0.5, 0.5), Internal);
        // A level with no probability is never retained, even at 0.
        assert_eq!(level_at(&[0.5, 0.5, 0.0, 0.0], 0.5, 0.0), Internal);
    }

    #[test]
    fn without_probabilities_the_score_is_rounded_up() {
        assert_eq!(level_at(&[], 1.2, 0.5), Confidential);
        assert_eq!(level_at(&[0.0, 0.0], 0.0, 0.5), Public);
        assert_eq!(level_at(&[], 9.0, 0.5), Restricted);
    }

    fn classified(kind: DocumentType, confidence: f64, level: Sensitivity) -> Classified {
        Classified {
            document_type: Some((kind, confidence)),
            level,
        }
    }

    #[test]
    fn a_stricter_level_is_raised_a_looser_one_only_proposed() {
        let raised = labels_outcome(
            None,
            Internal,
            classified(DocumentType::Invoice, 0.9, Restricted),
        );
        assert_eq!(raised.raise, Some(Restricted));
        assert_eq!(raised.lower, None);
        assert_eq!(raised.document_type, Some(DocumentType::Invoice));
        let looser = labels_outcome(
            None,
            Confidential,
            classified(DocumentType::Invoice, 0.9, Public),
        );
        assert_eq!((looser.raise, looser.lower), (None, Some(Public)));
        let same = labels_outcome(
            Some(DocumentType::Invoice),
            Internal,
            classified(DocumentType::Invoice, 0.9, Internal),
        );
        assert_eq!(same, LabelsOutcome::default());
        assert!(!same.proposes());
    }

    #[test]
    fn a_type_is_proposed_only_when_confident_and_said() {
        let unsure = labels_outcome(
            None,
            Internal,
            classified(DocumentType::Quote, 0.4, Internal),
        );
        assert_eq!(unsure.document_type, None);
        let other = labels_outcome(
            None,
            Internal,
            classified(DocumentType::Other, 0.9, Internal),
        );
        assert_eq!(other.document_type, None);
        let none = labels_outcome(
            None,
            Internal,
            Classified {
                document_type: None,
                level: Internal,
            },
        );
        assert!(!none.proposes());
    }

    #[test]
    fn types_and_levels_are_described_and_read_back() {
        for kind in DocumentType::ALL {
            assert_eq!(DocumentType::parse(kind.as_str()), Some(kind));
            assert!(!kind.description().is_empty());
        }
        assert_eq!(DocumentType::parse("secret"), None);
        assert_eq!(LEVEL_DESCRIPTIONS.len(), Sensitivity::ALL.len());
    }
}
