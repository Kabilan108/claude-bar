# Claude Bar - Agent Instructions

A Rust/GTK4 Linux system tray application for monitoring AI coding assistant usage limits, quotas, and costs. This is a port of [CodexBar](../2026-01-16-steipete-CodexBar/) (macOS).

## Build Instructions

### Development Environment

```bash
# Enter the development shell
nix develop

# Or with direnv (automatic)
direnv allow
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run directly
cargo run -- daemon
cargo run -- status
cargo run -- cost
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Linting

```bash
cargo clippy -- -W clippy::all
cargo fmt --check
```

## Architecture

```
src/
├── main.rs            # Entry point, clap subcommand dispatch
├── lib.rs             # Shared library code
├── cli/               # CLI subcommands (status, cost, refresh)
├── daemon/            # Daemon mode (tray, polling, D-Bus)
├── ui/                # GTK popup window and widgets
├── core/              # Data models, settings, stores
├── providers/         # Provider implementations (Claude, Codex)
├── cost/              # Cost tracking and log scanning
└── icons/             # Icon rendering
```

### Key Components

- **UsageStore**: In-memory store for usage snapshots, thread-safe with change notifications
- **CostStore**: Scans local JSONL logs to calculate API spending
- **SettingsStore**: TOML configuration with inotify hot-reload
- **Provider Registry**: Manages enabled providers, coordinates fetching
- **SNI Tray**: System tray icons via StatusNotifierItem (ksni)
- **GTK Popup**: libadwaita popup with usage details

## Coding Conventions

### Rust Style

- Follow standard Rust conventions (rustfmt, clippy)
- Use `thiserror` for error types, `anyhow` for propagation
- Prefer explicit error handling over unwrap/expect in library code
- Use `tracing` for structured logging

### Error Handling

```rust
// Define specific errors with thiserror
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("credentials not found: {0}")]
    CredentialsNotFound(String),
    #[error("token expired")]
    TokenExpired,
    #[error("API error: {0}")]
    Api(#[from] reqwest::Error),
}

// Use anyhow::Result for CLI/daemon code
pub async fn run() -> anyhow::Result<()> {
    // ...
}
```

### Async Code

- Use `tokio` for async runtime
- Prefer `async-trait` for async trait methods
- Use channels for inter-component communication

### Thread Safety

- Use `Arc<RwLock<T>>` for shared mutable state
- Prefer message passing over shared state where possible

## Reference Implementation

The original macOS implementation is at `/vault/experiments/2026-01-16-steipete-CodexBar/`. Key files to reference:

| Component | Reference File |
|-----------|----------------|
| Usage models | `Sources/CodexBarCore/UsageFetcher.swift` |
| Claude provider | `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift` |
| Codex provider | `Sources/CodexBarCore/Providers/Codex/CodexOAuth/CodexOAuthUsageFetcher.swift` |
| Cost scanning | `Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner.swift` |
| Icon rendering | `Sources/CodexBar/IconRenderer.swift` |
| Settings | `Sources/CodexBar/SettingsStore.swift` |

## Spec Reference

See `SPEC.md` for the full specification including:
- Design decisions and rationale
- API endpoints and data formats
- UI layout and behavior
- Phase-by-phase implementation checklist

## Quick Reference

### CLI Commands

```bash
claude-bar daemon     # Start the tray daemon
claude-bar status     # Show usage status (standalone)
claude-bar cost       # Show cost summary (standalone)
claude-bar refresh    # Trigger daemon refresh via D-Bus
```

### Configuration

Config file: `~/.config/claude-bar/config.toml`

```toml
[providers]
merge_icons = true

[providers.claude]
enabled = true

[providers.codex]
enabled = true

[display]
show_as_remaining = false

[notifications]
enabled = true
threshold = 0.9

debug = false
```

### Credentials Locations

- Claude: `~/.claude/.credentials.json`
- Codex: `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`)

### Log Locations

- Claude: `~/.claude/projects/` (JSONL files)
- Codex: `~/.codex/sessions/` (JSONL files)
