module RE = Runtime_events

let alive = Atomic.make true
let i64_of_ts = RE.Timestamp.to_int64

let bg_thread () =
  let cursor = RE.create_cursor None in

  let has_first_ts = ref false in
  let first_ts = ref 0L in
  let lost = ref 0 in
  let n_total = ref 0 in

  let indent = ref 0 in
  let[@inline] pp_indent () =
    for i = 1 to !indent do
      output_char stderr ' '
    done
  in

  let runtime_begin rid ts phase =
    if not !has_first_ts then (
      has_first_ts := true;
      first_ts := i64_of_ts ts
    );

    pp_indent ();
    Printf.eprintf "BEGIN %d at %.2fus: %s\n" rid
      Int64.(to_float (sub (i64_of_ts ts) !first_ts) /. 1e3)
      (RE.runtime_phase_name phase);
    incr indent;
    incr n_total;
    ()
  in

  let runtime_end rid ts phase =
    decr indent;
    pp_indent ();
    Printf.eprintf "END %d at %.2fus: %s\n" rid
      Int64.(to_float (sub (i64_of_ts ts) !first_ts) /. 1e3)
      (RE.runtime_phase_name phase);
    incr n_total
  in

  let lost_events rid n =
    Printf.eprintf "LOST %d: %d evs lost\n" rid n;
    lost := !lost + n
  in

  let cbs = RE.Callbacks.create ~runtime_begin ~runtime_end ~lost_events () in
  while Atomic.get alive do
    let n = RE.read_poll cursor cbs None in
    if n = 0 then Thread.delay 0.000_005;
    ()
  done;

  if !lost > 0 then Printf.eprintf "lost %d events total\n" !lost;
  Printf.eprintf "total: %d events\n" !n_total;
  ()

let do_work () =
  for i = 1 to 1000 do
    for j = 1 to 50 do
      ignore (Sys.opaque_identity (Array.make 1000 "a"))
    done;
    Thread.delay 0.000_5;
    ()
  done

let () =
  RE.start ();

  let t = Thread.create bg_thread () in

  let doms = Array.init 4 (fun _ -> Domain.spawn do_work) in
  Array.iter Domain.join doms;

  Atomic.set alive false;
  Thread.join t;
  ()
