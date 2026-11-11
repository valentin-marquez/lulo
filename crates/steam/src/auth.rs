//! Steam user login: the `Authentication` service, called over the CM socket before logon.
//!
//! Flow: begin a session (QR code or account + password), satisfy Steam Guard if asked, poll until
//! Steam hands out a refresh token, then [`crate::Client::user`] logs on with it. The refresh token
//! is what to store (OS keyring); it lasts months and skips the password next time.

use base64::Engine;
use rsa::{BigUint, Pkcs1v15Encrypt, RsaPublicKey};

use crate::cm::{DEVICE_NAME, OS};
use crate::proto::{
    CAuthenticationAllowedConfirmation, CAuthenticationBeginAuthSessionViaCredentialsRequest,
    CAuthenticationBeginAuthSessionViaCredentialsResponse,
    CAuthenticationBeginAuthSessionViaQrRequest, CAuthenticationBeginAuthSessionViaQrResponse,
    CAuthenticationDeviceDetails, CAuthenticationGetPasswordRsaPublicKeyRequest,
    CAuthenticationGetPasswordRsaPublicKeyResponse, CAuthenticationPollAuthSessionStatusRequest,
    CAuthenticationPollAuthSessionStatusResponse,
    CAuthenticationUpdateAuthSessionWithSteamGuardCodeRequest,
    CAuthenticationUpdateAuthSessionWithSteamGuardCodeResponse,
};
use crate::{Client, Error, Result};

/// EAuthTokenPlatformType::SteamClient; ESessionPersistence::Persistent.
const PLATFORM: i32 = 1;
const PERSISTENT: i32 = 1;

/// How Steam wants the login confirmed, in its order of preference (`EAuthSessionGuardType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Nothing to do; keep polling.
    None,
    /// A code sent by email: [`code`].
    EmailCode,
    /// A code from the Steam mobile authenticator: [`code`].
    DeviceCode,
    /// Approve the login in the Steam mobile app; keep polling.
    DeviceConfirmation,
    /// Click the link in the email; keep polling.
    EmailConfirmation,
    Other(i32),
}

impl Confirmation {
    fn from(raw: &CAuthenticationAllowedConfirmation) -> Self {
        match raw.confirmation_type.unwrap_or(0) {
            1 => Confirmation::None,
            2 => Confirmation::EmailCode,
            3 => Confirmation::DeviceCode,
            4 => Confirmation::DeviceConfirmation,
            5 => Confirmation::EmailConfirmation,
            other => Confirmation::Other(other),
        }
    }

    fn code_type(self) -> Option<i32> {
        match self {
            Confirmation::EmailCode => Some(2),
            Confirmation::DeviceCode => Some(3),
            _ => None,
        }
    }
}

/// A login in progress.
#[derive(Debug, Clone)]
pub struct Session {
    client_id: u64,
    request_id: Vec<u8>,
    pub steamid: u64,
    /// Seconds Steam asks to wait between polls.
    pub interval: f32,
    pub confirmations: Vec<Confirmation>,
    /// QR logins: the URL to show as a QR code. It can rotate while polling.
    pub challenge_url: Option<String>,
}

/// What a login hands back once it succeeds.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub account_name: String,
    pub steamid: u64,
    pub refresh_token: String,
    pub access_token: String,
    /// Remembers this machine, so the next password login skips the email code.
    pub guard_data: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Poll {
    Pending,
    /// The QR code changed; draw the new URL.
    Rotated(String),
    Done(Tokens),
}

fn device() -> CAuthenticationDeviceDetails {
    CAuthenticationDeviceDetails {
        device_friendly_name: Some(DEVICE_NAME.into()),
        platform_type: Some(PLATFORM),
        os_type: Some(OS),
        ..Default::default()
    }
}

/// Starts a QR login: show [`Session::challenge_url`] as a QR code for the Steam mobile app.
pub async fn qr(client: &mut Client) -> Result<Session> {
    let request = CAuthenticationBeginAuthSessionViaQrRequest {
        device_friendly_name: Some(DEVICE_NAME.into()),
        platform_type: Some(PLATFORM),
        device_details: Some(device()),
        ..Default::default()
    };
    let reply: CAuthenticationBeginAuthSessionViaQrResponse = client
        .call_unauthed("Authentication.BeginAuthSessionViaQR", &request)
        .await?;
    Ok(Session {
        client_id: reply.client_id(),
        request_id: reply.request_id().to_vec(),
        steamid: 0,
        interval: reply.interval.unwrap_or(5.0),
        confirmations: reply
            .allowed_confirmations
            .iter()
            .map(Confirmation::from)
            .collect(),
        challenge_url: reply.challenge_url,
    })
}

/// Starts a password login. `guard_data` is [`Tokens::guard_data`] from an earlier login, if any.
pub async fn password(
    client: &mut Client,
    account: &str,
    password: &str,
    guard_data: Option<&str>,
) -> Result<Session> {
    let key: CAuthenticationGetPasswordRsaPublicKeyResponse = client
        .call_unauthed(
            "Authentication.GetPasswordRSAPublicKey",
            &CAuthenticationGetPasswordRsaPublicKeyRequest {
                account_name: Some(account.to_owned()),
            },
        )
        .await?;
    let encrypted = encrypt(key.publickey_mod(), key.publickey_exp(), password)?;

    let request = CAuthenticationBeginAuthSessionViaCredentialsRequest {
        device_friendly_name: Some(DEVICE_NAME.into()),
        account_name: Some(account.to_owned()),
        encrypted_password: Some(encrypted),
        encryption_timestamp: key.timestamp,
        remember_login: Some(true),
        platform_type: Some(PLATFORM),
        persistence: Some(PERSISTENT),
        website_id: Some("Client".into()),
        device_details: Some(device()),
        guard_data: guard_data.map(str::to_owned),
        ..Default::default()
    };
    let reply: CAuthenticationBeginAuthSessionViaCredentialsResponse = client
        .call_unauthed("Authentication.BeginAuthSessionViaCredentials", &request)
        .await?;
    Ok(Session {
        client_id: reply.client_id(),
        request_id: reply.request_id().to_vec(),
        steamid: reply.steamid(),
        interval: reply.interval.unwrap_or(5.0),
        confirmations: reply
            .allowed_confirmations
            .iter()
            .map(Confirmation::from)
            .collect(),
        challenge_url: None,
    })
}

/// RSA PKCS#1 v1.5 of the UTF-8 password with the account's key (hex modulus and exponent),
/// base64-encoded, as Steam expects it.
fn encrypt(modulus_hex: &str, exponent_hex: &str, password: &str) -> Result<String> {
    let parse =
        |hex: &str| BigUint::parse_bytes(hex.as_bytes(), 16).ok_or(Error::Packet("bad RSA key"));
    let key = RsaPublicKey::new(parse(modulus_hex)?, parse(exponent_hex)?)
        .map_err(|_| Error::Packet("bad RSA key"))?;
    let data = key
        .encrypt(
            &mut rsa::rand_core::OsRng,
            Pkcs1v15Encrypt,
            password.as_bytes(),
        )
        .map_err(|_| Error::Packet("password too long for the RSA key"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

/// Sends the Steam Guard code the user typed (email or mobile authenticator).
pub async fn code(
    client: &mut Client,
    session: &Session,
    kind: Confirmation,
    code: &str,
) -> Result<()> {
    let request = CAuthenticationUpdateAuthSessionWithSteamGuardCodeRequest {
        client_id: Some(session.client_id),
        steamid: Some(session.steamid),
        code: Some(code.trim().to_owned()),
        code_type: Some(
            kind.code_type()
                .ok_or(Error::Packet("this confirmation takes no code"))?,
        ),
    };
    let _: CAuthenticationUpdateAuthSessionWithSteamGuardCodeResponse = client
        .call_unauthed(
            "Authentication.UpdateAuthSessionWithSteamGuardCode",
            &request,
        )
        .await?;
    Ok(())
}

/// Checks the session once; call every [`Session::interval`] seconds until `Done`.
pub async fn poll(client: &mut Client, session: &mut Session) -> Result<Poll> {
    let reply: CAuthenticationPollAuthSessionStatusResponse = client
        .call_unauthed(
            "Authentication.PollAuthSessionStatus",
            &CAuthenticationPollAuthSessionStatusRequest {
                client_id: Some(session.client_id),
                request_id: Some(session.request_id.clone()),
                ..Default::default()
            },
        )
        .await?;
    if let Some(id) = reply.new_client_id {
        session.client_id = id;
    }
    if let Some(token) = reply.refresh_token.filter(|t| !t.is_empty()) {
        let steamid = subject(&token).unwrap_or(session.steamid);
        return Ok(Poll::Done(Tokens {
            account_name: reply.account_name.unwrap_or_default(),
            steamid,
            access_token: reply.access_token.unwrap_or_default(),
            guard_data: reply.new_guard_data.filter(|g| !g.is_empty()),
            refresh_token: token,
        }));
    }
    match reply.new_challenge_url.filter(|u| !u.is_empty()) {
        Some(url) => {
            session.challenge_url = Some(url.clone());
            Ok(Poll::Rotated(url))
        }
        None => Ok(Poll::Pending),
    }
}

fn claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Unix time the token stops working (its JWT `exp` claim).
pub fn expiry(token: &str) -> Option<u64> {
    claims(token)?.get("exp")?.as_u64()
}

/// The SteamID the token belongs to (its JWT `sub` claim).
pub fn subject(token: &str) -> Option<u64> {
    claims(token)?.get("sub")?.as_str()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(claims: &str) -> String {
        let b64 = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
        format!(
            "{}.{}.sig",
            b64(r#"{"typ":"JWT","alg":"EdDSA"}"#),
            b64(claims)
        )
    }

    #[test]
    fn reads_token_claims() {
        let token = jwt(r#"{"sub":"76561197960287930","exp":1800000000}"#);
        assert_eq!(expiry(&token), Some(1_800_000_000));
        assert_eq!(subject(&token), Some(76_561_197_960_287_930));
        assert_eq!(expiry("not-a-jwt"), None);
    }

    #[test]
    fn encrypts_with_steam_key_format() {
        // A real 2048-bit modulus shape is not needed; any valid key exercises the path.
        let private = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).unwrap();
        let public = rsa::RsaPublicKey::from(&private);
        let hex = |n: &BigUint| n.to_str_radix(16);
        use rsa::traits::PublicKeyParts;
        let b64 = encrypt(&hex(public.n()), &hex(public.e()), "hunter2").unwrap();
        let data = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(private.decrypt(Pkcs1v15Encrypt, &data).unwrap(), b"hunter2");
    }

    #[test]
    fn only_code_confirmations_take_a_code() {
        assert_eq!(Confirmation::EmailCode.code_type(), Some(2));
        assert_eq!(Confirmation::DeviceCode.code_type(), Some(3));
        assert_eq!(Confirmation::DeviceConfirmation.code_type(), None);
        let raw = CAuthenticationAllowedConfirmation {
            confirmation_type: Some(4),
            ..Default::default()
        };
        assert_eq!(Confirmation::from(&raw), Confirmation::DeviceConfirmation);
    }
}
