# mio 1.2.4 for the Switch

mio 1.2.4 from crates.io, changed in two places for the Nintendo Switch build
(`packaging/switch/cargo-patch.toml` patches it in for that build only):

- `src/sys/unix/selector/poll.rs`: `PollFlagInt` is `c_short` on the Switch (aarch64 Horizon), whose
  newlib `pollfd` has short `events`; mio's `target_os = "horizon"` code was written for the 3DS,
  whose flags are `int`.
- `src/sys/unix/net.rs`: the Switch's `sockaddr_in` has a BSD `sin_len` (zeroed, as on the BSDs).

The game's server uses mio for its players' network threads (`lan/io_pool.rs`). MIT licensed
(`LICENSE`).
