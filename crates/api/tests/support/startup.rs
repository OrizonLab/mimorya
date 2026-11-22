//! An app that refuses to start: what `build_app` says, before it serves
//! anything (a brand the platform refuses, ADR-222).

use std::collections::HashMap;

use mimorya_api::{AppConfig, BuildOptions, build_app};

use super::TestApi;
use super::fixtures::{WEB_ORIGIN, urls};

impl TestApi {
    /// Why the app does not start with `env`, every cause said.
    pub async fn refused_start(env: &[(&str, &str)]) -> String {
        let mut vars: HashMap<&str, String> = HashMap::from([
            ("MIMORYA_ENV", "test".to_string()),
            ("LOG_LEVEL", "off".to_string()),
            ("DATABASE_APP_URL", urls().app.clone()),
            ("WEB_ORIGIN", WEB_ORIGIN.to_string()),
            ("LLM_PROVIDER", "none".to_string()),
        ]);
        vars.extend(env.iter().map(|(key, value)| (*key, (*value).to_string())));
        let config = AppConfig::from_vars(|key| vars.get(key).cloned()).expect("config");
        match build_app(config, BuildOptions::default()).await {
            Ok(_) => panic!("the app started"),
            Err(error) => format!("{error:#}"),
        }
    }
}
