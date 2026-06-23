# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

### Added
- Workspace skeleton: `tamer` (pure `no_std` core, optional `embedded-hal` seam,
  no primitives yet — grown on demand) plus thin `rustyfarian-esp-hal-peripherals`
  (esp-hal, `no_std`) and `rustyfarian-esp-idf-peripherals` (ESP-IDF, std)
  re-export tiers.
- Tooling, CI, docs, and dual MIT/Apache-2.0 licensing.
