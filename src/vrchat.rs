use std::sync::Arc;

use color_eyre::{eyre::eyre, Result};
use reqwest::{Client, Url};
use reqwest_cookie_store::CookieStoreRwLock;
use totp::{Algorithm, Secret, TOTP};
use vrchatapi::{
    apis::{
        authentication_api,
        configuration::Configuration,
        groups_api::{self, GetGroupAuditLogsError, GetGroupMemberError, UnbanGroupMemberError},
        users_api::{self, GetUserError},
        Error,
    },
    models::{
        EitherUserOrTwoFactor,
        GroupLimitedMember,
        GroupMember,
        LimitedUser,
        PaginatedGroupAuditLogEntryList,
        TwoFactorAuthCode,
        User,
    },
};

use crate::logsdb::Log;

pub const MAX: i32 = 100;

/// `VRChat` API config & cookie wrapper
pub struct VRChat {
    config: Configuration,
    cookie: Arc<CookieStoreRwLock>,
}

impl VRChat {
    /// # Create a new `VRChat` instance
    ///
    /// # Errors
    /// Will return `Err` if `ClientBuilder::build` fails.
    ///
    /// # Panics
    /// Will panic if `RwLock::write` fails.
    pub fn new(
        cookies: &Vec<String>,
        username: &str,
        password: &str,
        user_agent: &str,
    ) -> Result<Self> {
        /* Create the cookie store and client */
        let cookie = Arc::new(CookieStoreRwLock::default());
        let client = Client::builder()
            .cookie_store(true)
            .cookie_provider(cookie.clone())
            .build()?;

        /* Add the cookies to the cookie store */
        let request_url = Url::parse("https://vrchat.com/api/1")?;
        for cookie_str in cookies {
            cookie
                .write()
                .expect("Failed to lock")
                .parse(cookie_str, &request_url)?;
        }

        Ok(Self {
            cookie,
            config: Configuration {
                client,
                basic_auth: Some((username.to_owned(), Some(password.to_owned()))),
                user_agent: Some(user_agent.to_owned()),
                ..Default::default()
            },
        })
    }

    /// # Login and Verify 2FA
    ///
    /// # Errors
    /// Will return `Err` if `get_current_user` or `verify2_fa` fails.
    pub async fn login_and_verify(&self, secret: &str) -> Result<()> {
        if let EitherUserOrTwoFactor::RequiresTwoFactorAuth(_) =
            authentication_api::get_current_user(&self.config).await?
        {
            // Obtain the current TOTP code using saved TOTP
            let secret = Secret::Encoded(secret.to_owned()).to_bytes()?;
            let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret)?;
            let code = totp.generate_current()?;

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
    /// Will panic if `RwLock::read` fails.
    #[must_use]
    pub fn get_cookies(&self) -> Vec<String> {
        self.cookie
            .read()
            .unwrap()
            .iter_any()
            .map(|cookie| cookie.to_string())
            .collect()
    }

    /* Groups API */

    /// # Get the groups audit logs
    ///
    /// # Errors
    /// Will return `Err` if `get_group_audit_logs` fails.
    pub async fn get_all_group_audit_logs(&self, group_id: &str) -> Result<Vec<Log>> {
        let mut logs = Vec::new();
        let mut offset = 0;

        loop {
            let new = self.get_group_audit_logs(group_id, MAX, offset).await?;
            let Some(results) = new.results else {
                break; // There are no new results
            };

            if results.is_empty() {
                break; // There are no new results
            }

            let len = logs.len();
            for result in results {
                logs.push(Log::try_from(result)?);
            }

            if logs.len() == len {
                break; // There were no new results
            }

            let Some(has_next) = new.has_next else {
                break; // There are no more results
            };

            if !has_next {
                break; // There are no more results
            }

            offset += MAX;
        }

        Ok(logs)
    }

    /// # Get the groups audit logs
    ///
    /// # Errors
    /// Will return `Err` if `get_group_audit_logs` fails.
    pub async fn get_group_audit_logs(
        &self,
        group_id: &str,
        number: i32,
        offset: i32,
    ) -> Result<PaginatedGroupAuditLogEntryList, Error<GetGroupAuditLogsError>> {
        groups_api::get_group_audit_logs(
            &self.config,
            group_id,
            Some(number),
            Some(offset),
            None,
            None,
        )
        .await
    }

    /// # Get a group member
    ///
    /// # Errors
    /// Will return `Err` if `get_group_member` fails.
    pub async fn get_group_member(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<GroupLimitedMember, Error<GetGroupMemberError>> {
        groups_api::get_group_member(&self.config, group_id, user_id).await
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
        groups_api::unban_group_member(&self.config, group_id, user_id).await
    }

    /* Users API */

    /// # Get a user
    ///
    /// # Errors
    /// Will return `Err` if `get_user` fails.
    pub async fn get_user(&self, user_id: &str) -> Result<User, Error<GetUserError>> {
        users_api::get_user(&self.config, user_id).await
    }

    /// # Search for a user
    ///
    /// # Errors
    /// Will return `Err` if `search_users` fails.
    pub async fn search_users(&self, search: &str) -> Result<Vec<LimitedUser>> {
        let mut users = Vec::new();
        let mut offset = 0;

        loop {
            #[rustfmt::skip]
            let new = users_api::search_users(
                &self.config,
                Some(search),
                None,
                Some(MAX),
                Some(offset)
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
