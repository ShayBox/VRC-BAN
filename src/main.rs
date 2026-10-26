use color_eyre::Result;
use derive_config::DeriveTomlConfig;
#[cfg(feature = "discord")]
use poise::{serenity_prelude::*, Framework, FrameworkOptions};
#[cfg(feature = "discord")]
use vrc_ban::commands::prelude::*;
use vrc_ban::{config::Config, logsdb::LogsDB, vrchat::VRChat, Data};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    /* Load the config, or run the setup wizard if it doesn't exist yet */
    let mut config = if Config::exists()? {
        Config::load()?
    } else {
        Config::setup_wizard()?
    };
    let logsdb = LogsDB::connect(&config.sql_secret).await?;
    let vrchat = VRChat::new(
        &config.vrc_cookies,
        &config.vrc_username,
        &config.vrc_password,
        &config.user_agent,
    )?;

    /* Login to VRChat and save cookies */
    vrchat.login_and_verify(&config.vrc_secret).await?;
    config.vrc_cookies = vrchat.get_cookies();
    config.save()?;

    #[cfg(feature = "discord")]
    {
        let bot_secret = config.bot_secret.clone();
        let framework = Framework::builder()
            .options(FrameworkOptions {
                commands: vec![cheers(), pardon(), purge(), help()],
                event_handler: |framework, event| {
                    Box::pin(framework.user_data.event_handler(event, framework))
                },
                ..Default::default()
            })
            .setup(move |ctx, ready, framework| {
                Box::pin(Data::new(config, logsdb, vrchat).setup(ctx, ready, framework))
            })
            .build();

        let intent = GatewayIntents::non_privileged() | GatewayIntents::GUILDS;
        let mut client = ClientBuilder::new(&bot_secret, intent)
            .framework(framework)
            .await?;

        client.start().await?;
    }

    #[cfg(not(feature = "discord"))]
    {
        Data::new(config, logsdb, vrchat).sync_logs().await;
    }

    Ok(())
}
