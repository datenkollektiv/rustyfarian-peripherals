# signwriter

A `no_std`, alloc-free driver for 128x64 I2C OLED panels that works with
whichever controller is plugged in: **SSD1306 or SH1106**, hot-swappable while
the firmware runs.
The application only sees an [`embedded-graphics`](https://docs.rs/embedded-graphics-core)
`DrawTarget<Color = BinaryColor>`.

## Design

- **Runtime detection.** Both controllers answer at `0x3C` and have no ID
  register.
  Only the SH1106 can read its display RAM back, so `signwriter` writes two
  different values to an off-screen column and reads them back.
  Both returned means SH1106; anything else means SSD1306.
- **Hot-plug.** `Oled::tick(now_ms)` probes the address, debounces attach and
  detach through `tamer::hotplug::Hotplug`, re-detects and re-initialises on
  every attach, and keeps accepting draw calls while the panel is gone.
- **Page mode, dirty pages.** Only changed pages are sent, in writes of at most
  31 bytes.
- **Sans-sleep.** The caller supplies millisecond timestamps; `signwriter`
  never delays.
- **Bus ownership.** `Oled` owns an `embedded_hal::i2c::I2c`; share a bus with
  `embedded-hal-bus` if needed.

## Layers

1. Stateless functions: `probe`, `detect`, `init`, `display_on`, `write_page`.
2. `Framebuffer`: an I/O-free 1 KiB page-major buffer with a dirty mask.
3. `Oled`: the blocking runner that ties both to `Hotplug`.

## Example

```rust
use embedded_graphics_core::{pixelcolor::BinaryColor, prelude::*};
use signwriter::{Event, Oled, ADDRESS_PRIMARY};

# fn run<I: embedded_hal::i2c::I2c>(i2c: I, now_ms: u64) {
let mut oled = Oled::new(i2c, ADDRESS_PRIMARY, 200, 3, 250);
Pixel(Point::new(3, 9), BinaryColor::On).draw(&mut oled).ok();
if let Some(Event::Attached(controller)) = oled.tick(now_ms) {
    let _ = controller; // Ssd1306 or Sh1106
}
# }
```

## License

Licensed under either of Apache License 2.0 or MIT license at your option.
