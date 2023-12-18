let n = 100_000_000

let timeit f out =
  (* warmup *)
  ignore (Sys.opaque_identity (f @@ (n / 10)) : unit);

  let t_start = Mtime_clock.now_ns () in

  ignore (Sys.opaque_identity (f n) : unit);

  let t_stop = Mtime_clock.now_ns () in
  let t_ns = Int64.to_float t_stop -. Int64.to_float t_start in
  let t_ms = t_ns /. 1e6 in

  Format.fprintf out "run %d iterations in %.2fms (%f ns/iter)" n t_ms
    (t_ns /. float n)

let () =
  let run_mtime n =
    for i = 1 to n do
      ignore (Sys.opaque_identity (Mtime_clock.now ()) : Mtime.t)
    done
  in
  Format.printf "mtime: %t@." (timeit run_mtime);

  let run_rdtsc n =
    for i = 1 to n do
      ignore (Sys.opaque_identity (Ocaml_intrinsics.Perfmon.rdtsc ()) : int64)
    done
  in
  Format.printf "rdtsc: %t@." (timeit run_rdtsc);
  ()
