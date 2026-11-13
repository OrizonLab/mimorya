//! The e-mail `mail_send` hands (ADR-205): the address the person sends
//! from, chosen by the platform, never by the model; its text, written by
//! the platform with the sender's headers; and its envelope, each recipient
//! once. An address the SMTP library would refuse is named before any
//! connection.

use chrono::Utc;
use lettre::Address;
use mimorya_core::tools::MailAccount;
use uuid::Uuid;

use crate::delegated::{NewMail, is_plain_address, message_text};

/// What a person whose linked account gives no address to send from is
/// told.
pub const NO_SENDER: &str = "Le compte relié ne donne aucune adresse d’expéditeur : la personne \
                             le relie de nouveau dans « Mon compte » avec son adresse électronique";

/// An e-mail ready to be handed: its envelope and its text.
pub struct Outgoing {
    pub from: Address,
    pub to: Vec<Address>,
    pub text: String,
}

/// The address the person sends from: the identifier of their linked
/// mailbox when it is an address, which their server checked with its
/// password; else their platform account's.
pub fn sender(account: &MailAccount) -> Option<&str> {
    [account.username.as_str(), account.address.as_str()]
        .into_iter()
        .find(|address| is_plain_address(address))
}

/// `mail` from `from`, its text written by the platform with the sender's
/// headers, and its envelope: each recipient once. Or the address the
/// library refuses, named, in French.
pub fn outgoing(mail: &NewMail, from: &str) -> Result<Outgoing, String> {
    let refused = |whose: &str, address: &str| format!("{whose} refusée : {address}");
    let sender: Address = from
        .parse()
        .map_err(|_| refused("Adresse d’expéditeur", from))?;
    let mut to: Vec<Address> = Vec::new();
    for address in mail.to.iter().chain(&mail.cc) {
        let parsed: Address = address
            .parse()
            .map_err(|_| refused("Adresse de destinataire", address))?;
        let given = |known: &Address| {
            let known: &str = known.as_ref();
            known.eq_ignore_ascii_case(parsed.as_ref())
        };
        if !to.iter().any(given) {
            to.push(parsed);
        }
    }
    let domain = sender.domain();
    let date = Utc::now().to_rfc2822();
    let message_id = format!("<{}@{domain}>", Uuid::new_v4().simple());
    let headers = [("From", from), ("Date", &date), ("Message-ID", &message_id)];
    let text = message_text(mail, &headers).ok_or("Adresse électronique invalide")?;
    Ok(Outgoing {
        from: sender,
        to,
        text,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn account(username: &str, address: &str) -> MailAccount {
        MailAccount::new(
            json!({}),
            address.into(),
            username.into(),
            "secret".into(),
            Utc::now(),
        )
    }

    fn mail(to: &[&str], cc: &[&str]) -> NewMail {
        serde_json::from_value(json!({
            "to": to,
            "cc": cc,
            "subject": "Point",
            "body": "Bonjour",
        }))
        .unwrap()
    }

    #[test]
    fn sends_from_the_linked_mailbox_or_else_the_platform_account() {
        let linked = account("lea.martin@exemple.fr", "lea@exemple.fr");
        assert_eq!(sender(&linked), Some("lea.martin@exemple.fr"));
        let named = account("lmartin", "lea@exemple.fr");
        assert_eq!(sender(&named), Some("lea@exemple.fr"));
        let neither = account("lmartin", "lea@[127.0.0.1]");
        assert_eq!(sender(&neither), None);
    }

    #[test]
    fn writes_each_recipient_once_and_the_sender_s_headers() {
        let mail = mail(
            &["paul@exemple.fr", "anne@exemple.fr"],
            &["PAUL@exemple.fr"],
        );
        let outgoing = outgoing(&mail, "lea@exemple.fr").unwrap();
        let to: Vec<&str> = outgoing.to.iter().map(|to| to.as_ref()).collect();
        assert_eq!(to, ["paul@exemple.fr", "anne@exemple.fr"]);
        assert_eq!(outgoing.from.to_string(), "lea@exemple.fr");
        let text = outgoing.text;
        assert!(text.starts_with("From: lea@exemple.fr\r\nDate: "), "{text}");
        let id = text.lines().find(|line| line.starts_with("Message-ID: <"));
        assert!(id.is_some_and(|id| id.ends_with("@exemple.fr>")), "{text}");
    }

    #[test]
    fn an_address_the_library_refuses_is_named() {
        // Plain enough for the validation, not a dot-atom for the library.
        let mail = mail(&["paul@exemple.fr", "anne..b@exemple.fr"], &[]);
        let said = outgoing(&mail, "lea@exemple.fr").err().unwrap();
        assert_eq!(said, "Adresse de destinataire refusée : anne..b@exemple.fr");
        let mail = self::mail(&["paul@exemple.fr"], &[]);
        let said = outgoing(&mail, ".lea@exemple.fr").err().unwrap();
        assert_eq!(said, "Adresse d’expéditeur refusée : .lea@exemple.fr");
    }
}
