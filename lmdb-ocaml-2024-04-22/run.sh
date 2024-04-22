#!/bin/sh
exec dune exec --display=quiet --profile=release src/main.exe -- $@
