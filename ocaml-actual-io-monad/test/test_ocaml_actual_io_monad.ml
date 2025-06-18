open Ocaml_actual_io_monad

let () =
  let str =
    Io.(
      run
      @@
      let buf = Buffer.create 32 in
      let sink = Buf buf in
      let* () = write_line sink "hello" in
      let* () = write_line sink "world" in
      return (Buffer.contents buf))
  in
  assert (str = "hello\nworld\n")
