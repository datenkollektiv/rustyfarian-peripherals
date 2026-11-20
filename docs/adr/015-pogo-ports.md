# ADR-015: Bench pogo set — I2C, 3V3, 5V and button pin

## Status
Accepted (2026-11-19).
It amends [ADR-012](012-esp32-c3-pin-map.md): the C3 3-pin pogo row becomes the 3V3 pogo, and the button pin stays GPIO 3 and may sit on a button pogo.
The 5V pogo shares GPIO 1 with the rotary B input.
It corrects the CrowPanel I2C sentence of [ADR-014](014-board-profiles.md).
The family rule lives in the rustyfarian-ci convention `conventions/pogo.md`; this record adopts it and keeps the reasoning.
Amended (2026-11-19): the EC11 moves to a second C3 rig, `c3-rotary`, and an open-collector module brings its own pull-up, both as `conventions/pogo.md` 1.0.0 has it.

## Context
Only the C3 rig has pogo headers today: a 4-pin I2C pogo on GPIO 6 / 7 and one 3-pin pogo (GND, 3V3, SIG) on GPIO 4.

- That one signal pin carries the 3V3 sensors as inputs and the WS2812 data line as an output.
  - `rustyfarian-rgb-clock` drives its WS2812 from the same pin.
- 5 V parts have no home, although WS2812 parts are specified at a 5 V supply and some sensors need one.
- The button is wired straight to the board on some rigs and would sit better on a pogo on others.
- The C6, ESP32 and Feather V2 have no pogo at all, so each example picks its own pin.
- Each board also needs one input that wakes it from deep sleep, and that pin differs from board to board.

The electrical limits shape the design.

- ESP32-family GPIOs are not 5 V tolerant; the absolute maximum is VDD + 0.3 V, about 3.6 V.
- A WS2812B on a 5 V supply needs data above 0.7 × VDD = 3.5 V.
  - 3.3 V data is out of spec, but most parts accept it.
- No passive translator on one pin serves both directions.
  - A BSS138 MOSFET shifter suits open-drain inputs but is marginal at the WS2812's 800 kHz.
  - A TXB/TXS auto-direction translator suits the WS2812 but misbehaves with pull-ups.
  - Either removes the ADC from the pin.
- A direction-controlled translator works both ways but costs a third GPIO for its direction pin.

## Decision
1. Every bench board with free headers carries three pogos and one button pin:

| Pogo       | Pin order             | Use                                                                         |
|:-----------|:----------------------|:----------------------------------------------------------------------------|
| **I2C**    | GND / 3V3 / SDA / SCL | Qwiic order, 100 kHz, 4.7 kΩ pull-ups from SDA and SCL to 3V3 on the rig    |
| **3V3**    | GND / 3V3 / SIG       | parts that run at 3.3 V, analog or digital                                  |
| **5V**     | GND / 5V / SIG        | parts that need a 5 V supply; WS2812 data out, 5 V sensors in               |
| **Button** | GND / 3V3 / SIG       | optional per rig; a B3F from SIG to GND or a touch module; wake input       |

2. All signal pins carry 3V3 logic and go straight to the GPIO; only the 5V pogo's power pin differs.
   - The rig carries no level translator.
3. The 3V3 pogo pin is on ADC1, which keeps working alongside Wi-Fi, and it can wake the chip from deep sleep.
4. The 5V pogo pin is output-capable, works in both directions and can wake the chip; analog is not promised.
5. A module adapts itself to the port, because only the module knows its signal direction.
   - A WS2812 module takes 3V3 data directly; if a part is marginal, the module carries a 74AHCT1G125 or powers its first pixel through a diode.
   - A sensor with a 5 V push-pull output carries a divider on the module.
   - An open-collector module brings its own pull-up to 3V3, never to 5 V (amended 2026-11-19).
   - The MCU's internal pull-up is not enough there: ESP32 GPIO 34 has none.
   - A pull-up fitted on the rig is no substitute, because it would distort analog parts on the 3V3 pogo.
6. A part that runs at 3.3 V always goes on the 3V3 pogo.
7. The 5V pogo is marked red or keyed, because a 3V3 module plugged into it gets 5 V.
   - The 3V3 and button pogos share one layout, so swapping those two harms nothing.
8. The button pin is fixed per board; its form is the rig's choice.
   - A rig solders the B3F, attaches it directly from the pin to GND, or puts it on a button pogo.
   - The pin is the wake input, read active-low.
   - A B3F needs a pull-up that holds through deep sleep: the internal RTC/LP pull-up, or 10 kΩ to 3V3 on the rig where the firmware tier drops it (family convention).
   - Only a button pogo takes a TTP223-style touch module, set to active-low (pad A closed).
   - A touch module on the 3V3 pogo keeps its default active-high output and wakes the board on a high level.
9. The pogos sit on these pins:

| Board key       | I2C SDA / SCL        | 3V3 pogo      | 5V pogo       | Button pin   |
|:----------------|:---------------------|:--------------|:--------------|:-------------|
| `c3`            | 6 / 7                | 4 (ADC1_CH4)  | 1 (ADC1_CH1)  | 3            |
| `c6`            | 21 / 22              | 2 (ADC1_CH2)  | 3 (ADC1_CH3)  | 4            |
| `esp32`         | 21 / 22              | 34 (ADC1_CH6) | 25 (ADC2_CH8) | 4            |
| `esp32-feather` | 22 / 20 (STEMMA QT)  | 34 (A2)       | 25 (A1)       | 4 (A5)       |

   - The I2C pins are the [ADR-014](014-board-profiles.md) board profile pins; this record does not move them.
   - The 5V pogo is fed from the 5V pin (USB VBUS) on `c3`, J1 pin 14 on `c6`, the VIN pin on `esp32` and the USB pin on `esp32-feather`.
   - `c3`: the 5V pogo shares GPIO 1 with the EC11 B input, so the EC11 sits on a second C3 rig, `c3-rotary` (amended 2026-11-19).
   - `c6`: J1 pins 15 / 14 / 13 are GND / 5V / GPIO 3, one strip already in pogo order; J1 pin 1 is the board's only 3V3 pin, so it feeds all three 3V3 pins.
   - `esp32` and `esp32-feather`: GPIO 34 is input-only with no internal pull-up; GPIO 4 is touch channel T0.
   - `esp32-feather`: the I2C pogo is the STEMMA QT socket through a JST-SH lead; GPIO 2 switches its power, as the board profile already does, and the on-board SW38 (GPIO 38) stays an extra button.
   - Both ESP32 boards share the 3V3, 5V and button pins, so only the I2C pins differ, and the board profile covers those.

The pins were chosen by these rules.

- No strapping, USB, UART or flash pins, and none of the pins the board's other bench parts use.
- The 3V3 pogo pin is on ADC1 and wake-capable (C3 GPIO 0–5, C6 LP GPIO 0–7, ESP32 RTC GPIOs).
- The 5V pogo pin is output-capable and wake-capable; ADC1 is a bonus where it was free.
- The button pin is wake-capable and does not select the boot mode, because a wake while it is held re-reads the straps.
  - C6 GPIO 4 (MTMS) is a strapping pin, but it sets only the SDIO sampling edge, so it qualifies.
- Pins already used by a passed gate keep their role: C3 GPIO 3, 4, 6 and 7, C6 GPIO 4, ESP32 GPIO 4.

10. A bare capacitive touch pad is not part of the set, because the C3 and C6 have no touch sensor peripheral.
11. Fully wired boards that cannot carry the set are out of scope; they keep their own wiring.
   - The CrowPanel (`s3`) has only GPIO 4 and 12 free, on its 12-pin 0.5 mm FPC (vendor Eagle schematic `ESP32 Display-1.28-V1.0.sch`).
   - The FPC carries VCC_5V (pins 1–2, unswitched), GPIO 4 (pin 3), GPIO 12 (pin 4), UART and I2C shifted to 5 V (pins 5–8), USB (pins 9–10) and GND (pins 11–12); no connector carries 3V3.
   - An FPC adapter could carry a 5V pogo on GPIO 12, and a 3V3 pogo on GPIO 4 with an LDO; no pin is left for a button.
   - Its external I2C connector J36 (GPIO 38 / 39) is shifted to 5 V by BSN20 MOSFETs and has a switched 5 V supply, so it does not match the 3V3 I2C pogo.
   - The CrowPanel wakes from its touch panel INT (GPIO 5), as `rustyfarian-knob` does; GPIO 41 (knob button) is not an RTC pin, and GPIO 0 (BOOT) is a strapping pin.
12. Examples name the table's GPIO as constants until a second board needs the same example.
   - Then the pogo pins join the board profiles, as the I2C pins did.

## Consequences
- The C3 keeps GPIO 3, 4, 6 and 7, so its passed gates (`b3f`, `poti`, `hall-linear`, the I2C checks) stay valid.
  - The rig rewiring is mechanical: a 5V pogo is added on GPIO 1; the B3F may stay soldered.
- On the C6 and the ESP32 the B3F stays on GPIO 4, so their `b3f` gates stay valid.
- The Feather V2 gains a button on GPIO 4; its OLED checks keep using the STEMMA QT socket.
- WS2812 data moves to the 5V pogo.
  - `rustyfarian-rgb-clock` moves its C3 WS2812 from GPIO 4 to GPIO 1; that is a follow-up in its own repository.
  - WS2812 driving stays out of scope here (it belongs to `rustyfarian-ws2812`); this repository uses the 5V pogo for 5 V sensors only.
- On the C3, the EC11 and the 5V pogo both use GPIO 1, so the EC11 check runs on the `c3-rotary` rig (amended 2026-11-19).
  - The rotary examples do not change; `c3-rotary` is a second C3 SuperMini with only the EC11 fitted.
  - With both C3 rigs attached, a bench run picks one by `--port`.
- On the ESP32 boards, GPIO 34 has no internal pull-up, so an open-collector module on the 3V3 pogo carries its own pull-up to 3V3.
- On the ESP32 boards, the 5V pogo pin is on ADC2, which does not read while Wi-Fi runs.
- A ratiometric analog sensor run from 5 V (a 49E Hall sensor) can exceed 3.3 V, so analog parts belong on the 3V3 pogo.
- The 5V pogo is unpowered when a board runs from a battery without USB.
- The rig adds 4.7 kΩ I2C pull-ups on the C6 and the ESP32 as on the C3; module pull-ups in parallel stay fine at 100 kHz.
- The bench board pages list the three pogos and the button pin, and say whether the button is direct or on a pogo; a pogo stays "not fitted yet" until it is wired.
- The library crates, the `no_std` / `std` boundary and the tier APIs do not change.

## Alternatives Considered

|                                                Alternative | Pros                                         | Cons                                                                        | Why Rejected                                                  |
|-----------------------------------------------------------:|:---------------------------------------------|:----------------------------------------------------------------------------|:--------------------------------------------------------------|
|              One pogo, one GPIO, supply chosen by a jumper | One pin per board                            | Sensor and RGB never at once; a wrong jumper puts 5 V on a 3V3 part         | Input-to-light setups need both ports at the same time        |
|           Level translator on the 5V pogo (BSS138, TXB0101) | 5 V signals plug in directly                 | Neither type serves open-drain sensors and an 800 kHz WS2812; the ADC is lost | No one-pin translator works both ways for every part          |
|         Direction-controlled translator (74LVC1T45)        | Clean in both directions                     | Needs a third GPIO for the direction pin                                    | Breaks the two-GPIO budget                                    |
|                       2-pin button pogo (GND / SIG)        | Matches a bare B3F                           | A touch module has no supply                                                | The 3-pin layout takes both and matches the 3V3 pogo          |
|                 Button pogo mandatory on every rig         | Every rig takes a touch module               | Rigs with a soldered B3F need rework for no gain                            | The pin and its behaviour are what examples rely on, not the form |
|      C3: WS2812 stays on GPIO 4, sensors move to GPIO 1    | `rustyfarian-rgb-clock` stays unchanged      | Invalidates the passed pogo gates here                                      | One constant in a sibling is cheaper than re-running gates    |
|           C3: 5V pogo on GPIO 10 (buzzer / IR LED row)     | Already an output row                        | The NPN base resistor loads the line; an input reads low with the buzzer    | The port must work as an input                                |
|          C3: EC11 and 5V pogo take turns on GPIO 1         | One C3 board on the bench                    | Every rotary or 5V check waits for a rewire                                 | Split across rigs, not rewired (bench convention, Rigs)       |
|                       C3: 5V pogo on GPIO 2 or 9           | Both are free                                | Strapping pins; a module holding them low at reset breaks the boot          | Rejected as strapping pins                                    |
|               Feather V2: wake on the on-board SW38        | No wiring                                    | GPIO 38 differs from the WROOM's GPIO 4, and it takes no touch module       | GPIO 4 gives both ESP32 boards one pin set                    |
|                 CrowPanel: wake on the BOOT button (GPIO 0) | A real button, on the board                 | A wake while it is still held can boot into download mode                   | The touch panel INT is proven as a wake source in the knob    |
|              Wake on a bare capacitive touch pad           | No module, just a PCB pad                    | The C3 and C6 have no touch sensor peripheral                               | Not portable; a TTP223-style module works everywhere          |

## Verification
- Each bench board page lists the three pogos and the button pin of this table.
- When a pogo is fitted, a bench check reads a part on it: a potentiometer on the 3V3 pogo, a 5 V sensor on the 5V pogo, an I2C scan on the I2C pogo.
- On the C3, `b3f`, `poti`, `hall-linear` and the I2C checks still pass.
- A deep-sleep check per board wakes it from the button pin, and once from a touch module on the 3V3 pogo.
