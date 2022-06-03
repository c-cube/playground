module Rec = struct

  type reader = {
    get_i: unit -> int;
    get_end : unit -> unit
  }

  let[@inline] foo (f:reader) =
    let x = f.get_i () in
    let y = f.get_i () in
    f.get_end();
    x+y

  let mk_reader () =
    let x = ref 0 in
    { get_i= (fun () ->x := !x+1; !x);
      get_end = ignore
    }
end

module O = struct

  class virtual reader = object
    method virtual get_i : int
    method virtual get_end : unit
  end


  let[@inline] foo (f:reader) =
    let x = f#get_i in
    let y = f#get_i in
    f#get_end;
    x+y

  let mk_reader () = object
    inherit reader
    val mutable x = 0
    method get_i = x <- x+1; x
    method get_end = ()
  end
end

let now_s () = (Mtime.to_uint64_ns (Mtime_clock.now ()) |> Int64.to_float) *. 1e-9

let () =
  let n = try Sys.getenv "N" |> int_of_string with _ -> 1_000_000_000 in
  Printf.printf "N=%d\n%!" n;

  begin
    let open O in
    let t_start = now_s() in
    Sys.opaque_identity (
      let r = mk_reader() in
      for i=1 to n do
        let _x = foo r in
        ignore @@ Sys.opaque_identity _x
      done;
    );
    let elapsed_s = now_s () -. t_start in
    Printf.printf "OBJ: %d iterations in %.3fs (%.3f/s; %.fns / iteration)\n%!"
      n elapsed_s
      (float n /. elapsed_s) (elapsed_s /. float n *. 1e9);
  end;

  begin
    let open Rec in
    let t_start = now_s() in
    Sys.opaque_identity (
      let r = mk_reader() in
      for i=1 to n do
        let _x = foo r in
        ignore @@ Sys.opaque_identity _x
      done;
    );
    let elapsed_s = now_s () -. t_start in
    Printf.printf "REC: %d iterations in %.3fs (%.3f/s; %.1fns / iteration)\n%!"
      n elapsed_s
      (float n /. elapsed_s) (elapsed_s *. 1e9 /. float n);
  end;

  ()

