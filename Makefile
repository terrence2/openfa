libs =  $(sort $(dir $(wildcard ./libs/*/*)))

.PHONY: build
build:
	$(foreach libdir, $(libs), pushd $(libdir); cargo build; popd;)

