#[macro_use]
extern crate rocket;

pub mod route;

use crate::route::prelude::*;
use config::{eyre::Result, ConfigFile};
use vrc_ban::{login, Config};

#[rocket::main]
async fn main() -> Result<()> {
    let mut config = Config::load()?;
    let vrchat = login(&mut config).await?;
    let rocket = rocket::build()
        .manage(config)
        .manage(vrchat)
        .mount("/", routes![root, favicon, leaderboard]);

    rocket.launch().await?;

    Ok(())
}
