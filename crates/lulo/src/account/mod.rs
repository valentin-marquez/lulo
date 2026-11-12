//! The signed-in Steam account: login flow and the refresh token kept in the OS keyring
//! (or a private file when there is none).

#[path = "login.view.rs"]
pub mod login;

use serde::{Deserialize, Serialize};
use steam::auth::{self, Confirmation, Poll};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::runtime;

const SERVICE: &str = "lulo";
const ENTRY: &str = "steam";

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub steamid: u64,
    refresh_token: String,
}

impl Account {
    /// A CM connection logged on as this account. Call it on the background runtime.
    pub async fn client(
        &self,
        http: &steam::reqwest::Client,
    ) -> Result<steam::Client, steam::Error> {
        let mut client = steam::Client::connect(http).await?;
        client
            .user(&self.name, &self.refresh_token, self.steamid)
            .await?;
        Ok(client)
    }
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ENTRY).map_err(|err| err.to_string())
}

/// Where the login goes when there is no OS keyring (Linux without a Secret Service, common
/// on tiling window managers): a file only this user can read, next to the instances.
fn file() -> std::path::PathBuf {
    crate::instance::store::root().with_file_name("account.json")
}

/// The saved account, unless missing or its token has expired.
pub async fn load() -> Option<Account> {
    let text = runtime::blocking(|| {
        entry()
            .and_then(|e| e.get_password().map_err(|e| e.to_string()))
            .or_else(|_| std::fs::read_to_string(file()).map_err(|e| e.to_string()))
    })
    .await
    .ok()?
    .ok()?;
    let account: Account = serde_json::from_str(&text).ok()?;
    let alive =
        auth::expiry(&account.refresh_token).is_none_or(|exp| exp > crate::instance::store::now());
    alive.then_some(account)
}

/// Saves the account in the OS keyring, or in the private file when there is none.
/// Returns whether it went to the file.
pub async fn save(account: &Account) -> Result<bool, String> {
    let text = serde_json::to_string(account).map_err(|e| e.to_string())?;
    runtime::blocking(move || {
        if entry()
            .and_then(|e| e.set_password(&text).map_err(|e| e.to_string()))
            .is_ok()
        {
            // A login saved before the keyring was set up must not outlive this one.
            let _ = std::fs::remove_file(file());
            return Ok(false);
        }
        write_private(&file(), &text)
            .map(|()| true)
            .map_err(|e| e.to_string())
    })
    .await?
}

fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        // `mode` only applies when the file is created; an older file may be wider.
        if path.exists() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    options.open(path)?.write_all(text.as_bytes())
}

pub async fn forget() -> Result<(), String> {
    runtime::blocking(|| {
        match std::fs::remove_file(file()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.to_string()),
        }
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            // Nothing can be stored there, so nothing is left to delete.
            Err(keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_)) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    })
    .await?
}

/// How a login starts.
pub enum Start {
    Qr,
    Password { account: String, password: String },
}

/// What the login task tells the UI.
pub enum Event {
    /// Show this URL as a QR code (it can change while waiting).
    Qr(String),
    /// Ask the user for a Steam Guard code of this kind and send it back.
    Code(Confirmation),
    /// Waiting on the user outside lulo (approve in the app, click the email).
    Waiting(&'static str),
    Done(Account),
    Failed(String),
}

/// Runs a whole login on the background runtime. It stops when the UI drops `events`.
pub fn run(start: Start, events: UnboundedSender<Event>, mut codes: UnboundedReceiver<String>) {
    runtime::spawn(async move {
        let result = login(start, &events, &mut codes).await;
        let _ = events.send(match result {
            Ok(account) => Event::Done(account),
            Err(err) => Event::Failed(err),
        });
    });
}

async fn login(
    start: Start,
    events: &UnboundedSender<Event>,
    codes: &mut UnboundedReceiver<String>,
) -> Result<Account, String> {
    let fail = |err: steam::Error| err.to_string();
    let http = steam::http();
    let mut client = steam::Client::connect(&http).await.map_err(fail)?;
    let mut session = match &start {
        Start::Qr => auth::qr(&mut client).await.map_err(fail)?,
        Start::Password { account, password } => {
            auth::password(&mut client, account, password, None)
                .await
                .map_err(fail)?
        }
    };

    if let Some(url) = &session.challenge_url {
        let _ = events.send(Event::Qr(url.clone()));
    } else {
        // Password logins: satisfy Steam's preferred confirmation.
        match session.confirmations.first().copied() {
            Some(kind @ (Confirmation::EmailCode | Confirmation::DeviceCode)) => {
                let _ = events.send(Event::Code(kind));
                let code = codes.recv().await.ok_or("Login cancelled")?;
                auth::code(&mut client, &session, kind, &code)
                    .await
                    .map_err(fail)?;
            }
            Some(Confirmation::DeviceConfirmation) => {
                let _ = events.send(Event::Waiting(
                    "Approve the sign in from the Steam mobile app.",
                ));
            }
            Some(Confirmation::EmailConfirmation) => {
                let _ = events.send(Event::Waiting("Open the link Steam sent to your email."));
            }
            _ => {}
        }
    }

    loop {
        if events.is_closed() {
            return Err("Login cancelled".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs_f32(
            session.interval.max(1.0),
        ))
        .await;
        match auth::poll(&mut client, &mut session).await.map_err(fail)? {
            Poll::Pending => {}
            Poll::Rotated(url) => {
                let _ = events.send(Event::Qr(url));
            }
            Poll::Done(tokens) => {
                // Log on once with the new token, so a bad token fails here and not at install.
                client
                    .user(&tokens.account_name, &tokens.refresh_token, tokens.steamid)
                    .await
                    .map_err(fail)?;
                return Ok(Account {
                    name: tokens.account_name,
                    steamid: tokens.steamid,
                    refresh_token: tokens.refresh_token,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Writes and deletes a throwaway entry (not the real account) to see whether the OS keyring
    /// answers. Run with `cargo test -p lulo keyring -- --ignored --nocapture`.
    #[test]
    #[ignore = "talks to the OS keyring"]
    fn keyring_answers() {
        let started = std::time::Instant::now();
        let waiter = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = waiter.block_on(super::runtime::blocking(|| {
            let entry =
                keyring::Entry::new("lulo-diagnostic", "probe").map_err(|e| e.to_string())?;
            entry
                .set_password("not-a-secret")
                .map_err(|e| e.to_string())?;
            entry.delete_credential().map_err(|e| e.to_string())
        }));
        println!("keyring round trip: {result:?} in {:?}", started.elapsed());
        assert!(matches!(result, Ok(Ok(()))));
    }
}

#[cfg(test)]
mod file_tests {
    #[cfg(unix)]
    #[test]
    fn private_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir()
            .join(format!("lulo-account-{}", std::process::id()))
            .join("account.json");
        super::write_private(&path, "a").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::write_private(&path, "b").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "b");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
