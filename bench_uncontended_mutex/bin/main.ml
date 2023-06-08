let run n =
  let lock = Mutex.create() in
  for _i=1 to n do
    Mutex.lock lock;
    Sys.opaque_identity ();
    Mutex.unlock lock;
  done

let run_try n =
  let lock = Mutex.create() in
  for _i=1 to n do
    let ok =Mutex.try_lock lock in
    assert ok;
    Sys.opaque_identity ();
    Mutex.unlock lock;
  done

let () =
  let n = 40_000_000 in
  (match Sys.argv.(1) with
  | exception _ -> run n
  | "try" -> run_try n
  | _ -> failwith {|expected nothing or "try"|}
  );
  Printf.printf "done (%d iterations)\n%!" n
