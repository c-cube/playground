(** Host a Tcl/Tk UI in the same process.

    - Tcl → OCaml: primitives. Each one is a Tcl command [ocaml::NAME args...],
      either sync (runs on the Tcl thread, returns the result) or async (takes a
      callback as last argument, runs on a worker thread).
    - OCaml → Tcl: one queue, drained by Tcl every 10ms. It carries events
      ({!emit}, run by [ocaml::on]), replies to async calls, and changes to two
      dicts that only OCaml writes and Tcl reads: [ocaml::state] ({!set}) and
      [ocaml::pstate] ({!pset}), which is also saved to disk.

    UI files are re-sourced when they change on disk, with the copy embedded
    at build time as a fallback. [ocaml::state] survives that; [ocaml::pstate]
    also survives a restart. The Tcl side of the API is in [prelude.tcl].

    Rules for primitives: sync ones must be quick, and never wait for anything
    that needs the Tcl thread. Anything doing IO should be async. Raise
    [Failure msg] (or anything else) to return an error. *)

(** Writing Tcl lists and dicts, for results and payloads. *)
module Tcl = Tcl

(** A Tcl file embedded in the binary, reloaded from [path] whenever that file
    changes. [data] is used if [path] can't be read or fails. See [src/dune]
    for a rule that generates one. *)
type tcl_file = { data : string; path : string }

type t

(** [pstate] is the JSON file [ocaml::pstate] is loaded from and saved to.
    Without it, [ocaml::pstate] lives in memory only. *)
val create : ?pstate:string -> unit -> t

(** {2 Primitives}

    [add_prim t name f] makes [ocaml::NAME args...], which runs [f] on the Tcl
    thread and returns its result.

    With [~async:true], it's [ocaml::NAME args... callback] instead: it returns
    at once, [f] runs on the worker thread (one call at a time, in order), then
    [{*}$callback $result] runs on success ("" = ignore it). Errors are printed
    to stderr.

    Names must not start with [_] or contain [:], and [on] and [watch] are
    taken. Primitives can be added at any time, from any thread. *)

val add_prim : ?async:bool -> t -> string -> (string list -> string) -> unit

(** Same, with exactly one or two arguments. *)
val add_prim1 : ?async:bool -> t -> string -> (string -> string) -> unit

val add_prim2 : ?async:bool -> t -> string -> (string -> string -> string) -> unit

(** {2 Events} *)

(** Run the [ocaml::on name] command with this payload. *)
val emit : t -> string -> string -> unit

(** {2 Dictionaries} *)

(** [set t k v] sets [ocaml::state(k)]. Does nothing if unchanged. *)
val set : t -> string -> string -> unit

val unset : t -> string -> unit
val get : t -> string -> string option

(** Same for [ocaml::pstate], saved to the [pstate] file on each change. *)
val pset : t -> string -> string -> unit

val punset : t -> string -> unit
val pget : t -> string -> string option

(** The message for an exception, readable for [Failure] and [Unix_error]. *)
val error_message : exn -> string

(** Start Tk, source [files] in order, and run the event loop until the main
    window is closed. Call it once, from the main thread. *)
val run : t -> tcl_file list -> unit
