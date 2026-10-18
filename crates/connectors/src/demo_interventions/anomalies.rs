//! Deterministic consistency checks on intervention reports.

use super::dataset::{Intervention, InterventionStatus};

/// Inconsistencies detectable without interpretation (the model looks for the rest).
pub fn detect_anomalies(i: &Intervention) -> Vec<String> {
    let mut anomalies = Vec::new();
    let completed = i.status == InterventionStatus::Completed;
    if let (true, Some(part)) = (completed, i.parts_pending) {
        anomalies.push(format!(
            "Statut « terminée » alors qu’une pièce est en attente ({part})"
        ));
    }
    if completed && !i.client_signature {
        anomalies.push("Intervention terminée sans signature du client".to_string());
    }
    let worked = completed || i.status == InterventionStatus::InProgress;
    if worked && i.report.trim().is_empty() {
        anomalies.push("Compte rendu vide".to_string());
    }
    anomalies
}
