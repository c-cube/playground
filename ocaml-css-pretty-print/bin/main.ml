module H = Tiny_httpd
module Html = Tiny_httpd_html

let spf = Printf.sprintf

type t =
  | Var of string
  | App of t * t
  | Lam of string * t

let var s = Var s
let app f t = App (f, t)
let app_l = List.fold_left app
let lam x t = Lam (x, t)
let lam_l = List.fold_right lam

let rec mkchurch = function
  | 0 -> lam_l [ "f"; "s" ] (var "s")
  | n ->
    let f = spf "f%d" n in
    let s = spf "s%d" n in
    lam_l [ f; s ] (app (var f) (app_l (mkchurch (n - 1)) [ var f; var s ]))

let tuple l = app_l (var "tuple") l

let rec to_html = function
  | Var vname -> Html.[ span [ A.class_ "var" ] [ txt vname ] ]
  | App (f, t) ->
    Html.[ txt "("; span [ A.class_ "app" ] (to_html f @ to_html t); txt ")" ]
  | Lam (x, t) ->
    Html.
      [
        txt "(λ";
        span
          [ A.class_ "lam" ]
          [
            span [ A.class_ "var" ]
            @@ List.flatten [ [ txt x; txt "." ]; to_html t ];
          ];
        txt ")";
      ]

let css =
  {|
.app {
  display: inline-flex;
  flex: 1 1 auto;
  align-content: flex-start;
  /*row-gap: 10px;*/
  flex-wrap: wrap;
  flex-direction: row;
}

.lam {
  display: inline-flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  /*row-gap: 10px;*/
  align-content: flex-start;
  flex-direction: row;
}

|}

let () =
  let port = 3000 in
  let server = H.create ~port () in
  H.add_route_handler server
    H.Route.(exact "css" @/ return)
    (fun _req -> H.Response.make_string @@ Ok css);
  H.add_route_handler server
    H.Route.(return)
    (fun _req ->
      let html =
        Html.(
          html []
            [
              header []
                [
                  meta [ A.charset "utf-8" ];
                  link [ A.rel "stylesheet"; A.href "/css" ];
                ];
              body []
                [
                  h1 [] [ txt "term" ];
                  pre
                    [ A.style "width: 90%; display: flex; flex: 1 0 auto;" ]
                    (to_html @@ tuple [ mkchurch 10; mkchurch 20; mkchurch 10 ]);
                ];
            ])
      in
      H.Response.make_string @@ Ok (Html.to_string_top html));
  Printf.printf "running on http://localhost:%d\n%!" port;
  H.run_exn server
