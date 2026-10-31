//! The shared rate limit windows (ADR-088), through the functions that alone
//! touch them: a key's hits within its window, a refused hit not counted
//! on, a hit given back, distinct values counted once, even asked at once,
//! windows reopened once over and purged long after, never within two
//! hours whatever the horizon asked (ADR-152), the largest limits counted
//! without overflowing. Keys stay apart by limiter.

mod support;

use chrono::{Duration, Utc};
use mimorya_db::Database;
use mimorya_db::repositories::rate_limits::{
    hit_distinct_rate_limit, hit_rate_limit, purge_rate_limits, release_rate_limit,
};

const MINUTE: f64 = 60.0;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

/// Moves `key`'s window of `limiter` back by `minutes`, as time would.
async fn age(limiter: &str, key: &[u8], minutes: i64) {
    let owner = support::connect(&support::urls(|_| {}).owner).await;
    sqlx::query(
        "update rate_limit_windows set window_start = window_start - make_interval(mins => $3) \
         where limiter = $1 and key_digest = $2",
    )
    .bind(limiter)
    .bind(key)
    .bind(i32::try_from(minutes).unwrap())
    .execute(owner.pool())
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_key_has_so_many_hits_a_window_and_a_hit_given_back_counts_no_more() {
    let db = app().await;
    let pool = db.pool();
    let key = b"key-of-hits";
    for _ in 0..2 {
        assert!(
            hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
                .await
                .unwrap()
        );
    }
    // Refused, however often: no count past one more than the limit.
    for _ in 0..3 {
        assert!(
            !hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
                .await
                .unwrap()
        );
    }
    release_rate_limit(pool, "test_hits", key, 2, MINUTE)
        .await
        .unwrap();
    assert!(
        hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
            .await
            .unwrap()
    );
    assert!(
        !hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
            .await
            .unwrap()
    );
    // Another limiter keeps the same key apart.
    assert!(
        hit_rate_limit(pool, "test_other", key, 2, MINUTE)
            .await
            .unwrap()
    );

    // The window over, a new one opens; a hit given back afterwards no
    // longer reaches the former one.
    age("test_hits", key, 2).await;
    release_rate_limit(pool, "test_hits", key, 2, MINUTE)
        .await
        .unwrap();
    assert!(
        hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
            .await
            .unwrap()
    );
    assert!(
        hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
            .await
            .unwrap()
    );
    assert!(
        !hit_rate_limit(pool, "test_hits", key, 2, MINUTE)
            .await
            .unwrap()
    );
}

/// Sets `key`'s count of `limiter`, as so many hits would.
async fn set_hits(limiter: &str, key: &[u8], hits: i32) {
    let owner = support::connect(&support::urls(|_| {}).owner).await;
    sqlx::query("update rate_limit_windows set hits = $3 where limiter = $1 and key_digest = $2")
        .bind(limiter)
        .bind(key)
        .bind(hits)
        .execute(owner.pool())
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_largest_limits_count_without_overflowing() {
    let db = app().await;
    let pool = db.pool();
    for limit in [i32::MAX - 1, i32::MAX] {
        let key = format!("key-largest-{limit}");
        let key = key.as_bytes();
        for _ in 0..3 {
            assert!(
                hit_rate_limit(pool, "test_largest", key, limit, MINUTE)
                    .await
                    .unwrap()
            );
        }
        release_rate_limit(pool, "test_largest", key, limit, MINUTE)
            .await
            .unwrap();
    }
    // At the limit, then past it: refused, the count no further.
    let key = b"key-at-the-largest";
    let limit = i32::MAX - 1;
    assert!(
        hit_rate_limit(pool, "test_largest", key, limit, MINUTE)
            .await
            .unwrap()
    );
    set_hits("test_largest", key, limit).await;
    for _ in 0..2 {
        assert!(
            !hit_rate_limit(pool, "test_largest", key, limit, MINUTE)
                .await
                .unwrap()
        );
    }
    release_rate_limit(pool, "test_largest", key, limit, MINUTE)
        .await
        .unwrap();
    assert!(
        hit_rate_limit(pool, "test_largest", key, limit, MINUTE)
            .await
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires PostgreSQL"]
async fn values_asked_at_once_are_counted_one_at_a_time() {
    let db = app().await;
    let mut attempts = tokio::task::JoinSet::new();
    for i in 0..12 {
        let pool = db.pool().clone();
        attempts.spawn(async move {
            let value = format!("value-{i}");
            let key = b"key-asked-at-once";
            hit_distinct_rate_limit(&pool, "test_at_once", key, value.as_bytes(), 1, MINUTE)
                .await
                .unwrap()
        });
    }
    let allowed = attempts
        .join_all()
        .await
        .into_iter()
        .filter(|allowed| *allowed)
        .count();
    assert_eq!(allowed, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn distinct_values_are_counted_once_a_window() {
    let db = app().await;
    let pool = db.pool();
    let key = b"key-of-values";
    let hit =
        |value: &'static [u8]| hit_distinct_rate_limit(pool, "test_values", key, value, 2, MINUTE);
    assert!(hit(b"one").await.unwrap());
    assert!(hit(b"one").await.unwrap());
    assert!(hit(b"two").await.unwrap());
    assert!(!hit(b"three").await.unwrap());
    assert!(hit(b"two").await.unwrap());

    // A new window forgets the values of the former one.
    age("test_values", key, 2).await;
    assert!(hit(b"three").await.unwrap());
    assert!(hit(b"four").await.unwrap());
    assert!(!hit(b"one").await.unwrap());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn windows_long_over_are_purged_with_their_values() {
    let db = app().await;
    let pool = db.pool();
    let (old, recent) = (b"key-long-over", b"key-recent");
    assert!(
        hit_distinct_rate_limit(pool, "test_purge", old, b"v", 5, MINUTE)
            .await
            .unwrap()
    );
    assert!(
        hit_rate_limit(pool, "test_purge", recent, 5, MINUTE)
            .await
            .unwrap()
    );
    age("test_purge", old, 180).await;

    let purged = purge_rate_limits(pool, Utc::now() - Duration::hours(2))
        .await
        .unwrap();
    assert_eq!(purged, 1);
    let owner = support::connect(&support::urls(|_| {}).owner).await;
    let left: Vec<(Vec<u8>, i64)> = sqlx::query_as(
        "select w.key_digest, (select count(*) from rate_limit_values v \
           where v.limiter = w.limiter and v.key_digest = w.key_digest) \
         from rate_limit_windows w where w.limiter = 'test_purge'",
    )
    .fetch_all(owner.pool())
    .await
    .unwrap();
    assert_eq!(left, [(recent.to_vec(), 0)]);
    let values: i64 =
        sqlx::query_scalar("select count(*) from rate_limit_values where limiter = 'test_purge'")
            .fetch_one(owner.pool())
            .await
            .unwrap();
    assert_eq!(values, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_purge_never_forgets_a_window_opened_within_two_hours() {
    let db = app().await;
    let pool = db.pool();
    let keys: [(&[u8], i64); 3] = [
        (b"key-now", 0),
        (b"key-almost-two-hours", 115),
        (b"key-over-two-hours", 125),
    ];
    for (key, minutes) in keys {
        assert!(
            hit_rate_limit(pool, "test_horizon", key, 5, MINUTE)
                .await
                .unwrap()
        );
        age("test_horizon", key, minutes).await;
    }

    // Asked to forget everything, even what is still counting: only the
    // window opened over two hours ago goes.
    let purged = purge_rate_limits(pool, Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(purged, 1);
    let owner = support::connect(&support::urls(|_| {}).owner).await;
    let mut left: Vec<Vec<u8>> = sqlx::query_scalar(
        "select key_digest from rate_limit_windows where limiter = 'test_horizon'",
    )
    .fetch_all(owner.pool())
    .await
    .unwrap();
    left.sort();
    assert_eq!(
        left,
        [b"key-almost-two-hours".to_vec(), b"key-now".to_vec()]
    );
}
