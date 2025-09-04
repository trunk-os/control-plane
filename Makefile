test: charon/testdata/ubuntu.img
	sudo rm -rf charon/testdata/registry/installed gild/testdata/charon/installed
	sudo -E `which cargo` test -- --test-threads 1 --nocapture

clean:
	sudo `which cargo` clean
	cargo clean
	sudo rm -rf buckle/tmp charon/tmp	gild/tmp

charon/testdata/ubuntu.img:
	cd charon && make
