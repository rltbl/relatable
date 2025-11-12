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
.PHONY: debug rusqlite rusqlite_debug

debug: rusqlite_debug

rusqlite: rusqlite_debug

rusqlite_debug: target/debug/rltbl

target/debug/rltbl: src/resources/main.js src/resources/main.css FORCE
	cargo build

### Tokio-Postgres debuggable binary
.PHONY: tokio_postgres tokio_postgres_debug

tokio_postgres: tokio_postgres_debug

tokio_postgres_debug:
	cargo build --features tokio-postgres

### Rusqlite release binary
.PHONY: release rusqlite_release

release: rusqlite_release

rusqlite_release:
	cargo build --release

### Tokio-Postgres release binary
.PHONY: tokio_postgres_release

tokio_postgres_release:
	cargo build --release --features tokio-postgres

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

clean_postgres_test:
	rm -f test/tesh/common/*-postgres.md
	rm -f test/random-postgres.sh
	rm -Rf test/tesh/common/as_postgres

clean_sqlite_test:
	rm -f test/tesh/common/*-sqlite.md
	rm -f test/random-sqlite.sh
	rm -Rf test/tesh/common/as_sqlite

clean_test: clean_postgres_test clean_sqlite_test
	rm -Rf test/perf
	rm -Rf build/
	rm -Rf test/round_trip/output

### Code format and unit tests
.PHONY: test_fmt_and_unittest

test_fmt_and_unittest:
	cargo fmt --check
	cargo test

### Documentation tests
.PHONY: crate_docs crate_docs_tokio_postgres test_tesh_doc test_tesh_doc_postgres
crate_docs:
	RUSTDOCFLAGS="-D warnings" cargo doc

crate_docs_tokio_postgres:
	RUSTDOCFLAGS="-D warnings" cargo doc --features tokio-postgres

test_tesh_doc:
	cargo build --release --features rusqlite
	echo 'export RLTBL_CONNECTION=$(SQLITE_DB)' > doc/setup.sh
	PATH="$$(pwd)/target/release:$${PATH}"; tesh --debug false ./doc

test_tesh_doc_postgres:
	cargo build --release --features tokio-postgres
	echo 'export RLTBL_CONNECTION=$(PG_DB)' > doc/setup.sh
	PATH="$$(pwd)/target/release::$${PATH}"; tesh --debug false ./doc

### Round-trip load / validation tests
.PHONY: test_round_trip test_round_trip_sqlite test_round_trip_tokio_postgres

test/round_trip/output:
	mkdir -p $@

test_round_trip: test_round_trip_sqlite test_round_trip_tokio_postgres | test/round_trip/output

test_round_trip_sqlite: debug | test/round_trip/output
	@echo "Testing round trip on sqlite (rusqlite) ..."
	target/debug/rltbl -v --database $(SQLITE_DB) demo --size 0 --force
	target/debug/rltbl -v --database $(SQLITE_DB) load table --force test/round_trip/penguin.tsv
	target/debug/rltbl -v --database $(SQLITE_DB) save $|
	diff --strip-trailing-cr -q test/round_trip/penguin.tsv $|
	@echo "Success!"

test_round_trip_tokio_postgres: tokio_postgres_debug | test/round_trip/output
	@echo "Testing round trip on postgres ..."
	target/debug/rltbl -v --database $(PG_DB) demo --size 0 --force
	target/debug/rltbl -v --database $(PG_DB) load table --force test/round_trip/penguin.tsv
	target/debug/rltbl -v --database $(PG_DB) save $|
	diff --strip-trailing-cr -q test/round_trip/penguin.tsv $|
	@echo "Success!"

### SQLite tesh tests (rusqlite)
.PHONY: prepare_sqlite test_tesh_common_as_sqlite test_tesh_sqlite_only test_random_sqlite

test/tesh/common/as_sqlite:
	mkdir -p $@

prepare_sqlite: | test/tesh/common/as_sqlite
	for f in test/tesh/common/*.md; do cat test/tesh/common/sqlite-header._md $$f > $|/$$(basename $$f); done
	echo "$$(echo 'export RLTBL_CONNECTION=.relatable/relatable.db'; cat test/random.sh)" \
		> test/random-sqlite.sh

test_tesh_common_as_sqlite: debug prepare_sqlite
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common/as_sqlite

test_tesh_sqlite_only: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/sqlite_only

test_random_sqlite: debug prepare_sqlite
	bash test/random-sqlite.sh --varying-rate

### Postgres tesh tests
.PHONY: prepare_postgres test_tesh_tokio_postgres_common_as_postgres test_tesh_tokio_postgres_only test_random_tokio_postgres

test/tesh/common/as_postgres:
	mkdir -p $@

prepare_postgres: | test/tesh/common/as_postgres
	for f in test/tesh/common/*.md; do cat test/tesh/common/postgres-header._md $$f > $|/$$(basename $$f); done
	echo "$$(echo 'export RLTBL_CONNECTION=postgresql:///rltbl_db'; cat test/random.sh)" \
		> test/random-postgres.sh

test_tesh_tokio_postgres_common_as_postgres: tokio_postgres_debug prepare_postgres
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common/as_postgres

test_tesh_tokio_postgres_only: tokio_postgres_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/postgres_only

test_random_tokio_postgres: tokio_postgres_debug prepare_postgres
	bash test/random-postgres.sh --varying-rate

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

test_caching_postgres: tokio_postgres_debug
	target/debug/rltbl_test --database $(PG_DB) --caching trigger -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(PG_DB) --caching truncate -vv test-read-perf 100 100 10 5 --force
	target/debug/rltbl_test --database $(PG_DB) --caching truncate_all -vv test-read-perf 100 100 10 5 --force

test_caching_memory: debug
	target/debug/rltbl_test --database $(SQLITE_DB) --caching memory:100 -vv test-read-perf 100 100 10 5 --force

test_caching: test_caching_sqlite test_caching_postgres test_caching_memory

test_perf_sqlite: debug | test/perf/tsv
	target/debug/rltbl --database $(SQLITE_DB) demo --size $(perf_test_size) --force
	target/debug/rltbl --database $(SQLITE_DB) save $|
	target/debug/rltbl --database $(SQLITE_DB) init --force
	@echo "target/debug/rltbl --database $(SQLITE_DB) -vv load table --force $|/penguin.tsv"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl --database $(SQLITE_DB) -vv load table --force $|/penguin.tsv || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

### Postgres performance (rusqlite and sqlx)

test_perf_tokio_postgres: tokio_postgres_debug | test/perf/tsv
	target/debug/rltbl --database $(PG_DB) demo --size $(perf_test_size) --force
	target/debug/rltbl --database $(PG_DB) save $|
	target/debug/rltbl --database $(PG_DB) init --force
	@echo "target/debug/rltbl --database $(PG_DB) -vv load table --force $|/penguin.tsv"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl --database $(PG_DB) -vv load table --force $|/penguin.tsv || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

### Combined tests
.PHONY: test test_all test_rusqlite test_tokio_postgres

test_rusqlite: src/resources/main.js src/resources/main.css test_fmt_and_unittest test_tesh_doc test_round_trip_sqlite test_tesh_common_as_sqlite test_tesh_sqlite_only test_random_sqlite test_perf_sqlite test_caching_sqlite

test_tokio_postgres: src/resources/main.js src/resources/main.css test_round_trip_tokio_postgres test_tesh_tokio_postgres_common_as_postgres test_tesh_tokio_postgres_only test_random_tokio_postgres test_perf_tokio_postgres test_caching_postgres

# test: test_rusqlite
test: test_fmt_and_unittest test_tesh_doc test_tesh_doc_postgres

test_all: test_rusqlite test_tokio_postgres
