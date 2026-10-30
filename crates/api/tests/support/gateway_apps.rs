//! Test apps whose companies' gateways reach a local messaging service:
//! Slack (ADR-125), Telegram (ADR-133), Microsoft as a Teams bot reaches it
//! (ADR-138).

use super::{Setup, TestApi};

impl TestApi {
    /// App backed by the scripted model, its companies' gateways reaching
    /// the local Slack at `url` (ADR-125), with configuration variables
    /// overridden.
    pub async fn start_with_slack(env: &[(&str, &str)], url: &str) -> Self {
        Self::scripted(Setup {
            env,
            slack_api: Some(url.to_owned()),
            ..Setup::default()
        })
        .await
    }

    /// Same, its companies' bots reaching the local Telegram at `url`
    /// (ADR-133), with configuration variables overridden.
    pub async fn start_with_telegram(env: &[(&str, &str)], url: &str) -> Self {
        Self::scripted(Setup {
            env,
            telegram_api: Some(url.to_owned()),
            ..Setup::default()
        })
        .await
    }

    /// Same, its companies' bots reaching the local Microsoft at `url`
    /// (ADR-138), with configuration variables overridden.
    pub async fn start_with_teams(env: &[(&str, &str)], url: &str) -> Self {
        Self::scripted(Setup {
            env,
            teams_api: Some(url.to_owned()),
            ..Setup::default()
        })
        .await
    }

    /// Same, its companies' gateways reaching the local Slack and Telegram.
    pub async fn start_with_gateways(env: &[(&str, &str)], slack: &str, telegram: &str) -> Self {
        Self::scripted(Setup {
            env,
            slack_api: Some(slack.to_owned()),
            telegram_api: Some(telegram.to_owned()),
            ..Setup::default()
        })
        .await
    }

    /// Same, reaching the local Slack and the local Microsoft.
    pub async fn start_with_slack_and_teams(
        env: &[(&str, &str)],
        slack: &str,
        teams: &str,
    ) -> Self {
        Self::scripted(Setup {
            env,
            slack_api: Some(slack.to_owned()),
            teams_api: Some(teams.to_owned()),
            ..Setup::default()
        })
        .await
    }
}
