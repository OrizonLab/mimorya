//! What the e-mails about requests share (ADR-060, ADR-082): their risks,
//! once each, as the interface names them; the phrases that follow how many
//! requests there are; and sending one, its outcome audited, the relay's
//! answer (which may name the address) kept out of the logs.

use mimorya_core::autonomy::ToolRisk;
use uuid::Uuid;

use crate::mail::{Mail, Mailer};

/// The risks of requests, once each, in their order.
pub(super) fn distinct_risks<'a>(risks: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut distinct: Vec<&str> = Vec::new();
    for risk in risks {
        if !distinct.contains(&risk) {
            distinct.push(risk);
        }
    }
    distinct
}

/// What an action of `risk` does, as the interface names it; a value no
/// risk bears any more, as stored.
pub(super) fn risk_label(risk: &str) -> &str {
    match ToolRisk::parse(risk) {
        Some(ToolRisk::Read) => "lecture",
        Some(ToolRisk::Draft) => "brouillon",
        Some(ToolRisk::Write) => "modification",
        Some(ToolRisk::External) => "impact externe",
        Some(ToolRisk::Irreversible) => "irréversible",
        None => risk,
    }
}

/// The phrases of a message about `count` requests of `risks` risks.
pub(super) struct Phrases {
    pub risk_word: &'static str,
    pub see: &'static str,
    pub waiting: &'static str,
    pub expires: &'static str,
}

pub(super) fn phrases(count: usize, risks: usize) -> Phrases {
    if count == 1 {
        Phrases {
            risk_word: "risque",
            see: "l’action et ses paramètres exacts, puis la valider ou la refuser",
            waiting: "Tant que la demande attend",
            expires: "Elle expire",
        }
    } else {
        Phrases {
            risk_word: if risks == 1 { "risque" } else { "risques" },
            see: "les actions et leurs paramètres exacts, puis les valider ou les refuser",
            waiting: "Tant que les demandes attendent",
            expires: "Elles expirent",
        }
    }
}

/// Sends `mail` about requests of `run_id`; the outcome to audit. Refused,
/// it is not sent again: the requests still wait in « Validations ».
pub(super) async fn sent(
    mailer: &dyn Mailer,
    mail: &Mail,
    run_id: Uuid,
    message: &str,
) -> &'static str {
    match mailer.send(mail).await {
        Ok(()) => "success",
        Err(_) => {
            tracing::warn!(run_id = %run_id, message, "Approval e-mail refused by the relay");
            "failure"
        }
    }
}
