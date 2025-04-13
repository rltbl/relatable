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

.PHONY: clean
clean: clean-test

.PHONY: cleanall
cleanall: clean
	cargo clean

### Tests

.PHONY: test-code
test-code:
	cargo fmt --check

.PHONY: test-tesh-doc
test-tesh-doc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc 2>/dev/null

.PHONY: test-tesh-doc-sqlx
test-tesh-doc-sqlx: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc 2>/dev/null

.PHONY: test-tesh-misc
test-tesh-misc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test 2>/dev/null

.PHONY: test-tesh-misc-sqlx
test-tesh-misc-sqlx: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test 2>/dev/null

.PHONY: test-random
test-random: debug
	test/random.sh --varying-rate

.PHONY: test-random-sqlx
test-random-sqlx: sqlx_debug
	test/random.sh --varying-rate

# TODO: Postgres is real slow. We need to ideally get the timeout back down to 5.
perf_test_timeout = 15
perf_test_size = 100000

test/perf/tsv:
	mkdir -p $@

test/perf/tsv/penguin.tsv: | test/perf/tsv
	target/debug/rltbl demo --size $(perf_test_size) --force
	target/debug/rltbl save test/perf/tsv/

.PHONY: test-perf
test-perf: test/perf/tsv/penguin.tsv debug
	target/debug/rltbl init --force
	@echo "target/debug/rltbl -vv load table --force $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl -vv load table --force $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

.PHONY: test-perf-sqlx
test-perf-sqlx: test/perf/tsv/penguin.tsv sqlx_debug
	target/debug/rltbl init --force
	@echo "target/debug/rltbl -vv load table --force $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl -vv load table --force $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

.PHONY: test
test: src/resources/main.js src/resources/main.css test-code test-tesh-doc test-tesh-misc test-random test-perf

.PHONY: test_sqlx
test_sqlx: src/resources/main.js src/resources/main.css test-code test-tesh-doc-sqlx test-tesh-misc-sqlx test-random-sqlx test-perf-sqlx

.PHONY: clean-test
clean-test:
	rm -Rf test/perf
