//! Demo MCP server "documentation technique" — FICTIONAL procedures. Used by
//! tests (in-memory transport) and runnable over stdio for local demos:
//!   cargo run -p mimorya-connectors --bin mcp-demo-docs

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

struct Procedure {
    id: &'static str,
    title: &'static str,
    content: &'static str,
}

const PROCEDURES: [Procedure; 3] = [
    Procedure {
        id: "PROC-12",
        title: "Remplacement d’un circulateur de chauffage",
        content: "Couper l’alimentation, isoler le circuit, vidanger la portion concernée, remplacer le circulateur, purger, contrôler la pression (1,2 à 1,5 bar), remettre en service et faire signer le compte rendu.",
    },
    Procedure {
        id: "PROC-07",
        title: "Clôture d’une intervention",
        content: "Une intervention est clôturée lorsque le compte rendu est complet, signé par le client et qu’aucune pièce n’est en attente. Sinon, elle reste « en cours » et un suivi est planifié.",
    },
    Procedure {
        id: "PROC-21",
        title: "Recharge en fluide frigorigène",
        content: "Réservée aux techniciens titulaires de l’attestation de capacité. Rechercher et réparer la fuite avant toute recharge ; consigner la quantité dans le registre.",
    },
];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Termes recherchés (2 caractères au moins)
    #[schemars(length(min = 2))]
    pub query: String,
}

#[derive(Debug, Clone)]
pub struct DemoDocsServer {
    tool_router: ToolRouter<Self>,
}

impl Default for DemoDocsServer {
    fn default() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl DemoDocsServer {
    #[tool(
        name = "search_procedures",
        title = "Recherche de procédures",
        description = "Recherche dans la base documentaire technique (procédures internes).",
        annotations(read_only_hint = true)
    )]
    async fn search_procedures(&self, Parameters(request): Parameters<SearchRequest>) -> String {
        let terms: Vec<String> = request
            .query
            .to_lowercase()
            .split_whitespace()
            .filter(|t| t.chars().count() > 2)
            .map(str::to_string)
            .collect();
        let hits: Vec<String> = PROCEDURES
            .iter()
            .filter(|p| {
                let text = format!("{} {}", p.title, p.content).to_lowercase();
                terms.iter().any(|t| text.contains(t.as_str()))
            })
            .map(|p| format!("{} — {}\n{}", p.id, p.title, p.content))
            .collect();
        if hits.is_empty() {
            "Aucune procédure trouvée.".to_string()
        } else {
            hits.join("\n\n")
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DemoDocsServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("neo-docs-demo", "0.1.0"))
    }
}
