# Bench — ESP32-C3 SuperMini, EC11 rotary rig

A second C3 SuperMini that carries only the EC11 rotary encoder.
It exists because the encoder's B input and the `c3` rig's 5V pogo both need GPIO 1 ([ADR-015](../../docs/adr/015-pogo-ports.md)).
The pins follow the C3 map of [ADR-012](../../docs/adr/012-esp32-c3-pin-map.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| EC11 A | 0 | common to GND; internal pull-up |
| EC11 B | 1 | common to GND; internal pull-up |
| EC11 button | 5 | to GND; internal pull-up |

- Nothing else is fitted: no pogos, no B3F, no LED.
- With the `c3` rig attached too, pass `--port`, because detection cannot tell two C3 SuperMinis apart.
- A MAC pin in the git-ignored `bench/bench.local.toml` replaces `--port` once the runner pin moves past `v0.6.0`.

```sh
just bench c3-rotary rotary --unattended --port <port>
just bench c3-rotary rotary --attended --no-build --port <port>
```

Checks: `rotary`.
