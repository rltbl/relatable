MAKEFLAGS += --warn-undefined-variables
SHELL := bash
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
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

.PHONY: test-tesh-doc-sqlx
test-tesh-doc-sqlx: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

.PHONY: test-tesh-misc
test-tesh-misc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common

.PHONY: test-tesh-misc-sqlx
test-tesh-misc-sqlx: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common

.PHONY: test-tesh-sqlite-only
test-tesh-sqlite-only: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/sqlite_only

.PHONY: test-tesh-sqlx-postgres-only
test-tesh-sqlx-postgres-only: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/postgres_only

.PHONY: test-random
test-random: debug
	bash test/random.sh --varying-rate

.PHONY: test-random-sqlx
test-random-sqlx: sqlx_debug
	bash test/random.sh --varying-rate

# TODO: Postgres is real slow. We need to ideally get the timeout back down to 5.
perf_test_timeout = 7.5
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

.PHONY: test_rusqlite
test_rusqlite: src/resources/main.js src/resources/main.css test-code test-tesh-doc test-tesh-misc test-random test-perf test-tesh-sqlite-only

.PHONY: test_sqlx
test_sqlx: src/resources/main.js src/resources/main.css test-code test-tesh-doc-sqlx test-tesh-misc-sqlx test-random-sqlx test-perf-sqlx

.PHONY: test_sqlx_postgres
test_sqlx_postgres: test_sqlx test-tesh-sqlx-postgres-only

.PHONY: test_sqlx_sqlite
test_sqlx_sqlite: test_sqlx test-tesh-sqlite-only

.PHONY: test
test: test_rusqlite

.PHONY: clean-test
clean-test:
	rm -Rf test/perf
