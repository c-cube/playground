# rust-embedded-tk

A tiny chat that runs over a unix socket. Rust handles networking and JSON; the UI is
a Tcl/Tk script embedded in the binary with `include_str!`. That script talks to the
chat through a few Rust functions registered as Tcl commands.

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
- `src/gui.rs`: a small trampoline over the raw Tcl C API (`Tcl_CreateObjCommand`)
  that registers Rust closures as Tcl commands. Rust errors become Tcl errors.
  - The GUI opens two connections: one for request/reply, and one subscription that a
    reader thread forwards into an `mpsc` channel.
  - A Tcl `after` timer calls `chat::_pump`, which drains that channel and calls
    `ui::on_msg $dict` for each message.
  - Every 5s the client announces `presence {author, chans}` for its joined channels.
    Each client records when it last heard from each author and drops authors that
    have been silent for more than 40s. When the set of present authors changes, it
    calls `ui::on_presence`.
  - Both connections reconnect by themselves. A write that fails with `EPIPE`, or an
    EOF, drops the connection, and the request is retried once on a fresh one. If
    connecting fails, the client tries again every 10s. That also means a client can
    start before the daemon does. After resubscribing, `ui::on_reconnect` redraws
    the UI.
- `src/ui.tcl`: the whole UI. Every 2s the GUI checks the file's mtime and re-sources
  it if it changed. If the file has an error, the GUI falls back to the embedded copy.
  The script must stay idempotent: it destroys `.main` first and keeps its state in
  `namespace ui`.

Tcl commands available to `ui.tcl`:

```
chat::author
chat::channels
chat::messages chan ?before_id? ?limit?   ;# list of dicts, oldest first
chat::post chan msg
chat::set_chans chans   ;# channels to announce presence in
chat::present chan      ;# authors seen in chan within the last 40s
```

## Make

```sh
make daemon   # start a daemon in the background, unless one already answers
make client   # open a new GUI client in the background
make stop     # stop the daemon that `make daemon` started
```

## Build notes

- The `tk`/`tcl` crates only support Tcl/Tk **8.6**. On Fedora, install
  `tcl8-devel tk8-devel`; they replace the 9.0 devel packages.
- `.cargo/config.toml` sets `BINDGEN_EXTRA_CLANG_ARGS` so bindgen can find
  clang's `stddef.h`.
- The commands are registered with our own trampoline rather than `tcl::tclosure!`,
  because that macro returns `TCL_ERROR` without an error message.
