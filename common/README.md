

## Brainstorming timeout behavior

- client should test both upload/download speed and computation speed
  - upload/download: just upload/download to google cloud storage
  - computation: do tiny setup, extrapolate speed of real setup
- goal: 20s to kick if you are unresponsive at any stage


## parameters

Assume $20$ rounds per second. With baked-in margin, assume $30$ rounds per
second.

So the number of rounds we need to support, for a two-hour epoch, is $30*60*60*2 = 216000$.

We should maybe set number of CTs per round to 256, to also have margin in
that regard.


## misc

- What should the repr be for ContributionInner? Projective or Affine?
