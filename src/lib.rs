use config::{eyre::Result, ConfigFile};
use serde::{Deserialize, Serialize};
use totp::{Algorithm, Secret, TOTP};
use vrc::{
    api_client::{AuthenticatedVRC, UnauthenticatedVRC},
    id::Group,
    model::AdditionalAuthFactor,
    query::{Authenticating, Authentication, VerifySecondFactor},
};

pub const DEFAULT_USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " ",
    env!("CARGO_PKG_AUTHORS"),
);

#[derive(Clone, ConfigFile, Debug, Deserialize, Serialize)]
pub struct Config {
    pub authenticating: Authenticating,
    pub authentication: Option<Authentication>,
    pub group_id_audit: Group,
    pub totp_2f_secret: String,
    pub vrc_user_agent: Option<String>,
}

/// # Login to `VRChat` (`vrc_rs`)
///
/// # Errors
pub async fn login(config: &mut Config, user_agent: String) -> Result<AuthenticatedVRC> {
    // Fall back to obtaining a new session with USER/PASS/TOTP
    let vrc = UnauthenticatedVRC::new(user_agent, config.authenticating.clone())?;
    let (login_response, token) = vrc.login().await?;
    let mut authentication = Authentication {
        token,
        second_factor_token: None,
    };

    // Save the session for re-use later
    config.authentication = Some(authentication.clone());
    config.save()?;

    let vrchat = vrc.upgrade(authentication.clone())?;

    if login_response
        .requires_additional_auth
        .contains(&AdditionalAuthFactor::Totp)
    {
        // Obtain the current TOTP code using saved TOTP
        let secret = Secret::Encoded(config.totp_2f_secret.clone()).to_bytes()?;
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret)?;
        let code = totp.generate_current()?;

        // Verify the TOTP with VRChat
        let second_factor = VerifySecondFactor::Code(code);
        let (status, second_factor_token) = vrchat.verify_second_factor(second_factor).await?;
        if status.verified {
            // Save the session for re-use later
            authentication.second_factor_token = Some(second_factor_token);
            config.authentication = Some(authentication.clone());
            config.save()?;
        }
    }

    Ok(vrchat)
}
