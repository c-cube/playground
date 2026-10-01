//! Host a Tcl/Tk UI in the same process, talking to it only through
//! messages and two dictionaries:
//!
//! - Rust → Tcl: events ([`Ui::emit`]) and replies to calls.
//! - Tcl → Rust: calls to [`Ui::handle`], either async (`rs::call`, run
//!   on a worker thread, result via callback) or sync (`rs::cmd`, run right
//!   away on the Tcl thread, so only for quick ones), and fire-and-forget
//!   messages ([`Ui::on`]).
//! - Rust's dict ([`Ui::set`]), mirrored read-only in Tcl as `rs::state`.
//! - Tcl's dict `rs::ui`, mirrored read-only in Rust ([`Ui::tcl_get`]).
//!
//! UI files are re-sourced when they change on disk, with the copy embedded
//! at compile time as a fallback (see [`tcl_file!`]). The Tcl side of the API
//! is documented in `prelude.tcl`.
//!
//! Both ends share a socketpair: Tcl reads it with `fileevent`, so nothing
//! polls. Apart from `rs::cmd`, Rust never touches the interpreter after
//! startup.

mod ffi;
pub mod wire;

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::IntoRawFd;
use std::os::unix::net::UnixStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, mpsc};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

pub use wire::parse_list;

const PRELUDE: &str = include_str!("prelude.tcl");

/// A Tcl file embedded in the binary that is reloaded from `path` whenever
/// that file changes. `data` is used if `path` can't be read or fails.
#[derive(Clone, Debug)]
pub struct TclFile {
    pub data: &'static str,
    pub path: PathBuf,
}

/// `tcl_file!("src/ui.tcl")`: path relative to the calling crate's root
/// (where its `Cargo.toml` is).
#[macro_export]
macro_rules! tcl_file {
    ($path:literal) => {
        $crate::TclFile {
            data: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
            path: ::std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
        }
    };
}

type Handler = Arc<dyn Fn(&[String]) -> Result<serde_json::Value> + Send + Sync>;
type Listener = Arc<dyn Fn(&[String]) + Send + Sync>;
type ChangeFn = Arc<dyn Fn(&str, Option<&str>) + Send + Sync>;

struct Inner {
    /// our end of the socketpair (write side)
    out: Mutex<UnixStream>,
    /// Tcl's end, handed over in `run`
    tcl_end: Mutex<Option<UnixStream>>,
    state: Mutex<HashMap<String, String>>,
    tcl_state: RwLock<HashMap<String, String>>,
    handlers: RwLock<HashMap<String, Handler>>,
    listeners: RwLock<HashMap<String, Listener>>,
    on_change: RwLock<Vec<ChangeFn>>,
}

/// Handle to the UI. Cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct Ui(Arc<Inner>);

enum Job {
    Call { id: String, name: String, args: Vec<String> },
    Send { name: String, args: Vec<String> },
    Changed { key: String, value: Option<String> },
}

impl Ui {
    pub fn new() -> Result<Ui> {
        let (ours, theirs) = UnixStream::pair()?;
        Ok(Ui(Arc::new(Inner {
            out: Mutex::new(ours),
            tcl_end: Mutex::new(Some(theirs)),
            state: Default::default(),
            tcl_state: Default::default(),
            handlers: Default::default(),
            listeners: Default::default(),
            on_change: Default::default(),
        })))
    }

    /// Answer `rs::call name args... callback` (async: on the worker thread,
    /// the callback gets the result, an error is printed) and
    /// `rs::cmd name args...` (sync: on the Tcl thread, returns the result,
    /// an error is a Tcl error). The result is converted to Tcl: objects
    /// become dicts, arrays lists.
    pub fn handle<R: Serialize>(&self, name: &str, f: impl Fn(&[String]) -> Result<R> + Send + Sync + 'static) {
        let h: Handler = Arc::new(move |args| Ok(serde_json::to_value(f(args)?)?));
        self.0.handlers.write().unwrap().insert(name.into(), h);
    }

    /// Receive `rs::send name args...`.
    pub fn on(&self, name: &str, f: impl Fn(&[String]) + Send + Sync + 'static) {
        self.0.listeners.write().unwrap().insert(name.into(), Arc::new(f));
    }

    /// Called when Tcl sets (`Some`) or unsets (`None`) a key of `rs::ui`.
    pub fn on_tcl_change(&self, f: impl Fn(&str, Option<&str>) + Send + Sync + 'static) {
        self.0.on_change.write().unwrap().push(Arc::new(f));
    }

    /// Current value of `rs::ui(key)`.
    pub fn tcl_get(&self, key: &str) -> Option<String> {
        self.0.tcl_state.read().unwrap().get(key).cloned()
    }

    /// Set `rs::state(key)`. Values go through [`wire::to_tcl`], so a `Vec`
    /// becomes a list and a struct a dict. Does nothing if unchanged.
    pub fn set(&self, key: &str, value: impl Serialize) {
        let v = match serde_json::to_value(value) {
            Ok(v) => wire::to_tcl(&v),
            Err(e) => return eprintln!("tkhost: can't serialize {key}: {e}"),
        };
        let mut st = self.0.state.lock().unwrap();
        if st.get(key) == Some(&v) {
            return;
        }
        // under the lock, so the wire order matches the map
        self.write(&wire::list(["set", key, &v]));
        st.insert(key.into(), v);
    }

    pub fn unset(&self, key: &str) {
        let mut st = self.0.state.lock().unwrap();
        if st.remove(key).is_some() {
            self.write(&wire::list(["unset", key]));
        }
    }

    /// Current value of `rs::state(key)`, as Tcl sees it.
    pub fn get(&self, key: &str) -> Option<String> {
        self.0.state.lock().unwrap().get(key).cloned()
    }

    /// Run the `rs::on name` command with this payload.
    pub fn emit(&self, name: &str, payload: impl Serialize) {
        match serde_json::to_value(payload) {
            Ok(v) => self.write(&wire::list(["event", name, &wire::to_tcl(&v)])),
            Err(e) => eprintln!("tkhost: can't serialize event {name}: {e}"),
        }
    }

    /// Writes after the UI is gone are dropped: the process is exiting anyway.
    fn write(&self, line: &str) {
        let mut out = self.0.out.lock().unwrap();
        let _ = out.write_all(format!("{line}\n").as_bytes());
    }

    /// Start Tk, source `files` in order, and run the event loop until the
    /// main window is closed. Must be called on the main thread, once.
    /// Handlers registered later still work.
    pub fn run(&self, files: &[TclFile]) -> Result<()> {
        let tcl_end = self.0.tcl_end.lock().unwrap().take().ok_or_else(|| anyhow!("Ui::run called twice"))?;
        let reader = BufReader::new(self.0.out.lock().unwrap().try_clone()?);
        let (tx, rx) = mpsc::channel();
        let ui = self.clone();
        std::thread::spawn(move || ui.read_loop(reader, tx));
        let ui = self.clone();
        std::thread::spawn(move || ui.work_loop(rx));

        let argv0 = CString::new(std::env::args().next().unwrap_or_default())?;
        unsafe {
            ffi::Tcl_FindExecutable(argv0.as_ptr());
            let interp = ffi::Tcl_CreateInterp();
            if ffi::Tcl_Init(interp) != ffi::TCL_OK {
                bail!("Tcl_Init: {}", ffi::result(interp));
            }
            if ffi::Tk_Init(interp) != ffi::TCL_OK {
                bail!("Tk_Init: {}", ffi::result(interp));
            }
            // Tcl owns the fd from now on and closes it with the channel
            let fd = tcl_end.into_raw_fd() as isize as *mut std::ffi::c_void;
            let chan = ffi::Tcl_MakeFileChannel(fd, ffi::TCL_READABLE | ffi::TCL_WRITABLE);
            ffi::Tcl_RegisterChannel(interp, chan);
            let name = CStr::from_ptr(ffi::Tcl_GetChannelName(chan)).to_string_lossy();
            // `rs::cmd` holds a reference to our state until the interp deletes it
            let cd = Arc::into_raw(self.0.clone()) as *mut std::ffi::c_void;
            ffi::Tcl_CreateObjCommand(interp, c"rs::cmd".as_ptr(), cmd_proc, cd, Some(cmd_delete));
            let files = wire::list(files.iter().map(|f| wire::list([&*f.path.to_string_lossy(), f.data])));
            let setup = format!(
                "namespace eval rs {{ variable chan {}; variable files {} }}",
                wire::quote(&name),
                wire::quote(&files)
            );
            ffi::eval(interp, &setup).context("tkhost setup")?;
            ffi::eval(interp, PRELUDE).context("tkhost prelude")?;
            ffi::Tk_MainLoop();
        }
        Ok(())
    }

    /// Parse lines from Tcl. `rs::ui` is updated right away, everything
    /// else is handed to the worker so handlers run in order.
    fn read_loop(&self, reader: BufReader<UnixStream>, tx: mpsc::Sender<Job>) {
        for line in reader.lines() {
            let Ok(line) = line else { return };
            let words: Vec<String> = match serde_json::from_str(&line) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("tkhost: bad line from tcl {line:?}: {e}");
                    continue;
                }
            };
            let job = match words.as_slice() {
                [kind, id, name, args @ ..] if kind == "call" => {
                    Job::Call { id: id.clone(), name: name.clone(), args: args.to_vec() }
                }
                [kind, name, args @ ..] if kind == "send" => Job::Send { name: name.clone(), args: args.to_vec() },
                [kind, k, v] if kind == "set" => {
                    let old = self.0.tcl_state.write().unwrap().insert(k.clone(), v.clone());
                    if old.as_ref() == Some(v) {
                        continue;
                    }
                    Job::Changed { key: k.clone(), value: Some(v.clone()) }
                }
                [kind, k] if kind == "unset" => {
                    if self.0.tcl_state.write().unwrap().remove(k).is_none() {
                        continue;
                    }
                    Job::Changed { key: k.clone(), value: None }
                }
                _ => {
                    eprintln!("tkhost: unknown message from tcl: {line}");
                    continue;
                }
            };
            if tx.send(job).is_err() {
                return;
            }
        }
    }

    fn work_loop(&self, rx: mpsc::Receiver<Job>) {
        for job in rx {
            match job {
                Job::Call { id, name, args } => {
                    match self.0.call(&name, &args) {
                        Ok(v) => self.write(&wire::list(["reply", &id, "ok", &wire::to_tcl(&v)])),
                        Err(e) => self.write(&wire::list(["reply", &id, "err", &format!("{e:#}")])),
                    }
                }
                Job::Send { name, args } => {
                    let l = self.0.listeners.read().unwrap().get(&name).cloned();
                    match l {
                        None => eprintln!("tkhost: nothing listens to {name:?}"),
                        Some(l) => {
                            if catch_unwind(AssertUnwindSafe(|| l(&args))).is_err() {
                                eprintln!("tkhost: listener {name:?} panicked");
                            }
                        }
                    }
                }
                Job::Changed { key, value } => {
                    let fs = self.0.on_change.read().unwrap().clone();
                    for f in fs {
                        if catch_unwind(AssertUnwindSafe(|| f(&key, value.as_deref()))).is_err() {
                            eprintln!("tkhost: on_tcl_change panicked");
                        }
                    }
                }
            }
        }
    }
}

impl Inner {
    fn call(&self, name: &str, args: &[String]) -> Result<serde_json::Value> {
        let h = self.handlers.read().unwrap().get(name).cloned();
        match h {
            None => Err(anyhow!("no handler named {name:?}")),
            Some(h) => catch_unwind(AssertUnwindSafe(|| h(args))).unwrap_or_else(|_| Err(anyhow!("handler panicked"))),
        }
    }
}

/// `rs::cmd name ?arg ...?`
unsafe extern "C" fn cmd_proc(
    cd: *mut std::ffi::c_void,
    interp: *mut ffi::Interp,
    objc: std::ffi::c_int,
    objv: *const *mut ffi::Obj,
) -> std::ffi::c_int {
    let inner = unsafe { &*(cd as *const Inner) };
    let args: Vec<String> = (1..objc as usize).map(|i| unsafe { ffi::obj_str(*objv.add(i)) }).collect();
    let res = match args.split_first() {
        None => Err(anyhow!("wrong # args: should be \"rs::cmd name ?arg ...?\"")),
        Some((name, args)) => inner.call(name, args),
    };
    let (code, s) = match res {
        Ok(v) => (ffi::TCL_OK, wire::to_tcl(&v)),
        Err(e) => (ffi::TCL_ERROR, format!("{e:#}")),
    };
    unsafe { ffi::set_result(interp, &s) };
    code
}

unsafe extern "C" fn cmd_delete(cd: *mut std::ffi::c_void) {
    drop(unsafe { Arc::from_raw(cd as *const Inner) });
}
