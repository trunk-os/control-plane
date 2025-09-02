test: charon/testdata/ubuntu.img
	sudo rm -rf charon/testdata/registry/installed gild/testdata/charon/installed
	sudo -E `which cargo` test -- --test-threads 1 --nocapture

charon/testdata/ubuntu.img:
	cd charon && make
