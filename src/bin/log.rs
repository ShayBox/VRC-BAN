use std::time::Duration;

use color_eyre::Result;
use derive_config::DeriveTomlConfig;
use vrc_ban::{config::Config, logsdb::LogsDB, vrchat::VRChat};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    /* Load the config and connect to MySql */
    let mut config = Config::load()?;
    let logdb = LogsDB::connect(&config.sql_secret).await?;
    let vrchat = VRChat::new(
        &config.vrc_cookies,
        &config.vrc_username,
        &config.vrc_password,
        &config.user_agent,
    )?;

    /* Login, Verify, and Save the login cookies */
    vrchat.login_and_verify(&config.vrc_secret).await?;
    config.vrc_cookies = vrchat.get_cookies();
    config.save()?;

    loop {
        let Ok(logs) = vrchat
            .get_group_audit_logs(&config.vrc_group_id, 100, 0)
            .await
        else {
            continue;
        };

        for log in logs {
            if let Err(error) = logdb.insert_log(log).await {
                eprintln!("Error: {error}");
                break;
            };
        }

        tokio::time::sleep(Duration::from_secs(600)).await;
    }
}