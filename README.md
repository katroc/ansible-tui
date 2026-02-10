# ansible-tui

Foundation-first Rust TUI for Ansible operations.

## Prerequisites

- `python3` is required if you use managed runtime bootstrap
- Or provide an existing `ansible-playbook` via PATH/venv/custom path
- `sqlite3` is recommended for DB-backed run history (falls back to legacy TSV when unavailable)

## Current baseline

- Modular TUI shell with views: Dashboard, Inventory, Playbooks, Settings
- Catppuccin Mocha theme across panels, status, and prompts
- Project discovery for inventories and playbooks
- Async action/event loop
- Real `ansible-playbook` execution with live `stdout`/`stderr` streaming
- Local run history persistence in SQLite `./.ansible-tui/history.db` (with automatic legacy import from `runs.db`/`runs.tsv`)
- Runtime picker with managed Ansible bootstrap
- Side-by-side main panel and live logs
- Per-playbook run history list embedded in Playbooks tab
- Inventory management panel with details/preview plus create/delete actions
- In-app inventory editor with save/discard flow
- Guided YAML inventory builder (filename + host/group lists + dynamic assignment -> preview/save)
- Global shortcut helper strip visible on all tabs
- Editable global run profile in Settings tab (saved to `ansible.cfg` + app config)
- Per-playbook settings for common ansible-playbook flags

## Keybindings

- `Tab`: switch view
- `h`/`l`: switch view (except in Settings tab where they adjust selected setting)
- `j`/`k` or `Up`/`Down`: move selection in the focused list
- `n` (Inventory tab): create a new inventory file under `./inventories`
- `g` (Inventory tab): open guided YAML inventory builder (new inventory)
- `e` (Inventory tab): choose edit mode for selected inventory (guided, external `$VISUAL/$EDITOR`/`vim`, or built-in raw text editor)
- `Shift+D` (Inventory tab): delete selected inventory (double-press confirmation)
- Guided builder flow: select target in Group Tree, then attach/detach available groups or hosts with `space`/`d` (`Enter` also toggles attach)
- `Left`/`Right` (Playbooks tab): switch focus between Playbooks and Runs lists
- `Shift+J`/`Shift+K` (Playbooks tab): alternate run selection shortcuts
- `i`/`I` (Playbooks tab): cycle inventory target for selected playbook
- `r`: start run from Playbooks context with selected playbook + inventory
- `Ctrl+S`: save inventory while editor is open
- `t`: open/close playbook settings editor (in Playbooks tab)
- `e`: edit selected text field (or press `Enter`) in Playbook/Global settings editors
- `Left/Right` or `h/l` (Settings tab): adjust selected boolean/numeric setting
- `Space`: toggle selected boolean in Playbook/Global settings editors
- `Backspace`: delete while editing a text field
- `v`: toggle log select mode (selection constrained to Live Logs)
- `Space`: set/clear log selection mark (when log-select mode is active)
- `y`: copy selected log lines
- Mouse: click/drag/release in Live Logs to select and copy
- `c`: toggle `--check`
- `d`: toggle `--diff`
- `u`: open runtime picker (existing venv/system/runtime)
- `Enter`: select highlighted runtime in picker
- `b`: bootstrap managed runtime in `./.ansible-tui/runtime`
- `Esc`: close runtime picker
- `Shift+R`: refresh project discovery
- `q` or `Ctrl+C`: quit

## Global settings

In the Settings tab you can edit these global defaults:
- app runtime setting: ansible binary path
- ansible.cfg default: `interpreter_python`
- ansible.cfg default: `forks`
- ansible.cfg default: `timeout`
- ansible.cfg default: `verbosity`
- ansible.cfg default: `host_key_checking`
- ansible.cfg default: `stdout_callback`
- ansible.cfg default: `retry_files_enabled`
- ansible.cfg default: `retry_files_save_path`
- ansible.cfg default: `remote_user`
- ansible.cfg default: `private_key_file`
- ansible.cfg default: `pipelining`

## Project discovery rules

- Inventories: `./inventories/*.yml|*.yaml|*.ini`
- Playbooks: `./playbooks/*.yml|*.yaml` and root-level `*.yml|*.yaml`

## Optional run env vars

- `ANSIBLE_TUI_PLAYBOOK_BIN` -> playbook executable path (default: `ansible-playbook`)
- `ANSIBLE_TUI_PYTHON_BIN` -> python binary used for managed runtime bootstrap (default: `python3`)
- `ANSIBLE_TUI_VERBOSITY` -> default verbosity (0-4)
- `ANSIBLE_TUI_FORKS` -> default forks
- `ANSIBLE_TUI_TIMEOUT` -> default timeout seconds
- `ANSIBLE_TUI_LIMIT` -> `--limit`
- `ANSIBLE_TUI_TAGS` -> `--tags`
- `ANSIBLE_TUI_EXTRA_VARS` -> `--extra-vars`
- `ANSIBLE_TUI_EXTRA_ARGS` -> additional CLI args appended before playbook path (not an ansible option)

## Per-playbook settings

Per playbook, settings currently manage:
- `--check`
- `--diff`
- `--become`
- `-v` through `-vvvv`
- `--forks`
- `--timeout`
- `--limit`
- `--tags`
- `--extra-vars`
- additional CLI args appended as-is (split on whitespace, no `--extra-args` flag)

## Clipboard notes

- Copy uses platform clipboard tools when available: `wl-copy`, `xclip`, `xsel`, or `pbcopy`
- Fallback is OSC52 terminal clipboard escape

## Included validation assets

- Inventory: `inventories/local.ini`
- Playbook: `playbooks/validate_tui.yml`

## Run

```bash
cargo run
```
