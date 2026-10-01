RUST := ../2026-10-01-rust-embedded-tk
SOCK ?= $(RUST)/chat.sock
BIN := _build/default/src/main.exe

build:
	@dune build --root . 2>&1

# the daemon is the Rust one
daemon:
	@$(MAKE) -s -C $(RUST) daemon

client: build
	@nohup $(BIN) -s $(SOCK) >>client.log 2>&1 &
	@echo "client started (log: client.log)"

.PHONY: build daemon client
