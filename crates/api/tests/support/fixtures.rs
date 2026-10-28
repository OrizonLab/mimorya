//! What every test app shares: the test database (reset once per test
//! binary, its URLs for the application and owner roles), the demo
//! accounts' addresses and password, the web origin, and the week the demo
//! interventions are dated in.

use std::sync::OnceLock;

use chrono::{Days, Utc};
use mimorya_api::auth::credentials::hash_password;
use mimorya_connectors::demo_interventions::dates::{iso_date, monday_of};
use mimorya_core::demo;
use mimorya_db::testing::{TestDatabaseUrls, reset_test_database};
use serde_json::{Value, json};
use uuid::Uuid;

pub const PASSWORD: &str = "mot-de-passe-de-test";
pub const WEB_ORIGIN: &str = "http://localhost:3000";

static URLS: OnceLock<TestDatabaseUrls> = OnceLock::new();

/// The owner role's URL, for what the application role may not write.
pub fn owner_url() -> &'static str {
    &urls().owner
}

/// The application role's URL (row-level security applies).
pub fn app_url() -> &'static str {
    &urls().app
}

/// Resets the test database on first use (in a dedicated runtime, so that
/// each test keeps its own).
pub(super) fn urls() -> &'static TestDatabaseUrls {
    URLS.get_or_init(|| {
        std::thread::spawn(|| {
            let hash = hash_password(PASSWORD).expect("password hash");
            tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(reset_test_database(Some(hash)))
                .expect("test database reset")
        })
        .join()
        .expect("setup thread")
    })
}

pub fn email(user_id: Uuid) -> String {
    demo::users()
        .into_iter()
        .find(|u| u.id == user_id)
        .expect("demo user")
        .email
}

/// Monday to Friday of the current week, where the demo interventions are dated.
pub fn week() -> Value {
    let monday = monday_of(Utc::now().date_naive());
    json!({ "from": iso_date(monday), "to": iso_date(monday + Days::new(4)) })
}

pub fn next_monday() -> String {
    iso_date(monday_of(Utc::now().date_naive()) + Days::new(7))
}
