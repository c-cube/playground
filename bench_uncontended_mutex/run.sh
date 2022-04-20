#!/bin/sh
dune build --profile=release bin/main.exe
./_build/default/bin/main.exe
hyperfine './_build/default/bin/main.exe'

echo 'run: `qalc "610ms / 40M"`'
qalc '610ms / 40M'
