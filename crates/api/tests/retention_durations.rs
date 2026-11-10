//! Each company's own retention durations (ADR-195): the platform's until
//! it sets its own, within the platform's bounds, by who holds
//! `retention:manage` company-wide, not who manages its other settings. A
//! lengthening applies at once, a shortening a week later; a duration asked
//! as it already is changes nothing, its delay included. Each change is
//! audited, what it was and what it is.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin, holder_of};
use support::refusals::latest_entry;
use support::retention::{clear_retention, retention_changed_days_ago};

const RETENTION: &str = "/admin/retention";

fn asked(conversations: i32, episodes: i32, departed: i32) -> Value {
    json!({
        "conversationMonths": conversations,
        "episodeMonths": episodes,
        "departedDays": departed,
    })
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
    let set = paul.put(RETENTION, asked(36, 6, 14)).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
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
    let again = paul.put(RETENTION, asked(36, 6, 14)).await;
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
    let shorter = paul.put(RETENTION, asked(36, 6, 10)).await;
    assert_eq!(shorter.status, StatusCode::OK, "{}", shorter.text());
    let view = paul.get(RETENTION).await.json();
    assert_eq!(durations(&view)[2], ("departed".into(), 10, 30, true, true));
    assert_eq!(
        changes(&api, since).await[1],
        json!({ "departed": { "from": 14, "to": 10, "delayed": true } })
    );

    // Out of the bounds: refused with them, nothing changed.
    for (body, said) in [
        (asked(0, 6, 10), "Conversations inactives : de 1 à 60 mois"),
        (asked(36, 37, 10), "Mémoires épisodiques : de 1 à 36 mois"),
        (
            asked(36, 6, 6),
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
    for refused in [&claire, &settings, &partial] {
        assert_eq!(refused.get(RETENTION).await.status, StatusCode::FORBIDDEN);
        let set = refused.put(RETENTION, asked(1, 1, 7)).await;
        assert_eq!(set.status, StatusCode::FORBIDDEN, "{}", set.text());
    }
    assert!(changes(&api, since).await.is_empty());
    let platform = paul.get(RETENTION).await.json();
    assert!(
        durations(&platform).iter().all(|(_, _, _, own, _)| !own),
        "{platform}"
    );

    // Who holds it company-wide does.
    let set = keeper.put(RETENTION, asked(12, 12, 30)).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(
        changes(&api, since).await,
        [json!({ "conversations": { "from": 24, "to": 12, "delayed": true } })]
    );
    clear_retention(tenant::ID).await;
}
