module Tcl = Tcl

type tcl_file = { data : string; path : string }

type job =
  | Call of string * string * string list  (** id, name, args *)
  | Send of string * string list
  | Changed of string * string option

type t = {
  lock : Mutex.t;  (** protects the tables below *)
  state : (string, string) Hashtbl.t;
  tcl_state : (string, string) Hashtbl.t;
  prims : (string, string list -> string) Hashtbl.t;
  listeners : (string, string list -> unit) Hashtbl.t;
  mutable on_change : (string -> string option -> unit) list;
  out_lock : Mutex.t;
  ours : Unix.file_descr;  (** our end of the socketpair *)
  theirs : Unix.file_descr;  (** Tcl's end, handed over in [run] *)
  oc : out_channel;
  jobs : job Queue.t;  (** for the worker, under [lock] *)
  jobs_cond : Condition.t;
  mutable started : bool;
}

external c_run : string -> Unix.file_descr -> string -> string -> unit = "tkhost_run"

let create () =
  (* a write to a closed socket is an error, not a reason to die *)
  Sys.set_signal Sys.sigpipe Sys.Signal_ignore;
  let ours, theirs = Unix.socketpair Unix.PF_UNIX Unix.SOCK_STREAM 0 in
  {
    lock = Mutex.create ();
    state = Hashtbl.create 16;
    tcl_state = Hashtbl.create 16;
    prims = Hashtbl.create 16;
    listeners = Hashtbl.create 16;
    on_change = [];
    out_lock = Mutex.create ();
    ours;
    theirs;
    oc = Unix.out_channel_of_descr ours;
    jobs = Queue.create ();
    jobs_cond = Condition.create ();
    started = false;
  }

let locked t f = Mutex.protect t.lock f

(* Writes after the UI is gone are dropped: the process is exiting anyway. *)
let write t line =
  Mutex.protect t.out_lock (fun () ->
      try
        output_string t.oc line;
        output_char t.oc '\n';
        flush t.oc
      with Sys_error _ -> ())

let add_prim t name f = locked t (fun () -> Hashtbl.replace t.prims name f)

let wrong_args name params =
  failwith (Printf.sprintf "wrong # args: should be \"%s %s\"" name params)

let add_prim1 t name f =
  add_prim t name (function [ a ] -> f a | _ -> wrong_args name "arg")

let add_prim2 t name f =
  add_prim t name (function [ a; b ] -> f a b | _ -> wrong_args name "arg1 arg2")

let on t name f = locked t (fun () -> Hashtbl.replace t.listeners name f)
let on_tcl_change t f = locked t (fun () -> t.on_change <- t.on_change @ [ f ])
let tcl_get t k = locked t (fun () -> Hashtbl.find_opt t.tcl_state k)
let get t k = locked t (fun () -> Hashtbl.find_opt t.state k)

(* under the lock, so the wire order matches the table *)
let set t k v =
  locked t (fun () ->
      if Hashtbl.find_opt t.state k <> Some v then begin
        write t (Tcl.list [ "set"; k; v ]);
        Hashtbl.replace t.state k v
      end)

let unset t k =
  locked t (fun () ->
      if Hashtbl.mem t.state k then begin
        write t (Tcl.list [ "unset"; k ]);
        Hashtbl.remove t.state k
      end)

let emit t name payload = write t (Tcl.list [ "event"; name; payload ])

let error_message = function
  | Failure m -> m
  | Unix.Unix_error (e, fn, arg) ->
    Printf.sprintf "%s%s: %s" fn (if arg = "" then "" else " " ^ arg) (Unix.error_message e)
  | e -> Printexc.to_string e

let call_prim t name args =
  match locked t (fun () -> Hashtbl.find_opt t.prims name) with
  | None -> Error (Printf.sprintf "no handler named %S" name)
  | Some f -> ( try Ok (f args) with e -> Error (error_message e))

(* `ocaml::cmd`, called from the C stub on the Tcl thread *)
let cmd t (args : string array) : bool * string =
  match Array.to_list args with
  | [] -> (false, "wrong # args: should be \"ocaml::cmd name ?arg ...?\"")
  | name :: args -> (
    match call_prim t name args with Ok r -> (true, r) | Error e -> (false, e))

let push t job =
  locked t (fun () ->
      Queue.push job t.jobs;
      Condition.signal t.jobs_cond)

(* Parse lines from Tcl. ocaml::ui is updated right away, everything else goes
   to the worker so handlers run in order. *)
let read_loop t =
  let ic = Unix.in_channel_of_descr t.ours in
  let rec loop () =
    match input_line ic with
    | exception (End_of_file | Sys_error _) -> ()
    | line ->
      (match Yojson.Safe.from_string line with
       | `List l when List.for_all (function `String _ -> true | _ -> false) l -> (
         match List.map (function `String s -> s | _ -> assert false) l with
         | "call" :: id :: name :: args -> push t (Call (id, name, args))
         | "send" :: name :: args -> push t (Send (name, args))
         | [ "set"; k; v ] ->
           let changed =
             locked t (fun () ->
                 let old = Hashtbl.find_opt t.tcl_state k in
                 Hashtbl.replace t.tcl_state k v;
                 old <> Some v)
           in
           if changed then push t (Changed (k, Some v))
         | [ "unset"; k ] ->
           let had =
             locked t (fun () ->
                 let had = Hashtbl.mem t.tcl_state k in
                 Hashtbl.remove t.tcl_state k;
                 had)
           in
           if had then push t (Changed (k, None))
         | _ -> prerr_endline ("tkhost: unknown message from tcl: " ^ line))
       | _ | (exception Yojson.Json_error _) ->
         prerr_endline ("tkhost: bad line from tcl: " ^ line));
      loop ()
  in
  loop ()

let warn_exn what e = Printf.eprintf "tkhost: %s: %s\n%!" what (error_message e)

let work_loop t =
  let next () =
    locked t (fun () ->
        while Queue.is_empty t.jobs do
          Condition.wait t.jobs_cond t.lock
        done;
        Queue.pop t.jobs)
  in
  while true do
    match next () with
    | Call (id, name, args) -> (
      match call_prim t name args with
      | Ok r -> write t (Tcl.list [ "reply"; id; "ok"; r ])
      | Error e -> write t (Tcl.list [ "reply"; id; "err"; e ]))
    | Send (name, args) -> (
      match locked t (fun () -> Hashtbl.find_opt t.listeners name) with
      | None -> Printf.eprintf "tkhost: nothing listens to %S\n%!" name
      | Some f -> ( try f args with e -> warn_exn ("listener " ^ name) e))
    | Changed (k, v) ->
      List.iter
        (fun f -> try f k v with e -> warn_exn "on_tcl_change" e)
        (locked t (fun () -> t.on_change))
  done

let run t files =
  if t.started then failwith "Tkhost.run called twice";
  t.started <- true;
  Callback.register "tkhost_cmd" (cmd t);
  ignore (Thread.create read_loop t);
  ignore (Thread.create work_loop t);
  let files = Tcl.list (List.map (fun f -> Tcl.list [ f.path; f.data ]) files) in
  c_run Sys.executable_name t.theirs files Prelude.s
