//! What `mimorya-admin release-company` (ADR-193) tells the operator: for
//! each MCP server, whether the company's authorization and its people's
//! were revoked at the authorization server; for each Telegram bot,
//! whether its webhook was withdrawn; for each Google Workspace client,
//! how many of the accounts its people linked had their grant revoked at
//! Google (ADR-194). What was not is to withdraw there by hand; it is gone
//! here all the same. Never a token.

use mimorya_api::services::provisioning::{BotReleased, LinksReleased, Released, ServerReleased};

/// Prints what the release of the company `slug` did.
pub fn told(slug: &str, released: &Released) {
    println!("{}", said(slug, released));
}

fn said(slug: &str, released: &Released) -> String {
    let mut lines = vec![format!(
        "Entreprise de sigle {slug} (identifiant {}), suspendue :",
        released.tenant_id
    )];
    if released.servers.is_empty() && released.bots.is_empty() && released.links.is_empty() {
        lines.push("  rien n’était déclaré pour elle chez d’autres services.".into());
    }
    lines.extend(released.servers.iter().map(server));
    lines.extend(released.bots.iter().map(bot));
    lines.extend(released.links.iter().map(links));
    let kept = released
        .servers
        .iter()
        .any(|server| matches!(server.company, Some(Err(_))) || server.persons_failed > 0)
        || released.bots.iter().any(|bot| bot.webhook.is_err())
        || released.links.iter().any(|links| links.failed > 0);
    if kept {
        lines.push(
            "Ce qui n’a pas été retiré reste accordé chez le fournisseur : retirez-le là. \
             La plateforme ne le garde plus."
                .into(),
        );
    }
    lines.push(format!(
        "Pour la supprimer : mimorya-admin delete-company --slug {slug} (service migrate)."
    ));
    lines.join("\n")
}

fn server(server: &ServerReleased) -> String {
    let mut said = format!("  serveur MCP « {} » :", server.name);
    match &server.company {
        Some(Ok(())) => said.push_str(" autorisation de l’entreprise révoquée"),
        Some(Err(reason)) => said.push_str(&format!(
            " autorisation de l’entreprise non révoquée ({reason})"
        )),
        None => {}
    }
    let persons = server.persons_revoked + server.persons_failed;
    if persons > 0 {
        if server.company.is_some() {
            said.push_str(" ;");
        }
        said.push_str(&format!(
            " autorisations de ses personnes : {} révoquée(s) sur {persons}",
            server.persons_revoked
        ));
        if let Some(reason) = &server.persons_reason {
            said.push_str(&format!(" ({reason})"));
        }
    }
    said
}

fn links(links: &LinksReleased) -> String {
    let total = links.revoked + links.kept + links.failed;
    let mut said = format!(
        "  comptes reliés à « {} » : accord révoqué chez Google pour {} sur {total}",
        links.name, links.revoked
    );
    if let Some(reason) = &links.reason {
        said.push_str(&format!(" ({reason})"));
    }
    if links.kept > 0 {
        said.push_str(&format!(
            " ; {} relié(s) de nouveau entre-temps, à libérer encore",
            links.kept
        ));
    }
    said
}

fn bot(bot: &BotReleased) -> String {
    match &bot.webhook {
        Ok(()) => format!(
            "  bot Telegram « {} » : retiré, son webhook aussi",
            bot.name
        ),
        Err(reason) => format!(
            "  bot Telegram « {} » : retiré, son webhook reste chez Telegram ({reason})",
            bot.name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn says_what_was_withdrawn_and_what_stays() {
        let released = Released {
            tenant_id: Uuid::nil(),
            servers: vec![
                ServerReleased {
                    name: "Documents".into(),
                    company: Some(Ok(())),
                    persons_revoked: 2,
                    persons_failed: 1,
                    persons_reason: Some("Le point de révocation n’a pas répondu à temps".into()),
                },
                ServerReleased {
                    name: "Planning".into(),
                    company: Some(Err("Le serveur ne nomme aucun point de révocation".into())),
                    persons_revoked: 0,
                    persons_failed: 0,
                    persons_reason: None,
                },
            ],
            bots: vec![BotReleased {
                name: "Néo".into(),
                webhook: Ok(()),
            }],
            links: vec![LinksReleased {
                name: "Google Workspace".into(),
                revoked: 2,
                kept: 0,
                failed: 1,
                reason: Some("Google a refusé la révocation (invalid_request)".into()),
            }],
        };
        assert_eq!(
            said("acme", &released),
            "Entreprise de sigle acme (identifiant 00000000-0000-0000-0000-000000000000), \
             suspendue :\n  \
             serveur MCP « Documents » : autorisation de l’entreprise révoquée ; autorisations \
             de ses personnes : 2 révoquée(s) sur 3 (Le point de révocation n’a pas répondu à \
             temps)\n  \
             serveur MCP « Planning » : autorisation de l’entreprise non révoquée (Le serveur \
             ne nomme aucun point de révocation)\n  \
             bot Telegram « Néo » : retiré, son webhook aussi\n  \
             comptes reliés à « Google Workspace » : accord révoqué chez Google pour 2 sur 3 \
             (Google a refusé la révocation (invalid_request))\n\
             Ce qui n’a pas été retiré reste accordé chez le fournisseur : retirez-le là. La \
             plateforme ne le garde plus.\n\
             Pour la supprimer : mimorya-admin delete-company --slug acme (service migrate)."
        );
    }

    #[test]
    fn says_when_nothing_was_declared() {
        let released = Released {
            tenant_id: Uuid::nil(),
            servers: Vec::new(),
            bots: Vec::new(),
            links: Vec::new(),
        };
        assert!(
            said("acme", &released)
                .contains("rien n’était déclaré pour elle chez d’autres services")
        );
    }
}
