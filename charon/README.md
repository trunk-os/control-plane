# Charon is for describing how packages run

More to come here, watch this space.

## How to run the livetests (integration tests)

First, run `make get-testdata` to download some small disk images (250MB) to the `testdata/` directory. They are gitignored.

```bash
sudo `which cargo` test --features livetests -- --nocapture
```
