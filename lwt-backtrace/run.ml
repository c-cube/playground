
open Lwt.Syntax

(*
module Reraise = struct external reraise : exn -> 'a = "%reraise" end

let ( let* ) x f = Lwt.backtrace_bind Reraise.reraise x f
*)

let rec my_fib n =
  if n = 22 then Lwt.fail (Failure "oh no")
  else if n <= 1 then Lwt.return 1
  else (
    let* n1 = my_fib (n-1)
    and* n2 = my_fib (n-2) in
    Lwt.return (n1 + n2)
  )

let run n =
  (*Lwt.backtrace_catch Reraise.reraise
    *)
  Lwt.catch
  (fun () ->
    let* res = my_fib n in
    Lwt_io.printlf "fib(%d) = %d\n" n res)
  (fun e ->
    let bt = Printexc.get_raw_backtrace() in
    Lwt_io.printlf "raised %s with bt:\n%s" (Printexc.to_string e) (Printexc.raw_backtrace_to_string bt)
  )

module Sync = struct
  let rec my_fib n =
    if n = 22 then failwith "oh no"
    else if n <= 1 then 1
    else (
      let n1 = my_fib (n-1)
      and n2 = my_fib (n-2) in
      n1 + n2
    )

  let run n =
    try
      let res = my_fib n in
      Printf.printf "fib(%d) = %d\n" n res
    with e ->
      let bt = Printexc.get_raw_backtrace() in
      Printf.printf "sync raised %s with bt:\n%s" (Printexc.to_string e) (Printexc.raw_backtrace_to_string bt)
end

let main () =
  Printf.printf "SYNC\n%!";
  Sync.run 50;
  Printf.printf "ASYNC\n%!";
  let*() = run 20 in
  let*() = run 30 in
  Lwt_io.printl "all done"

let () =
  Printexc.record_backtrace true;
  Lwt_main.run @@ main()

