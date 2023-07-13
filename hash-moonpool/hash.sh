#!/bin/sh
OPTS="--display=quiet --profile=release"
exec dune exec $OPTS src/test.exe -- $@
