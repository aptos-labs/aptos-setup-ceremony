
## Brainstorming timeout behavior

- client should test both upload/download speed and computation speed
  - upload/download: just upload/download to google cloud storage
  - computation: do tiny setup, extrapolate speed of real setup
- goal: 20s to kick if you are unresponsive at any stage
