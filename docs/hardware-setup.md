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

| Board | Chip | Tier(s) | Status |
|:------|:-----|:--------|:-------|
| _TBD with first example_ | — | — | — |

---

## Wiring — debounced button (template)

Fill in when the debounce driver lands.

| Signal | GPIO | Pull | Notes |
|:-------|:-----|:-----|:------|
| Button | — | internal/external pull-up? | active-low vs active-high |

General notes to capture here when known: required pull direction, whether the
chip's internal pull resistors are used or external ones are needed, and any
contact-bounce timing observed (feeds the debounce window default in `tamer`).

---

## Wiring — rotary encoder (template)

Fill in when the rotary driver lands.

| Signal | GPIO | Pull | Notes |
|:-------|:-----|:-----|:------|
| Encoder A | — | pull-up? | quadrature channel A |
| Encoder B | — | pull-up? | quadrature channel B |
| Push switch | — | pull-up? | optional integrated button |

Capture here: detents-per-revolution vs. quadrature-states-per-detent for the
specific encoder (drives the detent handling in `tamer::rotary`), and whether
hardware RC filtering is present on the A/B lines.
