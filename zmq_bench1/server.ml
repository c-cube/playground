
let ctx = Zmq.Context.create()
let sock = Zmq.Socket.create ctx Zmq.Socket.router
let() = Zmq.Socket.bind sock "ipc:///tmp/foo.sock"

let () =
  while true do
    match Zmq.Socket.recv_all sock with
    | id::msg ->
      Zmq.Socket.send_all sock (id::msg)
    | _ -> assert false
  done
