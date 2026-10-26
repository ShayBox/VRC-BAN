use std::{
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

use color_eyre::eyre::{bail, Error, Result};
use poise::{serenity_prelude::*, Context, CreateReply};
use vrchatapi::models::GroupMember;

use crate::{vrchat::VRChat, Data};

/// Users banned this many times are left banned unless the `bans` option says
/// otherwise (three strike rule).
const DEFAULT_BANS: i64 = 3;

/// Users ranked below this are left banned unless the `level` option says
/// otherwise.
const DEFAULT_LEVEL: Rank = Rank::User;

/// Tag of a nuisance, they're left banned.
const NUISANCE_TAG: &str = "system_troll";

/// `VRChat` trust ranks, ordered from lowest to highest.
///
/// The `VRChat` trust tags are offset by one, `system_trust_known` is `User`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, poise::ChoiceParameter)]
pub enum Rank {
    /// Visitors have no trust tag at all
    Visitor,
    /// A `New User`, tagged with `system_trust_basic`
    #[name = "New User"]
    #[name = "NewUser"]
    NewUser,
    /// A `User`, tagged with `system_trust_known`
    User,
    /// A `Known User`, tagged with `system_trust_trusted`
    #[name = "Known User"]
    #[name = "KnownUser"]
    KnownUser,
    /// A `Trusted User`, tagged with `system_trust_veteran`
    #[name = "Trusted User"]
    #[name = "TrustedUser"]
    TrustedUser,
}

impl Rank {
    /// Every rank, ordered from lowest to highest
    const ALL: [Self; 5] = [
        Self::Visitor,
        Self::NewUser,
        Self::User,
        Self::KnownUser,
        Self::TrustedUser,
    ];

    /// The trust tag that marks this rank, visitors have no tag of their own
    const fn tag(self) -> Option<&'static str> {
        match self {
            Self::Visitor => None,
            Self::NewUser => Some("system_trust_basic"),
            Self::User => Some("system_trust_known"),
            Self::KnownUser => Some("system_trust_trusted"),
            Self::TrustedUser => Some("system_trust_veteran"),
        }
    }

    /// The highest rank that one of the given profile tags belongs to
    fn from_tags(tags: &[String]) -> Self {
        Self::ALL
            .iter()
            .rev()
            .copied()
            .find(|rank| {
                rank.tag()
                    .is_some_and(|tag| tags.iter().any(|other| other == tag))
            })
            .unwrap_or(Self::Visitor)
    }

    /// The rank name as it appears in `VRChat`
    const fn label(self) -> &'static str {
        match self {
            Self::Visitor => "Visitor",
            Self::NewUser => "New User",
            Self::User => "User",
            Self::KnownUser => "Known User",
            Self::TrustedUser => "Trusted User",
        }
    }
}

impl Display for Rank {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Minimum delay between progress edits to stay under the Discord rate limit.
const EDIT_DELAY: Duration = Duration::from_millis(1500);

/// Maximum number of skipped users to list in the final embed.
const MAX_LISTED: usize = 10;

/// Purge all the bans from the group.
///
/// Skips users that `VRChat` ranks below `level`, users that have been banned
/// `bans` times or more, and nuisances. Both options fall back to the usual
/// settings (a `User` rank and 3 bans) when they're left out.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn purge(
    ctx: Context<'_, Data, Error>,
    #[description = "Leave users below this trust rank banned (default: User)"] level: Option<Rank>,
    #[description = "Leave users banned this many times or more (default: 3)"] bans: Option<i64>,
) -> Result<()> {
    /* Fall back to the usual settings when the options are omitted */
    let level = level.unwrap_or(DEFAULT_LEVEL);
    let max_bans = match bans {
        Some(bans) if bans < 1 => bail!("The ban count must be at least 1"),
        Some(bans) => bans,
        None => DEFAULT_BANS,
    };

    let builder = CreateReply::default().embed(CreateEmbed::default().title("⏳"));
    let reply = ctx.send(builder.clone()).await?;

    let Data {
        config,
        logsdb,
        vrchat,
    } = ctx.data();

    /* Get the bans and how many times each user has been banned */
    let ban_list = vrchat.get_group_bans(&config.vrc_group_id).await?;
    let strikes = logsdb.get_ban_counts().await?;

    let mut progress = Progress::new(ban_list.len());
    let mut skipped_users = Vec::new();

    /* Edit the waiting message with the total before starting */
    reply
        .edit(ctx, builder.clone().embed(progress.embed(None)))
        .await?;

    /* Unban everyone that's banned, hasn't struck out, and isn't a newcomer */
    let mut last_edit = Instant::now();

    for ban in &ban_list {
        let name = display_name(ban);
        let strikes = strikes.get(&ban.user_id).copied().unwrap_or_default();
        let tags = get_tags(vrchat, &name, &ban.user_id).await;
        let rank = Rank::from_tags(&tags);

        /* Collect the reasons a user is left banned */
        let mut reasons = Vec::new();
        if tags.iter().any(|tag| tag == NUISANCE_TAG) {
            reasons.push(String::from("Nuisance"));
        } else if rank < level {
            reasons.push(rank.to_string());
        }

        if strikes >= max_bans {
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
            "Purged {} of {} bans, leaving anyone below {level} or with {max_bans}+ bans banned",
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

        embed = embed.field("Skipped", skipped_users.join("\n"), false);
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
