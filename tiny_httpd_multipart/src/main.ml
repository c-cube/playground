module H = Tiny_httpd
module Html = Tiny_httpd_html
module MF = Multipart_form

let spf = Printf.sprintf
let epf = Printf.eprintf

let setup_upload server : unit =
  H.add_route_handler server H.Route.(return) @@ fun _req ->
  let form =
    Html.(
      div []
        [
          form
            [
              A.action "/upload/";
              A.method_ "POST";
              A.enctype "multipart/form-data";
            ]
            [ input [ A.type_ "file"; A.name "f" ]; input [ A.type_ "submit" ] ];
        ])
  in
  H.Response.make_string @@ Ok (Html.to_string_top form)

let setup_upload_endpoint server : unit =
  H.add_route_handler_stream server ~meth:`POST
    H.Route.(exact "upload" @/ return)
  @@ fun req ->
  let content_type = H.Headers.get "content-type" req.headers |> Option.get in
  epf "content type: %S\n%!" content_type;

  let counter = ref 0 in
  let emitters (_ : MF.Header.t) =
    let filename = spf "/tmp/upload%d.tmp" !counter in
    incr counter;

    epf "emitter with filename=%S\n%!" filename;

    let oc = open_out filename in
    let emit = function
      | None -> close_out_noerr oc
      | Some str -> output_string oc str
    in
    emit, filename
  in

  let mf =
    MF.parse ~emitters
      (MF.Content_type.of_string (content_type ^ "\r\n") |> function
       | Ok x -> x
       | Error (`Msg msg) ->
         failwith (spf "parsing content type failed: %S" msg))
  in

  let n_bytes_body = ref 0 in
  let rec consume_body () =
    req.body.fill_buf ();
    let n = req.body.len in
    let next_chunk =
      if n = 0 then
        `Eof
      else (
        let str = Bytes.sub_string req.body.bs req.body.off n in
        req.body.consume n;
        n_bytes_body := !n_bytes_body + n;
        (* epf "read %S\n%!" str; *)
        `String str
      )
    in
    match mf next_chunk with
    | `Continue -> consume_body ()
    | `Fail msg ->
      epf "error: %s\n%!" msg;
      failwith "error while parsing body"
    | `Done tree -> tree
  in

  (* parse document *)
  let tree = consume_body () in
  epf "read %d body bytes\n%!" !n_bytes_body;

  let rec pr_tree = function
    | MF.Leaf elt -> spf "(file %S)" elt.body
    | MF.Multipart l ->
      spf "(multipart %s)"
        (String.concat "; "
        @@ List.map
             (function
               | None -> "None"
               | Some t -> pr_tree t)
             l.body)
  in

  epf "tree: %s\n%!" (pr_tree tree);

  H.Response.make_string
  @@ Ok {|<head> <meta http-equiv="Refresh" content="0; URL=https" /> </head>|}

let () =
  let port = try Sys.getenv "PORT" |> int_of_string with _ -> 8083 in
  let server = H.create ~port () in
  epf "listening on http://localhost:%d\n%!" port;

  setup_upload server;
  setup_upload_endpoint server;

  match H.run server with
  | Ok () -> ()
  | Error e ->
    epf "error: %s\n%!" (Printexc.to_string e);
    exit 1
