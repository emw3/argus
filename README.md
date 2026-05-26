# argus

The all-seeing VPS dashboard. Monitor services, sessions, and system metrics from a single terminal.

Built with Rust, Ratatui, and Tokio. ~1.5MB binary, ~3MB RAM.

## Features

- **Unified service monitoring** — systemd, Docker containers, PM2 processes in one view
- **Service control** — start, stop, restart, enable, disable with confirmation dialogs
- **Live system metrics** — CPU, memory, disk, network from `/proc`
- **Log viewing** — per-service log tailing with auto-scroll
- **Session tracking** — SSH, Mosh, tmux, Claude Code sessions with connection details
- **Fuzzy search** — `/` to filter services instantly
- **Command palette** — `Ctrl+P` for quick actions
- **Adaptive layout** — switches to vertical stack on narrow terminals
- **Sudo-aware** — detects tmux sessions and service permissions correctly under sudo
- **Zero config** — works out of the box, optional TOML config for customization

## Install

### Quick install

```bash
curl -sSfL https://raw.githubusercontent.com/emw3/argus/main/install.sh | sudo sh
```

### From source

```bash
# Requires Rust toolchain
cargo install --path .

# Or build manually
cargo build --release
sudo cp target/release/argus /usr/local/bin/
```

## Uninstall

```bash
curl -sSfL https://raw.githubusercontent.com/emw3/argus/main/uninstall.sh | sudo sh
```

## Usage

```bash
# Run as regular user (read-only for privileged services)
argus

# Run as root for full service control
sudo argus

# Start with a specific filter
argus --filter docker
argus --filter systemd
argus --filter pm2

# Custom config file
argus --config /path/to/config.toml
```

## Keybindings

### Views
| Key | Action |
|-----|--------|
| `Tab` | Toggle Services / Sessions view |
| `Shift+Tab` | Switch pane (list / detail) |

### Filters
| Key | Action |
|-----|--------|
| `1` | All services |
| `2` | Systemd only |
| `3` | Docker only |
| `4` | PM2 only |

### Navigation
| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g` / `G` | First / last |

### Service Control
| Key | Action |
|-----|--------|
| `s` | Stop (with confirmation) |
| `S` | Start |
| `r` | Restart (with confirmation) |
| `e` | Enable (systemd) |
| `d` | Disable (systemd, with confirmation) |

### Other
| Key | Action |
|-----|--------|
| `/` | Fuzzy search |
| `Ctrl+P` | Command palette |
| `i` | Info tab |
| `l` | Logs tab |
| `?` | Help |
| `q` | Quit |

## Configuration

Optional config at `~/.config/argus/config.toml`:

```toml
[general]
refresh_interval_ms = 2000    # polling interval
default_filter = "all"        # all | systemd | docker | pm2

[providers]
systemd = true
docker = true
pm2 = true

[sessions]
ssh = true
tmux = true
claude = true
```

## Requirements

- Linux (reads from `/proc`, uses systemd/Docker/PM2 CLI tools)
- Rust 1.70+ (to build from source)
- Optional: Docker, PM2, tmux, mosh (detected at runtime)

## License

MIT
