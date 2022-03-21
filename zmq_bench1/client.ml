
let ctx = Zmq.Context.create()
let sock = Zmq.Socket.create ctx Zmq.Socket.dealer
let() = Zmq.Socket.connect sock "ipc:///tmp/foo.sock"

let n = try int_of_string @@ Sys.getenv "N" with _ -> 100_000
let () = Printf.printf "do %d iterations\n%!" n

let received = ref 0
let () =
  let start = Mtime_clock.now() in
  for _i = 1 to n/100 do
    for _j = 1 to 100 do
      Zmq.Socket.send_all sock ["iter"; string_of_int _i];
    done;

    for _j = 1 to 100 do
      match Zmq.Socket.recv_all sock with
      | ["iter";_] -> incr received; ()
      | _ -> assert false
    done;

  done;
  let end_ = Mtime_clock.now() in
  let span = Mtime.span start end_ |> Mtime.Span.to_us in

  Printf.printf
    "sent %d/received %d messages in %.0f us (%.3f msg/s)\n%!"
    n !received span (float n /. (span /. 1e6));
  ()
