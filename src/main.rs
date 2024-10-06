use color_eyre::Result;
use derive_config::DeriveTomlConfig;
use poise::{serenity_prelude::*, Framework, FrameworkOptions};
use vrc_ban::{commands::prelude::*, config::Config, logsdb::LogsDB, vrchat::VRChat, Data};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    /* Load Config, LogsDB, and VRChat */
    let mut config = Config::load()?;
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

    let framework = {
        let config = config.clone();
        Framework::builder()
            .options(FrameworkOptions {
                commands: vec![cheers(), pardon(), help()],
                event_handler: |ctx, event, framework, data| {
                    Box::pin(data.event_handler(ctx, event, framework))
                },
                ..Default::default()
            })
            .setup(move |ctx, ready, framework| {
                Box::pin(Data::new(config, logsdb, vrchat).setup(ctx, ready, framework))
            })
            .build()
    };

    let intent = GatewayIntents::non_privileged() | GatewayIntents::GUILDS;
    let mut client = ClientBuilder::new(&config.bot_secret, intent)
        .framework(framework)
        .await?;

    client.start().await?;

    Ok(())
}
