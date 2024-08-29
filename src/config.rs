use derive_config::DeriveTomlConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, DeriveTomlConfig, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_user_agent")]
    #[serde(skip_serializing_if = "is_default")]
    pub user_agent:   String,
    pub bot_secret:   String,
    pub sql_secret:   String,
    pub vrc_secret:   String,
    pub vrc_group_id: String,
    pub vrc_password: String,
    pub vrc_username: String,

    #[serde(default)]
    pub vrc_cookies: Vec<String>,
}

/// Get the default user agent
fn default_user_agent() -> String {
    format!(
        "{}/{} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    )
}

/// Check if the user agent is default
fn is_default(user_agent: &str) -> bool {
    user_agent.starts_with(env!("CARGO_PKG_NAME"))
        && user_agent.ends_with(env!("CARGO_PKG_AUTHORS"))
}
