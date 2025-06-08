let n = try Sys.getenv "N" |> int_of_string with _ -> 500_000_000

let run st n : float =
  let n_hits = ref 0 in
  for _i = 1 to n do
    let x = Random.State.float st 2. -. 1. in
    let y = Random.State.float st 2. -. 1. in
    if sqrt ((x *. x) +. (y *. y)) <= 1. then incr n_hits
  done;
  float !n_hits /. float n

let () =
  Printf.printf "run for %d iterations\n" n;
  let st = Random.State.make_self_init () in
  let x = run st n in
  (* area: pi r^2 (r=1), square area: 2^2 *)
  Printf.printf "hit rate should be: %.8f\n" (Float.pi /. (2. *. 2.));
  Printf.printf "we get: %.8f\n%!" x;
  Printf.printf "pi = %.8f (ref: %.8f)\n" (4. *. x) Float.pi
