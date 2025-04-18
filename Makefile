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

.PHONY: debug
debug: rusqlite_debug

.PHONY: release
release: rusqlite_release

.PHONY: sqlx
sqlx: sqlx_debug

.PHONY: sqlx_debug
sqlx_debug:
	cargo build --features sqlx

.PHONY: sqlx_release
sqlx_release:
	cargo build --release --features sqlx

.PHONY: rusqlite
rusqlite: rusqlite_debug

.PHONY: rusqlite_debug
rusqlite_debug:
	cargo build

.PHONY: rusqlite_release
rusqlite_release:
	cargo build --release

.PHONY: debug-serve
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
clean: clean_test

.PHONY: cleanall
cleanall: clean
	cargo clean

### Tests

# Code format test

.PHONY: test_code
test_code:
	cargo fmt --check

# Documentation tests

.PHONY: test_tesh_doc
test_tesh_doc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

.PHONY: test_tesh_doc_sqlx
test_tesh_doc_sqlx: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

# SQLite tests (rusqlite)

test/tesh/common/as_sqlite:
	mkdir -p $@

.PHONY: prepare_sqlite
prepare_sqlite: | test/tesh/common/as_sqlite
	for f in test/tesh/common/*.md; do cat test/tesh/common/sqlite-header._md $$f > $|/$$(basename $$f); done
	echo "$$(echo 'export RLTBL_CONNECTION=.relatable/relatable.db'; cat test/random.sh)" \
		> test/random-sqlite.sh

.PHONY: test_tesh_common_as_sqlite
test_tesh_common_as_sqlite: debug prepare_sqlite
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common/as_sqlite

.PHONY: test_tesh_sqlite_only
test_tesh_sqlite_only: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/sqlite_only

.PHONY: test_random_sqlite
test_random_sqlite: debug prepare_sqlite
	bash test/random-sqlite.sh --varying-rate

# SQLite tests (sqlx)

.PHONY: test_tesh_sqlx_common_as_sqlite
test_tesh_sqlx_common_as_sqlite: sqlx_debug prepare_sqlite
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common/as_sqlite

.PHONY: test_tesh_sqlx_sqlite_only
test_tesh_sqlx_sqlite_only: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/sqlite_only

.PHONY: test_random_sqlx_sqlite
test_random_sqlx_sqlite: sqlx_debug prepare_sqlite
	bash test/random-sqlite.sh --varying-rate

# Postgres tests (sqlx)

test/tesh/common/as_postgres:
	mkdir -p $@

.PHONY: prepare_postgres
prepare_postgres: | test/tesh/common/as_postgres
	for f in test/tesh/common/*.md; do cat test/tesh/common/postgres-header._md $$f > $|/$$(basename $$f); done
	echo "$$(echo 'export RLTBL_CONNECTION=postgresql:///rltbl_db'; cat test/random.sh)" \
		> test/random-postgres.sh

.PHONY: test_tesh_sqlx_common_as_postgres
test_tesh_sqlx_common_as_postgres: sqlx_debug prepare_postgres
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/common/as_postgres

.PHONY: test_tesh_sqlx_postgres_only
test_tesh_sqlx_postgres_only: sqlx_debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test/tesh/postgres_only

.PHONY: test_random_sqlx_postgres
test_random_sqlx_postgres: sqlx_debug prepare_postgres
	bash test/random-postgres.sh --varying-rate

# Performance tests

test/perf/tsv:
	mkdir -p $@

test/perf/tsv/penguin.tsv: | test/perf/tsv
	target/debug/rltbl demo --size $(perf_test_size) --force
	target/debug/rltbl save test/perf/tsv/

perf_test_timeout = 7.5
perf_test_size = 100000

# SQLite performance (rusqlite and sqlx)

sqlite_db = ".relatable/relatable.db"
pg_db = "postgresql:///rltbl_db"

.PHONY: test_perf_sqlite
test_perf_sqlite: test/perf/tsv/penguin.tsv debug
	target/debug/rltbl --database $(sqlite_db) init --force
	@echo "target/debug/rltbl --database $(sqlite_db) -vv load table --force $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl --database $(sqlite_db) -vv load table --force $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

.PHONY: test_perf_sqlx_sqlite
test_perf_sqlx_sqlite: test/perf/tsv/penguin.tsv sqlx_debug
	target/debug/rltbl --database $(sqlite_db) init --force
	@echo "target/debug/rltbl --database $(sqlite_db) -vv load table --force $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl --database $(sqlite_db) -vv load table --force $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

# Postgres performance (rusqlite and sqlx)

.PHONY: test_perf_sqlx_postgres
test_perf_sqlx_postgres: test/perf/tsv/penguin.tsv sqlx_debug
	target/debug/rltbl --database $(pg_db) init --force
	@echo "target/debug/rltbl --database $(pg_db) -vv load table --force $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl --database $(pg_db) -vv load table --force $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

# Combined tests

.PHONY: test_rusqlite
test_rusqlite: src/resources/main.js src/resources/main.css test_code test_tesh_doc test_tesh_common_as_sqlite test_tesh_sqlite_only test_random_sqlite test_perf_sqlite

.PHONY: test_sqlx_sqlite
test_sqlx_sqlite: src/resources/main.js src/resources/main.css test_code test_tesh_doc_sqlx test_tesh_sqlx_common_as_sqlite test_tesh_sqlx_sqlite_only test_random_sqlx_sqlite test_perf_sqlx_sqlite

.PHONY: test_sqlx_postgres
test_sqlx_postgres: src/resources/main.js src/resources/main.css test_code test_tesh_doc_sqlx test_tesh_sqlx_common_as_postgres test_tesh_sqlx_postgres_only test_random_sqlx_postgres test_perf_sqlx_postgres

.PHONY: test
test: test_rusqlite

# Test cleaning

.PHONY: clean_postgres_test
clean_postgres_test:
	rm -f test/tesh/common/*-postgres.md
	rm -f test/random-postgres.sh
	rm -Rf test/tesh/common/as_postgres

.PHONY: clean_sqlite_test
clean_sqlite_test:
	rm -f test/tesh/common/*-sqlite.md
	rm -f test/random-sqlite.sh
	rm -Rf test/tesh/common/as_sqlite

.PHONY: clean_test
clean_test: clean_postgres_test clean_sqlite_test
	rm -Rf test/perf
