# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-05-26

### Added
- Ports view for monitoring listening TCP ports (`P` key to switch)
- Kill port processes with confirmation dialog (`K` key)
- Open ports in browser with fallback chain and WSL support (`O` key)
- Self-update command (`argus update`) for in-place binary updates
- 76 unit tests for config parsing, app state, keybindings, port scanning, and main helpers

### Fixed
- Browser open now uses robust fallback chain (xdg-open, wslview, explorer.exe)
- Fallback to `/proc/PID/fd` for port logs of non-managed processes
- Service log refresh no longer overwrites port logs

## [0.1.0] - 2026-05-25

### Added
- Terminal dashboard with service monitoring (systemd, Docker, PM2)
- Session tracking (SSH, Mosh, tmux, Claude)
- System metrics strip (CPU, memory, disk, network)
- Filterable service list with status indicators
- Detail pane with Info, Logs, and Actions tabs
- Service control actions (start, stop, restart) with confirmation
- Command palette (Ctrl+P)
- Adaptive layout for narrow terminals
- TOML configuration file support
- Install and uninstall scripts
- CI/CD pipeline with multi-architecture release builds (x86_64, aarch64)

[0.1.1]: https://github.com/emw3/argus/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/emw3/argus/releases/tag/v0.1.0
