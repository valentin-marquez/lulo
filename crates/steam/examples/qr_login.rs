//! Diagnoses a QR sign in step by step: `cargo run -p steam --example qr_login`.
//! Scan the code with the Steam mobile app. Tokens are never printed.

use std::time::Duration;

use steam::auth::{self, Poll};

#[tokio::main]
async fn main() -> steam::Result<()> {
    let http = steam::http();
    let mut client = steam::Client::connect(&http).await?;
    println!("connected");
    let mut session = auth::qr(&mut client).await?;
    println!("QR session started, poll every {}s", session.interval);
    draw(session.challenge_url.as_deref().unwrap_or_default());

    let tokens = loop {
        tokio::time::sleep(Duration::from_secs_f32(session.interval.max(1.0))).await;
        match auth::poll(&mut client, &mut session).await {
            Ok(Poll::Pending) => println!("poll: pending"),
            Ok(Poll::Rotated(url)) => {
                println!("poll: QR rotated");
                draw(&url);
            }
            Ok(Poll::Done(tokens)) => break tokens,
            Err(err) => {
                println!("poll failed: {err}");
                return Ok(());
            }
        }
    };
    println!(
        "poll: done. account_name {:?}, steamid {}, refresh token {} chars, expires {:?}, guard data {}",
        tokens.account_name,
        tokens.steamid,
        tokens.refresh_token.len(),
        auth::expiry(&tokens.refresh_token),
        tokens.guard_data.is_some()
    );

    println!("logging on with the token (20 s limit)...");
    let logon = client.user(&tokens.account_name, &tokens.refresh_token, tokens.steamid);
    match tokio::time::timeout(Duration::from_secs(20), logon).await {
        Ok(Ok(())) => println!("logon: OK, steamid {}", client.steamid()),
        Ok(Err(err)) => println!("logon failed: {err}"),
        Err(_) => println!("logon: no answer from Steam in 20 s"),
    }
    Ok(())
}

fn draw(url: &str) {
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let (w, colors) = (code.width(), code.to_colors());
            // Two rows per line with half blocks, plus a light border, so phones can scan it.
            let dark = |x: isize, y: isize| {
                x >= 0
                    && y >= 0
                    && (x as usize) < w
                    && (y as usize) < w
                    && colors[y as usize * w + x as usize] == qrcode::Color::Dark
            };
            let pad = 2isize;
            let mut y = -pad;
            while y < w as isize + pad {
                let line: String = (-pad..w as isize + pad)
                    .map(|x| match (dark(x, y), dark(x, y + 1)) {
                        (false, false) => '\u{2588}',
                        (true, false) => '\u{2584}',
                        (false, true) => '\u{2580}',
                        (true, true) => ' ',
                    })
                    .collect();
                println!("{line}");
                y += 2;
            }
        }
        Err(err) => println!("couldn't draw the QR: {err}"),
    }
}
