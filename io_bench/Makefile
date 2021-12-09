all: f_500M build
	@echo "### buffered"
	hyperfine --warmup 2 './run buf f_500M'
	@echo "### unix"
	hyperfine --warmup 2 './run unix f_500M'
	@echo "### by char"
	hyperfine --warmup 2 './run char f_500M'

f_500M:
	dd if=/dev/zero of=$@ bs=1M count=500 status=progress

build:
	@dune build io.exe --profile=release
	@ln -sf _build/default/io.exe run
