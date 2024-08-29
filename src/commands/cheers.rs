use color_eyre::{
    eyre::{Error, OptionExt},
    Result,
};
use poise::{Context, CreateReply};

use crate::Data;

/// Opt-in or out of the Cheers role
#[poise::command(slash_command, guild_only, required_bot_permissions = "MANAGE_ROLES")]
pub async fn cheers(ctx: Context<'_, Data, Error>) -> Result<()> {
    let author_member = ctx.author_member().await.ok_or_eyre("Guild Only")?;
    let (id, _role) = {
        let guild = ctx.guild().ok_or_eyre("Guild Only")?.clone();
        let role = guild.roles.into_iter().find(|(_, r)| r.name == "Cheers");
        role.ok_or_eyre("'Cheers' Role Missing")?
    };

    let content = if author_member.roles.contains(&id) {
        author_member.remove_role(&ctx, id).await?;
        "You were removed from the Cheers role"
    } else {
        author_member.add_role(&ctx, id).await?;
        "You were added to the Cheers role"
    };

    let builder = CreateReply::default().content(content).ephemeral(true);
    ctx.send(builder).await?;

    Ok(())
}
