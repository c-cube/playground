(* tkchat-ml: an OCaml GUI client for the Rust tkchat daemon. *)

let () =
  let sock = ref "/tmp/chat.sock" and author = ref None and ui = ref None and pstate = ref None in
  Arg.parse
    [
      ("-s", Arg.Set_string sock, "PATH  the daemon's unix socket (default /tmp/chat.sock)");
      ("-a", Arg.String (fun a -> author := Some a), "NAME  name to post as (random by default)");
      ( "--state",
        Arg.String (fun p -> pstate := Some p),
        "PATH  JSON file to remember the current and joined channels in (default: don't)" );
      ( "--ui",
        Arg.String (fun p -> ui := Some p),
        "PATH  Tcl UI script to hot-reload instead of src/ui.tcl (the embedded copy is still the fallback)" );
    ]
    (fun a -> raise (Arg.Bad ("unexpected argument " ^ a)))
    "tkchat-ml [-s SOCK] [-a NAME] [--state PATH] [--ui PATH]";
  let file = match !ui with None -> Ui_tcl.file | Some path -> { Ui_tcl.file with path } in
  try Gui.run ~sock:!sock ~author:!author ~pstate:!pstate file
  with Failure m ->
    prerr_endline m;
    exit 1
