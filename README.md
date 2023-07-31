## Building

Before building, rename `rust-toolchain.toml` temporarily to something else, or else it likely won't build properly.

Make sure you have Docker installed and running. Next run the following command to build inside docker:
```console
docker run -v %cd%:/volume --rm -t clux/muslrust:1.73.0-nightly-2023-07-30 cargo build --release
```

or if you're on Linux:
```console
docker run -v $PWD:/volume --rm -t clux/muslrust:1.73.0-nightly-2023-07-30 cargo build --release
```
The binary will be in target/x86_64-unknown-linux-musl/release/ and will be named `zama-fileserver`.

## Running
After you've confirmed that you have target/x86_64-unknown-linux-musl/release/zama-fileserver file, you can run it with:

```console
docker-compose up
```

## Experimental
Also check out the `experimental` branch https://github.com/TheoXD/zama-fileserver/tree/experimental for an alternative solution that doesn't use Merkle Trees or blake3.
