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

The size calculator reads the bounds in `common/src/constants.rs`. It
reports the compressed CRS points, the BCS contribution core (before
participant metadata and history), and the actual exported file formats.
`digest_key.bin` also contains uncompressed FFT precomputations; `pp.bin`
contains the DKG range-proof parameters and a discrete-log lookup table.
The latter uses the 256-player configuration in `client/src/run.rs`.
The calculator does not generate a full-size setup or allocate the large
lookup table.

### Setup size with the default parameters

For batch size **128**, **216,000 rounds**, and maximum total share weight
**256** (4,096 hiding-KZG powers), BN254 points occupy 32 bytes in G1 and
64 bytes in G2 when compressed.

| Representation | Bytes | Decimal GB |
| --- | ---: | ---: |
| Compressed BTE CRS points, without framing or caches | 891,648,064 | 0.891648 |
| BCS ceremony contribution core | 914,463,576 | 0.914464 |
| Exported `digest_key.bin`, including FFT precomputations | 5,322,240,558 | 5.322241 |
| Exported `pp.bin`, including the discrete-log table | 621,020,081 | 0.621020 |
| Both exported files | 5,943,260,639 | 5.943261 |

A full contribution file additionally contains the contributor's name,
email, verification key, and prior-contribution metadata. The core size
above is independent of the number of contributors.

The compact BTE CRS has `216000 * (128 + 1)` G1 points and one G2 point.
Each round in `digest_key.bin` stores 129 powers and 256 cached G1 points,
each using 64-byte uncompressed encoding; its header occupies 558 bytes.
The `pp.bin` estimate uses the exporter's maximum aggregation of 256 and
includes a 16,777,216-entry discrete-log table. These are serialization
sizes, not peak memory requirements. The calculator's formulas are checked
against actual serialization and a small exported-file round trip.

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
