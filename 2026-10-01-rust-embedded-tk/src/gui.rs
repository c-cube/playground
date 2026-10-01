//! Tk client: Rust talks to the daemon, `ui.tcl` draws, tkhost connects them.
//!
//! - async calls (`rs::call`): `channels`, `messages chan ?before? ?limit?`,
//!   `post chan msg`
//! - sync calls (`rs::cmd`): `author_color name`
//! - events (`rs::on`): `msg {dict}` for each new message, `reconnect` after
//!   the daemon came back
//! - our state (`rs::state`): `author`, `connected` (0/1), `here:<chan>`
//!   (authors seen there in the last 40s)
//! - UI state we read (`rs::ui`): `joined`, the channels we announce
//!   presence in (every 5s, and whenever it changes)
//!
//! Both daemon connections reconnect on their own.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tkhost::Ui;

use crate::proto::{Conn, Msg, Push, Req, Resp};

const PRESENCE_EVERY: Duration = Duration::from_secs(5);
const PRESENCE_TIMEOUT: Duration = Duration::from_secs(40);
/// After a failed connect, wait this long before trying again.
const RETRY_EVERY: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Ctrl {
    /// `None` after an IO error; reconnected on next call
    conn: Option<Conn>,
    /// no connect attempt before this (set after a failed connect)
    retry_at: Option<Instant>,
}

struct Seen {
    chans: Vec<String>,
    at: Instant,
}

struct Chat {
    ui: Ui,
    sock: PathBuf,
    author: String,
    ctrl: Mutex<Ctrl>,
    /// last presence broadcast received, per author
    seen: Mutex<HashMap<String, Seen>>,
    /// the `here:<chan>` keys currently set
    here_chans: Mutex<BTreeSet<String>>,
}

fn arity(args: &[String], min: usize, max: usize, usage: &str) -> Result<()> {
    if args.len() < min || args.len() > max {
        bail!("wrong # args: should be \"{usage}\"");
    }
    Ok(())
}

impl Chat {
    /// Request/response on the control connection; on an IO error, reconnect
    /// and retry once.
    fn call(&self, req: Req) -> Result<Resp> {
        let mut ctrl = self.ctrl.lock().unwrap();
        for attempt in 0..2 {
            if ctrl.conn.is_none() {
                if let Some(t) = ctrl.retry_at {
                    let now = Instant::now();
                    if now < t {
                        bail!("disconnected, retrying in {}s", (t - now).as_secs() + 1);
                    }
                }
                match Conn::connect(&self.sock) {
                    Ok(c) => *ctrl = Ctrl { conn: Some(c), retry_at: None },
                    Err(e) => {
                        ctrl.retry_at = Some(Instant::now() + RETRY_EVERY);
                        return Err(e);
                    }
                }
            }
            match ctrl.conn.as_mut().unwrap().call(&req) {
                Err(e) if e.downcast_ref::<std::io::Error>().is_some() => {
                    ctrl.conn = None;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
                r => return r,
            }
        }
        unreachable!()
    }

    fn channels(&self, args: &[String]) -> Result<Vec<String>> {
        arity(args, 0, 0, "channels")?;
        match self.call(Req::ListChans)? {
            Resp::Chans { chans } => Ok(chans),
            r => bail!("unexpected response {r:?}"),
        }
    }

    fn messages(&self, args: &[String]) -> Result<Vec<Msg>> {
        arity(args, 1, 3, "messages chan ?before? ?limit?")?;
        let before = match args.get(1).map(|s| s.as_str()) {
            None | Some("") => None,
            Some(s) => Some(s.parse::<u64>().with_context(|| format!("bad id {s:?}"))?),
        };
        let limit = match args.get(2) {
            None => 50,
            Some(s) => s.parse::<usize>().context("bad limit")?,
        };
        match self.call(Req::ListMsgs { chan: args[0].clone(), before, limit })? {
            Resp::Msgs { msgs } => Ok(msgs),
            r => bail!("unexpected response {r:?}"),
        }
    }

    fn post(&self, args: &[String]) -> Result<Msg> {
        arity(args, 2, 2, "post chan msg")?;
        let req = Req::Post { chan: args[0].clone(), author: self.author.clone(), msg: args[1].clone() };
        match self.call(req)? {
            Resp::Posted { msg } => Ok(msg),
            r => bail!("unexpected response {r:?}"),
        }
    }

    fn announce(&self) -> Result<()> {
        let chans = match self.ui.tcl_get("joined") {
            None => vec![],
            Some(l) => tkhost::parse_list(&l)?,
        };
        self.call(Req::Presence { author: self.author.clone(), chans })?;
        Ok(())
    }

    /// Expire silent authors and publish `here:<chan>` for every channel.
    fn update_presence(&self) {
        let mut snap: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        {
            let mut seen = self.seen.lock().unwrap();
            seen.retain(|_, s| s.at.elapsed() <= PRESENCE_TIMEOUT);
            for (author, s) in seen.iter() {
                for c in &s.chans {
                    snap.entry(c.clone()).or_default().insert(author.clone());
                }
            }
        }
        let mut here_chans = self.here_chans.lock().unwrap();
        for gone in here_chans.iter().filter(|c| !snap.contains_key(*c)) {
            self.ui.unset(&format!("here:{gone}"));
        }
        for (chan, authors) in &snap {
            self.ui.set(&format!("here:{chan}"), authors);
        }
        *here_chans = snap.into_keys().collect();
    }

    /// Keep a subscription open, reconnecting forever.
    fn subscriber(&self) {
        let mut first = true;
        let mut warned = false;
        loop {
            match Conn::connect(&self.sock).and_then(|c| c.subscribe()) {
                Ok(events) => {
                    self.ui.set("connected", 1);
                    warned = false;
                    // the UI may have been drawn while disconnected: refresh it
                    if !first {
                        eprintln!("resubscribed");
                        self.ctrl.lock().unwrap().retry_at = None;
                        let _ = self.announce();
                        self.ui.emit("reconnect", ());
                    }
                    for ev in events {
                        match ev {
                            Ok(Push::Msg(m)) => self.ui.emit("msg", m),
                            Ok(Push::Presence { author, chans }) => {
                                self.seen.lock().unwrap().insert(author, Seen { chans, at: Instant::now() });
                                self.update_presence();
                            }
                            Err(e) => {
                                eprintln!("subscription: {e:#}");
                                break;
                            }
                        }
                    }
                    eprintln!("subscription closed, reconnecting…");
                    self.ui.set("connected", 0);
                    first = false;
                    continue; // retry right away, then every RETRY_EVERY
                }
                Err(e) => {
                    self.ui.set("connected", 0);
                    if !warned {
                        eprintln!("subscribe: {e:#} (retrying every {}s)", RETRY_EVERY.as_secs());
                        warned = true;
                    }
                }
            }
            first = false;
            std::thread::sleep(RETRY_EVERY);
        }
    }
}

/// A color per author, the same in every client (FNV-1a, stable across runs).
fn author_color(args: &[String]) -> Result<&'static str> {
    arity(args, 1, 1, "author_color name")?;
    const PALETTE: &[&str] =
        &["SteelBlue4", "DarkOrange3", "purple3", "firebrick3", "DeepPink4", "turquoise4", "sienna4", "RoyalBlue3"];
    let h = args[0].bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    Ok(PALETTE[(h % PALETTE.len() as u64) as usize])
}

fn random_name() -> String {
    const ADJ: &[&str] = &["sleepy", "brave", "fuzzy", "grumpy", "shiny", "quiet", "loud", "tiny"];
    const ANIMAL: &[&str] = &["otter", "camel", "lynx", "heron", "yak", "newt", "badger", "gecko"];
    let a = ADJ[rand::random_range(0..ADJ.len())];
    let b = ANIMAL[rand::random_range(0..ANIMAL.len())];
    format!("{a}-{b}-{}", rand::random_range(0..100))
}

pub fn run(sock: &Path, author: Option<String>, ui_file: tkhost::TclFile) -> Result<()> {
    let ui = Ui::new()?;
    let chat = Arc::new(Chat {
        ui: ui.clone(),
        sock: sock.to_owned(),
        author: author.unwrap_or_else(random_name),
        ctrl: Mutex::default(),
        seen: Mutex::default(),
        here_chans: Mutex::default(),
    });
    ui.set("author", &chat.author);
    ui.set("connected", 0);

    let c = chat.clone();
    ui.handle("channels", move |args| c.channels(args));
    let c = chat.clone();
    ui.handle("messages", move |args| c.messages(args));
    let c = chat.clone();
    ui.handle("post", move |args| c.post(args));
    ui.handle("author_color", author_color);
    let c = chat.clone();
    ui.on_tcl_change(move |k, _| {
        if k == "joined" {
            let _ = c.announce();
        }
    });

    let c = chat.clone();
    std::thread::spawn(move || c.subscriber());
    let c = chat.clone();
    std::thread::spawn(move || {
        loop {
            // errors already show up as connected=0
            let _ = c.announce();
            c.update_presence();
            std::thread::sleep(PRESENCE_EVERY);
        }
    });

    ui.run(&[ui_file])
}
