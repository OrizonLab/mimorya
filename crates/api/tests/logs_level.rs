//! The server's logs at the level the operator names (ADR-205), in a process
//! of their own since they are set once: no library's `log` record finer
//! than that level is even formed, the bridge's own level following the
//! filter's.

use mimorya_api::init_logs;
use tracing_log::log::{self, Level, Metadata};

#[test]
fn no_record_finer_than_the_filter_is_formed() {
    init_logs("info", false).unwrap();
    assert_eq!(log::max_level(), log::LevelFilter::Info);
    let record = |level| Metadata::builder().target("lettre").level(level).build();
    assert!(log::logger().enabled(&record(Level::Info)));
    assert!(!log::logger().enabled(&record(Level::Debug)));
}
