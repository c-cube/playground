module Fmt = CCFormat
module Hash = Digestif.SHA1

let ( let@ ) = ( @@ )
let spf = Printf.sprintf
let ( / ) = Filename.concat
let use_mmap = ref false

let hash_of_blob_read filename : Hash.t =
  let@ _sp =
    Trace.with_span ~__FILE__ ~__LINE__ "hash-blob" ~data:(fun () ->
        [ "f", `String filename ])
  in
  let ic = open_in filename in
  let ln = in_channel_length ic in
  let rec go buf ctx =
    match input ic buf 0 (Bytes.length buf) with
    | 0 | (exception End_of_file) -> Hash.get ctx
    | len ->
      let ctx = Hash.feed_bytes ctx buf ~len in
      go buf ctx
  in
  let ctx = Hash.empty in
  let str = spf "blob %d\000" ln in
  let ctx = Hash.feed_string ctx str in
  let res = go (Bytes.create 0x10_000) ctx in
  close_in ic;
  res

let hash_of_blob_mmap filename : Hash.t =
  let@ _sp =
    Trace.with_span ~__FILE__ ~__LINE__ "hash-blob" ~data:(fun () ->
        [ "f", `String filename ])
  in

  let ic = open_in filename in
  let ln = in_channel_length ic in

  let fd = Unix.descr_of_in_channel ic in
  let map : Digestif.bigstring =
    Unix.map_file fd Bigarray.Char Bigarray.C_layout false [| ln |]
    |> Bigarray.array1_of_genarray
  in

  let ctx = Hash.empty in
  let str = spf "blob %d\000" ln in
  let ctx = Hash.feed_string ctx str in
  let ctx = Hash.feed_bigstring ctx map in

  close_in ic;
  Hash.get ctx

let hash_of_blob file =
  if !use_mmap then
    hash_of_blob_mmap file
  else
    hash_of_blob_read file

module Seq = struct
  let rec hash_of_tree filename : Hash.t =
    let entries = Sys.readdir filename in
    let entries =
      List.map
        (fun v ->
          let filename = filename / v in
          if Sys.is_directory filename then
            `Dir, filename
          else
            `Normal, filename)
        (Array.to_list entries)
    in
    hash_of_entries entries

  and hash_of_entries entries : Hash.t =
    let entries =
      List.map
        (function
          | `Dir, filename ->
            let name = Filename.basename filename in
            let hash = hash_of_tree filename in
            spf "40000 %s\000%s" name (Hash.to_raw_string hash)
          | `Normal, filename ->
            let name = Filename.basename filename in
            let hash = hash_of_blob filename in
            spf "100644 %s\000%s" name (Hash.to_raw_string hash))
        entries
    in
    let ctx = Hash.empty in
    let str =
      spf "tree %d\000"
        (List.fold_left (fun acc str -> acc + String.length str) 0 entries)
    in
    let ctx = Hash.feed_string ctx str in
    let ctx =
      List.fold_left (fun ctx str -> Hash.feed_string ctx str) ctx entries
    in
    Hash.get ctx

  let hash_files (l : string list) : Hash.t =
    let entries =
      List.map
        (fun s ->
          let kind =
            if Sys.is_directory s then
              `Dir
            else
              `Normal
          in
          kind, s)
        l
    in
    hash_of_entries entries
end

module Par = struct
  open Moonpool

  let rec hash_of_tree filename : Hash.t =
    (* Fmt.eprintf "hash of tree %S@." filename; *)
    let entries = Sys.readdir filename in
    let entries =
      List.map
        (fun v ->
          let filename = filename / v in
          if Sys.is_directory filename then
            `Dir, filename
          else
            `Normal, filename)
        (Array.to_list entries)
    in
    hash_of_entries entries

  and hash_of_entries entries : Hash.t =
    (* let@ _sp = Trace.with_span ~__FILE__ ~__LINE__ "hash-entries" in *)
    let entries =
      Fork_join.map_list
        (function
          | `Dir, filename ->
            let name = Filename.basename filename in
            let hash = hash_of_tree filename in
            spf "40000 %s\000%s" name (Hash.to_raw_string hash)
          | `Normal, filename ->
            let name = Filename.basename filename in
            let hash = hash_of_blob filename in
            spf "100644 %s\000%s" name (Hash.to_raw_string hash))
        entries
    in
    let ctx = Hash.empty in
    let str =
      spf "tree %d\000"
        (List.fold_left (fun acc str -> acc + String.length str) 0 entries)
    in
    let ctx = Hash.feed_string ctx str in
    let ctx =
      List.fold_left (fun ctx str -> Hash.feed_string ctx str) ctx entries
    in
    Hash.get ctx

  let hash_files ~j (l : string list) : Hash.t =
    (* let@ _sp = Trace.with_span ~__FILE__ ~__LINE__ "hash-files" in *)
    let per_domain, min =
      if j = 0 then
        Some 1, None
      else
        None, Some j
    in
    let@ pool = Pool.with_ ?min ?per_domain () in

    let entries =
      List.map
        (fun s ->
          let kind =
            if Sys.is_directory s then
              `Dir
            else
              `Normal
          in
          kind, s)
        l
    in
    Pool.run_wait_block pool (fun () -> hash_of_entries entries)
end

let () =
  (* Tracy_client_trace.setup (); *)
  let seq = ref false in
  let j = ref 0 in
  let opts =
    [
      "-seq", Arg.Set seq, " sequential";
      "-j", Arg.Set_int j, " parallelism";
      "-mmap", Arg.Set use_mmap, " use mmap";
    ]
    |> Arg.align
  in
  let files = ref [] in

  Arg.parse opts (fun s -> files := s :: !files) "";
  match !files with
  | [] -> Fmt.eprintf "give at least one file"
  | files ->
    let h =
      if !seq then
        Seq.hash_files files
      else
        Par.hash_files ~j:!j files
    in
    Format.printf "%a@." Hash.pp h
