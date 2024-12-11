module R = Rocksdb

let ( let@ ) = ( @@ )
let spf = Printf.sprintf

let unwrap_ = function
  | Ok x -> x
  | Error (`Msg msg) ->
    Printf.eprintf "error: %s\n%!" msg;
    failwith msg

let path = "/tmp/mydb.db"

let () =
  let db = R.open_db ~config:R.Options.default ~name:path |> unwrap_ in
  let@ () =
    Fun.protect ~finally:(fun () ->
        R.flush db (R.Options.Flush_options.create ()) |> unwrap_;
        R.close_db db |> unwrap_)
  in

  (let t1 = Mtime_clock.counter () in

   let n1 = 10_000 in
   let n2 = 200 in

   let write_options = R.Options.Write_options.create () in
   for i = 1 to n1 do
     let b = R.Batch.create () in
     for j = 1 to n2 do
       R.Batch.put b ~key:(spf "k.%d.%d" i j) ~value:"hello"
     done;

     R.Batch.write db write_options b |> unwrap_
   done;

   let t = (Mtime_clock.count t1 |> Mtime.Span.to_float_ns) *. 1e-9 in
   Printf.printf "done %d insertions in %.4fs (%.2f ins/s, %.2f batch/s)\n"
     (n1 * n2) t
     (float (n1 * n2) /. t)
     (float n1 /. t));

  (let t1 = Mtime_clock.counter () in

   let n1 = 10_000 in
   let n2 = 200 in

   let rd_options = R.Options.Read_options.create () in
   for i = 1 to n1 do
     for j = 1 to n2 do
       let _v = R.get db rd_options (spf "k.%d.%d" i j) |> unwrap_ in
       Sys.opaque_identity (ignore _v)
     done
   done;

   let t = (Mtime_clock.count t1 |> Mtime.Span.to_float_ns) *. 1e-9 in
   Printf.printf "done %d reads in %.4fs (%.2f read/s)\n" (n1 * n2) t
     (float (n1 * n2) /. t));

  ()
