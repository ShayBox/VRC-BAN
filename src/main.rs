#[macro_use]
extern crate rocket;

pub mod route;

use std::clone::Clone;

use config::{eyre::Result, ConfigFile};
use reqwest::Client;
use vrc_ban::{login_to_vrchat, Config, DEFAULT_USER_AGENT};

use crate::route::prelude::*;

#[rocket::main]
async fn main() -> Result<()> {
    let mut config = Config::load()?;
    let user_agent = config
        .vrc_user_agent
        .as_ref()
        .map_or_else(|| DEFAULT_USER_AGENT.to_owned(), Clone::clone);

    let client = Client::builder().user_agent(&user_agent).build()?;
    let vrchat = login_to_vrchat(&mut config, user_agent).await?;
    let rocket = rocket::build()
        .manage(config)
        .manage(client)
        .manage(vrchat)
        .mount("/", routes![root, favicon, leaderboard]);

    rocket.launch().await?;

    Ok(())
}
