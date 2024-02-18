use color_eyre::Result;
use derive_config::DeriveTomlConfig;
#[cfg(not(debug_assertions))]
use poise::samples::register_globally;
#[cfg(debug_assertions)]
use poise::samples::register_in_guild;
use poise::{serenity_prelude::*, Framework, FrameworkOptions};
use vrc_ban::{command::prelude::*, Config, Data};

#[rocket::main]
async fn main() -> Result<()> {
    let mut config = Config::load()?;
    let vrchat = vrc_ban::vrchat::login(&mut config).await?;
    let framework = {
        let config = config.clone();
        Framework::builder()
            .options(FrameworkOptions {
                commands: vec![user(), help()],
                ..Default::default()
            })
            .setup(move |ctx, _ready, framework| {
                Box::pin(async move {
                    #[cfg(debug_assertions)]
                    let guild_id = GuildId::new(824_865_729_445_888_041);
                    let commands = &framework.options().commands;

                    #[cfg(debug_assertions)]
                    register_in_guild(ctx, commands, guild_id).await?;
                    #[cfg(not(debug_assertions))]
                    register_globally(ctx, commands).await?;

                    Ok(Data { config, vrchat })
                })
            })
            .build()
    };

    let intent = GatewayIntents::non_privileged() | GatewayIntents::GUILDS;
    let mut client = ClientBuilder::new(&config.discord_client, intent)
        .framework(framework)
        .await?;

    client.start().await?;

    Ok(())
}
