use std::time::{Duration, Instant};

use color_eyre::{eyre::Error, Result};
use poise::{serenity_prelude::*, Context, CreateReply};
use vrchatapi::models::GroupMember;

use crate::{vrchat::VRChat, Data};

/// Users banned this many times are left banned (three strike rule).
const MAX_STRIKES: i64 = 3;

/// Trust tags for `User` and above, anyone below that is left banned.
/// The `VRChat` trust tags are offset by one, `system_trust_known` is `User`.
const USER_TAGS: [&str; 3] = [
    "system_trust_known",   // User
    "system_trust_trusted", // Known User
    "system_trust_veteran", // Trusted User
];

/// Trust tag of a `New User`, visitors have no trust tag at all.
const NEW_USER_TAG: &str = "system_trust_basic";

/// Tag of a nuisance, they're left banned.
const NUISANCE_TAG: &str = "system_troll";

/// Minimum delay between progress edits to stay under the Discord rate limit.
const EDIT_DELAY: Duration = Duration::from_millis(1500);

/// Maximum number of skipped users to list in the final embed.
const MAX_LISTED: usize = 10;

/// Purge all the bans from the group.
///
/// Skips users that have been banned `MAX_STRIKES` or more times, and users
/// that `VRChat` ranks below `User` (visitors, new users, and nuisances).
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn purge(ctx: Context<'_, Data, Error>) -> Result<()> {
    let builder = CreateReply::default().embed(CreateEmbed::default().title("⏳"));
    let reply = ctx.send(builder.clone()).await?;

    let Data {
        config,
        logsdb,
        vrchat,
    } = ctx.data();

    /* Get the bans and how many times each user has been banned */
    let bans = vrchat.get_group_bans(&config.vrc_group_id).await?;
    let strikes = logsdb.get_ban_counts().await?;

    let mut progress = Progress::new(bans.len());
    let mut skipped_users = Vec::new();

    /* Edit the waiting message with the total before starting */
    reply
        .edit(ctx, builder.clone().embed(progress.embed(None)))
        .await?;

    /* Unban everyone that's banned, hasn't struck out, and isn't a newcomer */
    let mut last_edit = Instant::now();

    for ban in &bans {
        let name = display_name(ban);
        let strikes = strikes.get(&ban.user_id).copied().unwrap_or_default();
        let tags = get_tags(vrchat, &name, &ban.user_id).await;

        /* Collect the reasons a user is left banned */
        let mut reasons = Vec::new();
        if tags.iter().any(|tag| tag == NUISANCE_TAG) {
            reasons.push(String::from("Nuisance"));
        } else if !tags.iter().any(|tag| USER_TAGS.contains(&tag.as_str())) {
            let rank = if tags.iter().any(|tag| tag == NEW_USER_TAG) {
                "New User"
            } else {
                "Visitor"
            };

            reasons.push(String::from(rank));
        }

        if strikes >= MAX_STRIKES {
            reasons.push(format!("{strikes} bans"));
        }

        if reasons.is_empty() {
            vrchat
                .pardon_member(&config.vrc_group_id, &ban.user_id)
                .await?;
            progress.purged += 1;
        } else {
            skipped_users.push(format!("{name} ({})", reasons.join(", ")));
            progress.skipped += 1;
        }

        /* Edit the waiting message with the current progress */
        if last_edit.elapsed() >= EDIT_DELAY {
            let last = if reasons.is_empty() {
                format!("{name} pardoned")
            } else {
                format!("{name} skipped ({})", reasons.join(", "))
            };

            reply
                .edit(ctx, builder.clone().embed(progress.embed(Some(&last))))
                .await?;
            last_edit = Instant::now();
        }
    }

    /* Report what happened */
    let mut embed = CreateEmbed::default()
        .title("✅")
        .description(format!(
            "Purged {} of {} bans",
            progress.purged, progress.total
        ))
        .field("Purged", progress.purged.to_string(), true)
        .field("Skipped", progress.skipped.to_string(), true)
        .field("Total", progress.total.to_string(), true);

    if !skipped_users.is_empty() {
        let extra = skipped_users.len().saturating_sub(MAX_LISTED);
        skipped_users.truncate(MAX_LISTED);

        if extra > 0 {
            skipped_users.push(format!("…and {extra} more"));
        }

        embed = embed.field(
            format!("Skipped ({MAX_STRIKES}+ bans or below User)"),
            skipped_users.join("\n"),
            false,
        );
    }

    reply.edit(ctx, builder.embed(embed)).await?;

    Ok(())
}

/// How far along a purge is
#[derive(Clone, Copy, Debug)]
struct Progress {
    total:   usize,
    purged:  usize,
    skipped: usize,
}

impl Progress {
    const fn new(total: usize) -> Self {
        Self {
            total,
            purged: 0,
            skipped: 0,
        }
    }

    /// How many bans have been handled so far
    const fn done(&self) -> usize {
        self.purged + self.skipped
    }

    /// How many bans are left to handle
    const fn remaining(&self) -> usize {
        self.total - self.done()
    }

    /// Create the progress embed, optionally showing the last user handled
    fn embed(self, last: Option<&str>) -> CreateEmbed {
        CreateEmbed::default()
            .title("⏳")
            .description(format!("Purging bans… {} of {}", self.done(), self.total))
            .field("Purged", self.purged.to_string(), true)
            .field("Skipped", self.skipped.to_string(), true)
            .field("Remaining", self.remaining().to_string(), true)
            .field("Last", last.unwrap_or("…"), false)
    }
}

/// Get a banned member's display name, falling back to their user ID
fn display_name(member: &GroupMember) -> String {
    member
        .user
        .as_ref()
        .and_then(|inner| inner.as_ref())
        .and_then(|user| user.display_name.clone())
        .unwrap_or_else(|| member.user_id.clone())
}

/// Get a user's profile tags, empty if the profile couldn't be fetched
async fn get_tags(vrchat: &VRChat, name: &str, user_id: &str) -> Vec<String> {
    match vrchat.get_user(user_id).await {
        Ok(user) => user.tags,
        Err(error) => {
            eprintln!("Failed to fetch the profile of {name}: {error}");
            Vec::new()
        }
    }
}
