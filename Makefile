MAKEFLAGS += --warn-undefined-variables
SHELL := bash
.DEFAULT_GOAL := debug
.DELETE_ON_ERROR:
.SUFFIXES:

SQLITE_DB = ".relatable/relatable.db"
PG_DB = "postgresql:///rltbl_db"

.PHONY: usage
usage:
	@echo "make [TASK]"
	@echo "  debug      build debug binary"

# See https://www.gnu.org/software/make/manual/html_node/Force-Targets.html
FORCE:

### Rusqlite debuggable binary
.PHONY: debug

debug: target/debug/rltbl

target/debug/rltbl: src/resources/main.js src/resources/main.css FORCE
	cargo build

### Rusqlite release binary
.PHONY: release

release:
	cargo build --release

### To start the server
.PHONY: debug-serve
debug-serve: target/debug/rltbl
	$< serve --port 3000 -vvv

### Frontend stuff
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

### To clean
.PHONY: clean cleanall clean_postgres_test clean_sqlite_test clean_test

clean: clean_test

cleanall: clean
	cargo clean

clean_test:
	rm -Rf test/perf
	rm -Rf build/

### Code format and unit tests
.PHONY: cargo_test

cargo_test:
	cargo fmt --check
	cargo test
	RLTBL_TEST_CONNECTION=$(PG_DB) cargo test

### Documentation tests
.PHONY: crate_docs test_tesh_doc test_tesh_doc_postgres
crate_docs:
	RUSTDOCFLAGS="-D warnings" cargo doc

test_tesh_doc: release
	echo 'export RLTBL_CONNECTION=$(SQLITE_DB)' > doc/setup.sh
	PATH="$$(pwd)/target/release:$${PATH}"; tesh --debug false ./doc

test_tesh_doc_postgres: release
	echo 'export RLTBL_CONNECTION=$(PG_DB)' > doc/setup.sh
	PATH="$$(pwd)/target/release::$${PATH}"; tesh --debug false ./doc

### Performance tests

test/perf/tsv:
	mkdir -p $@

perf_test_timeout = 9.5
perf_test_size = 100000

### SQLite performance (rusqlite and sqlx)
.PHONY: test_caching_sqlite test_caching_postgres test_caching_memory test_caching test_perf_sqlite test_perf_tokio_postgres

test_caching_sqlite: debug
	target/debug/rltbl_test --database $(SQLITE_DB) --caching trigger -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(SQLITE_DB) --caching truncate -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(SQLITE_DB) --caching truncate_all -vv test-read-perf 100 100 10 5 --force

test_caching_postgres: debug
	target/debug/rltbl_test --database $(PG_DB) --caching trigger -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(PG_DB) --caching truncate -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(PG_DB) --caching truncate_all -vv test-read-perf 100 100 10 5 --force

test_caching_memory: debug
	target/debug/rltbl_test --database $(SQLITE_DB) --caching memory:100 -vv test-read-perf 100 100 10 5 --force

test_caching: test_caching_sqlite test_caching_postgres test_caching_memory

test_perf_sqlite: release | test/perf/tsv
	target/release/rltbl --database $(SQLITE_DB) demo --size $(perf_test_size) --force
	target/release/rltbl --database $(SQLITE_DB) save $|
	target/release/rltbl --database $(SQLITE_DB) init --force
	@echo "target/release/rltbl --database $(SQLITE_DB) -vv load table --force $|/penguin.tsv"
	@timeout $(perf_test_timeout) time -p target/release/rltbl --database $(SQLITE_DB) -vv load table --force $|/penguin.tsv || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

### Postgres performance (rusqlite and sqlx)

test_perf_tokio_postgres: release | test/perf/tsv
	target/release/rltbl --database $(PG_DB) demo --size $(perf_test_size) --force
	target/release/rltbl --database $(PG_DB) save $|
	target/release/rltbl --database $(PG_DB) init --force
	@echo "target/release/rltbl --database $(PG_DB) -vv load table --force $|/penguin.tsv"
	@timeout $(perf_test_timeout) time -p target/release/rltbl --database $(PG_DB) -vv load table --force $|/penguin.tsv || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

### Combined tests
.PHONY: test test_all test_rusqlite test_tokio_postgres

test_rusqlite: src/resources/main.js src/resources/main.css test_tesh_doc test_perf_sqlite test_caching_sqlite

test_tokio_postgres: src/resources/main.js src/resources/main.css test_tesh_doc_postgres test_perf_tokio_postgres test_caching_postgres

# test: test_rusqlite
# test: cargo_test test_tesh_doc test_tesh_doc_postgres
test: cargo_test test_rusqlite test_tokio_postgres

test_all: test_rusqlite test_tokio_postgres
