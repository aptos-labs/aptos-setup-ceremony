

# Aptos Trusted Setup Ceremony

## Prerequisites

You will need the following in order to contribute:

- Access to internal `aptos-labs` git repos
- Rust (see [here](https://rustup.rs/) to install)
- A reasonably fast computer (e.g. a MacBook with an M-series chip) and a fast internet connection

## Instructions

Before starting, clone this repo:

```
git clone git@github.com:aptos-labs/aptos-setup-ceremony; cd aptos-setup-ceremony
```

Or if you would rather download directly instead of using git, [visit this release page](https://github.com/aptos-labs/aptos-setup-ceremony/releases/tag/dress-rehearsal) 
and download the linked zip file.

You should have received an authentication code. First step:

```
cargo run --release identify <auth code>
```

If this step succeeds, run:

```
cargo run --release contribute
```

The client will first perform a download, compute, and upload speed test to
determine if the machine you are using is well-equipped enough to
contribute. Once that is finished, it will join the queue.

Note that when it is your turn, contributing will take up to 20 minutes,
and is compute-intensive. If contributing from a laptop, please have your
computer plugged in to avoid draining the battery.
