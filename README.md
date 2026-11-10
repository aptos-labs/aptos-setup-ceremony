# TrX Trusted Setup Ceremony — S&P 2027 Artifact

This artifact implements a multi-party ceremony for the common reference
string used by our batch threshold encryption scheme. It adapts
powers-of-tau techniques from [Bowe, Gabizon, and Miers](https://eprint.iacr.org/2017/1050)
and [Kohlweiss et al.](https://eprint.iacr.org/2021/219).

This branch uses **BN254**, matching the paper's evaluation. The ceremony
and bundled batch-encryption code use the same curve aliases. Public-input
hashing uses the bundled SHA3-512 try-and-increment hash-to-curve routine,
with separate BN254-specific domains for ceremony proofs and batch
encryption.

## Build and test

Download and extract the anonymous repository ZIP, then open a terminal in
its root directory. Install [Rust](https://rustup.rs/); the toolchain is
specified in `rust-toolchain.toml`. On Linux, a C compiler and `lld` are
also required by the build configuration.

```sh
cargo build --locked --release -p client -p server
cargo test --locked -p common --lib
cargo test --locked -p server --lib
```

The BN254-specific checks and serialization-size checks can be run with:

```sh
cargo test --locked --release -p common --test bn254 --example setup_size
cargo run --locked --release -p common --example setup_size
```

The size calculator reads the bounds in `common/src/constants.rs`.

### Setup size with the default parameters

For batch size **128** and **216,000 rounds**, the BN254 BTE CRS contains
`216000 * (128 + 1)` G1 points and one G2 point. Its compressed size is
**891.65 MB** (decimal).

The five Aptos dependency crates required by this implementation are
included in `vendor/` as local path dependencies. Cargo downloads the
remaining public dependencies normally.

## Repository structure

- `common/src/powers_of_tau.rs`: powers-of-tau contributions and verification.
- `common/src/fptx.rs`: ceremony for the batch-encryption CRS.
- `common/src/hiding_kzg.rs`: hiding-KZG setup used by distributed key generation.
- `common/src/aptos.rs`: combines the setup components and checks their output.
- `client/`: contribution, transcript verification, and output extraction.
- `server/`: participant queue, contribution verification, and GCS storage.
- `e2e-test/`: integration harness requiring a configured GCP project.

## Run a ceremony

Configure a Google Cloud Storage project and bucket, administrator key,
and credentials as described in [server/README.md](server/README.md).
The example deployment files use configurable values rather than a live
ceremony service. Start the server with:

```sh
cargo run --locked --release -p server
```

Set the client endpoint to your server (the default is local port 8888):

```sh
export CEREMONY_SERVER_ADDRESS=http://127.0.0.1:8888
cargo run --locked --release -p client -- identify '<auth code>'
cargo run --locked --release -p client -- contribute
```

Use an authentication code supplied by the administrator of your ceremony.
The client tests download, computation, and upload performance before
joining the queue. Contributions are compute-intensive; keep laptops
connected to power. See `cargo run --locked -p client -- --help` for
offline transcript verification and CRS extraction commands.
