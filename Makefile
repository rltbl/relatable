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

.PHONY: debug
debug: target/debug/nanobot

target/debug/nanobot: Cargo.* src/** src/resources/main.js src/resources/main.css
	cargo build ${CARGOFLAGS}

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
