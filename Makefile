.DEFAULT_GOAL := help

DEBUG_LOG ?= $(CURDIR)/target/pi-watch-debug.log

.PHONY: help test check build open stop restart restart-debug

help:
	@printf '%s\n' 'make test     Test extension logic and product versions' 'make check    Run extension, package, and native checks' 'make build    Build and sign the app' 'make open     Open the built app' 'make stop     Stop the running app' 'make restart  Rebuild, stop, and reopen the app' 'make restart-debug  Rebuild and restart in the foreground with debug logs'

test:
	npm test

check:
	npm test
	npm run pack:check
	npm run check:app

build:
	sh scripts/bundle.sh

open:
	open "$(CURDIR)/dist/Pi Watch.app"

stop:
	@pkill -x pi-watch || test $$? -eq 1
	@for attempt in 1 2 3 4 5 6 7 8 9 10; do \
		if ! pgrep -x pi-watch >/dev/null; then exit 0; fi; \
		sleep 0.2; \
	done; \
	printf '%s\n' 'Pi Watch did not stop within two seconds.' >&2; exit 1

restart: build
	$(MAKE) stop
	$(MAKE) open

restart-debug: build
	$(MAKE) stop
	@mkdir -p "$(dir $(DEBUG_LOG))"
	@printf 'Debug log: %s\n' "$(DEBUG_LOG)"
	@bash -o pipefail -c 'PI_WATCH_DEBUG=1 "$(CURDIR)/dist/Pi Watch.app/Contents/MacOS/pi-watch" 2>&1 | tee "$(DEBUG_LOG)"'
