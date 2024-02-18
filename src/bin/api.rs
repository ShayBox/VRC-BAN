#[macro_use]
extern crate rocket;

use color_eyre::Result;
use derive_config::DeriveTomlConfig;
use reqwest::Client;
use vrc_ban::{route::prelude::*, Config};

#[rocket::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let client = Client::builder()
        .user_agent(&config.vrc_user_agent)
        .build()?;

    rocket::build()
        .manage(config)
        .manage(client)
        .mount("/", routes![root, favicon, leaderboard])
        .launch()
        .await?;

    Ok(())
}
