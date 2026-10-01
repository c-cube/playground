(* The Rust daemon's wire protocol: newline-delimited JSON over a unix socket.
   Messages are kept as JSON objects ({id, chan, author, msg, ts}) and handed
   to Tcl as dicts. *)

type conn = { fd : Unix.file_descr; ic : in_channel; oc : out_channel }

let connect sock =
  let fd = Unix.socket Unix.PF_UNIX Unix.SOCK_STREAM 0 in
  (try Unix.connect fd (Unix.ADDR_UNIX sock)
   with e ->
     Unix.close fd;
     raise e);
  { fd; ic = Unix.in_channel_of_descr fd; oc = Unix.out_channel_of_descr fd }

let close c = try Unix.close c.fd with Unix.Unix_error _ -> ()

(* IO problems mean the connection is gone; anything else is the daemon
   saying no. *)
let is_io_error = function
  | Unix.Unix_error _ | Sys_error _ | End_of_file -> true
  | _ -> false

let write_line c (j : Yojson.Safe.t) =
  output_string c.oc (Yojson.Safe.to_string j);
  output_char c.oc '\n';
  flush c.oc

let read_line c : Yojson.Safe.t = Yojson.Safe.from_string (input_line c.ic)
let field k (j : Yojson.Safe.t) = Yojson.Safe.Util.member k j

let typ j =
  match field "type" j with `String s -> s | _ -> failwith "response without a type"

(* one request, one response *)
let call c (req : Yojson.Safe.t) =
  write_line c req;
  let resp = read_line c in
  match typ resp with
  | "err" -> failwith ("daemon error: " ^ Yojson.Safe.Util.to_string (field "err" resp))
  | _ -> resp

let list_chans = `Assoc [ ("op", `String "list_chans") ]

let list_msgs ~chan ~before ~limit =
  `Assoc
    [
      ("op", `String "list_msgs");
      ("chan", `String chan);
      ("before", match before with None -> `Null | Some i -> `Int i);
      ("limit", `Int limit);
    ]

let post ~chan ~author ~msg =
  `Assoc
    [ ("op", `String "post"); ("chan", `String chan); ("author", `String author); ("msg", `String msg) ]

let presence ~author ~chans =
  `Assoc
    [
      ("op", `String "presence");
      ("author", `String author);
      ("chans", `List (List.map (fun c -> `String c) chans));
    ]

type push = Msg of Yojson.Safe.t | Presence of string * string list

(* From now on, [c] only receives pushes. *)
let subscribe c =
  match typ (call c (`Assoc [ ("op", `String "subscribe") ])) with
  | "subscribed" -> ()
  | t -> failwith ("unexpected response " ^ t)

(* Call [f] on each push until the daemon hangs up (returns normally) or
   something breaks (raises). *)
let iter_pushes c f =
  let open Yojson.Safe.Util in
  let rec loop () =
    match read_line c with
    | exception End_of_file -> ()
    | j ->
      (match typ j with
       | "event" -> f (Msg (field "msg" j))
       | "presence" ->
         f (Presence (to_string (field "author" j), List.map to_string (to_list (field "chans" j))))
       | t -> failwith ("unexpected push " ^ t));
      loop ()
  in
  loop ()
