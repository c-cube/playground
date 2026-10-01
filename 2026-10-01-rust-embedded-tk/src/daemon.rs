//! The chat daemon: in-memory channels, one thread per client.

use std::collections::{HashMap, VecDeque};
use std::io::BufReader;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::proto::{Msg, Req, Resp, read_line, write_line};

const MAX_MSGS_PER_CHAN: usize = 1000;

#[derive(Default)]
struct State {
    next_id: u64,
    chans: HashMap<String, VecDeque<Msg>>,
    subscribers: Vec<UnixStream>,
}

type Shared = Arc<Mutex<State>>;

pub fn run(sock: &Path) -> Result<()> {
    if sock.exists() {
        if UnixStream::connect(sock).is_ok() {
            bail!("a daemon is already listening on {}", sock.display());
        }
        std::fs::remove_file(sock).context("removing stale socket")?;
    }
    let listener = UnixListener::bind(sock).with_context(|| format!("binding {}", sock.display()))?;
    eprintln!("listening on {}", sock.display());

    let st: Shared = Arc::new(Mutex::new(State { next_id: 1, ..State::default() }));
    for conn in listener.incoming() {
        let conn = match conn {
            Ok(c) => c,
            Err(e) => {
                eprintln!("accept: {e}");
                continue;
            }
        };
        let st = st.clone();
        std::thread::spawn(move || {
            if let Err(e) = handle_client(&st, conn) {
                eprintln!("client error: {e:#}");
            }
        });
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn handle_client(st: &Shared, conn: UnixStream) -> Result<()> {
    let mut r = BufReader::new(conn.try_clone()?);
    let mut w = conn;
    loop {
        let req = match read_line::<Req>(&mut r) {
            Ok(Some(req)) => req,
            Ok(None) => break,
            // don't hang up on a request we don't understand (e.g. a newer client)
            Err(e) if e.downcast_ref::<serde_json::Error>().is_some() => {
                write_line(&mut w, &Resp::Err { err: format!("{e:#}") })?;
                continue;
            }
            Err(e) => return Err(e),
        };
        let resp = match req {
            Req::ListChans => {
                let st = st.lock().unwrap();
                let mut chans: Vec<String> = st.chans.keys().cloned().collect();
                chans.sort();
                Resp::Chans { chans }
            }
            Req::ListMsgs { chan, before, limit } => {
                let st = st.lock().unwrap();
                let msgs = match st.chans.get(&chan) {
                    None => vec![],
                    Some(q) => {
                        let before = before.unwrap_or(u64::MAX);
                        let mut v: Vec<Msg> =
                            q.iter().rev().filter(|m| m.id < before).take(limit).cloned().collect();
                        v.reverse();
                        v
                    }
                };
                Resp::Msgs { msgs }
            }
            Req::Post { chan, author, msg } => {
                if chan.is_empty() {
                    Resp::Err { err: "empty channel name".into() }
                } else {
                    Resp::Posted { msg: post(st, chan, author, msg) }
                }
            }
            Req::Presence { author, chans } => {
                broadcast(&mut st.lock().unwrap(), &Resp::Presence { author, chans, ts: now_ms() });
                Resp::Ack
            }
            Req::Subscribe => {
                write_line(&mut w, &Resp::Subscribed)?;
                st.lock().unwrap().subscribers.push(w);
                // the subscriber list owns the write half now; we just wait for EOF
                while read_line::<Req>(&mut r)?.is_some() {}
                return Ok(());
            }
        };
        write_line(&mut w, &resp)?;
    }
    Ok(())
}

fn post(st: &Shared, chan: String, author: String, msg: String) -> Msg {
    let mut st = st.lock().unwrap();
    let m = Msg { id: st.next_id, chan: chan.clone(), author, msg, ts: now_ms() };
    st.next_id += 1;
    let q = st.chans.entry(chan).or_default();
    q.push_back(m.clone());
    if q.len() > MAX_MSGS_PER_CHAN {
        q.pop_front();
    }
    broadcast(&mut st, &Resp::Event { msg: m.clone() });
    m
}

/// Push to every subscriber, dropping the ones that went away.
fn broadcast(st: &mut State, ev: &Resp) {
    st.subscribers.retain_mut(|s| write_line(s, ev).is_ok());
}
