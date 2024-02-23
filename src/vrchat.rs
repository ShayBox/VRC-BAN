use color_eyre::Result;
use derive_config::DeriveTomlConfig;
use totp::{Algorithm, Secret, TOTP};
use vrc::{
    api_client::{AuthenticatedVRC, UnauthenticatedVRC},
    model::AdditionalAuthFactor,
    query::{Authentication, VerifySecondFactor},
};

use crate::Config;

/// # Login to `VRChat` (`vrc_rs`)
///
/// # Errors
pub async fn login(config: &mut Config) -> Result<AuthenticatedVRC> {
    let user_agent = config.user_agent.clone();

    // Attempt to login using saved session if available
    if let Some(authentication) = config.authentication.clone() {
        if let Ok(vrchat) = AuthenticatedVRC::new(user_agent.clone(), authentication) {
            return Ok(vrchat);
        }
    }

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

    let mut vrchat = vrc.upgrade(authentication.clone())?;

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
        let (_, second_factor_token) = vrchat.verify_second_factor(second_factor).await?;

        // Save the session for re-use later
        authentication.second_factor_token = Some(second_factor_token);
        config.authentication = Some(authentication.clone());
        config.save()?;

        // Apply the new second factor token
        vrchat = vrchat.recreate(authentication.clone())?;
    }

    Ok(vrchat)
}
