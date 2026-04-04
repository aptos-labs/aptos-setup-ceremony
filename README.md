

# Aptos Trusted Setup Ceremony

## Instructions

You should have received a hex keypair. First step:

```
cargo run --release identify <keypair>
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
