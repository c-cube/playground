let () =
  let n = 40_000_000 in
  let lock = Mutex.create() in
  for _i=1 to n do
    Mutex.lock lock;
    Sys.opaque_identity ();
    Mutex.unlock lock;
  done;

  Printf.printf "done (%d iterations)\n%!" n;
  ()
