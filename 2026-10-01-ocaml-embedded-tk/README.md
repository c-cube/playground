# ocaml-embedded-tk

An OCaml take on `tkhost` from `../2026-10-01-rust-embedded-tk`, plus a chat client
for that project's Rust daemon. `src/ui.tcl` started as a copy of the Rust UI.

```sh
make daemon   # starts the Rust daemon (in ../2026-10-01-rust-embedded-tk) on /tmp/chat.sock
make client   # opens an OCaml GUI client in the background
_build/default/src/main.exe -a NAME   # -s PATH to use another socket than /tmp/chat.sock
```

## tkhost

`tkhost/` hosts a Tcl/Tk UI in the same process. `prelude.tcl` holds the whole Tcl
side.

- **Primitives (Tcl → OCaml):** each one becomes a Tcl command `ocaml::NAME`.
  - `add_prim1 t "f" (fun a -> ...)`, `add_prim2`, and the variadic `add_prim` are
    sync. They run on the Tcl thread and return the result, or raise a Tcl error.
    The UI freezes while they run, so keep them quick.
  - With `~async:true`, `add_prim ~async:true t "g" f` makes `ocaml::g args... callback`.
    It returns at once, and `f` runs on a worker thread, one call at a time, in
    order. The callback gets the result; errors go to stderr.

  Results are strings: build lists and dicts with `Tkhost.Tcl.list`/`dict`. Raise
  `Failure` to return an error.
- **One queue (OCaml → Tcl)**, drained by Tcl every 10ms. It carries:
  - events: `Tkhost.emit t name payload` runs `ocaml::on name cmd`
  - replies to async calls
  - changes to the two dicts below
- **Two dicts, written by OCaml, read-only in Tcl:**
  - `Tkhost.set t k v` sets `ocaml::state(k)`. Bind widgets with `-textvariable`, or
    react with `ocaml::watch pattern cmd`. It survives a hot reload.
  - `Tkhost.pset` does the same for `ocaml::pstate(k)`, which is also saved to the
    JSON file given to `create ~pstate`, so it survives a restart.

  When OCaml needs something from the UI, Tcl calls a primitive.
- **Hot reload:** `{ Tkhost.data; path }`. The rule in `src/dune` embeds `ui.tcl`
  and records the path of its source copy (not the one in `_build`). The file is
  re-sourced when its mtime changes, and the embedded copy is the fallback.

### Threads and the C stub

`tkhost_stubs.c` is the only C. It starts Tcl/Tk, registers `ocaml::_cmd` (the one
command every primitive is an alias of, also used to poll the queue), evaluates
the prelude, and runs `Tk_MainLoop`. All Tcl code runs with the OCaml runtime
released, so OCaml threads keep running; `ocaml::_cmd` re-acquires it.

It includes `<tcl.h>` (Fedora: `tcl8-devel`) but not `<tk.h>`, whose X11 headers
clash with OCaml's `Atom` macro. The two Tk functions it needs are declared by hand.
It links against `libtcl8.6` and `libtk8.6`.

## The chat client

`src/gui.ml` keeps the OCaml side small: it only talks to the daemon, and `ui.tcl`
does the rest. The primitives, events and state are listed at the top of `gui.ml`.
- a request/reply connection to the daemon, reopened when it breaks
- a subscription, reconnected every 10s, whose pushes are forwarded as events
- presence: `ui.tcl` announces itself every 5s, and keeps the authors heard from in
  the last 40s
- when `connected` flips to 1, `ui.tcl` reloads the channels and history
- with `--state FILE`, the current and joined channels are kept in `ocaml::pstate`
  across restarts

While the daemon can't be reached, posts are queued, up to 50; any more are refused
with an error. Every request that gets through sends the queued posts first, in
order. A status indicator in the top right shows a green or red dot for the
connection, and the number of queued posts if there are any.

`author_color` uses the same FNV-1a hash as the Rust client, so an author gets the
same color in both clients.
