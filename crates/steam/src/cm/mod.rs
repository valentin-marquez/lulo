//! Connection to a Steam connection manager (CM) over WebSocket.
//!
//! The socket is TLS, so there is no `ChannelEncrypt` handshake; each binary frame is one packet.

pub mod packet;

use std::collections::VecDeque;

use futures_util::{SinkExt, StreamExt};
use prost::Message;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::proto::{CMsgClientHello, CMsgClientLogon, CMsgClientLogonResponse, CMsgProtoBufHeader};
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
/// How this machine shows up in the user's Steam authorized devices.
pub(crate) const DEVICE_NAME: &str = "lulo";
pub(crate) const OS: i32 = OS_TYPE as i32;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A logged-on CM session. Requests are sequential: send, then wait for the matching reply.
// ponytail: one request at a time and no heartbeat (sessions last seconds). Move to a task that
// owns the socket, routes replies by job id and sends ClientHeartBeat (703) for long sessions.
pub struct Client {
    socket: Socket,
    steamid: u64,
    session: i32,
    job: u64,
    queue: VecDeque<Packet>,
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
        self.send(emsg::CLIENT_LOGON, &logon, CMsgProtoBufHeader::default())
            .await?;
        let reply = self.recv(emsg::CLIENT_LOGON_RESPONSE, None).await?;
        let response = CMsgClientLogonResponse::decode(reply.body.as_slice())?;
        check(response.eresult, context)?;
        self.steamid = reply.header.steamid.unwrap_or(self.steamid);
        self.session = reply.header.client_sessionid.unwrap_or(0);
        self.cell_id = response.cell_id.unwrap_or(0);
        Ok(())
    }

    /// Connects to a CM from the public directory and says hello, without logging on: the
    /// state [`crate::auth`] needs (steamid 0).
    pub async fn connect(http: &reqwest::Client) -> Result<Self> {
        let mut client = Self::open(http).await?;
        client
            .send(
                emsg::CLIENT_HELLO,
                &CMsgClientHello {
                    protocol_version: Some(PROTOCOL_VERSION),
                },
                CMsgProtoBufHeader::default(),
            )
            .await?;
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
                    return Ok(Self {
                        socket,
                        steamid: 0,
                        session: 0,
                        job: 0,
                        queue: VecDeque::new(),
                        cell_id: 0,
                    });
                }
                Err(err) => last = err.into(),
            }
        }
        Err(last)
    }

    async fn send(
        &mut self,
        emsg: u32,
        body: &impl Message,
        mut header: CMsgProtoBufHeader,
    ) -> Result<()> {
        header.steamid = Some(self.steamid);
        header.client_sessionid = Some(self.session);
        let bytes = packet::encode(emsg, &header, body);
        self.socket.send(Frame::Binary(bytes.into())).await?;
        Ok(())
    }

    /// Waits for a packet with `emsg` (and `job` as its target, when given), dropping others.
    async fn recv(&mut self, emsg: u32, job: Option<u64>) -> Result<Packet> {
        loop {
            while let Some(packet) = self.queue.pop_front() {
                let for_job = job.is_none() || packet.header.jobid_target == job;
                if packet.emsg == emsg::DEST_JOB_FAILED && job.is_some() && for_job {
                    return Err(Error::Steam {
                        code: 2, // EResult::Fail
                        context: "request (job failed on Steam's side)".into(),
                    });
                }
                if packet.emsg == emsg && for_job {
                    return Ok(packet);
                }
            }
            match self.socket.next().await.ok_or(Error::Closed)?? {
                Frame::Binary(bytes) => self.queue.extend(packet::expand(packet::decode(&bytes)?)?),
                Frame::Close(_) => return Err(Error::Closed),
                _ => {}
            }
        }
    }

    fn next_job(&mut self) -> u64 {
        self.job += 1;
        self.job
    }

    /// Calls a unified service method, e.g. `ContentServerDirectory.GetServersForSteamPipe`.
    pub async fn call<Resp: Message + Default>(
        &mut self,
        method: &str,
        request: &impl Message,
    ) -> Result<Resp> {
        self.service(emsg::SERVICE_METHOD_CALL, method, request)
            .await
    }

    /// Calls a service method that works before logon (the `Authentication` service).
    pub async fn call_unauthed<Resp: Message + Default>(
        &mut self,
        method: &str,
        request: &impl Message,
    ) -> Result<Resp> {
        self.service(emsg::SERVICE_METHOD_CALL_NON_AUTHED, method, request)
            .await
    }

    async fn service<Resp: Message + Default>(
        &mut self,
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
        self.send(emsg, request, header).await?;
        let reply = self.recv(emsg::SERVICE_METHOD_RESPONSE, Some(job)).await?;
        check(reply.header.eresult, method)?;
        Ok(Resp::decode(reply.body.as_slice())?)
    }

    /// Sends a classic job message and returns the first reply of type `reply`.
    pub async fn request<Resp: Message + Default>(
        &mut self,
        emsg: u32,
        reply: u32,
        request: &impl Message,
    ) -> Result<Resp> {
        let job = self.next_job();
        let header = CMsgProtoBufHeader {
            jobid_source: Some(job),
            ..Default::default()
        };
        self.send(emsg, request, header).await?;
        let packet = self.recv(reply, Some(job)).await?;
        Ok(Resp::decode(packet.body.as_slice())?)
    }

    /// Like [`Self::request`], but keeps reading replies for the same job until `done` accepts one.
    pub async fn request_until<Resp: Message + Default>(
        &mut self,
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
        self.send(emsg, request, header).await?;
        loop {
            let packet = self.recv(reply, Some(job)).await?;
            let response = Resp::decode(packet.body.as_slice())?;
            if done(&response) {
                return Ok(response);
            }
        }
    }
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
