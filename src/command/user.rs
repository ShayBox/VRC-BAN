use std::time::Duration;

use color_eyre::{Report, Result};
use poise::{
    serenity_prelude::{CreateInteractionResponse as CIR, *},
    Context,
    CreateReply,
};
use vrc::{
    api_client::ApiClient,
    query::{GroupBan, GroupUnban, SearchUser},
};

use crate::Data;

/// Manage a VRChat user
#[poise::command(slash_command, track_edits, required_permissions = "BAN_MEMBERS")]
pub async fn user(
    ctx: Context<'_, Data, Report>,
    #[description = "VRChat Username"] username: String,
) -> Result<()> {
    let Data { config, vrchat } = ctx.data();
    let query = SearchUser {
        search: username,
        ..Default::default()
    };

    let embed = CreateEmbed::default().title("⏳");
    let builder = CreateReply::default().embed(embed).ephemeral(true);
    let reply = ctx.send(builder).await?;

    let mut i = 0;
    let users = vrchat.query(query).await?;

    'submit: loop {
        let mut buttons = vec![
            CreateButton::new("ban")
                .emoji('🔨')
                .label("Ban")
                .style(ButtonStyle::Danger),
            CreateButton::new("pardon")
                .emoji('⚖')
                .label("Pardon")
                .style(ButtonStyle::Success),
        ];

        if i < users.len() - 1 {
            buttons.push(
                CreateButton::new("next")
                    .emoji('⏭')
                    .label("Next")
                    .style(ButtonStyle::Primary),
            );
        }

        if i > 0 {
            buttons.push(
                CreateButton::new("last")
                    .emoji('⏮')
                    .label("Last")
                    .style(ButtonStyle::Secondary),
            );
        }

        buttons.reverse();

        let user = &users[i];
        let builder = CreateReply::default()
            .components(vec![CreateActionRow::Buttons(buttons)])
            .embed(CreateEmbed::default()
                .title(&user.display_name)
                .url(format!("https://vrchat.com/home/user/{}", user.id))
                .description(&user.bio)
                .thumbnail(user.current_avatar_thumbnail_image_url.as_str())
                .footer(CreateEmbedFooter::new("VRC-BAN").icon_url("https://cdn.discordapp.com/avatars/1208696990284914719/ab66b12988c0b0ba0e70405abe8089b6"))
                .timestamp(Timestamp::now())
            );

        reply.edit(ctx, builder).await?;

        'edit: while let Some(mci) = ComponentInteractionCollector::new(ctx)
            .author_id(ctx.author().id)
            .channel_id(ctx.channel_id())
            .timeout(Duration::from_secs(60 * 60 * 24))
            .await
        {
            mci.create_response(ctx, CIR::Acknowledge).await?;
            match mci.data.custom_id.as_ref() {
                "last" => {
                    i -= 1;
                    break 'edit;
                }
                "next" => {
                    i += 1;
                    break 'edit;
                }
                "pardon" => {
                    let query = GroupUnban {
                        group_id: config.group_id_audit.clone(),
                        user_id:  user.id.clone(),
                    };
                    vrchat.query(query).await?;
                    reply.delete(ctx).await?;
                    break 'submit;
                }
                "ban" => {
                    let query = GroupBan {
                        group_id: config.group_id_audit.clone(),
                        user_id:  user.id.clone(),
                    };
                    vrchat.query(query).await?;
                    reply.delete(ctx).await?;
                    break 'submit;
                }
                _ => {}
            }
        }
    }

    Ok(())
}
