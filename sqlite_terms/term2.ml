

#require "containers";;
#require "sqlite3";;
#require "sqlite3_utils";;

module Fmt = CCFormat
module DB = Sqlite3_utils;;
let db = Sqlite3.db_open "terms2.db";;

#show DB.Cursor;;

DB.exec0 db {|
  CREATE TABLE IF NOT EXISTS
  term(id INTEGER PRIMARY KEY, kind TEXT NOT NULL, value TEXT NOT NULL);
|};;

DB.exec0 db {|
  CREATE INDEX IF NOT EXISTS term_kind ON term(kind);
|};;

module Term : sig
  type t = private int
  type view =
    | App of t * t
    | Var of string
    | Lam of string * t

  val equal : t -> t -> bool
  val view : t -> view

  val var : string -> t
  val app : t -> t -> t

  val all : unit -> t list
end = struct
  type t = int
  type view =
    | App of t * t
    | Var of string
    | Lam of string * t

  let equal : t -> t -> bool = (=)

  let var s =
    DB.transact db @@ fun _ ->
    match
      DB.exec_exn db {|
        SELECT id FROM term
        WHERE kind = 'var'
        AND json_extract(value, '$.name') = ?
        |} ~ty:DB.Ty.([text], [int], fun x->x) ~f:DB.Cursor.next s
    with
    | Some t -> t
    | None ->
      DB.exec_no_cursor_exn db {|
        INSERT INTO term(kind, value) VALUES('var', json_object('name', ?));
      |} ~ty:DB.Ty.[text] s;
      Int64.to_int @@ Sqlite3.last_insert_rowid db

  let app f a =
    DB.transact db @@ fun _ ->
    match
      DB.exec_exn db {|
        SELECT id FROM term
        WHERE kind = 'app'
        AND value = json_array(?,?)
        |} ~ty:DB.Ty.([int;int], [int], fun x->x) ~f:DB.Cursor.next
        f a
    with
    | Some id -> id
    | None ->
      DB.exec_no_cursor_exn db {|
        INSERT INTO term(kind, value)
        VALUES('app', json_array(?,?));
      |} ~ty:DB.Ty.[int; int] f a;
      Int64.to_int @@ Sqlite3.last_insert_rowid db

  let view id =
    match
      DB.exec_exn db {|
        SELECT id, kind
        FROM term WHERE id=?
        |} ~ty:DB.Ty.([int], [int;text], fun x y->x, y)
        ~f:DB.Cursor.get_one_exn id
    with
    | id, "app" ->
      DB.exec_exn db
        {| SELECT json_extract(value, '$[0]'),
                  json_extract(value, '$[1]')
        FROM term WHERE id=? |}
        ~ty:DB.Ty.([int], [int;int], fun x y -> App (x,y))
        ~f:DB.Cursor.get_one_exn id
    | id, "var" ->
      DB.exec_exn db
        {| SELECT json_extract(value, '$.name')
          FROM term WHERE id=? |}
        ~ty:DB.Ty.([int], [text], fun x -> Var x)
        ~f:DB.Cursor.get_one_exn id

    | _ -> assert false (* TODO *)

  let all () =
    DB.exec_no_params_exn db {| SELECT id FROM term; |}
      ~ty:DB.Ty.([int], fun x->x)
      ~f:DB.Cursor.to_list_rev
end;;

let rec pp_term out (t:Term.t) =
  match Term.view t with
  | Term.App (f,a) ->
    Fmt.fprintf out "(@[%a@ %a@])/%d" pp_term f pp_term a (t:>int)
  | Term.Var s -> Fmt.string out s
  | Term.Lam (x,bod) -> Fmt.fprintf out "(@[fun %s.@ %a@])" x pp_term bod
;;

#install_printer pp_term;;

Term.all();;

let f = Term.var "f";;
let a = Term.var "a";;
let b = Term.var "b";;
Term.equal a (Term.var "a");;

Term.app (Term.app f a) b;;
