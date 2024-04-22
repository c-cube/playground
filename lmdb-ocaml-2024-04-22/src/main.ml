module A = Atomic
module L = Lmdb

let spf = Printf.sprintf
let pf = Printf.printf
let db_file = "foo.db"

let () =
  let t0 = Unix.gettimeofday () in
  pf "using db=%s\n%!" db_file;

  let is_done = A.make false in

  let n = try int_of_string Sys.argv.(1) with _ -> 100_000 in
  pf "doing %d writes\n%!" n;

  let env = L.Env.create ~flags:L.Env.Flags.no_subdir Rw db_file in
  L.Env.set_map_size env 1_000_000_000;

  let writer =
    Domain.spawn (fun () ->
        let map =
          L.Map.create Nodup ~key:L.Conv.int64_le ~value:L.Conv.string env
        in

        for i = 1 to n do
          L.Map.set map (Int64.of_int i) (spf "hello %d" i);

          if i mod 100 = 0 then
            L.Env.sync env
          else if i mod 10 = 0 then
            Thread.yield ()
        done;
        A.set is_done true)
  in

  let all_reads = A.make 0 in

  let run_reader r_idx =
    let n_reads = ref 0 in
    let rand = Random.State.make_self_init () in
    let map =
      L.Map.create Nodup ~key:L.Conv.int64_le ~value:L.Conv.string env
    in

    while not (A.get is_done) do
      for _j = 1 to 10 do
        let idx = Int64.of_int @@ Random.State.int rand n in
        let expected = spf "hello %Ld" idx in

        (match L.Map.get map idx with
        | exception Not_found -> ()
        | s when s = expected -> ()
        | s -> pf "for key %Ld: expected %S, got %S\n%!" idx expected s);
        incr n_reads
      done;
      Thread.yield ()
    done;
    ignore (A.fetch_and_add all_reads !n_reads : int)
  in

  Thread.delay 0.001;
  let readers = List.init 5 (fun i -> Domain.spawn @@ fun () -> run_reader i) in

  Domain.join writer;
  List.iter Domain.join readers;

  let all_reads = A.get all_reads in

  let elapsed = Unix.gettimeofday () -. t0 in
  pf "done (in %.3fs, %d writes, %.2f writes/s,%d reads, %.2f reads/s)\n%!"
    elapsed n
    (float n /. elapsed)
    all_reads
    (float all_reads /. elapsed);
  ()
