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

rusqlite_debug: target/debug/rltbl
	cargo build

rusqlite_release:
	cargo build --release

target/debug/rltbl: Cargo.* src/** src/resources/main.js src/resources/main.css
	cargo build

debug-serve: target/debug/rltbl
	$< serve --port 3000 -vvv

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
test-code: target/debug/rltbl
	cargo fmt --check
	cargo test

.PHONY: test-tesh-doc
test-tesh-doc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

.PHONY: test-tesh-misc
test-tesh-misc: debug
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./test

.PHONY: test-random
test-random: debug
	test/random.sh --varying-rate

perf_test_timeout = 5
perf_test_size = 100000

test/perf/tsv:
	mkdir -p $@

test/perf/tsv/penguin.tsv: debug | test/perf/tsv
	target/debug/rltbl demo --size $(perf_test_size) --force
	target/debug/rltbl save test/perf/tsv/

.PHONY: test-perf
test-perf: test/perf/tsv/penguin.tsv
	target/debug/rltbl init --force
	@echo "target/debug/rltbl -vvv load table $<"
	@timeout $(perf_test_timeout) time -p target/debug/rltbl -vvv load table $< || \
		(echo "Performance test took longer than $(perf_test_timeout) seconds." && false)

.PHONY: test
test: src/resources/main.js src/resources/main.css test-code test-tesh-doc test-tesh-misc test-random test-perf

.PHONY: clean-test
clean-test:
	rm -Rf test/perf

# Build a Linux binary using Musl instead of GCC.
target/x86_64-unknown-linux-musl/release/rltbl: Cargo.toml src/*.rs src/templates/* rltbl-frontend/build/main.css
	mv Cargo.toml Cargo.toml.bk
	sed 's/"bundled", //' Cargo.toml.bk > Cargo.toml
	docker pull clux/muslrust:stable
	docker run \
		--platform linux/amd64 \
		-v cargo-cache:/root/.cargo/registry \
		-v $$PWD:/volume \
		--rm -t clux/muslrust:stable \
		cargo build --release
	mv Cargo.toml.bk Cargo.toml

.PHONY: musl
musl: target/x86_64-unknown-linux-musl/release/rltbl

.PHONY: push
push: target/x86_64-unknown-linux-musl/release/rltbl
	scp $< dev:/var/www/tdt-demo/bin/

.PHONY: pub
pub: target/x86_64-unknown-linux-musl/release/rltbl
	scp $< dev:/var/www/james.overton.ca/files/

.PHONY: cebs
cebs: target/x86_64-unknown-linux-musl/release/rltbl
	scp $< dev:/home/knocean/cebs-ddd-dev/bin/
