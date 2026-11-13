//! Test apps whose companies reach simulated mail servers (ADR-205): IMAP
//! and SMTP servers of a test authority, on local ports the private network
//! lets through, their certificates trusted as the authority's roots; a
//! master key seals the accounts people link there.

use std::sync::Arc;

use mimorya_api::App;
use mimorya_connectors::testing::{Accounts, FakeImap, FakeSmtp, Hold, SmtpMode, TestAuthority};
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::second_factor::KEYED;
use super::{Client, Setup, TestApi};

/// An account the simulated servers accept.
pub const USERNAME: &str = "paul.durand@neo-maintenance.example";
pub const PASSWORD: &str = "Messagerie-2026!";

/// Simulated IMAP and SMTP servers of one authority, accepting `accounts`.
pub struct MailServers {
    pub authority: TestAuthority,
    pub imap: FakeImap,
    pub smtp: FakeSmtp,
}

impl MailServers {
    pub async fn start(accounts: Accounts) -> Self {
        let authority = TestAuthority::new();
        let imap = FakeImap::start(&authority, accounts.clone()).await;
        let smtp = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts).await;
        Self {
            authority,
            imap,
            smtp,
        }
    }

    /// The same, the IMAP server answering an account only once `hold`
    /// lets it through.
    pub async fn held(accounts: Accounts, hold: Arc<Hold>) -> Self {
        let authority = TestAuthority::new();
        let imap = FakeImap::held(&authority, accounts.clone(), hold).await;
        let smtp = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts).await;
        Self {
            authority,
            imap,
            smtp,
        }
    }

    /// The servers accepting Paul's account.
    pub async fn for_paul() -> Self {
        Self::start(vec![(USERNAME.into(), PASSWORD.into())]).await
    }

    /// The standard mail declared on these servers.
    pub fn declaration(&self) -> Value {
        json!({
            "name": "Messagerie de l’entreprise",
            "imap": { "host": "127.0.0.1", "port": self.imap.address.port() },
            "smtp": {
                "host": "127.0.0.1",
                "port": self.smtp.address.port(),
                "security": "starttls",
            },
        })
    }
}

impl TestApi {
    /// App backed by the scripted model, reaching the private network the
    /// simulated mail servers listen on, their authority trusted, with a
    /// master key.
    pub async fn start_with_mail_servers(servers: &MailServers) -> Self {
        Self::scripted(Setup {
            env: &MAIL_DEPLOYMENT,
            connector_test_roots: servers.authority.roots().to_vec(),
            ..Setup::default()
        })
        .await
    }
}

const MAIL_DEPLOYMENT: [(&str, &str); 2] =
    [KEYED[0], ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true")];

/// Declares the standard mail on `servers` as `admin`, removing one an
/// earlier test of the binary left (one per company); its id.
pub async fn declare(admin: &Client<'_>, servers: &MailServers) -> Uuid {
    let listed = admin.get("/admin/connectors").await.json();
    let left = listed["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|instance| instance["type"] == "mail_server")
        .filter_map(|instance| instance["id"].as_str().map(String::from))
        .collect::<Vec<_>>();
    for id in left {
        let removed = admin.delete(&format!("/admin/connectors/{id}")).await;
        assert_eq!(removed.status.as_u16(), 200, "{}", removed.text());
    }
    let created = admin
        .post("/admin/connectors/mail-server", servers.declaration())
        .await;
    assert_eq!(created.status.as_u16(), 201, "{}", created.text());
    created.json()["id"].as_str().unwrap().parse().unwrap()
}

/// The sealed password and identifier kept for `user_id`, as stored.
pub async fn stored(app: &App, user_id: Uuid) -> Option<(String, Vec<u8>)> {
    let mut tx = app.services.db.begin(tenant::ID).await.unwrap();
    let row = sqlx::query_as(
        "select username, password_ciphertext from mail_accounts where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    row
}
