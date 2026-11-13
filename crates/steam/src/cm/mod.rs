//! Connection to a Steam connection manager (CM) over WebSocket.
//!
//! The socket is TLS, so there is no `ChannelEncrypt` handshake; each binary frame is one packet.

pub mod packet;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use prost::Message;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{Instant, Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::proto::{
    CMsgClientHeartBeat, CMsgClientHello, CMsgClientLogon, CMsgClientLogonResponse,
    CMsgProtoBufHeader,
};
use crate::{Error, Result, directory};
use packet::{Packet, emsg};

/// Values steamclient sends; accepted by the server (exact origin still to confirm).
const PROTOCOL_VERSION: u32 = 65580;
const OS_TYPE: u32 = 20;
/// Public universe, AnonUser account type.
const ANONYMOUS: u64 = (1 << 56) | (10 << 52);
/// How many CM endpoints to try before giving up.
const ATTEMPTS: usize = 3;
const OK: i32 = 1;
/// Steam's usual interval, for a logon response that doesn't say.
const HEARTBEAT: u64 = 9;
/// How this machine shows up in the user's Steam authorized devices.
pub(crate) const DEVICE_NAME: &str = "lulo";
pub(crate) const OS: i32 = OS_TYPE as i32;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Replies = UnboundedSender<Result<Packet>>;

/// What a reply is matched on: the job it answers, or just its message type (the logon reply
/// carries no job of ours).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Route {
    Job(u64),
    Emsg(u32),
}

enum Command {
    /// Sends a frame; replies on `route` that have message type `.1` go to the sender.
    Send {
        bytes: Vec<u8>,
        route: Option<(Route, u32, Replies)>,
    },
    /// Sends `packet` every `every` from now on.
    Beat { every: Duration, packet: Vec<u8> },
}

/// A logged-on CM session. A background task owns the socket, so requests can overlap and the
/// session keeps beating while nobody is asking for anything (installs last minutes).
/// Dropping the client closes the connection.
pub struct Client {
    commands: UnboundedSender<Command>,
    steamid: u64,
    session: i32,
    job: AtomicU64,
    /// Cell (region) Steam assigned at logon; CDN lookups use it.
    pub cell_id: u32,
}

impl Client {
    /// The logged-on account (or 0 before logon).
    pub fn steamid(&self) -> u64 {
        self.steamid
    }
}

impl Client {
    /// Connects to a CM from the public directory and logs on anonymously.
    pub async fn anonymous(http: &reqwest::Client) -> Result<Self> {
        let mut client = Self::connect(http).await?;
        client.steamid = ANONYMOUS;
        client
            .logon(CMsgClientLogon::default(), "anonymous logon")
            .await?;
        Ok(client)
    }

    /// Logs on as a user with a refresh token from [`crate::auth`]. `steamid` is the account's.
    pub async fn user(&mut self, account: &str, refresh_token: &str, steamid: u64) -> Result<()> {
        self.steamid = steamid;
        let logon = CMsgClientLogon {
            account_name: Some(account.to_owned()),
            // The field is named access_token, but Steam expects the refresh token here.
            access_token: Some(refresh_token.to_owned()),
            should_remember_password: Some(true),
            machine_name: Some(DEVICE_NAME.to_owned()),
            supports_rate_limit_response: Some(true),
            ..Default::default()
        };
        self.logon(logon, "user logon").await
    }

    async fn logon(&mut self, base: CMsgClientLogon, context: &str) -> Result<()> {
        let logon = CMsgClientLogon {
            protocol_version: Some(PROTOCOL_VERSION),
            client_os_type: Some(OS_TYPE),
            cell_id: Some(0),
            ..base
        };
        let reply = self
            .exchange(
                emsg::CLIENT_LOGON,
                &logon,
                CMsgProtoBufHeader::default(),
                Route::Emsg(emsg::CLIENT_LOGON_RESPONSE),
                emsg::CLIENT_LOGON_RESPONSE,
            )?
            .recv()
            .await?;
        let response = CMsgClientLogonResponse::decode(reply.body.as_slice())?;
        check(response.eresult, context)?;
        self.steamid = reply.header.steamid.unwrap_or(self.steamid);
        self.session = reply.header.client_sessionid.unwrap_or(0);
        self.cell_id = response.cell_id.unwrap_or(0);
        self.start_heartbeat(response.heartbeat_seconds)
    }

    /// Steam drops a session that stays silent past the logon response's `heartbeat_seconds`.
    fn start_heartbeat(&self, seconds: Option<i32>) -> Result<()> {
        let every = seconds
            .and_then(|s| u64::try_from(s).ok())
            .filter(|s| *s > 0)
            .unwrap_or(HEARTBEAT);
        let packet = self.packet(
            emsg::CLIENT_HEART_BEAT,
            &CMsgClientHeartBeat::default(),
            CMsgProtoBufHeader::default(),
        );
        self.commands
            .send(Command::Beat {
                every: Duration::from_secs(every),
                packet,
            })
            .map_err(|_| Error::Closed)
    }

    /// Connects to a CM from the public directory and says hello, without logging on: the
    /// state [`crate::auth`] needs (steamid 0).
    pub async fn connect(http: &reqwest::Client) -> Result<Self> {
        let client = Self::open(http).await?;
        let hello = CMsgClientHello {
            protocol_version: Some(PROTOCOL_VERSION),
        };
        let bytes = client.packet(emsg::CLIENT_HELLO, &hello, CMsgProtoBufHeader::default());
        client
            .commands
            .send(Command::Send { bytes, route: None })
            .map_err(|_| Error::Closed)?;
        Ok(client)
    }

    async fn open(http: &reqwest::Client) -> Result<Self> {
        crate::tls();
        let mut last = Error::NoServers;
        for endpoint in directory::websocket_endpoints(http)
            .await?
            .iter()
            .take(ATTEMPTS)
        {
            match tokio_tungstenite::connect_async(format!("wss://{endpoint}/cmsocket/")).await {
                Ok((socket, _)) => {
                    let (commands, queue) = mpsc::unbounded_channel();
                    tokio::spawn(run(socket, queue));
                    return Ok(Self {
                        commands,
                        steamid: 0,
                        session: 0,
                        job: AtomicU64::new(0),
                        cell_id: 0,
                    });
                }
                Err(err) => last = err.into(),
            }
        }
        Err(last)
    }

    fn packet(&self, emsg: u32, body: &impl Message, mut header: CMsgProtoBufHeader) -> Vec<u8> {
        header.steamid = Some(self.steamid);
        header.client_sessionid = Some(self.session);
        packet::encode(emsg, &header, body)
    }

    /// Sends a message and returns where its replies (type `reply`, matched by `route`) arrive.
    /// The route is registered before the frame goes out, so no reply can be missed.
    fn exchange(
        &self,
        emsg: u32,
        body: &impl Message,
        header: CMsgProtoBufHeader,
        route: Route,
        reply: u32,
    ) -> Result<Reply> {
        let (tx, rx) = mpsc::unbounded_channel();
        let bytes = self.packet(emsg, body, header);
        self.commands
            .send(Command::Send {
                bytes,
                route: Some((route, reply, tx)),
            })
            .map_err(|_| Error::Closed)?;
        Ok(Reply(rx))
    }

    fn next_job(&self) -> u64 {
        self.job.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Calls a unified service method, e.g. `ContentServerDirectory.GetServersForSteamPipe`.
    pub async fn call<Resp: Message + Default>(
        &self,
        method: &str,
        request: &impl Message,
    ) -> Result<Resp> {
        self.service(emsg::SERVICE_METHOD_CALL, method, request)
            .await
    }

    /// Calls a service method that works before logon (the `Authentication` service).
    pub async fn call_unauthed<Resp: Message + Default>(
        &self,
        method: &str,
        request: &impl Message,
    ) -> Result<Resp> {
        self.service(emsg::SERVICE_METHOD_CALL_NON_AUTHED, method, request)
            .await
    }

    async fn service<Resp: Message + Default>(
        &self,
        emsg: u32,
        method: &str,
        request: &impl Message,
    ) -> Result<Resp> {
        let job = self.next_job();
        let header = CMsgProtoBufHeader {
            target_job_name: Some(format!("{method}#1")),
            jobid_source: Some(job),
            ..Default::default()
        };
        let reply = self
            .exchange(
                emsg,
                request,
                header,
                Route::Job(job),
                emsg::SERVICE_METHOD_RESPONSE,
            )?
            .recv()
            .await?;
        check(reply.header.eresult, method)?;
        Ok(Resp::decode(reply.body.as_slice())?)
    }

    /// Sends a classic job message and returns the first reply of type `reply`.
    pub async fn request<Resp: Message + Default>(
        &self,
        emsg: u32,
        reply: u32,
        request: &impl Message,
    ) -> Result<Resp> {
        let job = self.next_job();
        let header = CMsgProtoBufHeader {
            jobid_source: Some(job),
            ..Default::default()
        };
        let packet = self
            .exchange(emsg, request, header, Route::Job(job), reply)?
            .recv()
            .await?;
        Ok(Resp::decode(packet.body.as_slice())?)
    }

    /// Like [`Self::request`], but keeps reading replies for the same job until `done` accepts one.
    pub async fn request_until<Resp: Message + Default>(
        &self,
        emsg: u32,
        reply: u32,
        request: &impl Message,
        mut done: impl FnMut(&Resp) -> bool,
    ) -> Result<Resp> {
        let job = self.next_job();
        let header = CMsgProtoBufHeader {
            jobid_source: Some(job),
            ..Default::default()
        };
        let mut replies = self.exchange(emsg, request, header, Route::Job(job), reply)?;
        loop {
            let response = Resp::decode(replies.recv().await?.body.as_slice())?;
            if done(&response) {
                return Ok(response);
            }
        }
    }
}

/// The receiving end of a request's replies.
struct Reply(UnboundedReceiver<Result<Packet>>);

impl Reply {
    async fn recv(&mut self) -> Result<Packet> {
        self.0.recv().await.ok_or(Error::Closed)?
    }
}

struct Beat {
    timer: Interval,
    packet: Vec<u8>,
}

/// Owns the socket: writes what the client sends, sends the heartbeat, and hands each incoming
/// packet to whoever waits for it. Ends when the client is dropped or the socket fails, after
/// telling every waiter that the session closed.
async fn run(mut socket: Socket, mut commands: UnboundedReceiver<Command>) {
    let mut routes: HashMap<Route, (u32, Replies)> = HashMap::new();
    let mut beat: Option<Beat> = None;
    loop {
        let step: Result<()> = tokio::select! {
            command = commands.recv() => match command {
                None => break,
                Some(Command::Send { bytes, route }) => {
                    if let Some((route, reply, tx)) = route {
                        routes.retain(|_, (_, tx)| !tx.is_closed());
                        routes.insert(route, (reply, tx));
                    }
                    send(&mut socket, bytes).await
                }
                Some(Command::Beat { every, packet }) => {
                    let mut timer = tokio::time::interval_at(Instant::now() + every, every);
                    timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
                    beat = Some(Beat { timer, packet });
                    Ok(())
                }
            },
            frame = socket.next() => match frame {
                None | Some(Ok(Frame::Close(_))) => Err(Error::Closed),
                Some(Err(err)) => Err(err.into()),
                Some(Ok(Frame::Binary(bytes))) => deliver(&bytes, &mut routes),
                Some(Ok(_)) => Ok(()),
            },
            _ = async {
                match &mut beat {
                    Some(beat) => beat.timer.tick().await,
                    None => std::future::pending().await,
                }
            } => match &beat {
                Some(beat) => send(&mut socket, beat.packet.clone()).await,
                None => Ok(()),
            },
        };
        if step.is_err() {
            break;
        }
    }
    for (_, tx) in routes.into_values() {
        let _ = tx.send(Err(Error::Closed));
    }
}

async fn send(socket: &mut Socket, bytes: Vec<u8>) -> Result<()> {
    socket.send(Frame::Binary(bytes.into())).await?;
    Ok(())
}

/// Passes each packet to the waiter for its job (or, without one, for its message type).
fn deliver(bytes: &[u8], routes: &mut HashMap<Route, (u32, Replies)>) -> Result<()> {
    for packet in packet::expand(packet::decode(bytes)?)? {
        let job = packet.header.jobid_target.map(Route::Job);
        if packet.emsg == emsg::DEST_JOB_FAILED {
            if let Some((_, tx)) = job.and_then(|job| routes.remove(&job)) {
                let _ = tx.send(Err(Error::Steam {
                    code: 2, // EResult::Fail
                    context: "request (job failed on Steam's side)".into(),
                }));
            }
            continue;
        }
        let wanted = |route: &Route| {
            routes
                .get(route)
                .is_some_and(|(reply, _)| *reply == packet.emsg)
        };
        let Some(route) = [job, Some(Route::Emsg(packet.emsg))]
            .into_iter()
            .flatten()
            .find(wanted)
        else {
            continue;
        };
        let delivered = routes
            .get(&route)
            .is_some_and(|(_, tx)| tx.send(Ok(packet)).is_ok());
        if !delivered || matches!(route, Route::Emsg(_)) {
            routes.remove(&route);
        }
    }
    Ok(())
}

/// Maps an EResult to an error; `None` counts as success only where Steam omits it on OK.
pub(crate) fn check(eresult: Option<i32>, context: &str) -> Result<()> {
    match eresult.unwrap_or(OK) {
        OK => Ok(()),
        code => Err(Error::Steam {
            code,
            context: context.to_owned(),
        }),
    }
}
