# Contributing to rustyfarian-peripherals

Thanks for your interest in contributing!
This project is maintained by the *rustyfarians* (Rust enthusiasts around datenkollektiv)
and is part of the [rustyfarian family](README.md#rustyfarian-family) of embedded-Rust crates
for battery-powered ESP32 field deployments. It is the **input** layer of the stack.

All kinds of contributions are welcome — code, documentation, bug reports, ideas, or small cleanups.

---

## 🧰 Prerequisites

- **[`just`](https://github.com/casey/just)** — the task runner behind every build/test/lint command; install it before running any `just …` recipe.
- **Rust (stable)** — host work (`just verify`) builds on stable; install via [rustup](https://rustup.rs).
- **The `esp` toolchain** — only needed for the hardware crates; install via [`espup`](https://github.com/esp-rs/espup) (or `just setup-toolchain`). Pure host-side work needs none of it.
- Optional: `cargo-deny` and `cargo-audit` (for `just deny` / `just audit`) — CI installs these automatically.

Run `just doctor` for a one-glance check of your environment.

---

## 🌱 Project status: skeleton

The workspace is a skeleton — structure, tooling, CI, and docs are in place, but
the input and output primitives grow **downstream-driven** (see [ROADMAP.md](docs/ROADMAP.md)).
When you add the first debounce or rotary primitive:

- Put the pure logic in `tamer` (host-testable, no hardware dependency).
- Ship a `Noop*` mock beside every new trait, in the same change.
- Add host tests for the logic.
- If it has a hardware adapter, wire it behind `tamer`'s `hal` feature, then add
  the thin esp-hal / esp-idf wrappers as a consumer needs them.

See [AGENTS.md](AGENTS.md) for the full conventions.

---

## 🚀 How to Contribute

### 1. Fork & Branch
- Fork the repository
- Create a feature branch from `main`
- Keep changes focused and small where possible

### 2. Make Your Changes
- Read [AGENTS.md](AGENTS.md) first — it is the cross-tool operating guide (overview, architecture, conventions)
- Follow existing code style; prefer clarity over cleverness
- Keep the hardware-independent core (`tamer`) free of HAL dependencies so it stays host-testable
- Avoid breaking existing behavior unless discussed

### 3. Validate Before Opening a PR
Run the non-modifying verification suite — this **mirrors the host-side CI gates** (the `fmt`, `clippy`, and `rust` workflows):

```shell
just verify
```

For changes to the hardware crates once their drivers land, also validate the cross-compile **locally** (CI has no ESP toolchain):

```shell
just check-idf
```

`just audit` (and the scheduled Audit workflow) scans dependencies for security advisories. Note it generates a `Cargo.lock` if one isn't present — the lockfile is gitignored for this library, so this is expected and not something to commit.

### 4. Open a Pull Request
- Describe **what** you changed and **why**
- If the change is visible or behavioral, mention it explicitly
- If it's cleanup-only, say so clearly

---

## 🧹 "Boy Scout Pass" (Cleanup Changes)

We sometimes refer to a **"Boy Scout pass"**, inspired by the Boy Scout Rule:

> *Always leave the code a little cleaner than you found it.*

A Boy Scout pass means small cleanups, improved readability, or structure — with no intentional
behavior changes. When in doubt, label your change `cleanup`, `refactor`, or
`boy scout pass (no behavior change)`.

---

## 🧪 Testing

- Host-side logic lives behind traits and is unit-tested without hardware — add tests for new core logic
- If your change affects behavior, mention what you tested in the PR (and, for hardware changes, which board you flashed)
- Cleanup-only changes need only a quick sanity check

---

## 📝 Commit Messages

We prefer simple, descriptive commit messages. No need to be overly formal — just be clear.

---

## 💬 Communication & Conduct

- Be respectful and friendly; assume good intent
- Keep discussions technical and constructive
- See our [Code of Conduct](CODE_OF_CONDUCT.md)

This is an open-source hobby project — let's keep it enjoyable.

---

## ❓ Questions or Ideas?

- Open an issue
- Start a discussion
- Or just submit a PR and see what happens 🙂

Thanks for helping make `rustyfarian-peripherals` better!
