use std::time::Duration;

use color_eyre::{
    eyre::{bail, Error, OptionExt},
    Result,
};
use poise::{
    serenity_prelude::{CreateInteractionResponse as CIR, *},
    Context,
    CreateReply,
    ReplyHandle,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{logsdb::Log, Data};

struct Message<'a> {
    builder: CreateReply,
    reply:   ReplyHandle<'a>,
}

impl Message<'_> {
    async fn new(ctx: Context<'_, Data, Error>) -> Result<Message<'_>> {
        let embed = CreateEmbed::default().title("⏳");
        let builder = CreateReply::default().embed(embed);
        let reply = ctx.send(builder.clone()).await?;

        Ok(Message { builder, reply })
    }
}

/// Pardon (unban) a user from Stoner Booth.
/// Search is sorted most by most recent bans by default.
#[allow(clippy::too_many_lines)]
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn pardon(
    ctx: Context<'_, Data, Error>,
    #[description = "Search by User Name"] name: Option<String>,
    #[description = "Search by User UUID"] uuid: Option<String>,
) -> Result<()> {
    let message = Message::new(ctx).await?;
    let Data {
        config: _,
        logsdb,
        vrchat,
    } = ctx.data();

    /* Parse the moderator input (name, uuid, recent) */
    let logs = if let Some(search) = name {
        let users = vrchat.search_users(&search).await?;
        let Some(user) = users.first() else {
            bail!("No user found")
        };

        logsdb.get_recent_actions_by_id(&user.id).await?
    } else if let Some(target_id) = uuid {
        logsdb.get_recent_actions_by_id(&target_id).await?
    } else {
        logsdb.get_all_recent_actions().await?
    };

    /* Paginate the unique user ids */
    paginate_logs(ctx, message, &logs).await
}

async fn paginate_logs(
    ctx: Context<'_, Data, Error>,
    message: Message<'_>,
    logs: &[Log],
) -> Result<()> {
    let mut index = 0;

    'done: loop {
        let Some(log) = logs.get(index) else {
            message.reply.delete(ctx).await?;
            bail!("No results found")
        };

        let user_id = log.target_id.clone().ok_or_eyre("None")?;
        edit_message_embed(ctx, &message, logs, index).await?;

        /* Capture users button input in a loop until valid input is received */
        'page: while let Some(mci) = ComponentInteractionCollector::new(ctx)
            .author_id(ctx.author().id)
            .channel_id(ctx.channel_id())
            .timeout(Duration::MAX)
            .await
        {
            let Data {
                config,
                logsdb: _,
                vrchat,
            } = ctx.data();

            mci.create_response(ctx, CIR::Acknowledge).await?;
            match mci.data.custom_id.as_ref() {
                "last" => {
                    message.reply.edit(ctx, message.builder.clone()).await?;
                    index -= 1;

                    break 'page;
                }
                "next" => {
                    message.reply.edit(ctx, message.builder.clone()).await?;
                    index += 1;

                    break 'page;
                }
                "pardon" => {
                    message.reply.delete(ctx).await?;
                    vrchat.pardon_member(&config.vrc_group_id, &user_id).await?;

                    break 'done;
                }
                "ban" => {
                    message.reply.delete(ctx).await?;
                    vrchat.ban_member(&config.vrc_group_id, &user_id).await?;

                    break 'done;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

async fn edit_message_embed(
    ctx: Context<'_, Data, Error>,
    message: &Message<'_>,
    logs: &[Log],
    index: usize,
) -> Result<()> {
    let Data {
        config,
        logsdb: _,
        vrchat,
    } = ctx.data();

    /* Get the user and member */
    let log = &logs[index];
    let user_id = log.target_id.clone().ok_or_eyre("None")?;
    let user = vrchat.get_user(&user_id).await?;

    /* Fallback to the avatar thumbnail without VRC+ */
    let mut url = user.profile_pic_override_thumbnail;
    if url.is_empty() {
        url = user.current_avatar_thumbnail_image_url;
    }

    /* Create the embed with information */
    let author = CreateEmbedAuthor::new(user.display_name)
        .icon_url(user.user_icon)
        .url(format!("https://vrchat.com/home/user/{user_id}"));
    let mut embed = CreateEmbed::default()
        .author(author)
        .description(user.bio)
        .image(url);

    /* Add the badges (Supporter, Early Supporter) */
    if let Some(badges) = user.badges {
        let badges = badges
            .into_iter()
            .map(|badge| badge.badge_name)
            .collect::<Vec<_>>();

        if !badges.is_empty() {
            embed = embed.field("Badges", badges.join(", "), true);
        }
    }

    /* Create and Add the last and next buttons */
    let mut buttons = Vec::new();
    if index > 0 {
        let button = CreateButton::new("last")
            .emoji('⬅')
            .label("Last")
            .style(ButtonStyle::Secondary);

        buttons.push(button);
    }
    if index < logs.len() {
        let button = CreateButton::new("next")
            .emoji('➡')
            .label("Next")
            .style(ButtonStyle::Secondary);

        buttons.push(button);
    }

    if let Ok(member) = vrchat
        .get_group_member(&config.vrc_group_id, &user_id)
        .await
    {
        let actor = vrchat.get_user(&log.actor_id).await?;
        if let Some(text) = match log.event_type.as_ref() {
            "group.user.ban" => Some(format!("Banned by {}", actor.display_name)),
            "group.user.unban" => Some(String::from("Pardoned")),
            _ => None,
        } {
            /* Add the staff member and when the action was done */
            let date_time = OffsetDateTime::parse(&log.created_at, &Rfc3339)?;
            let timestamp = Timestamp::from_unix_timestamp(date_time.unix_timestamp())?;
            let footer = CreateEmbedFooter::new(text).icon_url(actor.user_icon);
            embed = embed.footer(footer).timestamp(timestamp);
        }

        /* Use the `VRChat` API because the `LogsDB` might not be cached yet */
        let button = if let Some(banned_at) = member.banned_at.flatten() {
            /* Override with more accurate data if it's available */
            let date_time = OffsetDateTime::parse(&banned_at, &Rfc3339)?;
            let timestamp = Timestamp::from_unix_timestamp(date_time.unix_timestamp())?;
            embed = embed.timestamp(timestamp);

            CreateButton::new("pardon")
                .emoji('⚖')
                .label("Pardon")
                .style(ButtonStyle::Success)
        } else {
            CreateButton::new("ban")
                .emoji('🔨')
                .label("Ban")
                .style(ButtonStyle::Success)
        };

        buttons.push(button);
    }

    /* Wrap the buttons into components then build and send the reply */
    let mut builder = CreateReply::default().embed(embed);
    if !buttons.is_empty() {
        let components = vec![CreateActionRow::Buttons(buttons)];
        builder = builder.components(components);
    }

    message.reply.edit(ctx, builder).await?;

    Ok(())
}
