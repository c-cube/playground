
DUNE_OPTS?=--profile=release
all:
	dune build $(DUNE_OPTS) @install

WATCH?=@check
watch:
	dune build $(DUNE_OPTS) -w $(WATCH)
