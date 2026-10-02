use std::io::{self, Write};

use color_eyre::Result;
use derive_config::DeriveTomlConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, DeriveTomlConfig, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_user_agent")]
    #[serde(skip_serializing_if = "is_default")]
    pub user_agent:   String,
    #[cfg_attr(not(feature = "discord"), serde(default))]
    pub bot_secret:   String,
    pub sql_secret:   String,
    pub vrc_secret:   String,
    pub vrc_group_id: String,
    pub vrc_password: String,
    pub vrc_username: String,

    #[serde(default)]
    pub vrc_cookies: Vec<String>,
}

impl Config {
    /// # Check if the config file exists on disk
    ///
    /// # Errors
    /// Will return `Err` if `Self::path` fails.
    pub fn exists() -> Result<bool> {
        Ok(Self::path()?.exists())
    }

    /// # Run an interactive setup wizard to create and save a new config
    ///
    /// Prompts for each required field, then saves the resulting config.
    ///
    /// # Errors
    /// Will return `Err` if reading input or saving fails.
    pub fn setup_wizard() -> Result<Self> {
        println!("No configuration found. Let's set one up!");
        println!();

        let vrc_username = prompt("VRChat username (email): ")?;
        let vrc_password = prompt("VRChat password: ")?;
        let vrc_secret = prompt("VRChat 2FA/TOTP secret: ")?;
        let vrc_group_id = prompt("VRChat group ID: ")?;

        #[cfg(feature = "discord")]
        let bot_secret = prompt("Discord bot token: ")?;
        #[cfg(not(feature = "discord"))]
        let bot_secret = String::new();

        let sql_secret = prompt("MySQL connection URL: ")?;

        let config = Self {
            user_agent: default_user_agent(),
            bot_secret,
            sql_secret,
            vrc_secret,
            vrc_group_id,
            vrc_password,
            vrc_username,
            vrc_cookies: Vec::new(),
        };

        config.save()?;
        println!();
        println!("Configuration saved to {}", Self::path()?.display());

        Ok(config)
    }
}

/// Prompt the user for a non-empty line of input.
fn prompt(label: &str) -> Result<String> {
    loop {
        print!("{label}");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let value = input.trim_end_matches(['\n', '\r']).to_owned();

        if !value.is_empty() {
            return Ok(value);
        }

        println!("  (required)");
    }
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
