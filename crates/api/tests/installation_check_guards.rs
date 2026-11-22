//! What the installation check (ADR-223) must not take for ready: a
//! certificate no browser trusts, and the database reached by a role that
//! escapes the companies' isolation (a superuser, or a member of the
//! tables' owner without any privilege of its own). Each is tried, and
//! refused.

mod support;

use mimorya_api::services::installation::{Probes, State, check};
use mimorya_connectors::outbound::{NetworkPolicy, http_client};
use mimorya_connectors::testing::{TestAuthority, serve_https};
use mimorya_db::{Database, PoolOptions};
use support::installation::{DIRECT, check_config, found, health};
use support::{app_url, owner_url};

fn probes(web: NetworkPolicy) -> Probes {
    Probes {
        web: http_client(web).unwrap(),
        mailer: None,
        mail_to: None,
        model: None,
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_certificate_is_said_recognized_only_when_browsers_trust_it() {
    // The stack's domain behind a certificate of an authority of its own,
    // as Caddy's internal one.
    let authority = TestAuthority::new();
    let address = serve_https(&authority, health()).await;
    let origin = format!("https://localhost:{}", address.port());
    let config = check_config(&[("WEB_ORIGIN", &origin)]);

    let report = check(&config, probes(DIRECT)).await;
    let domain = found(&report, "Domaine et certificat");
    assert_eq!(domain.state, State::Failed);
    assert!(
        domain
            .said
            .starts_with(&format!("certificat de {origin} non reconnu")),
        "{}",
        domain.said
    );
    assert!(domain.said.contains("MIMORYA_TLS=internal"));

    // The same certificate, its authority trusted: recognized.
    let trusting = NetworkPolicy {
        test_roots: authority.roots(),
        ..DIRECT
    };
    let report = check(&config, probes(trusting)).await;
    let domain = found(&report, "Domaine et certificat");
    assert_eq!(domain.state, State::Ready, "{}", domain.said);
    assert_eq!(
        domain.said,
        format!("{origin} répond, son certificat est reconnu")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_owner_s_address_in_place_of_the_application_s_is_refused() {
    let config = check_config(&[("DATABASE_APP_URL", owner_url())]);
    let report = check(&config, probes(DIRECT)).await;
    let base = found(&report, "Base de données");
    assert_eq!(base.state, State::Failed);
    assert!(
        base.said.contains("contourne l’isolation des entreprises"),
        "{}",
        base.said
    );
    assert_eq!(found(&report, "Workers").said, "non vérifié sans la base");
    assert!(report.failed());

    let applications = check(&check_config(&[]), probes(DIRECT)).await;
    let base = found(&applications, "Base de données");
    assert_eq!(base.state, State::Ready);
    assert!(base.said.ends_with("soumis à l’isolation des entreprises"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_member_of_the_tables_owner_escapes_isolation_without_any_privilege() {
    // A role neither superuser nor BYPASSRLS, but a member of the role that
    // owns the tables: PostgreSQL spares it their row-level security.
    let owner = Database::connect(
        owner_url(),
        PoolOptions {
            max_connections: 1,
            application_name: "installation-check-test",
        },
    )
    .await
    .unwrap();
    let role = format!("mimorya_check_member_{}", std::process::id());
    let tables_owner: String = sqlx::query_scalar("select current_user::text")
        .fetch_one(owner.pool())
        .await
        .unwrap();
    for statement in [
        format!("drop role if exists {role}"),
        format!(
            "create role {role} login password 'membre-du-proprietaire' \
             nosuperuser nobypassrls in role \"{tables_owner}\""
        ),
    ] {
        sqlx::query(&statement).execute(owner.pool()).await.unwrap();
    }
    let mut member = url::Url::parse(app_url()).unwrap();
    member.set_username(&role).unwrap();
    member.set_password(Some("membre-du-proprietaire")).unwrap();

    let config = check_config(&[("DATABASE_APP_URL", member.as_str())]);
    let report = check(&config, probes(DIRECT)).await;
    let base = found(&report, "Base de données");
    sqlx::query(&format!("drop role {role}"))
        .execute(owner.pool())
        .await
        .unwrap();
    owner.close().await;
    assert_eq!(base.state, State::Failed);
    assert!(
        base.said
            .contains("propriétaire des tables ou membre de leur propriétaire"),
        "{}",
        base.said
    );
    assert!(!base.said.contains("membre-du-proprietaire"));
}
