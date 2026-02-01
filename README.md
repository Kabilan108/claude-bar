# Claude Bar

A Linux system tray application for monitoring AI coding assistant usage limits, quotas, and costs.

Claude Bar displays real-time usage information for Claude Code and Codex directly in your system tray, with detailed breakdowns available in a popup interface.

## Features

- System tray icons showing usage via two-bar meters (session and weekly quotas)
- GTK4/libadwaita popup with detailed usage percentages and reset countdowns
- Cost tracking from local session logs
- Desktop notifications when usage exceeds configurable thresholds
- CLI tool for scripting and debugging
- Hot-reloadable TOML configuration

## Supported Providers

| Provider | Authentication | Usage API | Cost Tracking |
|----------|----------------|-----------|---------------|
| Claude Code | OAuth tokens from `~/.claude/.credentials.json` | Anthropic OAuth API | `~/.claude/projects/` logs |
| Codex | OAuth tokens from `~/.codex/auth.json` | OpenAI ChatGPT API | `~/.codex/sessions/` logs |

## Installation

### Using Nix Flake

Add to your flake inputs:

```nix
{
  inputs.claude-bar.url = "github:kabilan/claude-bar";
}
```

Apply the overlay to make `pkgs.claude-bar` available. In your NixOS configuration or home-manager setup:

```nix
# In your NixOS configuration (configuration.nix or flake)
nixpkgs.overlays = [ inputs.claude-bar.overlays.default ];

# Or if using home-manager standalone
home-manager.users.<username> = {
  nixpkgs.overlays = [ inputs.claude-bar.overlays.default ];
};
```

Then use the Home Manager module:

```nix
{
  imports = [ inputs.claude-bar.homeManagerModules.default ];

  services.claude-bar = {
    enable = true;
    theme.mode = "system";
    settings = {
      providers = {
        claude.enabled = true;
        codex.enabled = true;
        merge_icons = false;
      };
      notifications = {
        enabled = true;
        threshold = 0.9;
      };
    };
  };
}
```

#### Alternative: Explicit Package

If you prefer not to use the overlay, you can pass the package explicitly (requires `inputs` in your module's arguments via `extraSpecialArgs`):

```nix
services.claude-bar = {
  enable = true;
  package = inputs.claude-bar.packages.${pkgs.system}.default;
  # ... settings
};
```

### Building from Source

```bash
nix develop
cargo build --release
```

## Usage

### Daemon

Start the system tray daemon:

```bash
claude-bar daemon
```

The daemon will:
- Display tray icons for enabled providers
- Poll usage APIs every 60 seconds (with exponential backoff on errors)
- Show a popup when clicking the tray icon
- Register a D-Bus interface for external control

### CLI Commands

Check current usage status:

```bash
claude-bar status
claude-bar status --json
claude-bar status --provider claude
```

View cost summary:

```bash
claude-bar cost
claude-bar cost --json
claude-bar cost --days 7
```

Trigger a manual refresh:

```bash
claude-bar refresh
```

Trigger a pricing refresh:

```bash
claude-bar refresh-pricing
```

Generate shell completions:

```bash
claude-bar completions bash > ~/.local/share/bash-completion/completions/claude-bar
claude-bar completions zsh > ~/.local/share/zsh/site-functions/_claude-bar
claude-bar completions fish > ~/.config/fish/completions/claude-bar.fish
```

## Configuration

Configuration is stored at `~/.config/claude-bar/config.toml`. Create from the example:

```bash
mkdir -p ~/.config/claude-bar
cp config.example.toml ~/.config/claude-bar/config.toml
```

### Configuration Options

```toml
[providers]
merge_icons = false  # Single merged icon vs separate per-provider icons

[providers.claude]
enabled = true

[providers.codex]
enabled = true

[display]
show_as_remaining = false  # "78% used" vs "22% remaining"

[browser]
preferred = "firefox"  # Optional: browser for dashboard links (default: xdg-open)

[notifications]
enabled = true
threshold = 0.9  # 90% usage triggers notification

[theme]
mode = "system"  # "system", "light", or "dark"

debug = false  # Enable verbose logging
```

The daemon watches the config file and reloads settings automatically on changes.

## Popup Positioning

The popup uses `gtk4-layer-shell` to position itself as a Wayland layer surface, anchored to a screen edge. This eliminates focus-stealing issues on compositors with focus-follows-mouse (e.g. Hyprland). No window manager rules are needed.

Configure the popup position in `config.toml`:

```toml
[popup]
anchor = "top-right"      # top-left | top-right | bottom-left | bottom-right
margin_top = 40            # pixels from anchored edge
margin_right = 10
margin_bottom = 0
margin_left = 0
dismiss_timeout_ms = 300   # grace period before closing on focus loss (0 = instant)
```

Changes are applied immediately via hot-reload.

## Architecture

```
claude-bar daemon
├── SNI Tray (ksni) - System tray icons with usage meters
├── GTK Popup (libadwaita) - Detailed usage display
├── D-Bus Interface (zbus) - External control API
└── Polling Loop - Background usage/cost fetching

claude-bar CLI
├── status - Direct API fetch for current usage
├── cost - Local log scanning for cost data
├── refresh - D-Bus call to trigger daemon refresh
└── refresh-pricing - D-Bus call to refresh pricing cache
```

## Logging

The daemon logs to multiple destinations:
- Console (stderr) - Human-readable format
- File (`~/.local/share/claude-bar/claude-bar.log`) - JSONL format
- journald - Structured logs for systemd integration

Set log level via environment:

```bash
RUST_LOG=debug claude-bar daemon
RUST_LOG=claude_bar=trace claude-bar daemon
```

## Troubleshooting

### "Run `claude` to authenticate"

Claude Bar reads credentials passively and does not refresh tokens. If you see this error:

1. Run `claude` in your terminal to start the Claude CLI
2. Complete the authentication flow
3. Claude Bar will automatically detect the new credentials

### "Run `codex` to authenticate"

Similar to Claude, run the `codex` CLI to refresh Codex credentials.

### Tray icon not appearing

Ensure your desktop environment supports StatusNotifierItem (SNI). Most modern DE's do, but you may need:
- GNOME: Install `gnome-shell-extension-appindicator`
- Other DEs: Check your system tray settings

### High CPU usage

The daemon checks for refresh conditions every second but only fetches data when needed. If you're seeing high CPU:
- Check if the daemon is in an error loop (review logs)
- Ensure your network is stable

## Development

```bash
# Enter dev environment
nix develop

# Build
cargo build

# Run tests
cargo test -- --test-threads=1

# Run clippy
cargo clippy

# Watch for changes
cargo watch -x check
```

## License

MIT
