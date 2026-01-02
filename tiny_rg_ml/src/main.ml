module Fmt = CCFormat
module M = Moonpool

let ( let@ ) = ( @@ )

type cli = {
  needle: string;
  dirs: string list;
  j: int option;
}
[@@deriving show]

let pp_atomic ppx out a = ppx out (Atomic.get a)

type stats = {
  bytes: (int Atomic.t[@polyprinter pp_atomic]); [@default Atomic.make 0]
  lines: (int Atomic.t[@polyprinter pp_atomic]); [@default Atomic.make 0]
  matches: (int Atomic.t[@polyprinter pp_atomic]); [@default Atomic.make 0]
  files: (int Atomic.t[@polyprinter pp_atomic]); [@default Atomic.make 0]
  errors: (int Atomic.t[@polyprinter pp_atomic]); [@default Atomic.make 0]
}
[@@deriving make, show]

let find_files (dir : string) : string Iter.t =
  CCIO.File.read_dir ~recurse:true dir |> Iter.of_gen

let process_file ~(stats : stats) ~(cli : cli) (file : string) : unit =
  Atomic.incr stats.files;
  let n_bytes = ref 0 in
  let n_lines = ref 0 in
  let n_matches = ref 0 in
  (try
     let@ ic = CCIO.with_in file in

     let continue = ref true in
     while !continue do
       match CCIO.read_line ic with
       | None -> continue := false
       | Some line ->
         n_bytes := !n_bytes + String.length line;
         incr n_lines;
         if CCString.mem ~sub:cli.needle line then
           (* Printf.printf "%s: %s\n" file line; *)
           incr n_matches
     done
   with _ -> Atomic.incr stats.errors);

  ignore (Atomic.fetch_and_add stats.matches !n_matches);
  ignore (Atomic.fetch_and_add stats.lines !n_lines);
  ignore (Atomic.fetch_and_add stats.bytes !n_bytes);
  ()

let run ~(runner : M.Runner.t) ~(stats : stats) (cli : cli) : unit =
  let@ () = M.run_wait_block runner in
  let files : string Iter.t =
    Iter.of_list cli.dirs |> Iter.flat_map find_files
  in
  let futs = ref [] in
  files (fun file ->
      let fut = M.spawn ~on:runner (fun () -> process_file ~stats ~cli file) in
      futs := fut :: !futs);

  List.iter M.Fut.await !futs

let () =
  let dirs = ref [] in
  let needle = ref "" in
  let j = ref None in

  let opts =
    [ "-j", Arg.Int (fun i -> j := Some i), " number of jobs" ] |> Arg.align
  in

  let first = ref true in
  Arg.parse opts
    (fun s ->
      if !first then (
        first := false;
        needle := s
      ) else
        dirs := s :: !dirs)
    "";

  let cli = { dirs = List.rev !dirs; needle = !needle; j = !j } in
  Fmt.printf "cli: %a@." pp_cli cli;

  let t_start = Unix.gettimeofday () in

  let stats = make_stats () in
  (let@ runner = M.Ws_pool.with_ ?num_threads:!j () in
   run ~stats ~runner cli);

  let elapsed = Unix.gettimeofday () -. t_start in

  Fmt.printf "done in %.2fs@.stats: %a@." elapsed pp_stats stats;
  Fmt.printf "MB/s: %.3f@." (float (Atomic.get stats.bytes) *. 1e-6 /. elapsed);

  ()
