# What it's about

This is the control plane for Trunk, a Linux distribution focused on making running a home-focused server easy. Most of the code is centered around managing server functionality such as compute and storage (via low level services and a package management system), serving content in ways that benefit home users (uPnP integration) and collecting statistics on all that. It is exposed via a REST interface with CBOR payloads and Problem Details per RFC for error handling.

It is currently deliberately unlicensed, meaning it is subject to conventional copyright and IP laws in the United States, California, and Internationally. It is expected to be released with a strict free software license, such as AGPL or MPL 2, on release. It is just being developed in the open so anyone can benefit from it.

The code will likely have its revision history reset at time of release; it is being developed open, and if people want to contribute, great, but the roadmap is not fully communicated in the issues list or elsewhere and pull requests are likely not going to be relevant for long. If you really do have something to share, wonderful! Thank you! However, a gist/patch would likely be better than a pull request.

## Development Instructions

To run tests: `make test`. It will run the tests as root. Do not use cargo directly as tests will likely not pass.

You need ZFS and systemd; look at the [platform](https://github.com/trunk-os/platform) Makefiles for support on setting the services up for playing with, but if you don't want to use real disks, they must be used to create a `trunk` zpool that will be modified by tests, backed by 200GB of sparsely allocated files. Tests can, on failure, leak volumes, datasets, temporary files and directories which may need to be cleaned up by hand, but most tests and the test utility functions go to great lengths to avoid doing that.

The first times the tests run, they will attempt to download an ubuntu disk image used for a few tests. This may take a while depending on your internet connectivity.
