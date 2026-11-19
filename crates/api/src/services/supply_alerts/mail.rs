//! The e-mail telling an administrator of a company's settings that its
//! supply of the platform's model comes near its end (ADR-211), in French
//! and plain text: the share reached, the amounts it was read on, what
//! follows past the cap or the credit, and whom to ask for more.

use mimorya_core::model_supply::{AlertKind, format_usd};

use crate::mail::Mail;

pub struct SupplyAlertMail<'a> {
    pub product: &'a str,
    pub company: &'a str,
    pub administrator: &'a str,
    pub email: &'a str,
    pub kind: AlertKind,
    /// The share told: of the cap reached, or of the last payment left.
    pub percent: u8,
    /// What the month was re-invoiced, or the credit's balance.
    pub amount_micros: i64,
    /// The cap, or the last payment.
    pub of_micros: i64,
    /// The administration.
    pub link: &'a str,
}

pub fn supply_alert_mail(alert: &SupplyAlertMail<'_>) -> Mail {
    let SupplyAlertMail {
        product,
        company,
        administrator,
        email,
        kind,
        percent,
        amount_micros,
        of_micros,
        link,
    } = alert;
    let (amount, of) = (format_usd(*amount_micros), format_usd(*of_micros));
    let (subject, what, then) = match (kind, percent) {
        (AlertKind::Cap, 100) => (
            "Plafond mensuel de consommation du modèle atteint".to_string(),
            format!("Ce mois-ci, {amount} ont été refacturés, pour un plafond de {of}."),
            "Jusqu’au mois prochain, aucun run ne part plus sur le modèle de la plateforme.",
        ),
        (AlertKind::Cap, percent) => (
            format!("Consommation du modèle : {percent} % du plafond mensuel atteints"),
            format!("Ce mois-ci, {amount} ont été refacturés, pour un plafond de {of}."),
            "Une fois le plafond atteint, aucun run ne part plus sur le modèle de la plateforme \
             jusqu’au mois prochain.",
        ),
        (AlertKind::Credit, 0) => (
            "Crédit de consommation du modèle épuisé".to_string(),
            format!("Le solde du crédit est de {amount}, pour un dernier versement de {of}."),
            "Aucun run ne part plus sur le modèle de la plateforme avant un nouveau versement.",
        ),
        (AlertKind::Credit, percent) => (
            format!(
                "Crédit de consommation du modèle : solde sous {percent} % du dernier versement"
            ),
            format!("Le solde du crédit est de {amount}, pour un dernier versement de {of}."),
            "Une fois le crédit épuisé, aucun run ne part plus sur le modèle de la plateforme \
             avant un nouveau versement.",
        ),
    };
    Mail {
        to: email.to_string(),
        to_name: administrator.to_string(),
        subject,
        text: format!(
            "Bonjour {administrator},\n\n\
             {company} utilise le modèle de la plateforme {product}, fourni par son exploitant. \
             {what}\n\n\
             {then} Pour relever le plafond ou verser un crédit, adressez-vous à l’exploitant \
             de la plateforme. Le détail est dans l’onglet « Modèle d’IA » de \
             l’administration :\n\
             {link}\n\n\
             Ce message est envoyé à chaque administrateur des réglages de l’entreprise.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(kind: AlertKind, percent: u8) -> Mail {
        supply_alert_mail(&SupplyAlertMail {
            product: "Mimorya",
            company: "Néo Maintenance",
            administrator: "Paul",
            email: "paul@example.test",
            kind,
            percent,
            amount_micros: 80_000_000,
            of_micros: 100_000_000,
            link: "https://app.example.test/admin",
        })
    }

    #[test]
    fn each_alert_says_what_it_reached_and_what_follows() {
        let cap = alert(AlertKind::Cap, 80);
        assert_eq!(
            cap.subject,
            "Consommation du modèle : 80 % du plafond mensuel atteints"
        );
        assert!(
            cap.text
                .contains("80,00 USD ont été refacturés, pour un plafond de 100,00 USD")
        );
        assert!(cap.text.contains("https://app.example.test/admin"));
        assert_eq!(
            alert(AlertKind::Cap, 100).subject,
            "Plafond mensuel de consommation du modèle atteint"
        );
        let credit = alert(AlertKind::Credit, 20);
        assert_eq!(
            credit.subject,
            "Crédit de consommation du modèle : solde sous 20 % du dernier versement"
        );
        assert!(credit.text.contains("avant un nouveau versement"));
        assert_eq!(
            alert(AlertKind::Credit, 0).subject,
            "Crédit de consommation du modèle épuisé"
        );
    }
}
