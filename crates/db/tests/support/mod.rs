//! Shared setup of the PostgreSQL integration tests of this crate: the test
//! database is recreated once per test binary.

use std::sync::OnceLock;

use mimorya_db::testing::{TestDatabaseUrls, reset_test_database};
use mimorya_db::{Database, PoolOptions};

static URLS: OnceLock<TestDatabaseUrls> = OnceLock::new();

/// Resets the test database on first use (in a dedicated runtime, so that
/// each test can keep its own).
pub fn urls(after_reset: fn(&TestDatabaseUrls)) -> &'static TestDatabaseUrls {
    URLS.get_or_init(|| {
        std::thread::spawn(move || {
            let urls = tokio::runtime::Runtime::new()
                .expect("runtime")
                .block_on(reset_test_database(None))
                .expect("test database reset");
            after_reset(&urls);
            urls
        })
        .join()
        .expect("setup thread")
    })
}

pub async fn connect(url: &str) -> Database {
    Database::connect(
        url,
        PoolOptions {
            max_connections: 4,
            application_name: "mimorya-test",
        },
    )
    .await
    .expect("test database connection")
}

/// The PostgreSQL message of a failed query.
#[allow(dead_code)] // not every test binary uses every helper
pub fn db_error<T: std::fmt::Debug>(result: sqlx::Result<T>) -> String {
    match result {
        Ok(value) => panic!("expected the query to fail, got {value:?}"),
        Err(error) => error.to_string(),
    }
}
