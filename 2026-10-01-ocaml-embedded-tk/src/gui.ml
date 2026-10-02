(* Tk client: OCaml talks to the daemon, ui.tcl does everything else.

   - async primitives: ocaml::channels, ocaml::messages chan before ("" =
     latest), ocaml::post chan msg, ocaml::presence chan..., each followed by
     a callback
   - sync primitives: ocaml::author_color name, ocaml::remember key value
     (sets ocaml::pstate(key), saved to the --state file)
   - events (ocaml::on): the daemon's pushes, as dicts: event {msg {...}} for
     each new message, presence {author .. chans ..} for each announcement
   - our state (ocaml::state): author, connected (0/1), outbox (number of
     posts waiting for the daemon to come back)

   While the daemon is unreachable, posts are queued (up to [max_outbox]) and
   sent in order before the next request that gets through. *)

let retry_every = 10.0 (* reconnecting the subscription *)
let max_outbox = 50

(* the daemon can't be reached right now (as opposed to: it said no) *)
exception Offline of string

let () = Printexc.register_printer (function Offline m -> Some m | _ -> None)

type chat = {
  ui : Tkhost.t;
  sock : string;
  author : string;
  lock : Mutex.t;  (** for the two below *)
  mutable conn : Proto.conn option;  (** [None] after an IO error *)
  outbox : (string * string) Queue.t;  (** (chan, msg) posts not sent yet *)
}

(* The daemon's JSON as Tcl: objects become dicts, arrays lists, null "". *)
let rec to_tcl : Yojson.Safe.t -> string = function
  | `Null -> ""
  | `Bool b -> if b then "1" else "0"
  | `Int i -> string_of_int i
  | `Intlit s -> s
  | `Float f -> Printf.sprintf "%.17g" f
  | `String s -> s
  | `List l -> Tkhost.Tcl.list (List.map to_tcl l)
  | `Assoc kvs -> Tkhost.Tcl.dict (List.map (fun (k, v) -> (k, to_tcl v)) kvs)

let offline e = Offline (Tkhost.error_message e)

(* One request on the control connection, under [lock]. A connection that
   went stale (the daemon restarted) gets one retry on a fresh one. *)
let request c req =
  let once () =
    let conn =
      match c.conn with
      | Some conn -> conn
      | None ->
        let conn = try Proto.connect c.sock with e -> raise (offline e) in
        c.conn <- Some conn;
        conn
    in
    try Proto.call conn req
    with e when Proto.is_io_error e ->
      Proto.close conn;
      c.conn <- None;
      raise (offline e)
  in
  let stale = Option.is_some c.conn in
  try once () with Offline _ when stale -> once ()

let send_post c (chan, msg) =
  ignore (request c (Proto.req "post" [ ("chan", `String chan); ("author", `String c.author); ("msg", `String msg) ]))

let set_outbox c = Tkhost.set c.ui "outbox" (string_of_int (Queue.length c.outbox))

(* Send queued posts in order, stopping at the first one that can't go yet.
   Under [lock]. *)
let flush c =
  let rec go () =
    match Queue.peek_opt c.outbox with
    | None -> ()
    | Some p -> (
      match send_post c p with
      | () -> ignore (Queue.pop c.outbox); go ()
      | exception Offline _ -> ()
      | exception e ->
        (* the daemon refused it: retrying won't help *)
        Printf.eprintf "dropping queued post to %s: %s\n%!" (fst p) (Tkhost.error_message e);
        ignore (Queue.pop c.outbox);
        go ())
  in
  go ();
  set_outbox c

(* Every request sends the queued posts first, so they stay in order. *)
let call c req = Mutex.protect c.lock (fun () -> flush c; request c req)

(* Every post goes through the outbox: sent now if the daemon is there, kept
   for later otherwise. *)
let post c chan msg =
  Mutex.protect c.lock (fun () ->
      if Queue.length c.outbox >= max_outbox then flush c;
      if Queue.length c.outbox >= max_outbox then
        failwith (Printf.sprintf "offline, and %d messages are already waiting" max_outbox);
      Queue.push (chan, msg) c.outbox;
      flush c;
      "")

(* Forward pushes to Tcl, reconnecting forever. *)
let subscriber c =
  while true do
    (match Proto.connect c.sock with
     | exception _ -> () (* shows up as connected=0 *)
     | conn ->
       (try
          Proto.subscribe conn;
          Tkhost.set c.ui "connected" "1";
          Mutex.protect c.lock (fun () -> flush c);
          Proto.iter_pushes conn (fun typ j -> Tkhost.emit c.ui typ (to_tcl j))
        with e -> Printf.eprintf "subscription: %s\n%!" (Tkhost.error_message e));
       Proto.close conn);
    Tkhost.set c.ui "connected" "0";
    Thread.delay retry_every
  done

(* A color per author, the same in every client, and the same as the Rust
   client's (FNV-1a over the bytes). *)
let author_color name =
  let palette =
    [| "SteelBlue4"; "DarkOrange3"; "purple3"; "firebrick3"; "DeepPink4"; "turquoise4"; "sienna4"; "RoyalBlue3" |]
  in
  let h = ref 0xcbf29ce484222325L in
  String.iter (fun ch -> h := Int64.mul (Int64.logxor !h (Int64.of_int (Char.code ch))) 0x100000001b3L) name;
  palette.(Int64.to_int (Int64.unsigned_rem !h (Int64.of_int (Array.length palette))))

let random_name () =
  let adj = [| "sleepy"; "brave"; "fuzzy"; "grumpy"; "shiny"; "quiet"; "loud"; "tiny" |] in
  let animal = [| "otter"; "camel"; "lynx"; "heron"; "yak"; "newt"; "badger"; "gecko" |] in
  let pick a = a.(Random.int (Array.length a)) in
  Printf.sprintf "%s-%s-%d" (pick adj) (pick animal) (Random.int 100)

let run ~sock ~author ~pstate ui_file =
  Random.self_init ();
  (* a write to a dead daemon connection is an error, not a reason to die *)
  Sys.set_signal Sys.sigpipe Sys.Signal_ignore;
  let ui = Tkhost.create ?pstate () in
  let author = Option.value author ~default:(random_name ()) in
  let c = { ui; sock; author; lock = Mutex.create (); conn = None; outbox = Queue.create () } in
  List.iter (fun (k, v) -> Tkhost.set ui k v) [ ("author", author); ("connected", "0"); ("outbox", "0") ];
  Tkhost.add_prim ~async:true ui "channels" (fun _ -> to_tcl (Proto.field "chans" (call c (Proto.req "list_chans" []))));
  Tkhost.add_prim2 ~async:true ui "messages" (fun chan before ->
      let before = if before = "" then `Null else `Int (int_of_string before) in
      let r = call c (Proto.req "list_msgs" [ ("chan", `String chan); ("before", before); ("limit", `Int 50) ]) in
      to_tcl (Proto.field "msgs" r));
  Tkhost.add_prim2 ~async:true ui "post" (post c);
  (* every 5s from Tcl: quietly skipped while offline *)
  Tkhost.add_prim ~async:true ui "presence" (fun chans ->
      (try ignore (call c (Proto.req "presence" [ ("author", `String author); ("chans", `List (List.map (fun s -> `String s) chans)) ]))
       with Offline _ -> ());
      "");
  Tkhost.add_prim1 ui "author_color" author_color;
  Tkhost.add_prim2 ui "remember" (fun k v -> Tkhost.pset ui k v; "");
  ignore (Thread.create subscriber c);
  Tkhost.run ui [ ui_file ]
