(* Tk client: OCaml talks to the daemon, ui.tcl draws, Tkhost connects them.

   - async calls (ocaml::call): channels, messages chan ?before? ?limit?, post chan msg
   - sync calls (ocaml::cmd): author_color name
   - events (ocaml::on): msg {dict} for each new message, reconnect after the
     daemon came back
   - our state (ocaml::state): author, connected (0/1), here:<chan> (authors
     seen there in the last 40s)
   - UI state we read (ocaml::ui): joined, the channels we announce presence in
     (every 5s, and whenever it changes)

   Both daemon connections reconnect on their own. *)

let presence_every = 5.0
let presence_timeout = 40.0

(* after a failed connect, wait this long before trying again *)
let retry_every = 10.0

type chat = {
  ui : Tkhost.t;
  sock : string;
  author : string;
  ctrl_lock : Mutex.t;
  mutable ctrl : Proto.conn option;  (** [None] after an IO error *)
  mutable retry_at : float option;  (** no connect attempt before this *)
  seen_lock : Mutex.t;
  seen : (string, string list * float) Hashtbl.t;
      (** last presence broadcast received, per author: channels and time *)
  mutable here_chans : string list;  (** the here:<chan> keys set, under [seen_lock] *)
}

(* Request/response on the control connection; on an IO error, reconnect and
   retry once. *)
let call c req =
  Mutex.protect c.ctrl_lock (fun () ->
      let rec go attempt =
        let conn =
          match c.ctrl with
          | Some conn -> conn
          | None -> (
            let now = Unix.gettimeofday () in
            (match c.retry_at with
             | Some t when now < t ->
               failwith (Printf.sprintf "disconnected, retrying in %.0fs" (Float.ceil (t -. now)))
             | _ -> ());
            match Proto.connect c.sock with
            | conn ->
              c.ctrl <- Some conn;
              c.retry_at <- None;
              conn
            | exception e ->
              c.retry_at <- Some (now +. retry_every);
              raise e)
        in
        match Proto.call conn req with
        | r -> r
        | exception e when Proto.is_io_error e ->
          Proto.close conn;
          c.ctrl <- None;
          if attempt = 1 then raise e else go 1
      in
      go 0)

let to_tcl_list j = Tkhost.Tcl.of_json (Proto.field "msgs" j)

let channels c = function
  | [] -> Tkhost.Tcl.of_json (Proto.field "chans" (call c Proto.list_chans))
  | _ -> failwith "wrong # args: should be \"channels\""

let messages c args =
  let chan, before, limit =
    match args with
    | [ chan ] -> (chan, "", "50")
    | [ chan; before ] -> (chan, before, "50")
    | [ chan; before; limit ] -> (chan, before, limit)
    | _ -> failwith "wrong # args: should be \"messages chan ?before? ?limit?\""
  in
  let num what s = match int_of_string_opt s with Some i -> i | None -> failwith (Printf.sprintf "bad %s %S" what s) in
  let before = if before = "" then None else Some (num "id" before) in
  to_tcl_list (call c (Proto.list_msgs ~chan ~before ~limit:(num "limit" limit)))

let post c chan msg = Tkhost.Tcl.of_json (Proto.field "msg" (call c (Proto.post ~chan ~author:c.author ~msg)))

let announce c =
  let chans = match Tkhost.tcl_get c.ui "joined" with None -> [] | Some l -> Tkhost.Tcl.parse_list l in
  ignore (call c (Proto.presence ~author:c.author ~chans))

(* Expire silent authors and publish here:<chan> for every channel. *)
let update_presence c =
  Mutex.protect c.seen_lock (fun () ->
      let now = Unix.gettimeofday () in
      Hashtbl.filter_map_inplace
        (fun _ ((_, at) as s) -> if now -. at <= presence_timeout then Some s else None)
        c.seen;
      let by_chan = Hashtbl.create 8 in
      Hashtbl.iter
        (fun author (chans, _) ->
          List.iter (fun ch -> Hashtbl.replace by_chan ch (author :: Option.value ~default:[] (Hashtbl.find_opt by_chan ch))) chans)
        c.seen;
      let chans = List.sort_uniq compare (List.of_seq (Hashtbl.to_seq_keys by_chan)) in
      List.iter (fun ch -> if not (List.mem ch chans) then Tkhost.unset c.ui ("here:" ^ ch)) c.here_chans;
      List.iter
        (fun ch -> Tkhost.set c.ui ("here:" ^ ch) (Tkhost.Tcl.list (List.sort_uniq compare (Hashtbl.find by_chan ch))))
        chans;
      c.here_chans <- chans)

let error_message = function
  | Failure m -> m
  | Unix.Unix_error (e, fn, _) -> fn ^ ": " ^ Unix.error_message e
  | e -> Printexc.to_string e

let on_push c = function
  | Proto.Msg m -> Tkhost.emit c.ui "msg" (Tkhost.Tcl.of_json m)
  | Proto.Presence (author, chans) ->
    Mutex.protect c.seen_lock (fun () -> Hashtbl.replace c.seen author (chans, Unix.gettimeofday ()));
    update_presence c

(* Keep a subscription open, reconnecting forever. *)
let subscriber c =
  let first = ref true and warned = ref false in
  while true do
    match
      let conn = Proto.connect c.sock in
      try
        Proto.subscribe conn;
        conn
      with e ->
        Proto.close conn;
        raise e
    with
    | conn ->
      Tkhost.set c.ui "connected" "1";
      warned := false;
      (* the UI may have been drawn while disconnected: refresh it *)
      if not !first then begin
        prerr_endline "resubscribed";
        Mutex.protect c.ctrl_lock (fun () -> c.retry_at <- None);
        (try announce c with _ -> ());
        Tkhost.emit c.ui "reconnect" ""
      end;
      (try
         Proto.iter_pushes conn (on_push c);
         prerr_endline "subscription closed, reconnecting…"
       with e -> Printf.eprintf "subscription: %s\n%!" (error_message e));
      Proto.close conn;
      Tkhost.set c.ui "connected" "0";
      (* retry right away, then every retry_every *)
      first := false
    | exception e ->
      Tkhost.set c.ui "connected" "0";
      if not !warned then begin
        Printf.eprintf "subscribe: %s %s (retrying every %.0fs)\n%!" c.sock (error_message e) retry_every;
        warned := true
      end;
      first := false;
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

let run ~sock ~author ui_file =
  Random.self_init ();
  let ui = Tkhost.create () in
  let c =
    {
      ui;
      sock;
      author = Option.value author ~default:(random_name ());
      ctrl_lock = Mutex.create ();
      ctrl = None;
      retry_at = None;
      seen_lock = Mutex.create ();
      seen = Hashtbl.create 8;
      here_chans = [];
    }
  in
  Tkhost.set ui "author" c.author;
  Tkhost.set ui "connected" "0";
  Tkhost.add_prim ui "channels" (channels c);
  Tkhost.add_prim ui "messages" (messages c);
  Tkhost.add_prim2 ui "post" (post c);
  Tkhost.add_prim1 ui "author_color" author_color;
  Tkhost.on_tcl_change ui (fun k _ -> if k = "joined" then try announce c with _ -> ());
  ignore (Thread.create subscriber c);
  ignore
    (Thread.create
       (fun () ->
         while true do
           (* errors already show up as connected=0 *)
           (try announce c with _ -> ());
           update_presence c;
           Thread.delay presence_every
         done)
       ());
  Tkhost.run ui [ ui_file ]
