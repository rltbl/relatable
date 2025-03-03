MAKEFLAGS += --warn-undefined-variables
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := debug
.DELETE_ON_ERROR:
.SUFFIXES:

.PHONY: usage
usage:
	@echo "make [TASK]"
	@echo "  debug      build debug binary"

.PHONY: debug release sqlx sqlx_debug sqlx_release rusqlite rusqlite_debug rusqlite_release

debug: rusqlite_debug

release: rusqlite_release

sqlx: sqlx_debug

sqlx_debug:
	cargo build --features sqlx

sqlx_release:
	cargo build --release --features sqlx

rusqlite: rusqlite_debug

rusqlite_debug:
	cargo build

rusqlite_release:
	cargo build --release

debug-serve: target/debug/rltbl
	$< serve --port 3000

src/resources/:
	mkdir -p $@

src/resources/main.%: rltbl-frontend/build/main.% | src/resources/
	cp $< $@

rltbl-frontend/build/main.js: rltbl-frontend/package.* rltbl-frontend/src/*
	cd rltbl-frontend \
	&& npm install \
	&& npm run build \
	&& cp build/static/js/main.js build/main.js \
	&& cp build/static/css/main.*.css build/main.css

rltbl-frontend/build/main.css: rltbl-frontend/build/main.js

### Tests

.PHONY: test-code
test-code: debug
	cargo fmt --check
	cargo test

.PHONY: test-docs
test-docs: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

# TODO: Add these to the tesh history tests and then remove this target
.PHONY: examples
examples:
	bash test/example_commands.sh

# TODO: Replace with a randomly operation sequence generator:
.PHONY: random
random:
	test/history/wrapper.sh

# TODO: Re-enable the random test once we've settled on the output format.
.PHONY: test
test: src/resources/main.js src/resources/main.css test-code test-docs examples # random
