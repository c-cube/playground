# ocaml-embedded-tk

An OCaml port of `tkhost` from `../2026-10-01-rust-embedded-tk`, plus a chat client
for that project's Rust daemon, using the same `ui.tcl`.

```sh
make daemon   # starts the Rust daemon (in ../2026-10-01-rust-embedded-tk) on /tmp/chat.sock
make client   # opens an OCaml GUI client in the background
_build/default/src/main.exe -a NAME   # -s PATH to use another socket than /tmp/chat.sock
```

## tkhost

`tkhost/` hosts a Tcl/Tk UI in the same process. OCaml and Tcl exchange messages
over a socketpair that Tcl watches with `fileevent`, so nothing polls. Apart from
the single `ocaml::cmd` command, OCaml never touches the interpreter after startup.
`prelude.tcl` holds the whole Tcl side. It matches the Rust version's, except that
the namespace is `ocaml::` instead of `rs::`, so `src/ui.tcl` is a copy of the Rust
UI with the namespace renamed.

- **Primitives:** `add_prim1 t "f" (fun a -> ...)`, `add_prim2 t "g" (fun a b -> ...)`
  and `add_prim t "h" (fun args -> ...)` all go into one table. Tcl can call any of
  them two ways:
  - `ocaml::cmd name args...` is sync. It runs on the Tcl thread and returns the result,
    or raises a Tcl error. The UI freezes while it runs, so use it for quick, pure
    functions.
  - `ocaml::call name args... callback` is async. It runs on a worker thread, and the
    callback gets the result.

  Results are strings. Build Tcl lists and dicts with `Tkhost.Tcl.list`/`dict`, or
  convert JSON with `Tkhost.Tcl.of_json`. Raise `Failure` to return an error. A wrong
  number of arguments is reported automatically.
- **Messages:** `ocaml::send name args...` goes to `Tkhost.on`, and
  `Tkhost.emit t name payload` goes to `ocaml::on name cmd`.
- **Two dicts, each written by one side:** `Tkhost.set t k v` shows up in
  `ocaml::state(k)` (bind widgets with `-textvariable`, react with
  `ocaml::watch pattern cmd`). Writes to `ocaml::ui(k)` show up in `Tkhost.tcl_get` and
  `Tkhost.on_tcl_change`.
- **Hot reload:** `{ Tkhost.data; path }`. The rule in `src/dune` embeds `ui.tcl`
  and records the path of its source copy (not the one in `_build`). The file is
  re-sourced when its mtime changes, and the embedded copy is the fallback.

### Threads and the C stub

`tkhost_stubs.c` is the only C. It starts Tcl/Tk, gives Tcl one end of the
socketpair, registers `ocaml::cmd`, and runs `Tk_MainLoop`. All Tcl code runs with the
OCaml runtime released, so the reader and worker threads keep running. `ocaml::cmd`
re-acquires the runtime to call back into OCaml.

It includes `<tcl.h>` (Fedora: `tcl8-devel`) but not `<tk.h>`, whose X11 headers
clash with OCaml's `Atom` macro. The two Tk functions it needs are declared by hand.
It links against `libtcl8.6` and `libtk8.6`.

## The chat client

`src/gui.ml` is a port of the Rust `gui.rs`:
- two daemon connections: request/reply, and a subscription
- reconnecting: one retry on an IO error, then a new attempt every 10s
- presence announced every 5s, and an author marked absent after 40s of silence
- primitives `channels`, `messages`, `post` and `author_color`

`author_color` uses the same FNV-1a hash as the Rust client, so an author gets the
same color in both clients.
