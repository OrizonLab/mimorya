//! The standard mail's `mail_send` (ADR-205): offered where the company
//! lets its tools write, always validated; the e-mail handed to the
//! company's SMTP server as validated, from the person's linked mailbox,
//! over TLS once signed in, nothing the model wrote adding a header or a
//! recipient; no server reached without an address to send from. A refused
//! password removes the account. What is said of other failures is tried
//! in `mail_send_failures.rs`.

mod support;

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_connectors::standard_mail::TRY_LATER;
use mimorya_connectors::testing::{FakeSmtp, SmtpMode, SmtpSetup, TestAuthority};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::MailServerRole;
use serde_json::json;
use support::linked_accounts::{error_of, validated_call};
use support::mail::{USER, WRONG, accounts, hides};
use support::mailboxes::linked_at;
use support::sending::{
    Sending, declaration, decoded_body, email, sending_as, sending_paul, tools,
};

#[tokio::test]
async fn sending_is_offered_where_the_company_lets_it_and_always_validated() {
    let authority = TestAuthority::new();
    let smtp = FakeSmtp::start(&authority, SmtpMode::ImplicitTls, accounts()).await;
    let mut declared = declaration(&smtp, "tls");
    declared["writeTools"] = json!(false);
    let reading = tools(&authority, &declared, Duration::from_secs(5)).await;
    assert!(reading.iter().all(|t| t.info().name != "mail_send"));
    declared["writeTools"] = json!(true);
    let writing = tools(&authority, &declared, Duration::from_secs(5)).await;
    let send = writing
        .iter()
        .find(|t| t.info().name == "mail_send")
        .unwrap();
    let info = send.info();
    assert_eq!(info.risk, ToolRisk::External);
    assert_eq!(info.required_permissions, ["connector:use"]);
    assert!(info.requires_approval);
    let input = serde_json::from_value(email()).unwrap();
    let described = send.describe_action(&input).unwrap();
    assert!(
        described.starts_with(
            "Envoyer depuis la boîte de messagerie reliée un e-mail à claire@exemple.fr, \
             marc@exemple.fr (copie : CLAIRE@exemple.fr) — objet : « Planning de novembre"
        ),
        "{described}"
    );
}

#[tokio::test]
async fn the_e_mail_leaves_as_validated_from_the_linked_mailbox() {
    for mode in [SmtpMode::ImplicitTls, SmtpMode::StartTls] {
        let Sending { smtp, paul, .. } = sending_paul(SmtpSetup::new(mode)).await;
        let result = validated_call(&paul, "mail_send", email()).await;
        assert!(!result.is_error, "{:?}", result.content);
        assert_eq!(result.content["envoye"], true);
        assert_eq!(result.content["de"], USER);
        assert_eq!(
            result.summary.as_deref(),
            Some("E-mail envoyé à 3 destinataire(s)")
        );
        let delivered = smtp.deliveries.all();
        assert_eq!(delivered.len(), 1, "{mode:?}");
        let delivery = &delivered[0];
        assert_eq!(delivery.from, USER);
        // Each recipient once, whatever its case in the copies.
        assert_eq!(delivery.to, ["claire@exemple.fr", "marc@exemple.fr"]);
        let text = &delivery.text;
        assert!(
            text.starts_with(&format!("From: {USER}\r\nDate: ")),
            "{text}"
        );
        assert!(text.contains("\r\nMessage-ID: <"), "{text}");
        assert!(
            text.contains("\r\nTo: claire@exemple.fr,\r\n marc@exemple.fr\r\n"),
            "{text}"
        );
        assert!(text.contains("\r\nCc: CLAIRE@exemple.fr\r\n"), "{text}");
        let subject = STANDARD.encode("Planning de novembre — révisé");
        assert!(
            text.contains(&format!("Subject: =?UTF-8?B?{subject}?=")),
            "{text}"
        );
        assert_eq!(
            decoded_body(text),
            "Bonjour,\r\n.\r\nLe planning est prêt.\r\n\tPaul"
        );
        // Signed in and handed over TLS alone.
        let heard = smtp.heard.lines();
        let secured: Vec<&String> = heard.iter().filter(|l| l.starts_with("tls ")).collect();
        let sent: Vec<&str> = secured.iter().map(|l| &l[4..]).collect();
        assert!(
            sent.ends_with(&["AUTH PLAIN", "MAIL", "RCPT", "RCPT", "DATA", "QUIT"]),
            "{heard:?}"
        );
        assert!(
            heard
                .iter()
                .all(|l| l.starts_with("tls ") || !l.contains("MAIL"))
        );
    }
}

#[tokio::test]
async fn nothing_the_model_writes_adds_a_header_nor_a_recipient() {
    let Sending { smtp, paul, .. } = sending_paul(SmtpSetup::new(SmtpMode::ImplicitTls)).await;
    let body = "Bonjour\n.\nMAIL FROM:<direction@exemple.fr>\nRCPT TO:<tous@exemple.fr>\nDATA\n\
                Bcc: tous@exemple.fr\n\n.\nFin";
    let mut forged = email();
    forged["body"] = json!(body);
    let result = validated_call(&paul, "mail_send", forged).await;
    assert!(!result.is_error, "{:?}", result.content);
    let delivered = smtp.deliveries.all();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].to, ["claire@exemple.fr", "marc@exemple.fr"]);
    let text = &delivered[0].text;
    assert!(!text.contains("Bcc") && !text.contains("tous@"), "{text}");
    assert_eq!(decoded_body(text), body.replace('\n', "\r\n"));
    // A subject that would end its header is refused before any connection.
    let mut header = email();
    header["subject"] = json!("Point\r\nBcc: tous@exemple.fr");
    let evaluation = paul.evaluate(&mimorya_agent::llm::LlmToolCall {
        id: "c".into(),
        name: "mail_send".into(),
        input: header,
    });
    let denied = matches!(
        evaluation,
        mimorya_agent::tools::ToolCallEvaluation::Deny { .. }
    );
    assert!(denied, "a subject ending its header was not refused");
    assert_eq!(smtp.deliveries.all().len(), 1);
}

#[tokio::test]
async fn no_address_to_send_from_reaches_no_server() {
    // Neither the identifier nor the platform address is a plain address.
    let account = ("paul@[127.0.0.1]", "secret");
    let setup = SmtpSetup::new(SmtpMode::ImplicitTls);
    let Sending { smtp, paul, .. } = sending_as(setup, account, Duration::from_secs(20)).await;
    let error = error_of(&validated_call(&paul, "mail_send", email()).await);
    assert!(
        error.starts_with("Le compte relié ne donne aucune adresse d’expéditeur"),
        "{error}"
    );
    assert!(error.ends_with("— rien n’est parti"), "{error}");
    assert!(smtp.heard.lines().is_empty());
}

#[tokio::test]
async fn a_refused_password_removes_the_account_a_refusal_for_now_keeps_it() {
    let quoting = SmtpSetup {
        quotes_refused_password: true,
        ..SmtpSetup::new(SmtpMode::ImplicitTls)
    };
    let account = (USER, WRONG);
    let Sending { smtp, ports, paul } = sending_as(quoting, account, Duration::from_secs(20)).await;
    let error = error_of(&validated_call(&paul, "mail_send", email()).await);
    assert!(
        error.starts_with("Le serveur SMTP de l’entreprise refuse le compte relié"),
        "{error}"
    );
    hides(&error, WRONG);
    assert_eq!(ports.mail_refusals(), [(linked_at(), MailServerRole::Smtp)]);
    assert!(smtp.deliveries.all().is_empty());
    // The account gone, the next call is told so, and reaches no server.
    let heard = smtp.heard.lines().len();
    let error = error_of(&validated_call(&paul, "mail_send", email()).await);
    assert!(error.contains("n’a relié aucun compte"), "{error}");
    assert_eq!(smtp.heard.lines().len(), heard);

    let later = SmtpSetup {
        refuses_for_now: true,
        ..SmtpSetup::new(SmtpMode::ImplicitTls)
    };
    let Sending { smtp, ports, paul } = sending_paul(later).await;
    let error = error_of(&validated_call(&paul, "mail_send", email()).await);
    assert!(error.starts_with(TRY_LATER), "{error}");
    assert!(error.ends_with("— rien n’est parti"), "{error}");
    assert!(ports.refused_mail_accounts().is_empty());
    assert!(smtp.deliveries.all().is_empty());
}
