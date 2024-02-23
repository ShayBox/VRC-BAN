#[macro_use]
extern crate rocket;

use color_eyre::Result;
use derive_config::{DeriveJsonConfig, DeriveTomlConfig};
use reqwest::Client;
use rocket::tokio::sync::Mutex;
use vrc_ban::{route::prelude::*, AuditLogs, Config};

#[rocket::main]
async fn main() -> Result<()> {
    let mut config = Config::load()?;
    let audits = Mutex::new(AuditLogs::load().unwrap_or_default());
    let client = Client::builder().user_agent(&config.user_agent).build()?;
    let vrchat = vrc_ban::vrchat::login(&mut config).await?;

    rocket::build()
        .manage(config)
        .manage(client)
        .manage(audits)
        .manage(vrchat)
        .mount("/", routes![root, favicon, leaderboard])
        .launch()
        .await?;

    Ok(())
}
