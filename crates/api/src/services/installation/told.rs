//! The report as the operator reads it, one line per point, then the
//! outcome.

use super::{Report, State};

pub fn told(report: &Report) -> String {
    let mut text = format!(
        "Vérification de l’installation de « {} »\n\n",
        report.product
    );
    for finding in &report.findings {
        let label = match finding.state {
            State::Ready => "OK",
            State::ToComplete => "À FAIRE",
            State::Failed => "ÉCHEC",
        };
        text.push_str(&format!(
            "{label:<8} {} : {}\n",
            finding.topic, finding.said
        ));
    }
    let count = |state| report.findings.iter().filter(|f| f.state == state).count();
    let (failed, missing) = (count(State::Failed), count(State::ToComplete));
    let outcome = match (failed, missing) {
        (0, 0) => "tout est prêt.".to_string(),
        (0, missing) => format!("prête à servir, {missing} point(s) à compléter."),
        (failed, missing) => {
            format!("{failed} point(s) empêchent de servir, {missing} à compléter.")
        }
    };
    text.push_str(&format!("\nBilan : {outcome}\n"));
    text
}

#[cfg(test)]
mod tests {
    use super::super::Finding;
    use super::*;

    #[test]
    fn the_report_reads_point_by_point_then_its_outcome() {
        let mut report = Report {
            product: "Atelier Nova".into(),
            findings: vec![
                Finding::new("Base de données", State::Ready, "joignable"),
                Finding::new("Relais SMTP", State::ToComplete, "non essayé"),
            ],
        };
        let text = told(&report);
        assert!(text.starts_with("Vérification de l’installation de « Atelier Nova »\n\n"));
        assert!(text.contains("OK       Base de données : joignable\n"));
        assert!(text.contains("À FAIRE  Relais SMTP : non essayé\n"));
        assert!(text.ends_with("Bilan : prête à servir, 1 point(s) à compléter.\n"));
        assert!(!report.failed());
        report
            .findings
            .push(Finding::new("Workers", State::Failed, "aucun"));
        assert!(
            told(&report).ends_with("Bilan : 1 point(s) empêchent de servir, 1 à compléter.\n")
        );
        assert!(report.failed());
    }
}
