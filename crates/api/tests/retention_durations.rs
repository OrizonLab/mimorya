//! Each company's own retention durations (ADR-195): the platform's until
//! it sets its own, within the platform's bounds, by who holds
//! `retention:manage` company-wide, not who manages its other settings. A
//! lengthening applies at once, a shortening a week later; a duration asked
//! as it already is changes nothing, its delay included. Each change is
//! audited, what it was and what it is. A change names the version of the
//! durations it was asked on, refused once another changed them, two
//! crossing on the same version included (ADR-196).

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_api::services::admin::DURATIONS_CHANGED;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin, holder_of};
use support::locks::release_after;
use support::refusals::latest_entry;
use support::retention::{clear_retention, retention_changed_days_ago};
use support::{Client, TestApi, TestResponse};

const RETENTION: &str = "/admin/retention";

fn asked(replaces: &str, conversations: i32, episodes: i32, departed: i32) -> Value {
    json!({
        "conversationMonths": conversations,
        "episodeMonths": episodes,
        "departedDays": departed,
        "replaces": replaces,
    })
}

/// The version of the durations as `client` reads them.
async fn version(client: &Client<'_>) -> String {
    let view = client.get(RETENTION).await.json();
    view["version"].as_str().unwrap().to_string()
}

/// `client` sets the durations on the version it reads first.
async fn set(
    client: &Client<'_>,
    conversations: i32,
    episodes: i32,
    departed: i32,
) -> TestResponse {
    let replaces = version(client).await;
    let body = asked(&replaces, conversations, episodes, departed);
    client.put(RETENTION, body).await
}

/// Each kind's (duration, in effect, own, whether a shortening waits).
fn durations(view: &Value) -> Vec<(String, i64, i64, bool, bool)> {
    view["durations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap().to_string(),
                d["duration"].as_i64().unwrap(),
                d["inEffect"].as_i64().unwrap(),
                d["own"].as_bool().unwrap(),
                d.get("shorteningAppliesAt").is_some(),
            )
        })
        .collect()
}

/// The audited changes of the company's retention after the entry `since`.
async fn changes(api: &TestApi, since: i64) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entries = sqlx::query_scalar(
        "select metadata from audit_log where action = 'retention.configured' and id > $1 \
         order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    entries
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_sets_its_retention_within_the_platform_s_bounds() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;

    // The platform's, with its bounds.
    let platform = paul.get(RETENTION).await.json();
    assert_eq!(platform["shorteningDelayDays"], 7);
    let label = |view: &Value, at: usize| view["durations"][at]["label"].clone();
    assert_eq!(label(&platform, 0), "Conversations inactives");
    assert_eq!(
        (platform["durations"][2].clone())["unit"],
        "days",
        "{platform}"
    );
    assert_eq!(platform["durations"][0]["min"], 1);
    assert_eq!(platform["durations"][0]["max"], 60);
    assert_eq!(platform["durations"][2]["min"], 7);
    assert_eq!(
        durations(&platform),
        [
            ("conversations".into(), 24, 24, false, false),
            ("episodes".into(), 12, 12, false, false),
            ("departed".into(), 30, 30, false, false),
        ]
    );

    // Longer conversations apply at once; shorter episodes and departed
    // memories wait a week, the platform's applied meanwhile.
    let first = set(&paul, 36, 6, 14).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let own = paul.get(RETENTION).await.json();
    assert_eq!(
        durations(&own),
        [
            ("conversations".into(), 36, 36, true, false),
            ("episodes".into(), 6, 12, true, true),
            ("departed".into(), 14, 30, true, true),
        ]
    );
    assert_eq!(
        changes(&api, since).await,
        [json!({
            "conversations": { "from": 24, "to": 36, "delayed": false },
            "episodes": { "from": 12, "to": 6, "delayed": true },
            "departed": { "from": 30, "to": 14, "delayed": true },
        })]
    );

    // Asked again as they are: nothing changes, the delay goes on, its
    // shortening a day away.
    retention_changed_days_ago(tenant::ID, "episodes", 6).await;
    let again = set(&paul, 36, 6, 14).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(changes(&api, since).await.len(), 1);
    let waiting = paul.get(RETENTION).await.json();
    assert_eq!(
        durations(&waiting)[1],
        ("episodes".into(), 6, 12, true, true)
    );
    let applies = waiting["durations"][1]["shorteningAppliesAt"]
        .as_str()
        .unwrap()
        .parse::<DateTime<Utc>>()
        .unwrap();
    let in_a_day = applies - Utc::now() - Duration::days(1);
    assert!(in_a_day.num_minutes().abs() < 5, "{applies}");
    retention_changed_days_ago(tenant::ID, "episodes", 8).await;
    let waited = paul.get(RETENTION).await.json();
    assert_eq!(
        durations(&waited)[1],
        ("episodes".into(), 6, 6, true, false)
    );

    // Shortened again within the week: what applied until then still
    // applies for a week, the platform's longer one.
    let shorter = set(&paul, 36, 6, 10).await;
    assert_eq!(shorter.status, StatusCode::OK, "{}", shorter.text());
    let view = paul.get(RETENTION).await.json();
    assert_eq!(durations(&view)[2], ("departed".into(), 10, 30, true, true));
    assert_eq!(
        changes(&api, since).await[1],
        json!({ "departed": { "from": 14, "to": 10, "delayed": true } })
    );

    // Out of the bounds: refused with them, nothing changed.
    let read = version(&paul).await;
    for (body, said) in [
        (
            asked(&read, 0, 6, 10),
            "Conversations inactives : de 1 à 60 mois",
        ),
        (
            asked(&read, 36, 37, 10),
            "Mémoires épisodiques : de 1 à 36 mois",
        ),
        (
            asked(&read, 36, 6, 6),
            "Mémoires personnelles d’un compte désactivé : de 7 à 90 jours",
        ),
    ] {
        let refused = paul.put(RETENTION, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert_eq!(refused.error_message(), said);
    }
    assert_eq!(paul.get(RETENTION).await.json(), view);
    assert_eq!(changes(&api, since).await.len(), 2);
    clear_retention(tenant::ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_holds_retention_manage_company_wide_sets_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let settings = holder_of(&api, &paul, "reglages.retention", &["settings:manage"]).await;
    let keeper = holder_of(&api, &paul, "gardien.retention", &["retention:manage"]).await;
    // An administrator of one team only: the role holds it, not over the
    // whole company.
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.retention").await;
    let partial = activate(&api, &mut partial).await;
    let since = latest_entry(&api).await;

    // Neither a member, nor who manages the other settings, nor who holds
    // it on one team.
    let read = version(&paul).await;
    for refused in [&claire, &settings, &partial] {
        assert_eq!(refused.get(RETENTION).await.status, StatusCode::FORBIDDEN);
        let put = refused.put(RETENTION, asked(&read, 1, 1, 7)).await;
        assert_eq!(put.status, StatusCode::FORBIDDEN, "{}", put.text());
    }
    assert!(changes(&api, since).await.is_empty());
    let platform = paul.get(RETENTION).await.json();
    assert!(
        durations(&platform).iter().all(|(_, _, _, own, _)| !own),
        "{platform}"
    );

    // Who holds it company-wide does.
    let kept = set(&keeper, 12, 12, 30).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    assert_eq!(
        changes(&api, since).await,
        [json!({ "conversations": { "from": 24, "to": 12, "delayed": true } })]
    );
    clear_retention(tenant::ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_asked_on_durations_changed_since_is_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let keeper = holder_of(&api, &paul, "second.retention", &["retention:manage"]).await;
    let since = latest_entry(&api).await;

    // Both read the platform's; the keeper lengthens the conversations.
    let opened = version(&paul).await;
    assert_eq!(version(&keeper).await, opened);
    let lengthened = keeper.put(RETENTION, asked(&opened, 36, 12, 30)).await;
    assert_eq!(lengthened.status, StatusCode::OK, "{}", lengthened.text());
    let read = version(&paul).await;
    assert_ne!(read, opened);

    // Paul, on what he read before, shortens the episodes: his page would
    // shorten the conversations back to 24 months without asking. Refused,
    // nothing changes.
    let stale = paul.put(RETENTION, asked(&opened, 24, 6, 30)).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text());
    assert_eq!(
        stale.error_message(),
        "Les durées de conservation ont changé depuis l’ouverture de cette page : \
         rechargez-la pour voir les durées actuelles"
    );
    let unchanged = paul.get(RETENTION).await.json();
    assert_eq!(unchanged["version"], read.as_str());
    assert_eq!(
        durations(&unchanged)[..2],
        [
            ("conversations".into(), 36, 36, true, false),
            ("episodes".into(), 12, 12, false, false),
        ]
    );
    assert_eq!(changes(&api, since).await.len(), 1);

    // On what he reads now, it is done; asked again as it is, nothing
    // changes, the version neither.
    let done = paul.put(RETENTION, asked(&read, 36, 6, 30)).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    let after = version(&paul).await;
    assert_ne!(after, read);
    let again = paul.put(RETENTION, asked(&after, 36, 6, 30)).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(version(&paul).await, after);
    assert_eq!(changes(&api, since).await.len(), 2);

    // A version longer than any, or empty, is refused as such; none, as a
    // body missing a field.
    for replaces in ["0".repeat(65), String::new()] {
        let malformed = paul.put(RETENTION, asked(&replaces, 36, 6, 30)).await;
        assert_eq!(
            malformed.status,
            StatusCode::BAD_REQUEST,
            "{}",
            malformed.text()
        );
    }
    let mut without = asked(&after, 36, 6, 30);
    without.as_object_mut().unwrap().remove("replaces");
    let missing = paul.put(RETENTION, without).await;
    assert!(missing.status.is_client_error(), "{}", missing.text());
    assert_ne!(missing.status, StatusCode::CONFLICT, "{}", missing.text());
    clear_retention(tenant::ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn of_two_changes_crossing_on_the_same_version_one_passes() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let keeper = holder_of(&api, &paul, "croise.retention", &["retention:manage"]).await;
    let since = latest_entry(&api).await;

    // Both read the same version; the test holds the company's
    // administration lock until both changes wait behind it.
    let read = version(&paul).await;
    let mut hold = api.app.services.db.begin(tenant::ID).await.unwrap();
    hold.lock_administration().await.unwrap();
    let (longer, shorter, ()) = tokio::join!(
        paul.put(RETENTION, asked(&read, 36, 12, 30)),
        keeper.put(RETENTION, asked(&read, 24, 6, 30)),
        release_after(&api, 2, hold),
    );

    // The first to take the lock passes; the other, asked on what the first
    // changed, is refused.
    let mut statuses = [longer.status, shorter.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    let refused = [&longer, &shorter]
        .into_iter()
        .find(|answer| answer.status == StatusCode::CONFLICT)
        .unwrap();
    assert_eq!(refused.error_message(), DURATIONS_CHANGED);
    let view = paul.get(RETENTION).await.json();
    let set = &durations(&view)[..2];
    let expected = if longer.status == StatusCode::OK {
        [
            ("conversations".into(), 36, 36, true, false),
            ("episodes".into(), 12, 12, false, false),
        ]
    } else {
        [
            ("conversations".into(), 24, 24, false, false),
            ("episodes".into(), 6, 12, true, true),
        ]
    };
    assert_eq!(set, expected);
    assert_eq!(changes(&api, since).await.len(), 1);
    clear_retention(tenant::ID).await;
}
