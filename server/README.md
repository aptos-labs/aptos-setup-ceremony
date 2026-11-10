# The ceremony server

This crate contains the code for the queue manager server. This server both
manages the queue and verifies each participant's contribution as they come
in.

## Configuration

The root `config.toml` contains example values. Before starting the server,
set `SERVER_GCP_PROJECT_ID`, `SERVER_BUCKET_ID`, and
`SERVER_ADMIN_VERIFYING_KEY` (the hex-encoded Ed25519 public key of your
ceremony administrator). Configure Google Cloud credentials using
`GOOGLE_APPLICATION_CREDENTIALS` or application-default credentials.
The configured account needs access to the ceremony's storage bucket.
All configuration fields can be overridden with the `SERVER_` prefix.

For Docker Compose, also set `CEREMONY_IMAGE`, `CEREMONY_HOST`, and
`GOOGLE_APPLICATION_CREDENTIALS` to an absolute path to your credentials
file. The database is stored in the `ceremony_data` Docker volume.
`scripts/build_image.sh` builds and pushes `CEREMONY_IMAGE`;
`scripts/redeploy_server.sh` uses `CEREMONY_INSTANCE` and `CEREMONY_ZONE`
and expects the checkout in `~/ceremony` on that machine.

The cloud-backed integration harness can be run with
`cargo run --locked -p e2e-test` after configuring credentials,
`SERVER_GCP_PROJECT_ID`, and a separate `SERVER_BUCKET_ID` for testing.

## Monitoring

```
 ~/.local/bin/harlequin -a duckdb ceremony.sqlite3
lnav logs/server.log
```

## TODO for debugging high memory

- Restart and see what happens
- Run w/ ContributionFilesStore behind an Arc<Mutex<.>>
- Run w/ small number of tokio worker threads

## Future improvements

Adding this section here for the whole project so as not to pollute the
main README.md, which contains instructions for contributors.

- Have the client report failed speed tests to the server, so that
  completion statistics also account for participants who fail a speed
  test before joining the queue.
- Human-readable speed test output. Right now the download and upload speed
  test failure messages report times taken to download/upload versus
  cutoff. Would be good to report in terms of Mbps speed, so that
  participants can more easily debug their connection speed.
- Queue length statistics. It would be nice to know max/min/avg queue
  length, times queue is hot, etc. At very least should record
  per-contributor wait time.
- Automated email invite functionality. It might be nice to have the system
  handle directly the sending of invite emails with auth codes, to send
  reminders, etc. Although I'm not convinced the RoI is that great here for
  the ceremony sizes we are planning on.
- Better client-server API error handling. Right now this error handling story is _bad_. Would be great to define an error enum, just like right now there's a Message enum that defines the possible API calls.
