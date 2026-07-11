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

| Feature                                                                                           | Status                  |
|:--------------------------------------------------------------------------------------------------|:------------------------|
| [Input primitives](input-primitives-v1.md) — debounce, rotary, button, digital presence           | Implemented             |
| [Hall-effect sensing](hall-sensing-v1.md) — linear analog + digital switch                        | Implemented             |
| [MPU6050 accelerometer / IMU](mpu6050-imu-v1.md) — sans-IO parse, calibration, feature-gated tilt | Design approved (Ready) |

## Archived

Completed features whose work has fully landed move to [`archive/`](archive/):

| Feature                                                                | Status  |
|:-----------------------------------------------------------------------|:--------|
| [Range map](archive/range-map-v1.md) — clamped linear `u16 → u8` remap | Shipped |
