module Tcl = Tcl

type tcl_file = { data : string; path : string }

type t = {
  lock : Mutex.t;  (** protects the tables and queues below *)
  prims : (string, string list -> string) Hashtbl.t;
  state : (string, string) Hashtbl.t;
  pstate : (string, string) Hashtbl.t;
  pstate_file : string option;
  out : string Queue.t;  (** lines for Tcl, drained by [ocaml::_poll] *)
  jobs : (unit -> unit) Queue.t;  (** async calls, for the worker *)
  jobs_cond : Condition.t;
  mutable started : bool;
}

external c_run : string -> string -> string -> unit = "tkhost_run"

let locked t f = Mutex.protect t.lock f

let error_message = function
  | Failure m -> m
  | Unix.Unix_error (e, fn, arg) ->
    Printf.sprintf "%s%s: %s" fn (if arg = "" then "" else " " ^ arg) (Unix.error_message e)
  | e -> Printexc.to_string e

(* under the lock *)
let push_line t words = Queue.push (Tcl.list words) t.out

let load_pstate file =
  match Yojson.Safe.from_file file with
  | `Assoc kvs -> List.filter_map (function k, `String v -> Some (k, v) | _ -> None) kvs
  | _ -> []
  | exception Sys_error _ -> [] (* not saved yet *)
  | exception e ->
    Printf.eprintf "tkhost: ignoring %s: %s\n%!" file (error_message e);
    []

let add_prim ?(async = false) t name f =
  if name = "" || name.[0] = '_' || List.mem name [ "on"; "watch" ] || String.contains name ':' then
    invalid_arg ("Tkhost: reserved primitive name " ^ name);
  locked t (fun () ->
      Hashtbl.replace t.prims name f;
      push_line t [ "define"; name; (if async then "async" else "sync") ])

let wrong_args ?(async = false) name params =
  failwith (Printf.sprintf "wrong # args: should be \"ocaml::%s %s%s\"" name params (if async then " callback" else ""))

let add_prim1 ?async t name f = add_prim ?async t name (function [ a ] -> f a | _ -> wrong_args ?async name "arg")

let add_prim2 ?async t name f =
  add_prim ?async t name (function [ a; b ] -> f a b | _ -> wrong_args ?async name "arg1 arg2")

let emit t name payload = locked t (fun () -> push_line t [ "event"; name; payload ])

(* Under the lock: set ([Some v]) or remove ([None]) [k] in [tbl], mirrored
   in Tcl as ocaml::$arr. Returns whether that changed anything. *)
let update t ~arr tbl k v =
  let changed = Hashtbl.find_opt tbl k <> v in
  if changed then begin
    (match v with Some v -> Hashtbl.replace tbl k v | None -> Hashtbl.remove tbl k);
    push_line t (match v with Some v -> [ "set"; arr; k; v ] | None -> [ "unset"; arr; k ])
  end;
  changed

let create ?pstate () =
  let t =
    {
      lock = Mutex.create ();
      prims = Hashtbl.create 16;
      state = Hashtbl.create 16;
      pstate = Hashtbl.create 16;
      pstate_file = pstate;
      out = Queue.create ();
      jobs = Queue.create ();
      jobs_cond = Condition.create ();
      started = false;
    }
  in
  Option.iter
    (fun f ->
      List.iter (fun (k, v) -> ignore (update t ~arr:"pstate" t.pstate k (Some v))) (load_pstate f))
    pstate;
  t

let set t k v = locked t (fun () -> ignore (update t ~arr:"state" t.state k (Some v)))
let unset t k = locked t (fun () -> ignore (update t ~arr:"state" t.state k None))
let get t k = locked t (fun () -> Hashtbl.find_opt t.state k)

(* The whole table, written to a temp file renamed into place: a crash leaves
   either the old file or the new one. *)
let save_pstate t file =
  let tmp = file ^ ".tmp" in
  try
    Yojson.Safe.to_file tmp (`Assoc (Hashtbl.fold (fun k v l -> (k, `String v) :: l) t.pstate []));
    Sys.rename tmp file
  with e -> Printf.eprintf "tkhost: saving %s: %s\n%!" file (error_message e)

let pset_opt t k v =
  locked t (fun () -> if update t ~arr:"pstate" t.pstate k v then Option.iter (save_pstate t) t.pstate_file)

let pset t k v = pset_opt t k (Some v)
let punset t k = pset_opt t k None
let pget t k = locked t (fun () -> Hashtbl.find_opt t.pstate k)

let find_prim t name =
  match locked t (fun () -> Hashtbl.find_opt t.prims name) with
  | Some p -> p
  | None -> failwith (Printf.sprintf "no primitive named %S" name)

let worker t =
  while true do
    let job =
      locked t (fun () ->
          while Queue.is_empty t.jobs do
            Condition.wait t.jobs_cond t.lock
          done;
          Queue.pop t.jobs)
    in
    job ()
  done

(* `ocaml::_cmd op args...`, called from the C stub on the Tcl thread:
   - call name args...: run a sync primitive, return its result
   - start id name args...: queue an async one, its reply comes via poll
   - poll: the lines queued for Tcl, newline-separated *)
let cmd t (args : string array) : bool * string =
  let wrap f = try (true, f ()) with e -> (false, error_message e) in
  match Array.to_list args with
  | "call" :: name :: args -> wrap (fun () -> find_prim t name args)
  | "start" :: id :: name :: args ->
    wrap (fun () ->
        let f = find_prim t name in
        let job () =
          let reply = match f args with r -> [ "ok"; r ] | exception e -> [ "err"; error_message e ] in
          locked t (fun () -> push_line t ("reply" :: id :: reply))
        in
        locked t (fun () ->
            Queue.push job t.jobs;
            Condition.signal t.jobs_cond);
        "")
  | [ "poll" ] ->
    ( true,
      locked t (fun () ->
          let s = String.concat "\n" (List.of_seq (Queue.to_seq t.out)) in
          Queue.clear t.out;
          s) )
  | _ -> (false, "usage: ocaml::_cmd call|start|poll ...")

let run t files =
  if t.started then failwith "Tkhost.run called twice";
  t.started <- true;
  Callback.register "tkhost_cmd" (cmd t);
  ignore (Thread.create worker t);
  let files = Tcl.list (List.map (fun f -> Tcl.list [ f.path; f.data ]) files) in
  c_run Sys.executable_name files Prelude.s
