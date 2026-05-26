# argus

Rust TUI dashboard for VPS monitoring. Built with Ratatui + Tokio.

## Build

```bash
cargo build --release
# Binary at target/release/argus (~1.5MB)
```

## Run

```bash
# As regular user (limited service control)
./target/release/argus

# As root (full service control)
sudo ./target/release/argus

# With filter
./target/release/argus --filter docker
```

## Architecture

Event-driven app with `tokio::select!` main loop. Separate Tokio tasks poll data sources on independent intervals and push updates via mpsc channels. The UI never blocks on data collection.

```
main.rs          — entry point, spawns pollers, runs event loop
app.rs           — App state, keybinding dispatch, all enums
event.rs         — EventLoop: crossterm keys + tick via mpsc
tui.rs           — terminal init/teardown, panic handler
config.rs        — TOML config (~/.config/argus/config.toml)

ui/mod.rs        — root render, layout switching (services vs sessions)
ui/metrics_strip — top bar: CPU/MEM/DISK/NET + session counts
ui/service_list  — left pane: filterable service list with status dots
ui/detail        — right pane: Info/Logs/Actions tabs
ui/sessions_view — full-width sessions panel (SSH/Mosh/tmux/Claude)
ui/status_bar    — bottom keybind hints
ui/palette       — Ctrl+P command palette overlay
ui/confirm       — confirmation dialog overlay

providers/mod    — ServiceProvider trait, Service/LogEntry types
providers/systemd — systemctl/journalctl (--no-ask-password)
providers/docker  — Docker Engine API via Unix socket (/var/run/docker.sock)
providers/pm2     — pm2 jlist JSON parsing

sessions/ssh     — `who` parser, detects SSH/Mosh/Local connection types
sessions/mosh    — mosh-server process scanner, links to tmux targets
sessions/tmux    — tmux list-sessions via socket (sudo-aware)
sessions/claude  — pgrep claude, detects agent vs interactive mode

metrics/system   — /proc/stat, /proc/meminfo, /proc/net/dev, statvfs
```

## Key patterns

- **Provider trait**: `ServiceProvider` in `providers/mod.rs`. Each provider (systemd/docker/pm2) implements `list`, `start`, `stop`, `restart`, `logs`.
- **Sudo awareness**: tmux tracker finds user's socket via `/tmp/tmux-$SUDO_UID/default`. systemctl uses `--no-ask-password` + piped stdio.
- **Adaptive layout**: splits vertically when terminal < 100 cols wide.
- **Notification bar**: action results (success/error) display above status bar, auto-dismiss after 5s via `Instant`.

## Adding a new provider

1. Create `src/providers/newprovider.rs` implementing `ServiceProvider`
2. Add `pub mod newprovider;` in `src/providers/mod.rs`
3. Add a `ProviderType::NewProvider` variant
4. Wire it into the services poller in `main.rs`
5. Add a config toggle in `config.rs`

## Adding a new session tracker

1. Create `src/sessions/newtracker.rs` with a `pub async fn list()` function
2. Add the struct to `src/sessions/mod.rs` and a `SessionInfo` variant
3. Wire into the sessions poller in `main.rs`
4. Add rendering in `src/ui/sessions_view.rs`

## Tests

```bash
cargo test
cargo clippy
```

## Config

Default config location: `~/.config/argus/config.toml`

```toml
[general]
refresh_interval_ms = 2000
default_filter = "all"

[providers]
systemd = true
docker = true
pm2 = true

[sessions]
ssh = true
tmux = true
claude = true
```
