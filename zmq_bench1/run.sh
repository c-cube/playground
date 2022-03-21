#!/bin/sh
dune build --profile=release server.exe
dune build --profile=release client.exe

./_build/default/server.exe &

for i in `seq 1 4`; do
  ./_build/default/client.exe &
done

wait client
kill %1
