# Feature Docs

A feature doc holds the design of one feature while it is in flight: decisions, constraints, open questions.
Durable rationale goes to an ADR (`docs/adr/`); what shipped is in [`CHANGELOG.md`](../../CHANGELOG.md); a shipped feature's doc is deleted, git keeps it.
Name files `feature-name-vN.md` and start from [`template.md`](template.md).

| Feature | Stage |
|:--|:--|
| [Chip matrix](chip-matrix-v1.md) | ships in 0.2.0 |
| [signwriter](signwriter-v1.md) | ships in 0.2.0 |
| [MPU6050 accelerometer / IMU](mpu6050-imu-v1.md) | ships in 0.2.0 |
| [BME680 environmental sensor](bme680-v1.md) | experimental in 0.2.0 |
| [Staged timer](staged-timer-v1.md) | implemented, unreleased |
| [IR codec](ir-codec-v1.md) | `tamer::ir` and the esp-hal RMT bridge implemented, unreleased; hardware unverified |
| [SHT3x temperature / humidity](sht3x-v1.md) | implemented, unreleased; `c6/sht3x` rerun due (driver changed after its pass) |
| [HX711 load-cell scale](hx711-v1.md) | implemented, unreleased; hardware unverified |
| [IRAM-safe ISR](iram-safe-isr-v1.md) | designed, not started |
