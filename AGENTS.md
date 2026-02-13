# AGENTS.md
Operational guide for agentic coding assistants working in `ansible-tui`.

## Quick Start (Agent Checklist)
- Confirm rule-file status (`.cursor/rules/`, `.cursorrules`, `.github/copilot-instructions.md`).
- Read `src/app.rs`, `src/ui.rs`, and the touched module before editing.
- Keep UI rendering in `ui.rs` and state transitions in `app.rs`.
- Implement the smallest change that matches existing patterns.
- Run `cargo fmt -- --check`.
- Run targeted tests first, then `cargo test` when practical.
- Run `cargo clippy --all-targets --all-features` for warning awareness.
- Update `README.md` when user-visible behavior or keybindings/env vars change.
- Never commit runtime data from `.ansible-tui/` or build output from `target/`.

## 1) Project Overview
- Language: Rust (`edition = 2021`), single binary crate.
- Entry point: `src/main.rs`.
- UI stack: `ratatui` + `crossterm`.
- Async/event model: `tokio` + channel-driven actions.
- Persistence: `rusqlite` with bundled SQLite.
- Runtime/project data: `.ansible-tui/` (ignored by git).

High-traffic modules:
- `src/app.rs`: main state machine and action handling.
- `src/ui.rs`: rendering only.
- `src/input.rs`: key/mouse input -> `Action` mapping.
- `src/run.rs`: command spawning and process/log handling.
- `src/*_settings.rs`, `src/projects.rs`, `src/config.rs`, `src/run_store.rs`: persistence and migrations.

## 2) Rule Files (Cursor/Copilot)
The following were checked and are currently absent:
- `.cursor/rules/`
- `.cursorrules`
- `.github/copilot-instructions.md`

If these files appear later, treat them as higher-priority repo instructions.

## 3) Build / Lint / Test Commands
Run from repo root: `/home/tron/projects/ansible-tui`.

### Build and Run
```bash
cargo check
cargo build
cargo run
```

Notes:
- `cargo run` requires an interactive terminal (TTY).
- In headless/CI-like shells, use `cargo check` and `cargo test` instead.

### Formatting
```bash
cargo fmt -- --check
cargo fmt
```

### Lint
```bash
cargo clippy --all-targets --all-features
```

Optional strict mode (currently fails due existing warnings):
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Test
Run all tests:
```bash
cargo test
```

List all tests:
```bash
cargo test -- --list
```

Run a single test (substring match):
```bash
cargo test config::tests::save_and_load_round_trip
```

Run a single exact test with log output:
```bash
cargo test run::tests::redacted_command_masks_sensitive_args -- --exact --nocapture
```

Run all tests in a module:
```bash
cargo test app::tests::
```

## 4) Coding Style and Conventions
Mirror existing code unless task requirements say otherwise.

### Imports
- Group order: `std`, third-party crates, then `crate::...`.
- Keep one blank line between import groups.
- Prefer explicit imports over glob imports.

### Formatting
- Use rustfmt defaults; do not hand-format against formatter output.
- Keep trailing commas in multiline literals/match arms.
- Use guard-style early returns to reduce nesting.
- Use `let ... else` for Option/Result control flow when clear.

### Naming
- Types/enums/traits: `PascalCase`.
- Functions/modules/locals: `snake_case`.
- Constants: `UPPER_SNAKE_CASE`.
- Test names: behavior-focused snake_case.

### Types
- Use domain-specific integer widths (`u8`, `u16`, `u64`, `i32`) already common in repo.
- Use `Option<T>` for optional config and nullable persisted values.
- Use `Path`/`PathBuf` for filesystem paths, not loose strings.
- Prefer enums/structs for stateful UI flow rather than untyped tuples.

### Error Handling
- Avoid panics in runtime code paths.
- `unwrap`/`expect` are acceptable in tests, not production paths.
- For DB-backed modules, return `io::Result<T>` and map sqlite errors via helpers.
- Handle `io::ErrorKind::NotFound` explicitly for optional/first-run files.
- In background tasks, communicate failures via `Action::Error` and status/log actions.

### Async and Event Flow
- Keep input translation in `input.rs` and app transitions in `app.rs`.
- Use `tokio::spawn` for async jobs; use `spawn_blocking` for blocking event polling.
- Prefer channel messages (`UnboundedSender<Action>`) over shared mutable state.

### UI Guidelines
- Keep `ui.rs` rendering-focused; avoid business logic side effects there.
- Clamp bounded values before sending to widgets (for example percentages for gauges).
- Follow existing focus-context and hint-strip patterns when adding interactions.

### Persistence / Migration
- Keep schema changes additive when possible.
- Preserve legacy migration paths (old TSV / old DB locations) unless task says to remove.
- Use transactions for multi-step writes that must remain consistent.

### Secrets and Logging
- Never log plaintext secrets.
- Preserve command redaction behavior for secret-bearing args.
- Respect `SecretEnforcementMode` and vault source behavior.

## 5) Test Expectations for Code Changes
- Add regression tests for bug fixes when feasible.
- Prefer narrow unit tests near touched module.
- During iteration: run focused tests first.
- Before final handoff: run `cargo test` (or explain why not run).
- If feature touches parsing/migrations/CLI arg assembly, include at least one direct test.

## 6) Workspace Hygiene
- Do not commit `target/`.
- Do not commit `.ansible-tui/` runtime data.
- Do not edit unrelated files just to satisfy lint/style.
- Keep diffs scoped to requested behavior.

## 7) Recommended Agent Workflow
1. Read relevant modules first (`app.rs`, `ui.rs`, `run.rs`, related persistence files).
2. Implement smallest viable change matching existing patterns.
3. Run `cargo fmt -- --check`.
4. Run targeted test(s), then `cargo test` if practical.
5. Run `cargo clippy --all-targets --all-features` for warning awareness.
6. Update `README.md` when user-visible behavior changes (keys, env vars, workflows).

## 8) Current Baseline (at time of writing)
- `cargo test`: passing (34 tests).
- `cargo clippy --all-targets --all-features`: succeeds with warnings.
- strict clippy with `-D warnings`: currently fails on existing lint debt.
