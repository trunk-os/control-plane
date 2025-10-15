test: charon/testdata/ubuntu.img
	sudo rm -rf charon/testdata/registry/installed gild/testdata/charon/installed
	sudo -E bash -c '. ~/.cargo/env && cargo test -- --test-threads 1 --nocapture'

build:
	sudo -E bash -c '. ~/.cargo/env && cargo build'

clean:
	sudo -E bash -c '. ~/.cargo/env && cargo clean'
	cargo clean
	sudo rm -rf buckle/tmp charon/tmp gild/tmp

charon/testdata/ubuntu.img:
	cd charon && make
