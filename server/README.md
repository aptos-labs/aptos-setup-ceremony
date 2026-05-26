# The ceremony server

This crate contains the code for the queue manager server. This server both
manages the queue and verifies each participant's contribution as they come
in.

## monitoring

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

- Have the client report failed speed tests to the server. In May 2025, we
  ran the ceremony for the Aptos encrypted mempool. The contribution
  success rate was 100% in terms of who joined the queue: anyone who joined
  was able to finish contributing. However, it would be nice to also know
  a more global success rate. It could be that some participants ran the
  client program but gave up when their speed tests failed.
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


