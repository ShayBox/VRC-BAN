use color_eyre::{eyre::Error, Result};
use poise::{serenity_prelude::*, Context, CreateReply};

use crate::Data;

/// Purge all the bans from the group.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn purge(ctx: Context<'_, Data, Error>) -> Result<()> {
    let embed = CreateEmbed::default().title("⏳");
    let builder = CreateReply::default().embed(embed);
    let reply = ctx.send(builder.clone()).await?;

    let Data {
        config,
        logsdb: _,
        vrchat,
    } = ctx.data();

    /* Get all the banned group members */
    let bans = vrchat.get_group_bans(&config.vrc_group_id).await?;

    /* Unban everyone that's banned */
    for ban in &bans {
        vrchat
            .pardon_member(&config.vrc_group_id, &ban.user_id)
            .await?;
    }

    /* Report how many people were unbanned */
    let embed = CreateEmbed::default()
        .title("✅")
        .description(format!("Purged {} bans", bans.len()));
    reply.edit(ctx, builder.embed(embed)).await?;

    Ok(())
}
