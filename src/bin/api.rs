#[macro_use]
extern crate rocket;

use anyhow::Result;
use derive_config::DeriveTomlConfig;
use reqwest::Client;
use vrc_ban::{config::Config, logsdb::LogsDB, routes::prelude::*};

#[rocket::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = Client::builder().user_agent(&config.user_agent).build()?;
    let logsdb = LogsDB::connect(&config.sql_secret).await?;

    rocket::build()
        .manage(config)
        .manage(client)
        .manage(logsdb)
        .mount("/", routes![root, favicon, leaderboard])
        .launch()
        .await?;

    Ok(())
}
