let spf = Printf.sprintf

let str_of_sockaddr = function
  | Unix.ADDR_INET (a, p) -> spf "%s:%d" (Unix.string_of_inet_addr a) p
  | Unix.ADDR_UNIX s -> s

let handle_client addr sock =
  Unix.setsockopt sock Unix.TCP_NODELAY true;

  let ic = Unix.in_channel_of_descr sock in
  let oc = Unix.out_channel_of_descr sock in
  let buf = Bytes.create 1024 in

  let continue = ref true in
  while !continue do
    let n = input ic buf 0 (Bytes.length buf) in
    Printf.eprintf "got %dB from %s\n%!" n (str_of_sockaddr addr);
    output oc buf 0 n;
    if n = 0 then continue := false
  done

let () =
  let sock = Unix.socket Unix.PF_INET Unix.SOCK_STREAM 0 in
  Unix.bind sock (Unix.ADDR_INET (Unix.inet_addr_any, 12345));
  Unix.listen sock 16;

  while true do
    let sock, addr = Unix.accept sock in
    ignore (Thread.create (handle_client addr) sock : Thread.t)
  done
