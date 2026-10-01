//! Tk client: Rust owns the sockets, Tcl (`ui.tcl`) owns the widgets.
//!
//! Commands exposed to Tcl:
//! - `chat::author`                           → our name
//! - `chat::channels`                         → list of channel names
//! - `chat::messages chan ?before? ?limit?`   → list of msg dicts, chronological
//! - `chat::post chan msg`                    → posts, returns the msg dict
//! - `chat::set_chans chans`                  → channels we announce presence in
//! - `chat::present chan`                     → authors seen in `chan` recently
//!
//! Pushed events are drained by `chat::_pump` (on a timer), which calls
//! `ui::on_msg $dict` for each message, and `ui::on_presence` whenever the
//! set of present authors changes.
//! `chat::_presence` announces our presence (every 5s).
//! `chat::_reload` re-sources `ui.tcl` if its mtime changed.
//! Both connections reconnect on their own if the daemon restarts.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::{CStr, c_int};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use tcl::Obj;

use crate::proto::{Conn, Msg, Push, Req, Resp};

const PRESENCE_TIMEOUT: Duration = Duration::from_secs(40);
/// After a failed connect, wait this long before trying again.
const RETRY_EVERY: Duration = Duration::from_secs(10);

const EMBEDDED_UI: &str = include_str!("ui.tcl");

/// Installed once; the reloadable `ui.tcl` must not start timers itself.
const BOOTSTRAP: &str = r#"
proc chat::_loop {ms cmd} {
    if {[catch $cmd err]} { puts stderr "$cmd: $err" }
    after $ms [list chat::_loop $ms $cmd]
}
chat::_loop 100 chat::_pump
chat::_loop 2000 chat::_reload
chat::_loop 5000 chat::_presence
"#;

struct Seen {
    chans: Vec<String>,
    at: Instant,
}

/// chan -> authors present in it
type PresenceSnapshot = BTreeMap<String, BTreeSet<String>>;

struct Gui {
    sock: PathBuf,
    author: String,
    /// `None` after an IO error; reconnected on next call
    ctrl: RefCell<Option<Conn>>,
    /// no connect attempt before this (set after a failed connect)
    ctrl_retry_at: Cell<Option<Instant>>,
    pushed: mpsc::Receiver<Event>,
    /// what we announce
    my_chans: RefCell<Vec<String>>,
    /// last presence broadcast received, per author
    seen: RefCell<HashMap<String, Seen>>,
    /// last snapshot handed to Tcl, to detect changes
    presence: RefCell<PresenceSnapshot>,
    ui_path: PathBuf,
    ui_mtime: Cell<Option<SystemTime>>,
}

enum Event {
    Push(Push),
    Reconnected,
}

// ---- tiny binding layer over the raw Tcl C API ----

type Interp = *mut clib::Tcl_Interp;
type Cmd = Box<dyn Fn(Interp, &[Obj]) -> Result<Obj>>;

extern "C" fn trampoline(
    cd: clib::ClientData,
    interp: Interp,
    objc: c_int,
    objv: *const *mut clib::Tcl_Obj,
) -> c_int {
    let f = unsafe { &*(cd as *const Cmd) };
    let args: Vec<Obj> = (1..objc as usize).map(|i| unsafe { Obj::from_raw(*objv.add(i)) }).collect();
    let (code, res) = match std::panic::catch_unwind(AssertUnwindSafe(|| f(interp, &args))) {
        Ok(Ok(o)) => (clib::TCL_OK, o),
        Ok(Err(e)) => (clib::TCL_ERROR, Obj::from(format!("{e:#}"))),
        Err(_) => (clib::TCL_ERROR, Obj::from("panic in rust command")),
    };
    // Tcl_SetObjResult takes its own reference; ours is dropped with `res`
    unsafe { clib::Tcl_SetObjResult(interp, res.as_ptr()) };
    code as c_int
}

extern "C" fn delete_cmd(cd: clib::ClientData) {
    drop(unsafe { Box::from_raw(cd as *mut Cmd) });
}

fn def_cmd(interp: Interp, name: &str, f: impl Fn(Interp, &[Obj]) -> Result<Obj> + 'static) {
    let cmd: Box<Cmd> = Box::new(Box::new(f));
    let name = std::ffi::CString::new(name).unwrap();
    unsafe {
        clib::Tcl_CreateObjCommand(
            interp,
            name.as_ptr(),
            Some(trampoline),
            Box::into_raw(cmd) as clib::ClientData,
            Some(delete_cmd),
        );
    }
}

/// Evaluate at global level; on error, return the Tcl `errorInfo`.
fn eval(interp: Interp, script: impl Into<Obj>) -> Result<Obj> {
    let script: Obj = script.into();
    let code = unsafe { clib::Tcl_EvalObjEx(interp, script.as_ptr(), clib::TCL_EVAL_GLOBAL as c_int) };
    if code == clib::TCL_OK as c_int {
        return Ok(unsafe { Obj::from_raw(clib::Tcl_GetObjResult(interp)) });
    }
    let info = unsafe {
        let p = clib::Tcl_GetVar2(interp, c"errorInfo".as_ptr(), std::ptr::null(), clib::TCL_GLOBAL_ONLY as c_int);
        if p.is_null() {
            Obj::from_raw(clib::Tcl_GetObjResult(interp)).to_string()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    Err(anyhow!("tcl error: {info}"))
}

fn msg_to_dict(m: &Msg) -> Obj {
    Obj::from(vec![
        Obj::from("id"),
        Obj::from(m.id as i64),
        Obj::from("chan"),
        Obj::from(m.chan.as_str()),
        Obj::from("author"),
        Obj::from(m.author.as_str()),
        Obj::from("msg"),
        Obj::from(m.msg.as_str()),
        Obj::from("ts"),
        Obj::from(m.ts as i64),
    ])
}

fn arity(args: &[Obj], min: usize, max: usize, usage: &str) -> Result<()> {
    if args.len() < min || args.len() > max {
        bail!("wrong # args: should be \"{usage}\"");
    }
    Ok(())
}

// ---- the chat commands ----

impl Gui {
    /// Request/response on the control connection; on an IO error, reconnect
    /// and retry once.
    fn call(&self, req: Req) -> Result<Resp> {
        let mut ctrl = self.ctrl.borrow_mut();
        for attempt in 0..2 {
            if ctrl.is_none() {
                if let Some(t) = self.ctrl_retry_at.get() {
                    let now = Instant::now();
                    if now < t {
                        bail!("disconnected, retrying in {}s", (t - now).as_secs() + 1);
                    }
                }
                match Conn::connect(&self.sock) {
                    Ok(c) => {
                        self.ctrl_retry_at.set(None);
                        *ctrl = Some(c);
                    }
                    Err(e) => {
                        self.ctrl_retry_at.set(Some(Instant::now() + RETRY_EVERY));
                        return Err(e);
                    }
                }
            }
            match ctrl.as_mut().unwrap().call(&req) {
                Err(e) if e.downcast_ref::<std::io::Error>().is_some() => {
                    *ctrl = None;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
                r => return r,
            }
        }
        unreachable!()
    }

    fn set_chans(&self, args: &[Obj]) -> Result<Obj> {
        arity(args, 1, 1, "chat::set_chans chans")?;
        let chans = Vec::<String>::try_from(args[0].clone()).map_err(|_| anyhow!("expected a list"))?;
        *self.my_chans.borrow_mut() = chans;
        self.announce()
    }

    fn announce(&self) -> Result<Obj> {
        let chans = self.my_chans.borrow().clone();
        self.call(Req::Presence { author: self.author.clone(), chans })?;
        Ok(Obj::new())
    }

    fn present(&self, args: &[Obj]) -> Result<Obj> {
        arity(args, 1, 1, "chat::present chan")?;
        let p = self.presence.borrow();
        let authors: Vec<String> = p.get(&args[0].to_string()).into_iter().flatten().cloned().collect();
        Ok(Obj::from(authors))
    }

    fn presence_snapshot(&self) -> PresenceSnapshot {
        let mut seen = self.seen.borrow_mut();
        seen.retain(|_, s| s.at.elapsed() <= PRESENCE_TIMEOUT);
        let mut snap = PresenceSnapshot::new();
        for (author, s) in seen.iter() {
            for c in &s.chans {
                snap.entry(c.clone()).or_default().insert(author.clone());
            }
        }
        snap
    }

    fn channels(&self) -> Result<Obj> {
        match self.call(Req::ListChans)? {
            Resp::Chans { chans } => Ok(Obj::from(chans)),
            r => bail!("unexpected response {r:?}"),
        }
    }

    fn messages(&self, args: &[Obj]) -> Result<Obj> {
        arity(args, 1, 3, "chat::messages chan ?before? ?limit?")?;
        let chan = args[0].to_string();
        let before = match args.get(1).map(|o| o.to_string()) {
            None => None,
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.parse::<u64>().with_context(|| format!("bad id {s:?}"))?),
        };
        let limit = match args.get(2) {
            None => 50,
            Some(o) => o.to_string().parse::<usize>().context("bad limit")?,
        };
        match self.call(Req::ListMsgs { chan, before, limit })? {
            Resp::Msgs { msgs } => Ok(Obj::from(msgs.iter().map(msg_to_dict).collect::<Vec<_>>())),
            r => bail!("unexpected response {r:?}"),
        }
    }

    fn post(&self, args: &[Obj]) -> Result<Obj> {
        arity(args, 2, 2, "chat::post chan msg")?;
        let req = Req::Post { chan: args[0].to_string(), author: self.author.clone(), msg: args[1].to_string() };
        match self.call(req)? {
            Resp::Posted { msg } => Ok(msg_to_dict(&msg)),
            r => bail!("unexpected response {r:?}"),
        }
    }

    /// Hand pushed events over to Tcl.
    fn pump(&self, interp: Interp) -> Result<Obj> {
        while let Ok(ev) = self.pushed.try_recv() {
            match ev {
                Event::Push(Push::Msg(m)) => {
                    let call = Obj::from(vec![Obj::from("ui::on_msg"), msg_to_dict(&m)]);
                    if let Err(e) = eval(interp, call) {
                        eprintln!("ui::on_msg: {e:#}");
                    }
                }
                Event::Push(Push::Presence { author, chans }) => {
                    self.seen.borrow_mut().insert(author, Seen { chans, at: Instant::now() });
                }
                Event::Reconnected => {
                    // the daemon is back, no need to wait for the control connection
                    self.ctrl_retry_at.set(None);
                    let _ = self.announce();
                    if let Err(e) = eval(interp, "ui::on_reconnect") {
                        eprintln!("ui::on_reconnect: {e:#}");
                    }
                }
            }
        }
        let snap = self.presence_snapshot();
        if snap != *self.presence.borrow() {
            *self.presence.borrow_mut() = snap;
            if let Err(e) = eval(interp, "ui::on_presence") {
                eprintln!("ui::on_presence: {e:#}");
            }
        }
        Ok(Obj::new())
    }

    /// Source `ui.tcl` from disk if it changed, falling back to the embedded copy.
    fn load_ui(&self, interp: Interp, force: bool) -> Result<Obj> {
        let mtime = std::fs::metadata(&self.ui_path).and_then(|m| m.modified()).ok();
        if !force && mtime == self.ui_mtime.get() {
            return Ok(Obj::new());
        }
        self.ui_mtime.set(mtime);
        let from_disk = match std::fs::read_to_string(&self.ui_path) {
            Ok(src) => match eval(interp, src) {
                Ok(_) => {
                    eprintln!("loaded {}", self.ui_path.display());
                    true
                }
                Err(e) => {
                    eprintln!("error in {}: {e:#}", self.ui_path.display());
                    false
                }
            },
            Err(e) => {
                if force {
                    eprintln!("can't read {}: {e}", self.ui_path.display());
                }
                false
            }
        };
        if !from_disk {
            eprintln!("using embedded ui.tcl");
            eval(interp, EMBEDDED_UI).context("embedded ui.tcl")?;
        }
        Ok(Obj::new())
    }
}

fn random_name() -> String {
    const ADJ: &[&str] = &["sleepy", "brave", "fuzzy", "grumpy", "shiny", "quiet", "loud", "tiny"];
    const ANIMAL: &[&str] = &["otter", "camel", "lynx", "heron", "yak", "newt", "badger", "gecko"];
    let a = ADJ[rand::random_range(0..ADJ.len())];
    let b = ANIMAL[rand::random_range(0..ANIMAL.len())];
    format!("{a}-{b}-{}", rand::random_range(0..100))
}

/// Keep a subscription open, reconnecting forever.
fn subscriber(sock: PathBuf, tx: mpsc::Sender<Event>) {
    let mut first = true;
    let mut warned = false;
    loop {
        match Conn::connect(&sock).and_then(|c| c.subscribe()) {
            Ok(events) => {
                // the UI may have been drawn while disconnected: refresh it
                if !first {
                    eprintln!("resubscribed");
                    if tx.send(Event::Reconnected).is_err() {
                        return;
                    }
                }
                warned = false;
                for ev in events {
                    match ev {
                        Ok(ev) => {
                            if tx.send(Event::Push(ev)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            eprintln!("subscription: {e:#}");
                            break;
                        }
                    }
                }
                eprintln!("subscription closed, reconnecting…");
                first = false;
                continue; // retry right away, then every RETRY_EVERY
            }
            Err(e) => {
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

pub fn run(sock: &Path, author: Option<String>, ui_path: PathBuf) -> Result<()> {
    // no daemon yet is fine: we keep retrying
    let ctrl = Conn::connect(sock).inspect_err(|e| eprintln!("{e:#}")).ok();
    let (tx, rx) = mpsc::channel();
    let sock2 = sock.to_owned();
    std::thread::spawn(move || subscriber(sock2, tx));

    let gui = Rc::new(Gui {
        sock: sock.to_owned(),
        author: author.unwrap_or_else(random_name),
        ctrl_retry_at: Cell::new(ctrl.is_none().then(|| Instant::now() + RETRY_EVERY)),
        ctrl: RefCell::new(ctrl),
        pushed: rx,
        my_chans: RefCell::new(vec![]),
        seen: RefCell::new(HashMap::new()),
        presence: RefCell::new(PresenceSnapshot::new()),
        ui_path,
        ui_mtime: Cell::new(None),
    });
    let tk = tk::Tk::new(|| ()).map_err(|e| anyhow!("tk init: {e:?}"))?;
    let interp = tk.as_ptr();
    eval(interp, "namespace eval chat {}")?;

    let g = gui.clone();
    def_cmd(interp, "chat::author", move |_, args| {
        arity(args, 0, 0, "chat::author")?;
        Ok(Obj::from(g.author.as_str()))
    });
    let g = gui.clone();
    def_cmd(interp, "chat::channels", move |_, args| {
        arity(args, 0, 0, "chat::channels")?;
        g.channels()
    });
    let g = gui.clone();
    def_cmd(interp, "chat::messages", move |_, args| g.messages(args));
    let g = gui.clone();
    def_cmd(interp, "chat::post", move |_, args| g.post(args));
    let g = gui.clone();
    def_cmd(interp, "chat::set_chans", move |_, args| g.set_chans(args));
    let g = gui.clone();
    def_cmd(interp, "chat::present", move |_, args| g.present(args));
    let g = gui.clone();
    def_cmd(interp, "chat::_presence", move |_, _| g.announce());
    let g = gui.clone();
    def_cmd(interp, "chat::_pump", move |interp, _| g.pump(interp));
    let g = gui.clone();
    def_cmd(interp, "chat::_reload", move |interp, _| g.load_ui(interp, false));

    gui.load_ui(interp, true)?;
    eval(interp, BOOTSTRAP)?;
    tk::main_loop();
    Ok(())
}
