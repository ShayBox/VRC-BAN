use color_eyre::{eyre::Error, Result};
use poise::{serenity_prelude::*, Context, CreateReply};

use crate::Data;

/// Information about VRC-BAN
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_, Data, Error>) -> Result<()> {
    let embed = CreateEmbed::default()
    .title("GitHub Source Code")
    .url("https://github.com/ShayBox/VRC-BAN")
    .description("Stoner Booth VRChat Group Discord Bot")
    .timestamp(Timestamp::now())
    .author(CreateEmbedAuthor::new("").name("Shayne Hartford (ShayBox)").url("https://shaybox.com").icon_url("https://avatars1.githubusercontent.com/u/9505196"))
    .field("Commands", "", false)
    .field("User", "Manage a VRChat user", true)
    .field("Help", "Information about VRC-BAN", true)
    .footer(CreateEmbedFooter::new("VRC-BAN").icon_url("https://cdn.discordapp.com/avatars/1208696990284914719/ab66b12988c0b0ba0e70405abe8089b6"));

    let builder = CreateReply::default().embed(embed).ephemeral(true);
    ctx.send(builder).await?;

    Ok(())
}
