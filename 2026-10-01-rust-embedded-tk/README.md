# rust-embedded-tk

A tiny chat that runs over a unix socket. Rust handles networking and JSON; the UI is
a Tcl/Tk script embedded in the binary (and hot-reloaded from disk). The two talk
through `tkhost`, a small crate in this repo: messages and two shared dicts.

```sh
cargo build
target/debug/tkchat daemon                    # binds ./chat.sock
target/debug/tkchat gui                       # random author name, or -a NAME
target/debug/tkchat send general "hello"      # one-shot post, author = $USER
for i in $(seq 10); do target/debug/tkchat send spam "msg $i"; done
```

Every subcommand takes `-s/--sock PATH` (default `./chat.sock`).

## Pieces

- `src/proto.rs`: `Msg { id, chan, author, msg, ts }`, plus the request/response enums.
  The wire format is JSON lines, flushed after each line.
- `src/daemon.rs`: an in-memory `HashMap<chan, VecDeque<Msg>>` capped at 1000 messages
  per channel. One thread per client. The daemon assigns `id` and `ts` (unix ms). A
  connection that sends `subscribe` gets every new message pushed to it. The daemon
  doesn't store presence: it relays each `presence` request to every subscriber.
- `src/gui.rs`: the client's Rust side, hosted with `tkhost` (below). It keeps two
  daemon connections: one for request/reply, and one subscription whose pushes it
  forwards to the UI.
  - Every 5s the client announces `presence {author, chans}` for its joined channels.
    Each client records when it last heard from each author and drops authors that
    have been silent for more than 40s.
  - Both connections reconnect by themselves. A write that fails with `EPIPE`, or an
    EOF, drops the connection, and the request is retried once on a fresh one. If
    connecting fails, the client tries again every 10s. That also means a client can
    start before the daemon does.
- `src/ui.tcl`: the whole UI. It's hot-reloaded: when the file changes it is
  re-sourced, and if it fails the embedded copy is used. The script must stay
  idempotent: it destroys `.main` first and keeps its state in `namespace ui`.
  Unread channels get a red dot; Alt+Up/Alt+Down switch channels.

What the UI and Rust exchange:

```
rs::call channels | messages chan ?before_id? ?limit? | post chan msg
rs::cmd  author_color name   ;# sync: a stable color per author
rs::on    msg {dict} | reconnect
rs::state(author) | rs::state(connected) | rs::state(here:CHAN)   ;# set by Rust
rs::ui(joined)    ;# set by Tcl: the channels to announce presence in
```

## tkhost

`tkhost/` is a small crate that hosts a Tcl/Tk UI in the same process. Rust and Tcl
exchange messages over a socketpair that Tcl watches with `fileevent`, so nothing
polls. Apart from the single `rs::cmd` command, Rust never touches the interpreter
after startup.

- Calls: Rust handlers (`ui.handle`) can be called two ways, and the caller picks:
  - `rs::call name args... callback` is async. It runs the handler on a worker
    thread, and the callback gets the result. Errors are printed.
  - `rs::cmd name args...` is sync. It runs the handler right away on the Tcl
    thread and returns the result, or raises a Tcl error. The UI freezes while it
    runs, so use it for quick, pure functions.

  Results are converted to Tcl: structs become dicts, `Vec`s become lists.
- Messages: `rs::send name args...` goes to `ui.on(name, ...)`, and `ui.emit(name, payload)`
  goes to `rs::on name cmd`.
- Two dicts, each written by one side: `ui.set(k, v)` shows up in `rs::state(k)`
  (bind widgets to it with `-textvariable`, react with `rs::watch pattern cmd`).
  Writes to `rs::ui(k)` show up in `ui.tcl_get(k)` and `ui.on_tcl_change`.
- `tcl_file!("src/ui.tcl")` embeds the file and remembers its path. The file is
  re-sourced when its mtime changes (checked every 2s), and the embedded copy is
  the fallback.

The Tcl side is documented in `tkhost/src/prelude.tcl`, and `cargo run -p tkhost
--example hello` shows every feature.

## Make

```sh
make daemon   # start a daemon in the background, unless one already answers
make client   # open a new GUI client in the background
make stop     # stop the daemon that `make daemon` started
```

## Build notes

- `tkhost` declares the ~10 Tcl/Tk C functions it uses by hand and links
  `libtcl8.6`/`libtk8.6` (Fedora: `tcl8 tk8`). There's no bindgen and no need for
  the devel packages.
