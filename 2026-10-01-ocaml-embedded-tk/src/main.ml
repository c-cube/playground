(* tkchat-ml: an OCaml GUI client for the Rust tkchat daemon. *)

let () =
  let sock = ref "/tmp/chat.sock" and author = ref None and ui = ref None in
  Arg.parse
    [
      ("-s", Arg.Set_string sock, "PATH  the daemon's unix socket (default /tmp/chat.sock)");
      ("-a", Arg.String (fun a -> author := Some a), "NAME  name to post as (random by default)");
      ( "--ui",
        Arg.String (fun p -> ui := Some p),
        "PATH  Tcl UI script to hot-reload instead of src/ui.tcl (the embedded copy is still the fallback)" );
    ]
    (fun a -> raise (Arg.Bad ("unexpected argument " ^ a)))
    "tkchat-ml [-s SOCK] [-a NAME] [--ui PATH]";
  let file = match !ui with None -> Ui_tcl.file | Some path -> { Ui_tcl.file with path } in
  try Gui.run ~sock:!sock ~author:!author file
  with Failure m ->
    prerr_endline m;
    exit 1
