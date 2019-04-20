SHELL := /bin/bash

libs = $(sort $(dir $(wildcard ./libs/*/*)))
apps = $(sort $(dir $(wildcard ./apps/*/*)))

.PHONY: check-outdated
check-outdated:
	$(foreach libdir, $(libs), pushd $(libdir); cargo outdated; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo outdated; popd;)

.PHONY: update
update:
	$(foreach libdir, $(libs), pushd $(libdir); cargo update; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo update; popd;)
