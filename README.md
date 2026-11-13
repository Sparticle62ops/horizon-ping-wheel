# horizon-ping-wheel (1:1 port)

Exact 1:1 Rust translation of [Ping Wheel](https://modrinth.com/mod/ping-wheel)
(LukenSkyne, MIT) for Horizon.

Upstream logic carried 1:1 in `mixins/ping_wheel/runtime.rs` as a Horizon
client runtime splice (`crates/horizon-client/src/game.rs`): all fourteen
modules (`global`, `common_client`, `integration`, `screen`, `render`,
`compat`, `config`, `network`, `math`, `util`, `resource`, `core`, `seams`)
plus the test module, with `crate::` mapped to `super::` for the carrier.
Screens (`SettingsScreen` with its nine options, channel EditBox and Done
button), renderers (overlay, direction indicator, ping location), the network
play channel, and the client integration (screen-factory seam, team-context
ladder voice-chat / FTB / vanilla / none) match the upstream source 1:1.

Ported by sparticle.

## License

Upstream Ping Wheel is MIT, Copyright (c) LukenSkyne. This port carries the
upstream license and credit per the port policy.
