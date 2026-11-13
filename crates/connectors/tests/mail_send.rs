//! The standard mail's `mail_send` (ADR-205): offered where the company
//! lets its tools write, always validated; the e-mail handed to the
//! company's SMTP server as validated, from the person's linked mailbox,
//! over TLS once signed in, nothing the model wrote adding a header or a
//! recipient. What is known of a failure is said: refused or cut before its
//! text was handed, nothing left; handed and unanswered, perhaps sent, not
//! to be sent again unchecked. A refused password removes the account.

mod support;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::standard_mail::{MailServerConnector, TRY_LATER};
use mimorya_connectors::testing::{AfterData, FakeSmtp, SmtpMode, SmtpSetup, TestAuthority};
use mimorya_connectors::types::{Connector, InstanceContext};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, MailServerRole, ToolRef};
use serde_json::{Value, json};
use support::linked_accounts::{error_of, mailbox_executor, validated_call};
use support::mail::{PASSWORD, USER, WRONG, accounts, hides, trusting};
use support::mailboxes::linked_at;
use uuid::Uuid;

/// The company's declaration naming `smtp`, its tools allowed to write.
fn declaration(smtp: &FakeSmtp, security: &str) -> Value {
    let host = smtp.address.ip().to_string();
    json!({
        "imap": { "host": host, "port": 993 },
        "smtp": { "host": host, "port": smtp.address.port(), "security": security },
        "writeTools": true,
    })
}

async fn tools(
    authority: &TestAuthority,
    declared: &Value,
    call_timeout: Duration,
) -> Vec<ToolRef> {
    let context = InstanceContext {
        network: trusting(authority),
        call_timeout,
        ..InstanceContext::default()
    };
    MailServerConnector
        .instantiate(declared, context)
        .await
        .unwrap()
        .tools
}

struct Sending {
    smtp: FakeSmtp,
    ports: Arc<InMemoryPorts>,
    paul: ToolExecutor,
}

/// Paul, his account linked with `password` at a simulated SMTP server set
/// up so.
async fn sending_paul_with(setup: SmtpSetup, password: &str, call_timeout: Duration) -> Sending {
    let authority = TestAuthority::new();
    let smtp = FakeSmtp::with(&authority, setup, accounts()).await;
    let security = match setup.mode {
        SmtpMode::StartTls => "starttls",
        _ => "tls",
    };
    let declared = declaration(&smtp, security);
    let ports = InMemoryPorts::new(user::PAUL, Uuid::new_v4());
    ports.link_mail_account(declared.clone(), USER, password, linked_at());
    let tools = tools(&authority, &declared, call_timeout).await;
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let paul = mailbox_executor(tools, user::PAUL, &ports, accounts);
    Sending { smtp, ports, paul }
}

async fn sending_paul(setup: SmtpSetup) -> Sending {
    sending_paul_with(setup, PASSWORD, Duration::from_secs(20)).await
}

fn email() -> Value {
    json!({
        "to": ["claire@exemple.fr", "marc@exemple.fr"],
        "cc": ["CLAIRE@exemple.fr"],
        "subject": "Planning de novembre — révisé",
        "body": "Bonjour,\n.\nLe planning est prêt.\n\tPaul",
    })
}

/// The decoded text of a delivered e-mail.
fn decoded_body(text: &str) -> String {
    let (_, body) = text.split_once("\r\n\r\n").unwrap();
    let joined: String = body.split("\r\n").collect();
    String::from_utf8(STANDARD.decode(joined).unwrap()).unwrap()
}

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
            after_data,
            ..SmtpSetup::new(SmtpMode::ImplicitTls)
        };
        let Sending { smtp, ports, paul } = sending_paul(setup).await;
        let error = error_of(&validated_call(&paul, "mail_send", email()).await);
        assert!(error.contains(said), "{error}");
        assert!(error.ends_with("— rien n’est parti"), "{error}");
        assert!(smtp.deliveries.all().is_empty());
        assert!(ports.refused_mail_accounts().is_empty());
        if recipient.is_some() {
            assert!(!smtp.heard.lines().iter().any(|l| l.ends_with("DATA")));
        }
    }
}

#[tokio::test]
async fn an_e_mail_handed_without_answer_may_have_left() {
    for after_data in [AfterData::Drops, AfterData::Hangs] {
        let setup = SmtpSetup {
            after_data,
            ..SmtpSetup::new(SmtpMode::ImplicitTls)
        };
        let Sending { smtp, paul, .. } =
            sending_paul_with(setup, PASSWORD, Duration::from_secs(3)).await;
        let error = error_of(&validated_call(&paul, "mail_send", email()).await);
        assert!(
            error.contains("il est peut-être parti, ne le renvoyez pas sans vérifier"),
            "{after_data:?}: {error}"
        );
        assert!(smtp.heard.lines().iter().any(|l| l == "tls DATA"));
    }
}

#[tokio::test]
async fn a_refused_password_removes_the_account_a_refusal_for_now_keeps_it() {
    let quoting = SmtpSetup {
        quotes_refused_password: true,
        ..SmtpSetup::new(SmtpMode::ImplicitTls)
    };
    let Sending { smtp, ports, paul } =
        sending_paul_with(quoting, WRONG, Duration::from_secs(20)).await;
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
