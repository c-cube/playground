(* Writing Tcl lists: queue lines, primitive results, event payloads. *)

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
