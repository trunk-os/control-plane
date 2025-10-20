SUDO_ENV := sudo -E bash -c

define sudo_cargo
	$(SUDO_ENV) '. ~/.cargo/env && cargo $(@)'
endef

test: charon/testdata/ubuntu.img
	sudo rm -rf charon/testdata/registry/installed gild/testdata/charon/installed
	@$(call sudo_cargo, "test", "--", "--test-threads", "1", "--nocapture")

build:
	@$(call sudo_cargo, "build")

clean:
	@$(call sudo_cargo, "clean")
	cargo clean
	sudo rm -rf buckle/tmp charon/tmp gild/tmp

charon/testdata/ubuntu.img:
	cd charon && make
