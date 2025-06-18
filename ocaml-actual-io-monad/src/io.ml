type src =
  (* String of string * int ref | *)
  | IC of in_channel

type sink = Buf of Buffer.t | OC of out_channel

type 'a t =
  | Return : 'a -> 'a t
  | Fail : exn * Printexc.raw_backtrace -> 'a t
  | Bind : ('a -> 'b t) * 'a t -> 'b t
  | Map : ('a -> 'b) * 'a t -> 'b t
  | Pair : 'a t * 'b t -> ('a * 'b) t
  | Read : src * bytes * int * int -> int t
  | Write : sink * bytes * int * int -> unit t

let rec run : type a. a t -> a = function
  | Return x -> x
  | Fail (e, bt) -> Printexc.raise_with_backtrace e bt
  | Bind (f, x) ->
      let x = run x in
      run (f x)
  | Map (f, x) -> f (run x)
  | Pair (a, b) ->
      let a = run a in
      let b = run b in
      (a, b)
  | Read (IC ic, bs, i, len) -> input ic bs i len
  | Write (Buf buf, bs, i, len) -> Buffer.add_subbytes buf bs i len
  | Write (OC oc, bs, i, len) -> output oc bs i len

let[@inline] return x = Return x
let[@inline] ( let* ) x f = Bind (f, x)
let[@inline] ( let+ ) x f = Map (f, x)
let[@inline] ( and+ ) x y = Pair (x, y)
let ( and* ) = ( and+ )
let read src bs i len : int t = Read (src, bs, i, len)
let write sink bs i len : unit t = Write (sink, bs, i, len)

let write_string sink str : unit t =
  write sink (Bytes.unsafe_of_string str) 0 (String.length str)

let write_line sink str : unit t =
  let* () = write_string sink str in
  write_string sink "\n"
