//! The server's logs as it sets them up (ADR-205), in a process of their
//! own since they are set once: the IMAP library's `log` records, which
//! would write a person's `LOGIN` and its password at `trace`, never reach
//! the subscriber, whatever the level the operator names, while every other
//! library's do, down to that level.

use mimorya_api::init_logs;
use tracing_log::log::{self, Level, Metadata};

fn enabled(target: &str, level: Level) -> bool {
    let record = Metadata::builder().target(target).level(level).build();
    log::logger().enabled(&record)
}

#[test]
fn the_imap_library_writes_nothing_even_at_trace() {
    init_logs("trace,async_imap=trace", false).unwrap();
    for target in [
        "async_imap",
        "async_imap::imap_stream",
        "imap_proto::parser",
    ] {
        for level in [Level::Error, Level::Info, Level::Trace] {
            assert!(!enabled(target, level), "{target} {level}");
        }
    }
    // Every other library's, down to the level named.
    assert!(enabled("lettre::transport::smtp", Level::Info));
    assert!(enabled("hyper_util::client", Level::Trace));
    assert_eq!(log::max_level(), log::LevelFilter::Trace);
}
