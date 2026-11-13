//! Test support (feature `testing`): MCP servers over Streamable HTTP on a
//! local port, optionally behind a bearer token, and misbehaving ones (what
//! they declare, a token quoted back, a server that never answers, one held
//! until the test releases it, answers that never end: `endless`, a slow one
//! that counts the pings it sees: `slow`); an HTTP proxy that lists where it
//! went (`proxy`); an MCP server protected by OAuth and its authorization
//! server (`oauth`); a REST API (`rest`); Microsoft Graph (`graph`, its writes
//! in `graph_writes`); Gmail and Google Calendar (`google`, its writes in
//! `google_writes`); Slack's Web API (`slack`); Telegram's Bot API
//! (`telegram`); Microsoft as a Teams bot reaches it: Entra, the Bot
//! Framework's keys and a Teams service (`teams`); an authority of the tests
//! and a server speaking TLS with its certificate, the proxy reached over
//! TLS too (`tls`, ADR-205); a company's IMAP and SMTP servers (`mail`).

mod endless;
mod google;
mod google_writes;
mod graph;
mod graph_writes;
mod hold;
mod mail;
mod oauth;
mod proxy;
mod quirks;
mod rest;
mod slack;
mod slow;
mod teams;
mod telegram;
mod tls;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::{self, Next};
use rmcp::ServerHandler;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::oneshot;

use crate::mcp::DemoDocsServer;
pub use endless::serve_one_event;
pub use google::{FakeGoogle, GOOGLE_DETAIL, GoogleRequest, LISTED_GONE, LISTED_REFUSED};
pub use google_writes::SentMail;
pub use graph::{FORBIDDEN_MESSAGE, FakeGraph, GRAPH_DETAIL, GraphRequest, OVERSIZED_MESSAGE};
pub use hold::Hold;
pub use mail::{
    Accounts, AfterData, Deliveries, Delivery, FakeImap, FakeSmtp, FetchBehaviour, Heard,
    ImapBehaviour, ImapSetup, Instead, Mailboxes, SmtpMode, SmtpSetup, TestFolder, TestMessage,
};
pub use oauth::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, IdTokenQuirk, LONG_TOKEN_CHARS,
    LURE, METADATA_PATH, OAuthQuirk, ProtectedMcp, SLOW_ANSWER, Settings as AuthorizationSettings,
    SignedIn, serve_oauth_mcp_http, serve_oauth_mcp_http_on,
};
pub use proxy::{Proxied, serve_proxy, serve_tls_proxy};
pub use quirks::{DeclaredToolsServer, HIDDEN_TEXT, Quirk};
pub use rest::{KEY_HEADER, serve_rest_http};
pub use slack::{
    BOT_TOKEN, BOT_USER, FakeSlack, OTHER_TOKEN, OTHER_WORKSPACE, SIGNING_SECRET, SlackPost,
    WORKSPACE, WORKSPACE_NAME, signed, signed_at,
};
pub use slow::{PING_DELAY, Ping, SLOW_CALL, Seen, serve_slow_mcp_http};
pub use teams::{
    FakeTeams, OTHER_TEAMS_APP, OTHER_TEAMS_DIRECTORY, OTHER_TEAMS_SECRET, Signer, TEAMS_APP,
    TEAMS_DIRECTORY, TEAMS_KEY_ID, TEAMS_SECRET, TeamsSent, conversation_of, member_id,
    signed_token,
};
pub use telegram::{
    FakeTelegram, OTHER_BOT, OTHER_BOT_TOKEN, TELEGRAM_BOT, TELEGRAM_BOT_USERNAME, TELEGRAM_TOKEN,
    TelegramMessage, update,
};
pub use tls::{LOCAL_NAMES, TestAuthority, serve_tls_echo};

/// Serves the demo documentation server at `http://127.0.0.1:<port>/mcp`
/// until the process ends; returns that address.
pub async fn serve_demo_docs_http(token: Option<&str>) -> String {
    serve_mcp_http(token, Quirk::None).await
}

/// Serves an MCP server with the given quirk; returns its address.
pub async fn serve_mcp_http(token: Option<&str>, quirk: Quirk) -> String {
    serve_mcp_http_on(0, token, quirk).await
}

/// The demo documentation server held at its first request until
/// `Hold::release`: a test acts while a start is surely under way.
pub async fn serve_held_mcp_http() -> (String, Arc<Hold>) {
    let hold = Arc::new(Hold::default());
    let held = hold.clone();
    let router = mcp_router(DemoDocsServer::default).layer(middleware::from_fn(
        move |request: Request, next: Next| {
            let hold = held.clone();
            async move {
                hold.wait().await;
                next.run(request).await
            }
        },
    ));
    (serve(router, 0).await, hold)
}

/// The same, on a given local port (a server coming back where it was).
pub async fn serve_mcp_http_on(port: u16, token: Option<&str>, quirk: Quirk) -> String {
    let router = match quirk {
        Quirk::None
        | Quirk::EchoesTokenOnStart
        | Quirk::EchoesEscapedTokenOnStart
        | Quirk::EchoesTokenOnCall
        | Quirk::RefusesUnknownSessions => mcp_router(DemoDocsServer::default),
        _ => mcp_router(move || DeclaredToolsServer::new(quirk)),
    };
    let expected = token.map(|t| format!("Bearer {t}"));
    let router = router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let expected = expected.clone();
        async move { quirks::guarded(expected, quirk, request, next).await }
    }));
    serve(router, port).await
}

/// The demo documentation server, at a given local port (0: any) until
/// `stop` is sent or dropped: it then refuses new connections, while those
/// already open are let finish (an event stream goes on being served). A
/// server served again on the same port has none of its sessions: a restart
/// as a client sees it.
pub async fn serve_stoppable_demo_docs_http(port: u16) -> (String, oneshot::Sender<()>) {
    let router = mcp_router(DemoDocsServer::default);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    let (stop, stopped) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await;
    });
    (format!("http://{address}/mcp"), stop)
}

/// Serves the router at `http://127.0.0.1:<port>/mcp` until the process ends.
async fn serve(router: Router, port: u16) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test MCP server");
    });
    format!("http://{address}/mcp")
}

/// An address that accepts connections and never answers.
pub async fn serve_mute() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    format!("http://{address}/mcp")
}

fn mcp_router<S>(server: impl Fn() -> S + Send + Sync + 'static) -> Router
where
    S: ServerHandler + Send + 'static,
{
    let service = StreamableHttpService::new(
        move || Ok(server()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    Router::new().nest_service("/mcp", service)
}
