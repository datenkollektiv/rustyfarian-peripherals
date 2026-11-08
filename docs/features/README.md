# Feature Docs

Feature docs capture in-flight design: the decisions made, the alternatives
rejected, the constraints discovered, and session-by-session progress. They
complement ADRs (`docs/adr/`) — ADRs hold durable architectural rationale; feature
docs track a single feature from design through implementation.

## Naming convention

Files are named `feature-name-vN.md` — kebab-case slug, no numeric prefix.

> Feature docs were previously numbered (`001-…`, `002-…`). The numbers implied a
> global ordering that didn't reflect the demand-driven roadmap, so they were
> dropped in favour of stable, descriptive names. If you followed an old
> `NNN-slug` link, drop the `NNN-` prefix. (ADRs in `docs/adr/` keep their numbers
> — that ordering *is* meaningful.)

Start a new doc from [`template.md`](template.md), or run `/feature`.

## Active

This index lists only feature docs still **in flight** (design or scoped stage).
Docs for already-shipped features are intentionally omitted — they remain in this
directory as historical records, and move to [`archive/`](archive/) once fully
closed out; [`../../CHANGELOG.md`](../../CHANGELOG.md) is the source of truth for
what has landed.

| Feature                                                                                           | Status                                 |
|:--------------------------------------------------------------------------------------------------|:---------------------------------------|
| [MPU6050 accelerometer / IMU](mpu6050-imu-v1.md) — sans-IO parse, calibration, feature-gated tilt | Merged; hardware in release pass       |
| [BME680 environmental sensor](bme680-v1.md) — pure core + first I2C driver in `tamer` (ADR-009)   | Core, driver and examples landed; hardware pass pending |
| [Chip matrix](chip-matrix-v1.md) — ESP32/C3/C6/S3 on both tiers, AVR link check of `tamer`        | Compile and link evidence recorded; hardware pass mostly `not run` (one cell passed) |
| [IRAM-safe ISR](iram-safe-isr-v1.md) — run the encoder ISR from SRAM for flash-cache-off safety   | Scoped (skeleton)                      |
| [signwriter](signwriter-v1.md) — 128x64 OLED driver for SSD1306 and SH1106 with hot-plug (ADR-013) | Crate and C3 examples done; bench pass pending |
