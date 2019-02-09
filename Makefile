libs = $(sort $(dir $(wildcard ./libs/*/*)))
apps = $(sort $(dir $(wildcard ./apps/*/*)))

.PHONY: build
build:
	$(foreach libdir, $(libs), pushd $(libdir); cargo build; popd;)
	$(foreach appdir, $(apps), pushd $(appdir); cargo build; popd;)

