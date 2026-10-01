(** Host a Tcl/Tk UI in the same process, talking to it only through
    messages, a table of primitives, and two dictionaries:

    - host → Tcl: events ({!emit}) and replies to calls.
    - Tcl → host: primitives ({!add_prim} and friends), called either async
      ([ocaml::call], on a worker thread, result via callback) or sync
      ([ocaml::cmd], right away on the Tcl thread, so only for quick ones), and
      fire-and-forget messages ({!on}).
    - our dict ({!set}), mirrored read-only in Tcl as [ocaml::state].
    - Tcl's dict [ocaml::ui], mirrored read-only here ({!tcl_get}).

    UI files are re-sourced when they change on disk, with the copy embedded
    at build time as a fallback. The Tcl side of the API is documented in
    [prelude.tcl].

    Both ends share a socketpair that Tcl reads with [fileevent], so nothing
    polls. Apart from [ocaml::cmd], OCaml never touches the interpreter after
    startup. Values are strings; build Tcl lists and dicts with {!Tcl}. *)

module Tcl = Tcl

(** A Tcl file embedded in the binary, reloaded from [path] whenever that file
    changes. [data] is used if [path] can't be read or fails. See [src/dune]
    for a rule that generates one. *)
type tcl_file = { data : string; path : string }

type t

val create : unit -> t

(** {2 Primitives}

    All three share one table: a name is callable as [ocaml::cmd name args...]
    and [ocaml::call name args... callback]. Raise [Failure msg] (or anything
    else) to return an error. A wrong number of arguments is an error too. *)

val add_prim : t -> string -> (string list -> string) -> unit
val add_prim1 : t -> string -> (string -> string) -> unit
val add_prim2 : t -> string -> (string -> string -> string) -> unit

(** Receive [ocaml::send name args...]. Runs on the worker thread. *)
val on : t -> string -> (string list -> unit) -> unit

(** {2 Dictionaries} *)

(** [set t k v] sets [ocaml::state(k)]. Does nothing if unchanged. *)
val set : t -> string -> string -> unit

val unset : t -> string -> unit

(** Current value of [ocaml::state(k)], as Tcl sees it. *)
val get : t -> string -> string option

(** Current value of [ocaml::ui(k)]. *)
val tcl_get : t -> string -> string option

(** Called (on the worker thread) when Tcl sets ([Some v]) or unsets ([None])
    a key of [ocaml::ui]. *)
val on_tcl_change : t -> (string -> string option -> unit) -> unit

(** {2 Events} *)

(** Run the [ocaml::on name] command with this payload. *)
val emit : t -> string -> string -> unit

(** Start Tk, source [files] in order, and run the event loop until the main
    window is closed. Call it once, from the main thread. Primitives and
    listeners can still be added afterwards, from any thread. *)
val run : t -> tcl_file list -> unit
