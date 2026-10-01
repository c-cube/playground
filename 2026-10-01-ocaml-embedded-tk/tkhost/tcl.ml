(* Tcl list syntax: written for host → Tcl lines and handler results, parsed
   for values Tcl stores in ocaml::ui. *)

(* Quote [s] as one list element. Backslash-escaping every special character
   is always valid list syntax, and newlines become \n, so the result never
   spans lines. *)
let quote s =
  if s = "" then "{}"
  else begin
    let b = Buffer.create (String.length s + 8) in
    String.iter
      (function
        | '\n' -> Buffer.add_string b "\\n"
        | '\r' -> Buffer.add_string b "\\r"
        | '\t' -> Buffer.add_string b "\\t"
        | '\x0b' -> Buffer.add_string b "\\v"
        | '\x0c' -> Buffer.add_string b "\\f"
        | ('\\' | '{' | '}' | '[' | ']' | '$' | '"' | ';' | ' ' | '#') as c ->
          Buffer.add_char b '\\';
          Buffer.add_char b c
        | c -> Buffer.add_char b c)
      s;
    Buffer.contents b
  end

let list l = String.concat " " (List.map quote l)
let dict kvs = list (List.concat_map (fun (k, v) -> [ k; v ]) kvs)

(* Objects become dicts, arrays lists, scalars strings, null the empty string. *)
let rec of_json : Yojson.Safe.t -> string = function
  | `Null -> ""
  | `Bool b -> if b then "1" else "0"
  | `Int i -> string_of_int i
  | `Intlit s -> s
  | `Float f -> Printf.sprintf "%.17g" f
  | `String s -> s
  | `List l -> list (List.map of_json l)
  | `Assoc kvs -> dict (List.map (fun (k, v) -> (k, of_json v)) kvs)

let is_space = function ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' -> true | _ -> false

(* Parse a Tcl list into its elements (like Tcl_SplitList). *)
let parse_list s =
  let n = String.length s in
  let i = ref 0 in
  let peek () = if !i < n then Some s.[!i] else None in
  let next () = let c = s.[!i] in incr i; c in
  let hex b max =
    let v = ref 0 and digits = ref 0 in
    let continue = ref true in
    while !continue && !digits < max && !i < n do
      match s.[!i] with
      | '0' .. '9' as c -> v := (!v * 16) + Char.code c - 48; incr digits; incr i
      | 'a' .. 'f' as c -> v := (!v * 16) + Char.code c - 87; incr digits; incr i
      | 'A' .. 'F' as c -> v := (!v * 16) + Char.code c - 55; incr digits; incr i
      | _ -> continue := false
    done;
    if !digits = 0 then false
    else begin
      let u = if Uchar.is_valid !v then Uchar.of_int !v else Uchar.rep in
      Buffer.add_utf_8_uchar b u;
      true
    end
  in
  (* the character(s) after a backslash, substituted *)
  let backslash b =
    if !i >= n then Buffer.add_char b '\\'
    else
      match next () with
      | 'n' -> Buffer.add_char b '\n'
      | 't' -> Buffer.add_char b '\t'
      | 'r' -> Buffer.add_char b '\r'
      | 'v' -> Buffer.add_char b '\x0b'
      | 'f' -> Buffer.add_char b '\x0c'
      | 'a' -> Buffer.add_char b '\x07'
      | 'b' -> Buffer.add_char b '\x08'
      | ('x' | 'u' | 'U') as c ->
        let max = match c with 'x' -> 2 | 'u' -> 4 | _ -> 8 in
        if not (hex b max) then Buffer.add_char b c
      | '\n' ->
        while peek () = Some ' ' || peek () = Some '\t' do incr i done;
        Buffer.add_char b ' '
      | c -> Buffer.add_char b c
  in
  let out = ref [] in
  let rec loop () =
    while !i < n && is_space s.[!i] do incr i done;
    if !i < n then begin
      let b = Buffer.create 16 in
      (match next () with
       | '{' ->
         let depth = ref 1 in
         while !depth > 0 do
           if !i >= n then failwith "unmatched open brace in list";
           match next () with
           | '\\' ->
             Buffer.add_char b '\\';
             if !i < n then Buffer.add_char b (next ())
           | '{' -> incr depth; Buffer.add_char b '{'
           | '}' -> decr depth; if !depth > 0 then Buffer.add_char b '}'
           | c -> Buffer.add_char b c
         done;
         if !i < n && not (is_space s.[!i]) then
           failwith "list element in braces followed by garbage"
       | '"' ->
         let closed = ref false in
         while not !closed do
           if !i >= n then failwith "unmatched open quote in list";
           match next () with
           | '"' -> closed := true
           | '\\' -> backslash b
           | c -> Buffer.add_char b c
         done;
         if !i < n && not (is_space s.[!i]) then
           failwith "list element in quotes followed by garbage"
       | _ ->
         decr i;
         while !i < n && not (is_space s.[!i]) do
           match next () with '\\' -> backslash b | c -> Buffer.add_char b c
         done);
      out := Buffer.contents b :: !out;
      loop ()
    end
  in
  loop ();
  List.rev !out
