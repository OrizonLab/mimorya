//! The brand an installation sets (ADR-222), served before any session:
//! nothing set, « Mimorya » and no image; set, its names in the sign-in, the
//! e-mails and the authenticator apps, its accents, and its images drawn
//! under a policy that lets them do nothing else. A brand the platform
//! refuses stops it before it serves anything.

mod support;

use std::fs;
use std::path::PathBuf;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::llm::MASTER_KEY;
use support::manager_approvals::{applying, route_to_managers};
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};
use support::resets::ask_reset;
use support::second_factor::{KEYED, challenge_of, enrolment_step, require};
use support::{PASSWORD, TestApi, TestResponse, email};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0";
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><style>.a{fill:#6b2fa3}</style><rect class="a" width="8" height="8"/></svg>"##;

/// A brand folder of this test alone.
fn folder(test: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mimorya-brand-it-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    for (name, bytes) in files {
        fs::write(path.join(name), bytes).unwrap();
    }
    path
}

fn header(response: &TestResponse, name: &str) -> String {
    response.headers[name].to_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_set_nothing_changes() {
    let api = TestApi::start().await;
    let anonymous = api.anonymous();
    let brand = anonymous.get("/brand").await;
    assert_eq!(brand.status, StatusCode::OK);
    assert_eq!(
        brand.json(),
        json!({
            "productName": "Mimorya",
            "assistantName": "Mimorya",
            "filePrefix": "mimorya",
            "logo": null,
            "favicon": null,
            "light": null,
            "dark": null,
        })
    );
    for image in ["/brand/logo", "/brand/favicon"] {
        assert_eq!(
            anonymous.get(image).await.status,
            StatusCode::NOT_FOUND,
            "{image}"
        );
    }
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        paul.get("/auth/me").await.json()["assistant"]["name"],
        "Mimorya"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_brand_set_is_served_and_its_images_draw_nothing_else() {
    let folder = folder(
        "served",
        &[("logo.svg", SVG.as_bytes()), ("favicon.png", PNG)],
    );
    let api = TestApi::start_with_mailer_and(&[
        ("MIMORYA_PRODUCT_NAME", "Atelier Nova"),
        ("MIMORYA_ASSISTANT_NAME", "Nova"),
        ("MIMORYA_ACCENT_LIGHT", "#6b2fa3"),
        ("MIMORYA_BRAND_DIR", folder.to_str().unwrap()),
        ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ])
    .await;
    let anonymous = api.anonymous();
    let brand = anonymous.get("/brand").await.json();
    assert_eq!(brand["productName"], "Atelier Nova");
    assert_eq!(brand["assistantName"], "Nova");
    assert_eq!(brand["filePrefix"], "atelier-nova");
    assert_eq!(
        brand["light"],
        json!({ "accent": "#6b2fa3", "accentFg": "#ffffff", "accentSoft": "#ede6f4" })
    );
    assert_eq!(brand["dark"], Value::Null);
    for (image, kind, served_as, bytes) in [
        (
            "logo",
            "image/svg+xml",
            "image/svg+xml; charset=utf-8",
            SVG.as_bytes(),
        ),
        ("favicon", "image/png", "image/png", PNG),
    ] {
        assert_eq!(brand[image]["mediaType"], kind);
        let version = brand[image]["version"].as_str().unwrap();
        assert_eq!(version.len(), 16);
        let served = anonymous.get(&format!("/brand/{image}?v={version}")).await;
        assert_eq!(served.status, StatusCode::OK);
        assert_eq!(&served.body[..], bytes);
        assert_eq!(header(&served, "content-type"), served_as);
        assert_eq!(header(&served, "x-content-type-options"), "nosniff");
        assert_eq!(
            header(&served, "content-security-policy"),
            "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox; \
             frame-ancestors 'none'"
        );
        assert_eq!(
            header(&served, "cache-control"),
            "public, max-age=31536000, immutable"
        );
    }
    // The assistant's name reaches what people read of it.
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(
        paul.get("/auth/me").await.json()["assistant"]["name"],
        "Nova"
    );
    // The product's, the e-mails and the authenticator apps.
    ask_reset(&api, &email(user::PAUL)).await;
    let sent = api.mail.sent();
    let reset = sent.last().expect("reset e-mail");
    assert_eq!(
        reset.subject,
        "Réinitialisation de votre mot de passe Atelier Nova"
    );
    let enrolment = paul
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(enrolment.status, StatusCode::OK, "{}", enrolment.text());
    let uri = enrolment.json()["uri"].as_str().unwrap().to_string();
    assert!(uri.starts_with("otpauth://totp/Atelier%20Nova:"), "{uri}");
    assert!(uri.contains("&issuer=Atelier%20Nova&"), "{uri}");
    // Every other response keeps the policy of the API.
    assert_eq!(
        header(&anonymous.get("/brand").await, "content-security-policy"),
        "default-src 'none'; frame-ancestors 'none'"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_svg_that_runs_a_script_stops_the_start() {
    let folder = folder(
        "scripted",
        &[(
            "logo.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>fetch('/api/auth/me')</script></svg>"#,
        )],
    );
    let refused = TestApi::refused_start(&[("MIMORYA_BRAND_DIR", folder.to_str().unwrap())]).await;
    assert!(
        refused.contains("MIMORYA_BRAND_DIR : logo.svg : SVG refusé : élément <script> refusé"),
        "{refused}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_name_goes_where_it_is_read() {
    let env = [
        &PRIVATE_DEPLOYMENT[..],
        &KEYED[..],
        &[
            ("MIMORYA_PRODUCT_NAME", "Atelier Nova"),
            ("MIMORYA_ASSISTANT_NAME", "Nova"),
        ][..],
    ]
    .concat();
    let api = TestApi::start_with_mailer_and(&env).await;
    let paul = api.sign_in(user::PAUL).await;

    // The client the platform registers at an authorization server: the
    // product's.
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let id = add(&paul, mcp_body("Fichiers", "fichiers", &server.url, None)).await;
    let oauth = format!("/admin/connectors/{id}/oauth");
    let declared = paul.put(&oauth, json!({ "register": true })).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let requests = authority.requests();
    let asked = requests.iter().find(|r| r.endpoint == "register").unwrap();
    assert_eq!(asked.params["client_name"], "Atelier Nova");

    // The e-mail to a manager speaks of the assistant.
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = applying(&api, &nadia, ("INT-2049", "11:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    assert!(approval.is_some());
    let sent = api.mail.sent();
    let notice = sent
        .iter()
        .find(|mail| mail.to == email(user::MARC))
        .unwrap();
    assert!(
        notice.text.contains("L’assistant Nova (Néo Maintenance"),
        "{}",
        notice.text
    );

    // The enrolment a sign-in requires names the product in the app.
    assert_eq!(
        require(&paul, "administrators").await.status,
        StatusCode::OK
    );
    let signing_in = api.try_login(&email(user::PAUL), PASSWORD).await;
    let challenge = challenge_of(&signing_in);
    let enrolment = "/auth/second-factor/enrolment";
    let started = enrolment_step(&api, &challenge, enrolment, json!({})).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let uri = started.json()["uri"].as_str().unwrap().to_string();
    assert!(uri.starts_with("otpauth://totp/Atelier%20Nova:"), "{uri}");
    assert!(uri.contains("&issuer=Atelier%20Nova&"), "{uri}");
    // Left as found: the other tests sign in with a password alone.
    assert_eq!(require(&paul, "optional").await.status, StatusCode::OK);
}
