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
| [IRAM-safe ISR](iram-safe-isr-v1.md) | designed, not started |
