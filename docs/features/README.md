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

| Feature                                                                                           | Status                  |
|:--------------------------------------------------------------------------------------------------|:------------------------|
| [MPU6050 accelerometer / IMU](mpu6050-imu-v1.md) — sans-IO parse, calibration, feature-gated tilt | Design approved (Ready) |
| [IRAM-safe ISR](iram-safe-isr-v1.md) — run the encoder ISR from SRAM for flash-cache-off safety   | Scoped (skeleton)       |
