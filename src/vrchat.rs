use std::{future::Future, sync::Arc, time::Duration};

use color_eyre::{eyre::eyre, Result};
use reqwest::{
    cookie::{CookieStore, Jar},
    Client,
    StatusCode,
    Url,
};
use totp::{Algorithm, Builder, Secret};
use vrchatapi::{
    apis::{
        authentication_api,
        configuration::Configuration,
        groups_api::{
            self,
            BanGroupMemberError,
            GetGroupAuditLogsError,
            GetGroupBansError,
            GetGroupMemberError,
            UnbanGroupMemberError,
        },
        users_api,
        Error,
    },
    models::{
        BanGroupMemberRequest,
        CurrentUserLoginResponse,
        GroupMember,
        LimitedUserSearch,
        TwoFactorAuthCode,
        User,
        UserResponse,
    },
};

use crate::logsdb::Log;

pub const MAX: i32 = 100;

/// Initial delay before retrying a rate limited request
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(5);

/// Maximum delay between retries of a rate limited request
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

/// Maximum number of times to retry a rate limited request
const MAX_RETRIES: u32 = 8;

/// Base URL of the `VRChat` API (also used to scope cookies)
const API_BASE_URL: &str = "https://api.vrchat.cloud/api/1";

/// `VRChat` API config & cookie wrapper
pub struct VRChat {
    config: Configuration,
    cookie: Arc<Jar>,
}

impl VRChat {
    /// # Create a new `VRChat` instance
    ///
    /// # Errors
    /// Will return `Err` if `ClientBuilder::build` or `Url::parse` fails.
    pub fn new(
        cookies: &Vec<String>,
        username: &str,
        password: &str,
        user_agent: &str,
    ) -> Result<Self> {
        /* Create the cookie store and client */
        let cookie = Arc::new(Jar::default());
        let client = Client::builder().cookie_provider(cookie.clone()).build()?;

        /* Add the cookies to the cookie store */
        let request_url = Url::parse(API_BASE_URL)?;
        for cookie_str in cookies {
            cookie.add_cookie_str(cookie_str, &request_url);
        }

        /* Build the API configuration around our client */
        let mut config = Configuration::new();
        config.client = client.into();
        config.basic_auth = Some((username.to_owned(), Some(password.to_owned())));
        config.user_agent = Some(user_agent.to_owned());
        config.debug = true;

        Ok(Self { config, cookie })
    }

    /// # Login and Verify 2FA
    ///
    /// # Errors
    /// Will return `Err` if `get_current_user` or `verify2_fa` fails.
    pub async fn login_and_verify(&self, secret: &str) -> Result<()> {
        let current_user = authentication_api::get_current_user(&self.config).await?;

        if let CurrentUserLoginResponse::RequiresTwoFactorAuth(_) = current_user {
            // Obtain the current TOTP code using saved TOTP
            let secret = Secret::try_from_base32(secret)?;
            let totp = Builder::new()
                .with_algorithm(Algorithm::SHA1)
                .with_digits(6)
                .with_skew(1)
                .with_step_duration(30)
                .with_secret(secret)
                .build()?;
            let code = totp.generate_current().to_string();

            // Verify the TOTP with VRChat
            let two_factor_auth_code = TwoFactorAuthCode::new(code);
            let login = authentication_api::verify2_fa(&self.config, two_factor_auth_code).await?;
            if !login.verified {
                return Err(eyre!("2FA: Failed to verify"));
            }
        }

        Ok(())
    }

    /// # Get the cookies in the cookie store
    ///
    /// # Panics
    /// Will panic if `API_BASE_URL` is not a valid URL.
    #[must_use]
    pub fn get_cookies(&self) -> Vec<String> {
        let url = Url::parse(API_BASE_URL).expect("valid URL");
        let Some(value) = self.cookie.cookies(&url) else {
            return Vec::new();
        };
        let Ok(combined) = value.to_str() else {
            return Vec::new();
        };

        combined
            .split("; ")
            .map(str::trim)
            .filter(|cookie| !cookie.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /* Groups API */

    /// # Get the groups audit logs
    ///
    /// # Errors
    /// Will return `Err` if `get_group_audit_logs` fails.
    pub async fn get_group_audit_logs(
        &self,
        group_id: &str,
        number: i32,
        offset: i32,
    ) -> Result<Vec<Log>, Error<GetGroupAuditLogsError>> {
        let audit_logs = groups_api::get_group_audit_logs(
            &self.config,
            group_id,
            Some(number),
            Some(offset),
            None,
            None,
            None,
            None,
            None,
        )
        .await?;

        let logs = audit_logs
            .results
            .into_iter()
            .flatten()
            .map(Log::try_from)
            .filter_map(Result::ok)
            .collect();

        Ok(logs)
    }

    /// # Get a group member
    ///
    /// # Errors
    /// Will return `Err` if `get_group_member` fails.
    pub async fn get_group_member(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<GroupMember, Error<GetGroupMemberError>> {
        groups_api::get_group_member(&self.config, group_id, user_id).await
    }

    /// # Get all the banned group members
    ///
    /// # Errors
    /// Will return `Err` if `get_group_bans` fails.
    pub async fn get_group_bans(
        &self,
        group_id: &str,
    ) -> Result<Vec<GroupMember>, Error<GetGroupBansError>> {
        let mut bans = Vec::new();
        let mut offset = 0;

        loop {
            #[rustfmt::skip]
            let new = retry_rate_limited(|| {
                groups_api::get_group_bans(
                    &self.config,
                    group_id,
                    Some(MAX),
                    Some(offset)
                )
            }).await?;

            if new.is_empty() {
                break; // There are no bans
            }

            let len = bans.len();
            bans.extend(new);

            if bans.len() == len {
                break; // There were no new bans
            }

            offset += MAX;
        }

        Ok(bans)
    }

    /// # Ban a group member
    ///
    /// # Errors
    /// Will return `Err` if `unban_group_member` fails.
    pub async fn ban_member(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<GroupMember, Error<BanGroupMemberError>> {
        retry_rate_limited(|| {
            let request = BanGroupMemberRequest::new(user_id.to_owned());
            groups_api::ban_group_member(&self.config, group_id, request)
        })
        .await
    }

    /// # Pardon a group member
    ///
    /// # Errors
    /// Will return `Err` if `unban_group_member` fails.
    pub async fn pardon_member(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<GroupMember, Error<UnbanGroupMemberError>> {
        retry_rate_limited(|| groups_api::unban_group_member(&self.config, group_id, user_id)).await
    }

    /* Users API */

    /// # Get a user
    ///
    /// # Errors
    /// Will return `Err` if `get_user` fails or returns an unexpected response.
    pub async fn get_user(&self, user_id: &str) -> Result<User> {
        let response = retry_rate_limited(|| users_api::get_user(&self.config, user_id)).await?;

        match response {
            UserResponse::User(user) => Ok(user),
            UserResponse::CurrentUser(_) => Err(eyre!("Unexpected current-user response")),
        }
    }

    /// # Search for a user
    ///
    /// # Errors
    /// Will return `Err` if `search_users` fails.
    pub async fn search_users(&self, search: &str) -> Result<Vec<LimitedUserSearch>> {
        let mut users = Vec::new();
        let mut offset = 0;

        loop {
            #[rustfmt::skip]
            let new = users_api::search_users(
                &self.config,
                Some(search),
                None,
                None,
                None,
                Some(MAX),
                Some(offset),
                None
            ).await?;

            if new.is_empty() {
                break; // There are no users
            }

            let len = users.len();
            users.extend(new);

            if users.len() == len {
                break; // There were no new users
            }

            offset += MAX;
        }

        Ok(users)
    }
}

/// # Retry a rate limited API call
///
/// Retries the operation while it is rate limited (HTTP 429), waiting with
/// exponential backoff starting at `INITIAL_RETRY_DELAY`, doubling each time,
/// and capped at `MAX_RETRY_DELAY` for up to `MAX_RETRIES` retries.
async fn retry_rate_limited<T, E, F, Fut>(mut operation: F) -> Result<T, Error<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error<E>>>,
{
    let mut delay = INITIAL_RETRY_DELAY;
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(Error::ResponseError(content))
                if content.status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES =>
            {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(MAX_RETRY_DELAY);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}
