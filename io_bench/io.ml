
let buf_size = try Sys.getenv "BUFSIZE" |> int_of_string with _ -> 64 * 1024

let read_buffered ic =
  let len = ref 0 in
  let buf = Bytes.create buf_size in
  try
    while true do
      let n = input ic buf 0 (Bytes.length buf) in
      if n=0 then raise End_of_file;
      len := !len + n;
    done;
    assert false
  with End_of_file -> !len

let read_unix file =
  let len = ref 0 in
  let buf = Bytes.create buf_size in
  let fd = Unix.openfile file [Unix.O_RDONLY] 0 in
  try
    while true do
      let n = Unix.read fd buf 0 (Bytes.length buf) in
      if n=0 then raise End_of_file;
      len := !len + n;
    done;
    assert false
  with End_of_file -> !len

let read_by_char ic =
  let len = ref 0 in
  try
    while true do
      let _c = input_char ic in
      incr len
    done;
    assert false
  with End_of_file -> !len

let () =
  let mode = Sys.argv.(1) in

  let file = Sys.argv.(2) in
  Printf.printf "input: %S, mode: %S\n" file mode;

  let n =
    match mode with
    | "char" ->
      let ic = open_in file in
      read_by_char ic
    | "buf" ->
      let ic = open_in file in
      read_buffered ic
    | "unix" ->
      read_unix file
    | s -> failwith ("unknown mode (char|buf|unix): " ^ s)
  in
  Printf.printf "size: %d\n" n; flush stdout

