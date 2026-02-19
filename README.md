# ansible-tui

[![Release build](https://github.com/katroc/ansible-tui/actions/workflows/release.yml/badge.svg)](https://github.com/katroc/ansible-tui/actions/workflows/release.yml)

Fast, keyboard-driven terminal UI for running Ansible playbooks with live logs, per-project state, and built-in secret-aware workflows.

## Highlights

- Multi-project workspace with tabs for Dashboard, Inventory, Playbooks, and Settings
- Native `ansible-playbook` execution with streamed `stdout`/`stderr` and run history
- Inventory tooling: list, preview, create/delete, guided YAML builder, and in-app editing
- Playbook controls: inventory targeting, run options, run history, and task/tag preview (`w`)
- Secret-aware workflows: vault create/edit, vault prompt/file auth modes, command redaction
- 18 built-in themes and per-project UI session restore (focus, selected items, filters)

## Requirements

- Rust toolchain (for local builds)
- Ansible available either by:
  - Existing `ansible-playbook` in your PATH/venv/custom runtime, or
  - Managed runtime bootstrap from the app (requires `python3`)

## Quick start

```bash
cargo run
```

The app discovers files in the current project directory:

- Inventories: `./inventories/*.yml|*.yaml|*.ini`
- Playbooks: `./playbooks/*.yml|*.yaml` and `./*.yml|*.yaml`

Bundled validation assets:

- `inventories/local.ini`
- `playbooks/validate_tui.yml`

## Keybindings (core)

Global navigation:

- `Tab` or `h`/`l`: switch tabs/views
- `j`/`k` or `Up`/`Down`: move selection in focused list
- `/`: filter focused list (`Enter` apply, `Esc` clear)
- `?`: open context help overlay
- `Shift+R`: refresh project discovery
- `q` or `Ctrl+C`: quit

Playbooks:

- `r`: run selected playbook
- `w`: preview playbook tasks/tags (`--list-tasks --list-tags`)
- `i`/`I`: cycle inventory target
- `t`: open/close playbook settings editor
- `Left`/`Right`: switch Playbooks vs Runs panes

Runtime:

- `u`: open runtime picker
- `b`: bootstrap managed runtime into `./.ansible-tui/runtime`

Logs:

- `v`: toggle log-select mode
- `Space`: set/clear selection mark
- `y`: copy selected lines
- `PgUp`/`PgDn`: scroll logs

Projects and inventory power actions:

- `Shift+V`: create and encrypt new vault vars file
- `Shift+E`: decrypt, edit, and re-encrypt vault file
- `Shift+P`: create vault password file and set source to `file`
- `g` (Inventory): guided YAML inventory builder
- `Shift+D`: guarded delete for project/inventory

## Settings and configuration

Global settings include runtime path, common `ansible.cfg` fields (`forks`, `timeout`, `verbosity`, `private_key_file`, etc.), `secret_enforcement_mode`, and `theme`.

Per-playbook settings include:

- `--check`, `--diff`, `--become`, `-v..-vvvv`
- `--forks`, `--timeout`, `--limit`, `--tags`
- `--extra-vars` (strict mode expects references such as `@vars/secrets.vault.yml`)
- SSH private key via file path or inline key material
- Additional CLI args appended as-is

Optional env vars:

- `ANSIBLE_TUI_PLAYBOOK_BIN` (default `ansible-playbook`)
- `ANSIBLE_TUI_PYTHON_BIN` (default `python3`)
- `ANSIBLE_TUI_VERBOSITY` (`0..4`)
- `ANSIBLE_TUI_FORKS`
- `ANSIBLE_TUI_TIMEOUT`
- `ANSIBLE_TUI_LIMIT`
- `ANSIBLE_TUI_TAGS`
- `ANSIBLE_TUI_EXTRA_VARS`
- `ANSIBLE_TUI_EXTRA_ARGS`
- `ANSIBLE_TUI_THEME` (supports named themes and legacy `dark`/`light` aliases)

## Secret handling and vault behavior

- Vault auth defaults are set per project: `prompt` or `file`, plus optional vault ID
- Template-level vault options can override project defaults
- Prompt-mode vault passwords are requested in-app and cached in-memory per project session
- Command logging redacts secret-bearing arguments
- `secret_enforcement_mode`:
  - `strict` (default): blocks plaintext-sensitive persistence
  - `compat`: allows legacy plaintext behavior with warnings

## Data and state

- Runtime/project data lives under `./.ansible-tui/`
- Run history uses SQLite under:
  - `./.ansible-tui/project-history/<hash>/.ansible-tui/history.db`
- Legacy DB/TSV history is imported automatically when found

## Release binaries via GitHub Actions

This repo includes a release workflow at `.github/workflows/release.yml` that:

- Builds release binaries for Linux/macOS (x86_64 + arm64) and Windows (x86_64)
- Uploads per-platform archives as workflow artifacts
- Publishes archives to GitHub Releases when a `v*` tag is pushed

Create a release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Development

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets --all-features
```
