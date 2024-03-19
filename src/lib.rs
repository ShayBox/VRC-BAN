#[macro_use]
extern crate rocket;

use std::{collections::HashSet, fmt::Display};

use color_eyre::Result;
use derive_config::{DeriveJsonConfig, DeriveTomlConfig};
use regex::Regex;
use rocket::response::status::BadRequest;
use serde::{Deserialize, Serialize};
use vrc::{
    api_client::AuthenticatedVRC,
    id::Group,
    model::GroupAuditLog,
    query::{Authenticating, Authentication},
};

pub mod command;
pub mod route;
pub mod vrchat;

pub fn bad_request<E: Display>(error: E) -> BadRequest<String> {
    BadRequest(error.to_string())
}

#[must_use]
pub fn default_user_agent() -> String {
    format!(
        "{}/{} {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    )
}

fn is_default_user_agent(haystack: &str) -> bool {
    Regex::new(&format!(
        "{}/(\\d+.\\d+.\\d+) {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_AUTHORS")
    ))
    .expect("Failed to parse regex")
    .is_match(haystack)
}

#[derive(Clone, Debug, Default, DeriveJsonConfig, Deserialize, Serialize)]
pub struct AuditLogs(HashSet<GroupAuditLog>);

#[derive(Clone, Debug, DeriveTomlConfig, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_user_agent")]
    #[serde(skip_serializing_if = "is_default_user_agent")]
    pub user_agent:     String,
    pub totp_2f_secret: String,
    pub discord_client: String,
    pub group_id_audit: Group,
    pub authenticating: Authenticating,
    pub authentication: Option<Authentication>,
}

pub struct Data {
    pub config: Config,
    pub vrchat: AuthenticatedVRC,
}
