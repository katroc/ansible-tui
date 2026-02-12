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
- Local run history persistence in SQLite under `./.ansible-tui/project-history/<hash>/.ansible-tui/history.db` per project (with automatic legacy import from older `runs.db`/`runs.tsv` locations)
- Runtime picker with managed Ansible bootstrap
- Side-by-side main panel and live logs
- Per-playbook run history list embedded in Playbooks tab
- Inventory management panel with details/preview plus create/delete actions
- In-app inventory editor with save/discard flow
- Guided YAML inventory builder (filename + host/group lists + dynamic assignment -> preview/save)
- SSH private key assignment at project scope and per-playbook scope (file path or inline key text)
- Vault reference defaults at project scope with template-level override (`prompt`/`file` + optional vault-id)
- Secret enforcement mode (`strict` default, `compat` optional) for plaintext-sensitive options
- Redacted run command logging for secret-bearing arguments
- Global shortcut helper strip visible on all tabs
- Editable global run profile in Settings tab (saved to `ansible.cfg` + app config)
- Per-playbook settings for common ansible-playbook flags

## Keybindings

- `Tab`: switch view (or move to next field in vault password prompt)
- `h`/`l`: switch view (except in split/field-focused contexts where they move focus or adjust values)
- `j`/`k` or `Up`/`Down`: move selection in the focused list
- `n` (Inventory tab): create a new inventory file under `./inventories`
- `g` (Inventory tab): open guided YAML inventory builder (new inventory)
- `e` (Inventory tab): choose edit mode for selected inventory (guided, external `$VISUAL/$EDITOR`/`vim`, or built-in raw text editor)
- `p` (Inventory Hosts/Groups sub-tabs): run a quick `ansible.builtin.ping` test for the selected host or group target
- `Shift+D` (Inventory tab): delete selected inventory (double-press confirmation)
- `Shift+D` (Projects tab): delete selected project (double-press confirmation, keeps at least one project)
- `e` (Projects tab): edit selected project's secret settings (SSH key refs + vault refs)
- `Shift+V` (Projects tab): create and encrypt a new vault vars file from inside the app (new-file flow)
- `Shift+E` (Projects tab): open vault editor and auto-decrypt the default vault path for editing (or change path and press `Enter` to reload), then re-encrypt on save
- `Shift+P` (Projects tab): create a vault password file from inside the app (sets vault source to `file`)
- Guided builder flow: select target in Group Tree, then attach/detach available groups or hosts with `space`/`d` (`Enter` also toggles attach)
- `Left`/`Right` (Playbooks tab): switch focus between Playbooks and Runs lists
- `Enter` (Playbooks/Templates tab): toggle focus between list and runs pane
- `Shift+J`/`Shift+K` (Playbooks tab): alternate run selection shortcuts
- `i`/`I` (Playbooks tab): cycle inventory target for selected playbook
- `r`: start run from Playbooks context with selected playbook + inventory
- `Ctrl+S`: save inventory editor/builder, project SSH key prompt, or playbook text-edit field
- `t`: open/close playbook settings editor (in Playbooks tab)
- `e`: edit selected text field (or press `Enter`) in Playbook/Global settings editors
- `Left/Right` or `h/l` (Settings tab): adjust selected boolean/numeric setting
- `Space`: toggle selected boolean in Playbook/Global settings editors
- `Backspace`: delete while editing a text field
- `?`: toggle keyboard help overlay for the current context (`Esc` or `?` closes)
- `v`: toggle log select mode (selection constrained to Live Logs)
- `PgUp`/`PgDn`: scroll Live Logs (also works via mouse wheel over logs)
- `End`: return Live Logs to follow-latest mode
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
- app config: `secret_enforcement_mode` (`strict` or `compat`)

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
- `--extra-vars` input (strict mode expects vars files, for example `@vars/secrets.vault.yml`)
- SSH private key via file path (`--private-key`)
- SSH private key via inline pasted key material (written to a temporary key file at run-time)
- additional CLI args appended as-is (split on whitespace, no `--extra-args` flag)

## Vault references and enforcement

- Project settings can define vault auth defaults:
  - `vault_source_type`: `prompt` or `file`
  - `vault_password_file`: path used when source is `file`
  - `vault_id_label`: optional vault id label (for `--vault-id`)
- Template editor can override those vault settings per template.
- Vault creation flow in Projects tab (`Shift+V`) creates new files only (does not open existing vault content), writes your YAML to a temp file, encrypts it with `ansible-vault`, then moves the encrypted file into your target path.
- Vault edit flow in Projects tab (`Shift+E`) decrypts the target file into the editor buffer (`Enter` on path), then re-encrypts updated content on `Ctrl+S`.
- When project vault source is `prompt`, run/create/edit flows use an in-app vault password modal (instead of relying on TTY prompts), cache that password in-memory per project for the current app session, and reuse it for subsequent prompt-mode actions.
- Vault password helper (`Shift+P`) creates the password file with secure permissions and updates project vault settings.
- Effective CLI behavior:
  - `prompt` + no vault-id -> `--ask-vault-pass`
  - `prompt` + vault-id -> `--vault-id <id>@prompt`
  - `file` + no vault-id -> `--vault-password-file <path>`
  - `file` + vault-id -> `--vault-id <id>@<path>`
- Secret enforcement mode:
  - `strict` (default): blocks inline SSH keys and plaintext `--extra-vars`; use references/files.
  - `compat`: allows legacy plaintext with run warnings.
  - New plaintext persistence is blocked in editors/prompts (legacy values are read-only until removed).

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
