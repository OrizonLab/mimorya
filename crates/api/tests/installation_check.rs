//! The installation check (ADR-223) against a real database: a ready
//! installation is said point by point, what it lacks as « à faire »;
//! what stops it from serving fails the check, said with what to do. The
//! model is a scripted one: no test calls a real provider.

mod support;

use std::fs;
use std::str::FromStr;
use std::sync::Arc;

use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use mimorya_api::services::installation::{Probes, State, check, told};
use mimorya_connectors::outbound::http_client;
use mimorya_db::migrate::MIGRATOR;
use mimorya_db::notifications::{RUN_EVENTS_LISTENER, RUN_QUEUE_LISTENER};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use support::app_url;
use support::installation::{DIRECT, check_config as config, found, health};
use support::llm::MASTER_KEY;
use support::mail::RecordingMailer;

/// A connection held as a process of the stack holds it, listening.
async fn listening(name: &'static str) -> PgPool {
    let options = PgConnectOptions::from_str(app_url())
        .unwrap()
        .application_name(name);
    let pool = PgPoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("select 1").execute(&pool).await.unwrap();
    pool
}

/// A site answering as the stack's does, over plain HTTP.
async fn site() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, health()).await });
    format!("http://{address}")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_ready_installation_says_so_point_by_point() {
    let origin = site().await;
    let (_worker, _api) = (
        listening(RUN_QUEUE_LISTENER).await,
        listening(RUN_EVENTS_LISTENER).await,
    );
    let config = config(&[
        ("WEB_ORIGIN", &origin),
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
        ("SMTP_URL", "smtp://relais.example:587"),
        ("MAIL_FROM", "Atelier Nova <no-reply@example.com>"),
        ("MIMORYA_PRODUCT_NAME", "Atelier Nova"),
        ("LLM_PROVIDER", "anthropic"),
        ("ANTHROPIC_API_KEY", "cle-de-test"),
    ]);
    let mailer = Arc::new(RecordingMailer::default());
    let model = Arc::new(ScriptedProvider::new(vec![ScriptedStep::text("OK")]));
    let probes = Probes {
        web: http_client(DIRECT).unwrap(),
        mailer: Some(mailer.clone()),
        mail_to: Some("exploitation@example.com".into()),
        model: Some(model.clone()),
    };
    let report = check(&config, probes).await;

    let state = |topic| found(&report, topic).state;
    let said = |topic| found(&report, topic).said.clone();
    assert_eq!(state("Base de données"), State::Ready);
    assert_eq!(state("Migrations"), State::Ready);
    let last = MIGRATOR.iter().map(|m| m.version).max().unwrap();
    let migrations = said("Migrations");
    assert!(
        migrations.ends_with(&format!("la dernière {last:04}")),
        "{migrations}"
    );
    assert_eq!(state("Clé maître"), State::Ready);
    // Served over HTTP here: to complete, not failed.
    assert_eq!(state("Domaine et certificat"), State::ToComplete);
    assert!(said("Domaine et certificat").contains("sans HTTPS"));
    assert_eq!(state("Relais SMTP"), State::Ready);
    assert!(said("Relais SMTP").starts_with("relais.example:587 a accepté l’e-mail d’essai"));
    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "exploitation@example.com");
    assert_eq!(sent[0].subject, "Essai d’envoi de Atelier Nova");
    // No archive of the WAL in the tests' database: to complete.
    assert_eq!(state("Sauvegardes"), State::ToComplete);
    assert_eq!(
        said("Workers"),
        "1 worker à l’écoute de la file des runs, 1 processus api relayant leurs événements"
    );
    assert_eq!(state("Marque"), State::Ready);
    assert!(said("Marque").starts_with("« Atelier Nova » ; accent clair celui de l’interface"));
    assert_eq!(state("Modèle de la plateforme"), State::Ready);
    assert!(said("Modèle de la plateforme").contains("a répondu en"));
    assert_eq!(model.requests().len(), 1);

    assert!(!report.failed());
    let text = told(&report);
    assert!(
        text.ends_with("Bilan : prête à servir, 2 point(s) à compléter.\n"),
        "{text}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_stops_the_platform_fails_the_check_and_says_what_to_do() {
    let mut unreachable = url::Url::parse(app_url()).unwrap();
    unreachable.set_password(Some("pas-le-bon")).unwrap();
    let folder = std::env::temp_dir().join(format!("mimorya-check-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("logo.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
    )
    .unwrap();
    let config = config(&[
        ("DATABASE_APP_URL", unreachable.as_str()),
        ("WEB_ORIGIN", "https://127.0.0.1:1"),
        ("MIMORYA_BRAND_DIR", folder.to_str().unwrap()),
        (
            "SMTP_URL",
            "smtps://ops:Relais-Secret-2026@relais.example:465",
        ),
        ("MAIL_FROM", "Mimorya <no-reply@example.com>"),
        ("LLM_PROVIDER", "anthropic"),
        ("ANTHROPIC_API_KEY", "cle-de-test-secrete"),
    ]);
    // The relay refuses the test e-mail, the model fails.
    let mailer = Arc::new(RecordingMailer::default());
    mailer.fail();
    let model = ScriptedProvider::new(vec![ScriptedStep::fails("Le fournisseur est surchargé")]);
    let probes = Probes {
        web: http_client(DIRECT).unwrap(),
        mailer: Some(mailer),
        mail_to: Some("exploitation@example.com".into()),
        model: Some(Arc::new(model)),
    };
    let report = check(&config, probes).await;

    let state = |topic| found(&report, topic).state;
    let said = |topic| found(&report, topic).said.clone();
    assert_eq!(state("Base de données"), State::Failed);
    assert!(said("Base de données").starts_with("injoignable"));
    // Nothing of the base is said as if it had been read.
    assert_eq!(said("Sauvegardes"), "non vérifié sans la base");
    assert_eq!(said("Workers"), "non vérifié sans la base");
    assert_eq!(state("Clé maître"), State::Failed);
    assert!(said("Clé maître").contains("MIMORYA_MASTER_KEY"));
    assert_eq!(state("Domaine et certificat"), State::Failed);
    assert!(said("Domaine et certificat").starts_with("https://127.0.0.1:1 injoignable"));
    assert_eq!(state("Relais SMTP"), State::Failed);
    assert!(
        said("Relais SMTP").starts_with("relais.example:465 : "),
        "{}",
        said("Relais SMTP")
    );
    assert_eq!(state("Marque"), State::Failed);
    assert!(
        said("Marque").contains("logo.svg : SVG refusé"),
        "{}",
        said("Marque")
    );
    assert_eq!(state("Modèle de la plateforme"), State::Failed);
    assert!(said("Modèle de la plateforme").contains("Le fournisseur est surchargé"));

    assert!(report.failed());
    let text = told(&report);
    assert!(text.contains("point(s) à corriger avant d’ouvrir la plateforme"));
    // No secret is said: not the database's password, the relay's nor the
    // model's key.
    for secret in ["pas-le-bon", "Relais-Secret-2026", "cle-de-test-secrete"] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}
