use std::{
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

use color_eyre::eyre::{bail, Error, Result};
use poise::{
    Context,
    CreateReply,
    ReplyHandle,
    serenity_prelude::{CreateEmbed, Error as SerenityError, HttpError},
};
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

/// Edit round trips slower than this count as Discord holding them back.
const SLOW_EDIT: Duration = Duration::from_millis(500);

/// Number of extra times a progress edit is retried after Discord rate limits it.
const MAX_EDIT_RETRIES: u32 = 3;

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

    /* Send a waiting message to edit with the progress. Every edit gets a fresh
    CreateReply, reusing one piles the embeds on top of each other */
    let waiting = CreateEmbed::default().title("⏳");
    let reply = ctx.send(CreateReply::default().embed(waiting)).await?;

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
    let mut throttle = Throttle::new();

    /* Edit the waiting message with the total before starting */
    throttle.edit(ctx, &reply, |status| progress.embed(None, status)).await?;

    /* Unban everyone that's banned, hasn't struck out, and isn't a newcomer */
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

        /* Edit the waiting message with the current progress as often as the
        Discord edit rate limit allows, saying how long the updates wait for it */
        if throttle.ready() {
            let last = if reasons.is_empty() {
                format!("{name} pardoned")
            } else {
                format!("{name} skipped ({})", reasons.join(", "))
            };

            throttle.edit(ctx, &reply, |status| progress.embed(Some(&last), status)).await?;
        } else {
            throttle.queue();
        }
    }

    /* Report what happened, waiting for the rate limit so the report lands */
    throttle.cooldown().await;

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

    throttle.edit(ctx, &reply, |_| embed.clone()).await?;

    Ok(())
}

/// Keeps progress edits inside Discord's message edit rate limit.
///
/// Updates that come too soon are held back so the purge keeps running at full
/// speed, and the message says how long they waited, when the next one is
/// allowed, and how long Discord held the edit requests themselves back.
#[derive(Debug)]
struct Throttle {
    /// When the last edit went out
    last:     Instant,
    /// How many updates came due but had to wait for the edit rate limit
    waiting:  usize,
    /// When the oldest of those updates came due
    due:      Option<Instant>,
    /// How long the last edit request took, when Discord held it back
    held:     Duration,
    /// Whether Discord rejected the last edit with a rate limit
    rejected: bool,
}

impl Throttle {
    fn new() -> Self {
        Self {
            last:     Instant::now(),
            waiting:  0,
            due:      None,
            held:     Duration::ZERO,
            rejected: false,
        }
    }

    /// Whether the rate limit allows another edit
    fn ready(&self) -> bool {
        self.last.elapsed() >= EDIT_DELAY
    }

    /// Mark an update as waiting on the rate limit, keeping the oldest one timed
    fn queue(&mut self) {
        self.waiting += 1;
        self.due = Some(self.due.unwrap_or_else(Instant::now));
    }

    /// Wait out the edit cooldown, for the edits that have to get through
    async fn cooldown(&self) {
        let waiting = EDIT_DELAY.saturating_sub(self.last.elapsed());

        if !waiting.is_zero() {
            tokio::time::sleep(waiting).await;
        }
    }

    /// What the message says about the rate limit, nothing when it never kicked in
    fn status(&self) -> Option<String> {
        let mut status = Vec::new();

        if let Some(due) = self.due {
            let updates = self.waiting;
            let waited = seconds(due.elapsed());
            let next = seconds(EDIT_DELAY);

            status.push(format!("{updates} updates waited {waited}, next update in {next}"));
        }

        if self.rejected {
            let retry = seconds(EDIT_DELAY);
            status.push(format!("Discord rejected the edit, retried after {retry}"));
        }

        if !self.held.is_zero() {
            let held = seconds(self.held);
            status.push(format!("Discord held the edit for {held}"));
        }

        (!status.is_empty()).then(|| status.join(", "))
    }

    /// Edit the message, rebuilding the embed for every retry so the rate limit
    /// shows up on it
    async fn edit(
        &mut self,
        ctx: Context<'_, Data, Error>,
        reply: &ReplyHandle<'_>,
        embed: impl Fn(Option<&str>) -> CreateEmbed,
    ) -> Result<()> {
        let mut retries = 0;

        loop {
            let status = self.status();
            let edited = self.send(ctx, reply, embed(status.as_deref())).await;

            match edited {
                Ok(()) => return Ok(()),
                /* Discord doesn't always say how long a rejected edit waits for */
                Err(error) if is_rate_limited(&error) && retries < MAX_EDIT_RETRIES => {
                    retries += 1;
                    self.rejected = true;
                    tokio::time::sleep(EDIT_DELAY).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Send an edit and record how long Discord took for it
    async fn send(
        &mut self,
        ctx: Context<'_, Data, Error>,
        reply: &ReplyHandle<'_>,
        embed: CreateEmbed,
    ) -> Result<(), Box<SerenityError>> {
        let started = Instant::now();
        let edited = reply.edit(ctx, CreateReply::default().embed(embed)).await;

        /* Discord waits out most rate limits on its own, which shows up as a
        slow edit instead of a rejected one */
        let elapsed = started.elapsed();
        self.last = Instant::now();
        self.waiting = 0;
        self.due = None;
        self.rejected = false;
        self.held = if elapsed >= SLOW_EDIT { elapsed } else { Duration::ZERO };

        edited.map_err(Box::new)
    }
}

/// Error code Discord gives rate limited requests (the HTTP 429 status)
const RATE_LIMITED: isize = 429;

/// Whether Discord rejected a request with a rate limit
const fn is_rate_limited(error: &SerenityError) -> bool {
    let SerenityError::Http(HttpError::UnsuccessfulRequest(response)) = error else {
        return false;
    };

    response.error.code == RATE_LIMITED
}

/// Format a duration for the progress message
fn seconds(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
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

    /// Create the progress embed, optionally showing the last user handled and
    /// what the rate limit is holding back
    fn embed(self, last: Option<&str>, status: Option<&str>) -> CreateEmbed {
        let mut embed = CreateEmbed::default()
            .title("⏳")
            .description(format!("Purging bans… {} of {}", self.done(), self.total))
            .field("Purged", self.purged.to_string(), true)
            .field("Skipped", self.skipped.to_string(), true)
            .field("Remaining", self.remaining().to_string(), true)
            .field("Last", last.unwrap_or("…"), false);

        if let Some(status) = status {
            embed = embed.field("Rate Limited", status, false);
        }

        embed
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
