//! What `mail_send` says of a failure (ADR-205): refused by the server, or
//! cut, silent or answering at length before the e-mail's text was handed,
//! nothing left; handed and unanswered, perhaps sent, not to be sent again
//! unchecked. A server slow to say goodbye once it answered the text
//! changes nothing of it.

mod support;

use std::time::Duration;

use mimorya_connectors::outbound::TOO_LONG;
use mimorya_connectors::testing::{AfterData, Instead, SmtpMode, SmtpSetup};
use serde_json::json;
use support::linked_accounts::{error_of, validated_call};
use support::sending::{Sending, email, sending_paul, sending_paul_with};

const NOTHING_LEFT: &str = "— rien n’est parti";
const PERHAPS_SENT: &str = "il est peut-être parti, ne le renvoyez pas sans vérifier";

fn setup(after_data: AfterData, instead: Option<(&'static str, Instead)>) -> SmtpSetup {
    SmtpSetup {
        after_data,
        instead,
        ..SmtpSetup::new(SmtpMode::ImplicitTls)
    }
}

#[tokio::test]
async fn a_refusal_says_nothing_left() {
    let refusals = [
        (
            Some("marc@exemple.fr"),
            AfterData::Takes,
            "refuse l’e-mail (",
        ),
        (None, AfterData::Refuses, "refuse l’e-mail ("),
        (
            None,
            AfterData::RefusesForNow,
            "pour l’instant : réessayez plus tard",
        ),
    ];
    for (recipient, after_data, said) in refusals {
        let setup = SmtpSetup {
            refuses_recipient: recipient,
            ..setup(after_data, None)
        };
        let Sending { smtp, ports, paul } = sending_paul(setup).await;
        let error = error_of(&validated_call(&paul, "mail_send", email()).await);
        assert!(error.contains(said), "{error}");
        assert!(error.ends_with(NOTHING_LEFT), "{error}");
        assert!(smtp.deliveries.all().is_empty());
        assert!(ports.refused_mail_accounts().is_empty());
        if recipient.is_some() {
            assert!(!smtp.heard.lines().iter().any(|l| l.ends_with("DATA")));
        }
    }
}

#[tokio::test]
async fn a_cut_or_a_silence_before_the_text_says_nothing_left() {
    for at in ["MAIL", "RCPT", "DATA"] {
        for instead in [Instead::Drops, Instead::Hangs] {
            let setup = setup(AfterData::Takes, Some((at, instead)));
            let Sending { smtp, ports, paul } =
                sending_paul_with(setup, Duration::from_secs(3)).await;
            let error = error_of(&validated_call(&paul, "mail_send", email()).await);
            assert!(error.ends_with(NOTHING_LEFT), "{at} {instead:?}: {error}");
            assert!(smtp.deliveries.all().is_empty());
            assert!(ports.refused_mail_accounts().is_empty());
        }
    }
}

#[tokio::test]
async fn what_the_server_says_while_sending_is_bounded() {
    // Each answer within what lettre reads of one, their sum past the
    // platform's bound.
    let setup = setup(AfterData::Takes, Some(("RCPT", Instead::AnswersAtLength)));
    let Sending { smtp, paul, .. } = sending_paul(setup).await;
    let mut many = email();
    many["cc"] = json!(["anne@exemple.fr"]);
    let error = error_of(&validated_call(&paul, "mail_send", many).await);
    assert!(error.starts_with(TOO_LONG), "{error}");
    assert!(error.ends_with(NOTHING_LEFT), "{error}");
    assert!(smtp.deliveries.all().is_empty());
    assert!(!smtp.heard.lines().iter().any(|l| l.ends_with("DATA")));
}

#[tokio::test]
async fn an_e_mail_handed_without_answer_may_have_left() {
    for after_data in [AfterData::Drops, AfterData::Hangs] {
        let setup = setup(after_data, None);
        let Sending { smtp, paul, .. } = sending_paul_with(setup, Duration::from_secs(3)).await;
        let error = error_of(&validated_call(&paul, "mail_send", email()).await);
        assert!(error.contains(PERHAPS_SENT), "{after_data:?}: {error}");
        assert!(smtp.heard.lines().iter().any(|l| l == "tls DATA"));
    }
}

#[tokio::test]
async fn a_late_goodbye_changes_nothing() {
    // The server answers the text at once, then never answers QUIT: the
    // call's delay is shorter than the wait for its goodbye.
    let taken = setup(AfterData::Takes, Some(("QUIT", Instead::Hangs)));
    let Sending { smtp, paul, .. } = sending_paul_with(taken, Duration::from_secs(3)).await;
    let result = validated_call(&paul, "mail_send", email()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["envoye"], true);
    assert_eq!(smtp.deliveries.all().len(), 1);

    let refused = setup(AfterData::Refuses, Some(("QUIT", Instead::Hangs)));
    let Sending { smtp, paul, .. } = sending_paul_with(refused, Duration::from_secs(3)).await;
    let error = error_of(&validated_call(&paul, "mail_send", email()).await);
    assert!(error.ends_with(NOTHING_LEFT), "{error}");
    assert!(smtp.deliveries.all().is_empty());
}
