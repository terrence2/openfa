SHELL := /bin/bash

libs = $(sort $(dir $(wildcard ./libs/*/*)))
apps = $(sort $(dir $(wildcard ./apps/*/*)))

.PHONY: build
build:
	$(foreach libdir, $(libs), pushd $(libdir); cargo build; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo build; popd;)

.PHONY: clippy
clippy:
	$(foreach libdir, $(libs), pushd $(libdir); cargo clippy -- -Dwarnings; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo clippy -- -Dwarnings; popd;)

.PHONY: fmt
fmt:
	$(foreach libdir, $(libs), pushd $(libdir); cargo fmt; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo fmt; popd;)

.PHONY: check-fmt
check-fmt:
	$(foreach libdir, $(libs), pushd $(libdir); cargo fmt -- --check; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo fmt -- --check; popd;)

.PHONY: test
test:
	$(foreach libdir, $(libs), pushd $(libdir); cargo test; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo test; popd;)

.PHONY: clean
clean:
	$(foreach libdir, $(libs), pushd $(libdir); cargo clean; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo clean; popd;)

.PHONY: release-windows
release-windows:
	$(foreach appdir, $(apps), pushd $(appdir); cross build --target x86_64-pc-windows-gnu; popd;)
