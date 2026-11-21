use super::{activity, bare, names};
use crate::gateways::GatewayKind;
use crate::identity_providers::ProviderKind;
use crate::processing_register::*;

#[test]
fn each_connector_is_named_by_what_its_tools_reach() {
    let connector = |kind, name: &str, hosts: &[&str]| ConnectorFact {
        kind,
        name: name.into(),
        hosts: hosts.iter().map(|h| h.to_string()).collect(),
    };
    let mut facts = bare();
    facts.connectors = vec![
        connector(ConnectorKind::Demo, "Interventions", &[]),
        connector(ConnectorKind::Mcp, "Docs", &["mcp.example.com"]),
        connector(ConnectorKind::McpProcess, "Docs locaux", &[]),
        connector(ConnectorKind::Mcp, "Docs illisibles", &[]),
        connector(
            ConnectorKind::StandardMail,
            "Messagerie",
            &["imap.example.com", "smtp.example.com"],
        ),
        connector(ConnectorKind::Microsoft365, "Microsoft 365", &[]),
        connector(ConnectorKind::GoogleWorkspace, "Google", &[]),
    ];
    let register = register(&facts);
    let connectors = activity(&register, ActivityKind::Connectors);
    assert_eq!(
        names(connectors),
        [
            "Serveur MCP (mcp.example.com)",
            "Serveur MCP installé par l’exploitant, lancé par la plateforme",
            "Serveur MCP",
            "Serveurs de messagerie (imap.example.com, smtp.example.com)",
            "Microsoft 365 (Microsoft Graph)",
            "Google Workspace (Gmail, Google Agenda)",
        ]
    );
    assert_eq!(connectors.recipients[0].role, "Connecteur « Docs »");
    assert_eq!(connectors.recipients[0].place, Place::DeclaredByCompany);
    assert_eq!(connectors.recipients[1].place, Place::Operator);
    // Only the operator installs such a program (ADR-025).
    assert_eq!(connectors.recipients[1].chosen_by, ChosenBy::Operator);
    assert_eq!(connectors.recipients[2].place, Place::DeclaredByCompany);
    assert_eq!(connectors.recipients[4].place, Place::OutsideEuropeanUnion);
    assert_eq!(connectors.notes.len(), 2);
    assert!(connectors.notes[0].contains("« Interventions »"));
    assert!(connectors.notes[0].contains("n’atteint aucun service extérieur"));
    assert!(connectors.notes[1].contains("« Docs locaux »"));
    assert!(connectors.notes[1].contains("ne sont pas connus"));
    assert!(connectors.sensitive.unwrap().contains("e-mails"));
    let kept = connectors.retention.join("\n");
    assert!(kept.contains("changement d’hôte d’un serveur"), "{kept}");
    assert!(kept.contains("ou de qui l’autorise"), "{kept}");
}

#[test]
fn gateways_and_sign_in_name_their_services() {
    let mut facts = bare();
    facts.gateways = vec![GatewayKind::Slack, GatewayKind::Teams];
    facts.identity_providers = vec![
        IdentityFact {
            kind: ProviderKind::Oidc,
            name: "Keycloak".into(),
            host: Some("sso.example.com".into()),
        },
        IdentityFact {
            kind: ProviderKind::Saml,
            name: "AD FS".into(),
            host: None,
        },
        IdentityFact {
            kind: ProviderKind::Microsoft,
            name: "Microsoft 365".into(),
            host: Some("login.microsoftonline.com".into()),
        },
    ];
    facts.scim = true;
    facts.mail_relay = Some(MailRelay {
        host: Some("smtp.exploitant.example".into()),
    });
    facts.audit_export = true;
    let register = register(&facts);
    assert!(
        register
            .activities
            .iter()
            .all(|a| a.kind != ActivityKind::Connectors)
    );
    let gateways = activity(&register, ActivityKind::Gateways);
    assert_eq!(
        names(gateways),
        ["Slack", "Microsoft Teams (Bot Framework)"]
    );
    assert!(gateways.retention[0].contains("celui de la passerelle"));
    let accounts = activity(&register, ActivityKind::Accounts);
    assert_eq!(
        names(accounts),
        [
            "Fournisseur OpenID Connect (sso.example.com)",
            "Fournisseur SAML",
            "Microsoft Entra ID",
            "Fournisseur d’identité de l’entreprise",
            "Relais d’envoi des e-mails de l’exploitant (smtp.exploitant.example)",
        ]
    );
    assert_eq!(
        accounts.recipients[0].role,
        "Fournisseur d’identité « Keycloak »"
    );
    assert_eq!(accounts.recipients[4].chosen_by, ChosenBy::Operator);
    assert_eq!(accounts.recipients[4].place, Place::OperatorServer);
    // Its notices, of missions and validations, as well.
    let assistance = activity(&register, ActivityKind::Assistance);
    assert_eq!(
        names(assistance),
        ["Relais d’envoi des e-mails de l’exploitant (smtp.exploitant.example)"]
    );
    let audit = activity(&register, ActivityKind::Audit);
    assert_eq!(names(audit), ["SIEM de l’entreprise"]);
    assert!(
        register
            .measures
            .iter()
            .any(|m| m.contains("fournisseur d’identité"))
    );
}
