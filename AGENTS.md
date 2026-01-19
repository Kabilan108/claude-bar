# Claude Bar - Agent Instructions

## Project Overview

Claude Bar is a Linux system tray application for monitoring AI coding assistant usage limits, quotas, and costs. It's a Rust/GTK4 port of [CodexBar](https://github.com/steipete/CodexBar) (macOS).

**Target providers**: Claude Code, Codex (Phase 1); Cursor, OpenCode Zen (Future)

## Quick Start

```bash
# Enter dev environment
nix develop

# Build all crates
cargo build --workspace

# Run the daemon
cargo run -p claude-bar

# Run the CLI
cargo run -p claude-bar-cli -- status

# Run tests
cargo test --workspace
```

## Project Structure

```
claude-bar/
├── SPEC.md              # Implementation specification (READ THIS FIRST)
├── AGENTS.md            # This file
├── flake.nix            # Nix development environment
├── Cargo.toml           # Workspace root
├── crates/
│   ├── claude-bar/      # Main daemon (tray + GTK popup)
│   ├── claude-bar-core/ # Shared library (models, providers, cost)
│   └── claude-bar-cli/  # Command-line tool
└── nix/
    └── hm-module.nix    # Home Manager module
```

## Specification

**Always read `SPEC.md` before starting work.** It contains:
- Detailed phase-by-phase implementation plan
- Todo items to check off as you complete work
- References to the original CodexBar files for each component
- Data models and API details

## Reference Implementation

The original macOS implementation is at:
```
/vault/experiments/2026-01-16-steipete-CodexBar/
```

Key directories:
- `Sources/CodexBarCore/` - Core logic (providers, models, cost tracking)
- `Sources/CodexBar/` - UI layer (tray, popup views)
- `Sources/CodexBarCLI/` - CLI tool

When porting logic, reference the Swift files listed in SPEC.md's "Reference Files" section.

## Coding Conventions

### Rust Style
- Follow standard Rust conventions (rustfmt, clippy)
- Use `thiserror` for error types
- Use `tracing` for logging (not println!)
- Prefer `async/await` with tokio for I/O

### Error Handling
- Return `Result<T, E>` from fallible functions
- Use `?` for propagation
- Create domain-specific error types
- Never panic in library code

### Testing
- Write unit tests in the same file (`#[cfg(test)]`)
- Use mock responses for API tests
- Test error cases, not just happy paths

### Comments
- Don't comment what the code does (code should be self-documenting)
- Do comment **why** for non-obvious decisions
- No commented-out code

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `reqwest` | HTTP client |
| `serde` | Serialization |
| `toml` | Config parsing |
| `ksni` | System tray (SNI) |
| `gtk4` | GUI toolkit |
| `libadwaita` | Modern GTK styling |
| `keyring` | Secret storage |
| `clap` | CLI parsing |
| `tracing` | Logging |
| `chrono` | Date/time |

## Architecture Notes

### Data Flow
1. Polling loop fetches usage from provider APIs every 60 seconds
2. Cost scanner parses local JSONL logs for spending data
3. UsageStore holds snapshots, notifies tray/popup on changes
4. Tray icons render two-bar meters from UsageStore
5. Popup shows detailed view, triggers instant refresh on open

### Provider Authentication
- **Claude**: Reads OAuth tokens from `~/.claude/.credentials.json` (created by `claude login`)
- **Codex**: Reads OAuth tokens from `~/.codex/auth.json` (created by `codex login`)

We never handle passwords - just read tokens created by the official CLIs.

### System Tray
- Uses StatusNotifierItem (SNI) protocol via `ksni` crate
- One tray icon per enabled provider
- Icons are 22x22 RGBA pixmaps with two-bar meter

### Config Location
- Config: `~/.config/claude-bar/config.toml`
- Cache: `~/.cache/claude-bar/`
- Logs: `~/.local/share/claude-bar/`

## Common Tasks

### Adding a New Provider

1. Create `crates/claude-bar-core/src/providers/<name>.rs`
2. Implement `UsageProvider` trait
3. Add to `Provider` enum in `models.rs`
4. Register in `ProviderRegistry`
5. Add settings in `SettingsStore`
6. Reference the CodexBar implementation for API details

### Modifying the Popup UI

1. Edit `crates/claude-bar/src/ui/popup.rs`
2. Use libadwaita widgets for consistency
3. Test both light and dark modes
4. Keep layout consistent with other providers

### Adding a CLI Command

1. Add subcommand to `crates/claude-bar-cli/src/main.rs`
2. Implement handler using core library
3. Support both text and JSON output formats

## Debugging

### Enable Debug Logging
```bash
RUST_LOG=debug cargo run -p claude-bar
```

### Check Credentials
```bash
# Claude
cat ~/.claude/.credentials.json | jq .

# Codex
cat ~/.codex/auth.json | jq .
```

### Test API Directly
```bash
# Claude usage API
curl -H "Authorization: Bearer $(jq -r .accessToken ~/.claude/.credentials.json)" \
     -H "anthropic-beta: oauth-2025-04-20" \
     https://api.anthropic.com/api/oauth/usage
```

## Troubleshooting

### "No secret service available"
The keyring crate needs a running secret service (GNOME Keyring, KDE Wallet, etc.).
```bash
# Check if running
dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames | grep -i secret
```

### "GTK not found" during build
Make sure you're in the nix dev shell:
```bash
nix develop
```

### Tray icon not appearing
Check that your compositor/panel supports SNI:
- Waybar: Enable `tray` module
- Other bars: May need `snixembed` or similar

## Links

- [CodexBar (original)](https://github.com/nickvonkaenel/CodexBar)
- [ksni docs](https://docs.rs/ksni)
- [gtk4-rs docs](https://gtk-rs.org/gtk4-rs/stable/latest/docs/gtk4/)
- [libadwaita docs](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/index.html)
