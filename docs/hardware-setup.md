# Hardware Setup Guide

This guide will cover the physical wiring and pin configuration for running
`rustyfarian-peripherals` input drivers on supported boards.

It is a **skeleton** today: no drivers or device examples exist yet, so there is
nothing to wire. As the first downstream-driven primitives and their
`{driver}_{chip}_{name}` examples land, populate the per-board sections below
using the table formats provided — one wiring table per input type per board,
matching the style used in `rustyfarian-power`'s hardware-setup guide.

---

## Supported boards

ESP32 (RISC-V and Xtensa), via the esp-hal (bare-metal) and esp-idf (std) tiers.
Specific dev boards are listed here as examples target them.

| Board               | Chip     | Tier(s)  | Status                         |
|:--------------------|:---------|:---------|:-------------------------------|
| CrowPanel 1.28" HMI | ESP32-S3 | esp-idf  | rotary encoder (idf_s3_rotary) |

---

## Wiring — debounced button (template)

Fill in when the debounce driver lands.

| Signal  | GPIO  | Pull                       | Notes                     |
|:--------|:------|:---------------------------|:--------------------------|
| Button  | —     | internal/external pull-up? | active-low vs active-high |

General notes to capture here when known: required pull direction, whether the
chip's internal pull resistors are used or external ones are needed, and any
contact-bounce timing observed (feeds the debounce window default in `tamer`).

---

## Wiring — EC11 rotary encoder (CrowPanel 1.28" HMI / ESP32-S3)

EC11 full-step encoder (4 quadrature states per detent) with integral push button.

| Signal      | GPIO    | Pull             | Notes                                           |
|:------------|:--------|:-----------------|:------------------------------------------------|
| A / CLK     | GPIO 45 | internal pull-up | quadrature channel A; persistent AnyEdge ISR    |
| B / DT      | GPIO 42 | internal pull-up | quadrature channel B; persistent AnyEdge ISR    |
| Button / SW | GPIO 41 | internal pull-up | active-low (pressed = LOW); polled for debounce |
| +           | 3V3     | —                | power                                           |
| −           | GND     | —                | ground                                          |

The A and B quadrature channels are monitored via persistent AnyEdge GPIO interrupts
registered directly against the ESP-IDF C API; every edge is captured regardless of
main-loop latency. Button timing (debounce, click, double-click, long-press) is
polled via [`Encoder::update`], so call it regularly (a 1 ms loop is typical).
The driver delegates all decode logic to `tamer::rotary::QuadratureDecoder` and
`tamer::button::ButtonDecoder`.
