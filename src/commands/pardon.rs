use std::time::Duration;

use color_eyre::{
    eyre::{Error, OptionExt},
    Result,
};
use poise::{
    serenity_prelude::{CreateInteractionResponse as CIR, *},
    Context,
    CreateReply,
    ReplyHandle,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::Data;

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
#[poise::command(slash_command, track_edits, required_permissions = "BAN_MEMBERS")]
pub async fn pardon(
    ctx: Context<'_, Data, Error>,
    #[description = "Search by User Name"] name: Option<String>,
    #[description = "Search by User UUID"] uuid: Option<String>,
) -> Result<()> {
    let message = Message::new(ctx).await?;
    let Data {
        config,
        logsdb: _,
        vrchat,
    } = ctx.data();

    /* Parse the moderator input (uuid, name, recent) */
    let uuids = if let Some(uuid) = uuid {
        vec![uuid]
    } else if let Some(name) = name {
        vrchat
            .search_users(&name)
            .await?
            .into_iter()
            .map(|user| user.id)
            .collect()
    } else {
        vrchat
            .get_group_audit_logs(&config.vrc_group_id, 100, 0)
            .await?
            .results
            .ok_or_eyre("None")?
            .into_iter()
            .filter(|log| {
                log.event_type
                    .as_ref()
                    .map_or(false, |event_type| event_type == "group.user.ban")
            })
            .filter_map(|log| log.target_id)
            .collect()
    };

    /* Paginate the unique user ids */
    paginate(ctx, message, &uuids).await
}

async fn paginate(
    ctx: Context<'_, Data, Error>,
    message: Message<'_>,
    uuids: &[String],
) -> Result<()> {
    let mut index = 0;
    let Data {
        config,
        logsdb: _,
        vrchat,
    } = ctx.data();

    'pardon: loop {
        let uuid: &String = &uuids[index];
        edit_message(ctx, &message, uuids, index).await?;

        /* Capture users button input in a loop until valid input is received */
        'page: while let Some(mci) = ComponentInteractionCollector::new(ctx)
            .author_id(ctx.author().id)
            .channel_id(ctx.channel_id())
            .timeout(Duration::MAX)
            .await
        {
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
                    vrchat.pardon_member(&config.vrc_group_id, uuid).await?;

                    break 'pardon Ok(());
                }
                _ => {}
            }
        }
    }
}

async fn edit_message(
    ctx: Context<'_, Data, Error>,
    message: &Message<'_>,
    uuids: &[String],
    index: usize,
) -> Result<()> {
    let Data {
        config,
        logsdb,
        vrchat,
    } = ctx.data();

    /* Get the user and member */
    let user_id = &uuids[index];
    let user = vrchat.get_user(user_id).await?;

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
    if index < uuids.len() {
        let button = CreateButton::new("next")
            .emoji('➡')
            .label("Next")
            .style(ButtonStyle::Secondary);

        buttons.push(button);
    }

    // TODO: Replace with database query for last ban/unban
    if let Ok(member) = vrchat.get_group_member(&config.vrc_group_id, user_id).await {
        if let Some(banned_at) = member.banned_at.flatten() {
            /* Search the logs database for the users most recent ban */
            let logs = logsdb
                .find_recent_logs("group.user.ban", user_id, 100)
                .await?;
            if let Some(log) = logs.first() {
                let actor = vrchat.get_user(&log.actor_id).await?;
                let text = format!("Banned by {}", actor.display_name);
                let footer = CreateEmbedFooter::new(text).icon_url(actor.user_icon);
                embed = embed.footer(footer);
            }

            /* Parse, Convert, and Add the ban timestamp */
            let date_time = OffsetDateTime::parse(&banned_at, &Rfc3339)?;
            let timestamp = Timestamp::from_unix_timestamp(date_time.unix_timestamp())?;
            embed = embed.timestamp(timestamp);

            /* Create & Add the pardon button */
            let button = CreateButton::new("pardon")
                .emoji('⚖')
                .label("Pardon")
                .style(ButtonStyle::Success);

            buttons.push(button);
        }
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
