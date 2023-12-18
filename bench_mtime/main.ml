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

let run_mtime n =
  for i = 1 to n do
    ignore (Sys.opaque_identity (Mtime_clock.now ()) : Mtime.t)
  done

let run_rdtsc n =
  for i = 1 to n do
    ignore (Sys.opaque_identity (Ocaml_intrinsics.Perfmon.rdtsc ()) : int64)
  done

let compute_time_rdtsc () =
  let ts1 = Mtime_clock.now_ns () in
  let rdtsc1 = Ocaml_intrinsics.Perfmon.rdtsc () in

  ignore (Sys.opaque_identity (run_mtime @@ (n / 50)));

  (* Unix.sleepf 0.002; *)
  let rdtsc2 = Ocaml_intrinsics.Perfmon.rdtsc () in
  let ts2 = Mtime_clock.now_ns () in

  let ts_span_ns = Int64.(sub ts2 ts1) in
  let rdtsc_span = Int64.(sub rdtsc2 rdtsc1) in

  Format.printf "for %d iters: mtime span: %Ldns, rdtsc span: %Ld@." n
    ts_span_ns rdtsc_span;

  let t_ns_per_rdtsc = Int64.to_float ts_span_ns /. Int64.to_float rdtsc_span in
  Format.printf "1 rdtsc cycle = %.5fns (%.2GHz)@." t_ns_per_rdtsc
    (1. /. t_ns_per_rdtsc);
  ()

let () =
  Format.printf "mtime: %t@." (timeit run_mtime);
  Format.printf "rdtsc: %t@." (timeit run_rdtsc);

  compute_time_rdtsc ();
  ()
