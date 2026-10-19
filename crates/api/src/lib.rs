//! HTTP API of the platform (axum): sessions, conversations, agent runs and
//! their event stream, approvals, memory and administration. It composes the
//! other crates and holds no business rule of its own.

pub mod app;
pub mod auth;
pub mod config;
pub mod http;
pub mod logging;
pub mod mail;
pub mod plugins;
pub mod routes;
pub mod secrets;
pub mod services;

pub use app::{App, AppState, BuildOptions, Services, build_app};
pub use config::AppConfig;
pub use logging::log_filter;
