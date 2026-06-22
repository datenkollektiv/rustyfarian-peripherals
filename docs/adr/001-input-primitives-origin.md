# ADR-001: Origin and relicensing of the first input primitives

## Status
Accepted

## Context
The first input primitives to land in `tamer` — debounce/edge detection and
quadrature rotary decoding — are not greenfield code.
Proven, host-tested implementations of the same logic already existed in two
sibling projects by the same author:

- Quadrature decoding (Gray-code transition table, accumulator-based detent
  debouncing) and the button/touch state machines come from the
  `rustyfarian-knob` project's `zoetrope` crate
  (`crates/zoetrope/src/encoder_state_machine.rs`, `button_state_machine.rs`,
  `touch_packet.rs`), licensed **MIT**.
- The debounce/edge state machines come from `rustbox-peripherals`'
  `rustbox-peripherals-pure` crate (`src/input/debounce.rs`, `src/input/edge.rs`),
  licensed **MIT OR Apache-2.0**.

Both sources are authored solely by the rustyfarian maintainer (verified via
`git log` authorship: a single author on every donated file).
We needed to decide how to bring this logic into `rustyfarian-peripherals`
without entangling the repos or muddying provenance and licensing.

## Decision
Donate the logic as a **clean reimplementation** ported into idiomatic `tamer`
modules, rather than a verbatim copy or a git-history graft.

- `tamer::debounce` and `tamer::rotary` are written to this repo's conventions
  (MSRV 1.88, sans-io pure core, trait-first, `Noop*`/`MockInputPin` mock, the
  `hal` feature seam, `prelude` re-exports), not copied byte-for-byte.
- The `zoetrope` code is **relicensed MIT OR Apache-2.0** for use here.
  The maintainer is the sole copyright holder of the donated files, so
  relicensing the originally MIT-only knob code under this repo's dual license
  is the copyright holder's prerogative and requires no third-party consent.
- Provenance is recorded in commit trailers (`Adapted-from: <repo> <module> @ <sha>`),
  in `CHANGELOG.md`, and in this ADR. Source revision at donation time:
  `rustyfarian-knob` and `rustbox-peripherals` both at `a169dd8`.
- `rustyfarian-peripherals` does **not** depend on either source repo.
  The donation is one-directional; the sources owe nothing back but a citation.

## Consequences
- The new repo owns the primitives outright, with a clean history that matches
  its deliberately demand-driven, skeleton-grown-on-demand timeline.
- Public APIs diverge from the sources where this repo's conventions differ —
  e.g. `QuadratureDecoder::update` returns `Option<EncoderDirection>` rather than
  the knob's raw `i32` delta. Downstream of the sources is unaffected; this is a
  fresh API.
- Git blame does not trace back to the original commits. Provenance lives in the
  commit trailers, `CHANGELOG`, and this ADR instead — an acceptable trade for a
  clean start.
- Future donations (button events, touch, display) follow the same recipe and
  cite this ADR.

## Alternatives Considered
|  Alternative |  Pros | Cons  | Why Rejected  |
|-------------:|------:|:------|:--------------|
| git subtree / `filter-repo` history graft | Preserves authorship commits | Imports unrelated knob history; clashes with the clean skeleton; API still needs reworking | History noise outweighs the blame trail for a single-author donation |
| Verbatim copy, no provenance record | Fastest | No citation; carries MIT-only headers into a dual-licensed repo; non-idiomatic API | Fails the "proper donation" bar; licensing left implicit |
| Develop in `rustbox-peripherals`, then migrate | Reuses an active workspace | Entangles the repos; the new team never gets a clean origin | Defeats the point of a fresh-start home for basic peripherals |
