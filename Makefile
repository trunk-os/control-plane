define sudo_cargo
	sudo -E bash -c '. ~/.cargo/env && cargo $(1) $(2) $(3) $(4)'
endef

test: charon/testdata/ubuntu.img
	sudo rm -rf charon/testdata/registry/installed gild/testdata/charon/installed
	$(call sudo_cargo, "test", "--", "--nocapture")

build:
	$(call sudo_cargo, "build")

clean:
	$(call sudo_cargo, "clean")
	cargo clean
	sudo rm -rf buckle/tmp charon/tmp gild/tmp

charon/testdata/ubuntu.img:
	cd charon && make
