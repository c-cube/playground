BIN := target/debug/tkchat
SOCK ?= ./chat.sock

build:
	@cargo build -q

# start a daemon in the background, unless one already answers on $(SOCK)
# (restarts it if the binary was rebuilt since; history is lost)
daemon: build
	@if [ -f daemon.pid ] && [ $(BIN) -nt daemon.pid ] && $(BIN) -s $(SOCK) ls >/dev/null 2>&1; then \
	  echo "daemon is older than $(BIN), restarting"; $(MAKE) -s stop; sleep 0.3; \
	fi
	@if $(BIN) -s $(SOCK) ls >/dev/null 2>&1; then \
	  echo "daemon already running on $(SOCK)"; \
	else \
	  nohup $(BIN) -s $(SOCK) daemon >>daemon.log 2>&1 & echo $$! > daemon.pid; \
	  sleep 0.2; echo "daemon started (log: daemon.log)"; \
	fi

# open a new GUI client in the background
client: build
	@nohup $(BIN) -s $(SOCK) gui >>client.log 2>&1 &
	@echo "client started (log: client.log)"

stop:
	@if [ -f daemon.pid ]; then kill `cat daemon.pid` && echo "daemon stopped"; rm -f daemon.pid; fi

.PHONY: build daemon client stop
