# How lulo talks to Steam

lulo downloads Project Zomboid and its Workshop mods itself, without SteamCMD. This is the
`crates/steam` crate.

## Connection

1. Ask Steam's public directory for connection managers (CMs) and connect to one over a
   WebSocket (TLS). Each binary frame carries one Steam message.
2. Say hello, then log on: anonymously, or as a user with a refresh token.
3. Requests are protobuf messages. Replies are matched to requests by a job id. Several
   replies can arrive bundled (and gzipped) in one frame.

## Signing in

Sign in happens before logon, through Steam's Authentication service:

- QR: Steam returns a URL shown as a QR code; the user approves it in the Steam mobile app.
- Password: the password is encrypted with Steam's RSA key for the account, then Steam
  Guard may ask for a code or an approval.

Either way lulo polls until Steam returns a refresh token, logs on with it and keeps it in
the OS keyring. The password itself is never stored.

## Game versions

Each Project Zomboid version is a Steam branch (`public`, `legacy41`...). lulo reads the
app's product info, picks the branch's depots for the current OS and the manifest each one
uses on that branch.

## Downloads

1. Get the depot key and a content server (CDN) host.
2. Download the manifest: the list of files, each split into chunks.
3. Download chunks in parallel. Each one is decrypted (AES-256), decompressed (zstd or
   LZMA) and checked (Adler-32 and SHA-1) before it is written. Every file is checked again
   against its SHA-1 when complete.

Workshop mods use the same path: a mod is a manifest in the game's own depot.

## Testing

`cargo test -p steam -- --ignored` runs tests against the real Steam network: anonymous
logon, product info, starting a QR login and downloading a small Workshop mod.
