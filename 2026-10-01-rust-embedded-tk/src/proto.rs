//! Wire protocol: newline-delimited JSON over a unix socket.

use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    /// Assigned by the daemon, strictly increasing.
    pub id: u64,
    pub chan: String,
    pub author: String,
    pub msg: String,
    /// Unix timestamp in milliseconds, assigned by the daemon.
    pub ts: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Req {
    ListChans,
    /// Up to `limit` messages with `id < before` (or the most recent ones),
    /// in chronological order.
    ListMsgs {
        chan: String,
        before: Option<u64>,
        limit: usize,
    },
    Post {
        chan: String,
        author: String,
        msg: String,
    },
    /// "I'm here, in these channels". Relayed to all subscribers.
    Presence {
        author: String,
        chans: Vec<String>,
    },
    /// From now on, this connection only receives pushed
    /// `Resp::Event` and `Resp::Presence`.
    Subscribe,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Resp {
    Chans { chans: Vec<String> },
    Msgs { msgs: Vec<Msg> },
    Posted { msg: Msg },
    Subscribed,
    Ack,
    Event { msg: Msg },
    Presence { author: String, chans: Vec<String>, ts: u64 },
    Err { err: String },
}

pub fn write_line<T: Serialize>(w: &mut impl Write, x: &T) -> Result<()> {
    let mut buf = serde_json::to_vec(x)?;
    buf.push(b'\n');
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

/// Read one JSON line. Returns `None` on EOF.
pub fn read_line<T: for<'de> Deserialize<'de>>(r: &mut impl BufRead) -> Result<Option<T>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let x = serde_json::from_str(&line).with_context(|| format!("invalid json: {line:?}"))?;
    Ok(Some(x))
}

/// What a subscription receives.
#[derive(Debug)]
pub enum Push {
    Msg(Msg),
    Presence { author: String, chans: Vec<String> },
}

/// Synchronous request/response connection.
pub struct Conn {
    r: std::io::BufReader<std::os::unix::net::UnixStream>,
    w: std::os::unix::net::UnixStream,
}

impl Conn {
    pub fn connect(sock: &std::path::Path) -> Result<Self> {
        let w = std::os::unix::net::UnixStream::connect(sock)
            .with_context(|| format!("connecting to {}", sock.display()))?;
        let r = std::io::BufReader::new(w.try_clone()?);
        Ok(Conn { r, w })
    }

    pub fn call(&mut self, req: &Req) -> Result<Resp> {
        write_line(&mut self.w, req)?;
        match read_line(&mut self.r)? {
            None => Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "daemon closed the connection").into()),
            Some(Resp::Err { err }) => bail!("daemon error: {err}"),
            Some(resp) => Ok(resp),
        }
    }

    /// Turn this connection into a stream of pushed events.
    pub fn subscribe(mut self) -> Result<impl Iterator<Item = Result<Push>>> {
        match self.call(&Req::Subscribe)? {
            Resp::Subscribed => (),
            r => bail!("unexpected response {r:?}"),
        }
        let mut r = self.r;
        Ok(std::iter::from_fn(move || match read_line::<Resp>(&mut r) {
            Ok(None) => None,
            Ok(Some(Resp::Event { msg })) => Some(Ok(Push::Msg(msg))),
            Ok(Some(Resp::Presence { author, chans, .. })) => Some(Ok(Push::Presence { author, chans })),
            Ok(Some(r)) => Some(Err(anyhow::anyhow!("unexpected push {r:?}"))),
            Err(e) => Some(Err(e)),
        }))
    }
}
