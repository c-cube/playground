(* The Rust daemon's wire protocol: one JSON object per line over a unix
   socket. Requests are {op, ...}, responses and pushes have a "type". *)

type conn = in_channel * out_channel

let connect sock : conn = Unix.open_connection (Unix.ADDR_UNIX sock)
(* closing [oc] closes the fd, and drops whatever a failed write left in its
   buffer (which would otherwise be kept until exit) *)
let close ((_, oc) : conn) = close_out_noerr oc

(* IO problems mean the connection is gone; anything else is the daemon
   saying no. *)
let is_io_error = function Unix.Unix_error _ | Sys_error _ | End_of_file -> true | _ -> false

let req op fields : Yojson.Safe.t = `Assoc (("op", `String op) :: fields)
let field k (j : Yojson.Safe.t) = Yojson.Safe.Util.member k j
let typ j = Yojson.Safe.Util.to_string (field "type" j)
let read ((ic, _) : conn) = Yojson.Safe.from_string (input_line ic)

(* one request, one response *)
let call ((_, oc) as c : conn) req =
  Yojson.Safe.to_channel oc req;
  output_char oc '\n';
  flush oc;
  let r = read c in
  if typ r = "err" then failwith ("daemon error: " ^ Yojson.Safe.Util.to_string (field "err" r));
  r

(* From now on, [c] only receives pushes. *)
let subscribe c = ignore (call c (req "subscribe" []))

(* Call [f type push] on each push until the daemon hangs up (returns
   normally) or something breaks (raises). *)
let iter_pushes c f =
  try
    while true do
      let j = read c in
      f (typ j) j
    done
  with End_of_file -> ()
