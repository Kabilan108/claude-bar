# Claude Bar - GTK/Linux Port Specification

A Linux system tray application for monitoring AI coding assistant usage limits, quotas, and costs. This is a Rust/GTK4 port of [CodexBar](../2026-01-16-steipete-CodexBar/) (macOS) focused on Claude Code and Codex providers.

---

## Agent Instructions

**IMPORTANT**: As you complete work on this project:
1. Check off completed items by changing `- [ ]` to `- [x]`
2. Add notes under items if there are important implementation details
3. If you encounter blockers, add a `> BLOCKED:` note under the item
4. Reference the original CodexBar implementation when porting logic

**Reference codebase**: `/vault/experiments/2026-01-16-steipete-CodexBar/`

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Design Decisions](#design-decisions)
4. [Technology Stack](#technology-stack)
5. [Phase 1: Project Setup](#phase-1-project-setup)
6. [Phase 2: Core Data Layer](#phase-2-core-data-layer)
7. [Phase 3: Provider Implementations](#phase-3-provider-implementations)
8. [Phase 4: Cost Tracking](#phase-4-cost-tracking)
9. [Phase 5: System Tray Integration](#phase-5-system-tray-integration)
10. [Phase 6: GTK Popup UI](#phase-6-gtk-popup-ui)
11. [Phase 7: CLI Tool](#phase-7-cli-tool)
12. [Phase 8: Nix Integration](#phase-8-nix-integration)
13. [Phase 9: Polish & Testing](#phase-9-polish--testing)
14. [Future Work](#future-work)
15. [Reference Files](#reference-files)

---

## Overview

### What Claude Bar Does

- Displays real-time usage limits for Claude Code and Codex in the system tray
- Shows two-bar meter icons: primary (5-hour session) and secondary (weekly quota)
- Provides detailed popup with percentages, reset countdowns, and cost tracking
- Scans local JSONL session logs to calculate API spending
- Supports merged (single icon) or separate (per-provider) tray icons

### Key Features

- **System tray icons** via StatusNotifierItem (SNI)
- **GTK4/libadwaita popup** with modern styling
- **60-second polling** with instant refresh on popup open (5s cooldown)
- **Cost tracking** from local session logs with pricing from models.dev
- **CLI tool** for scripting and debugging (same binary, subcommands)
- **TOML configuration** with hot-reload via inotify
- **Desktop notifications** when usage exceeds 90%

### Providers (Phase 1)

| Provider | Auth Method | Usage API | Cost Tracking |
|----------|-------------|-----------|---------------|
| **Claude Code** | OAuth tokens from `~/.claude/.credentials.json` | Anthropic OAuth API | `~/.claude/projects/` logs |
| **Codex** | OAuth tokens from `~/.codex/auth.json` | OpenAI ChatGPT API | `~/.codex/sessions/` logs |

### Providers (Future)

- Cursor (browser cookies, monthly billing model)
- OpenCode Zen (browser cookies, cost-only display)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     claude-bar daemon                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ UsageStore  │  │ CostStore   │  │ SettingsStore       │ │
│  │ (API data)  │  │ (log scans) │  │ (TOML config)       │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
│         │                │                     │            │
│         └────────────────┼─────────────────────┘            │
│                          │                                  │
│                          ▼                                  │
│              ┌───────────────────────┐                      │
│              │   Provider Registry   │                      │
│              │  ┌───────┐ ┌───────┐  │                      │
│              │  │Claude │ │Codex  │  │                      │
│              │  └───────┘ └───────┘  │                      │
│              └───────────────────────┘                      │
│                          │                                  │
│         ┌────────────────┼────────────────┐                 │
│         ▼                ▼                ▼                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ SNI Tray    │  │ GTK Popup   │  │ D-Bus API   │         │
│  │ (ksni)      │  │ (libadwaita)│  │ (zbus)      │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                     claude-bar CLI                           │
├─────────────────────────────────────────────────────────────┤
│  Standalone binary (same executable, subcommands)           │
│  - Fetches directly from provider APIs                      │
│  - Shares config (~/.config/claude-bar/config.toml)         │
│  - Shares cache (~/.cache/claude-bar/)                      │
│  - `refresh` command talks to daemon via D-Bus              │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Polling loop** (60s interval) fetches usage from provider APIs
2. **Cost scanner** (60s interval) parses JSONL logs for spending data
3. **UsageStore** holds current snapshots in memory, notifies UI on changes
4. **SNI tray** renders two-bar icons from UsageStore data
5. **Popup** shows detailed view on tray icon click (triggers instant refresh with 5s cooldown)
6. **CLI** fetches directly from APIs (standalone), shares config/cache with daemon

---

## Design Decisions

These decisions were made during the spec interview process.

### Authentication & Tokens

| Decision | Choice |
|----------|--------|
| Token refresh | **Don't refresh ourselves** - read tokens passively, show clear errors directing users to run `claude` or `codex` CLI to re-authenticate (matching CodexBar behavior) |
| Credentials location | Read from documented paths only: `~/.claude/.credentials.json`, `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`) |

### Architecture

| Decision | Choice |
|----------|--------|
| CLI/Daemon relationship | **Both standalone** - CLI fetches directly from APIs, daemon manages tray. Both share TOML config and cache directory |
| Binary structure | **Single binary** `claude-bar` with subcommands: `daemon`, `status`, `cost`, `refresh` |
| Default command (no args) | **Show help** - user must specify subcommand |
| CLI `refresh` command | **D-Bus to daemon** - pokes running daemon to refresh immediately |
| IPC mechanism | **D-Bus** following freedesktop patterns |

### System Tray

| Decision | Choice |
|----------|--------|
| Animation frame rate | **15 FPS** for Knight Rider loading animation (balance CPU/smoothness) |
| Popup positioning | **Top-right corner**, auto-detect panel height via wlr-layer-shell, fallback to default margin |
| Icon merging | **`merge_icons = true` by default** (matching CodexBar). Single icon + combined popup when true; separate independent icons when false |
| Merged icon display | **Primary provider only** in tray icon, others shown in popup |
| Icon colors | **Same gold/amber (#F5A623)** for all providers, differentiated by SVG icon shape |
| Icon assets | **Embedded SVG assets** for provider logos, rendered to pixmap |
| Bar orientation | **Horizontal** (like CodexBar) - bars stack vertically, fill left-to-right |
| Icon bars represent | **Primary (session) + Secondary (weekly-all)** only; Opus shown in popup if present |

### State & Data

| Decision | Choice |
|----------|--------|
| State persistence | **Memory only** - no disk persistence for usage snapshots, always start fresh |
| Malformed JSONL logs | **Skip bad lines** silently, log at debug level |
| Model pricing source | **models.dev** - fetch daily, cache to disk, fallback to embedded defaults (same JSON format) |
| Pricing fallback order | Fresh fetch → Cached prices → Embedded defaults |
| Opus rate window | **Show if present** as third section in popup |
| Cache layout | **Flat files**: `~/.cache/claude-bar/{pricing,claude-cost,codex-cost}.json` |
| Config reload | **Hot-reload with inotify** - daemon watches config file |

### UI & Display

| Decision | Choice |
|----------|--------|
| Dashboard link visibility | **Auth-only** - only show "Open Dashboard" when credentials are valid |
| Reset countdown updates | **Live update every 1 minute** while popup is visible |
| Browser launch | **Try configured browser first**, fallback to `xdg-open` |
| No-auth icon state | **Show with error state** - icon visible but grayed, popup shows "credentials needed" with hints |
| Progress bar colors | **Single brand color** (gold/amber) regardless of usage level |
| Update time display | **Relative time** ("Updated 30s ago") that updates live |
| Error states | **Show troubleshooting hints** (e.g., "Run `claude` to authenticate") |
| Popup dismiss | **Click outside to close** (focus loss) |

### Notifications

| Decision | Choice |
|----------|--------|
| Notification library | **notify-rust** crate (handles D-Bus internally) |
| Notification style | **Simple text** - no action buttons |
| Notification toggle | **`notifications.enabled = true` by default**, user can disable |
| Notification frequency | **Once per reset cycle** - fire once when crossing 90%, don't repeat |
| High usage threshold | **90%** triggers notification |

### Network & Retry

| Decision | Choice |
|----------|--------|
| Retry strategy | **Exponential backoff**: 60s, 120s, 240s, max 10 minutes |
| Poll interval | **Fixed 60 seconds** (not configurable) |
| Popup refresh cooldown | **5 seconds** - skip refresh if last fetch was <5s ago |
| Request timeout | **30 seconds** |

### GTK & Application

| Decision | Choice |
|----------|--------|
| GTK model | **Single binary** that is a proper GTK Application with app ID (`com.github.kabilan.claude-bar`) for Hyprland window rules |
| libadwaita | **Required** - no fallback to plain GTK4 |
| D-Bus interface | **Follow freedesktop patterns** - methods, signals, properties |

### Logging

| Decision | Choice |
|----------|--------|
| Log format | **JSONL** (JSON Lines) to file |
| Log destination | **File + journald** - structured logs with detailed metadata |
| Log file location | `~/.local/share/claude-bar/claude-bar.log` |
| Debug mode | **Config option** `debug = true` enables verbose tooltips and trace logging |

### Nix Integration

| Decision | Choice |
|----------|--------|
| Modules provided | **Both Home Manager and NixOS** modules |
| HM auto-start | **Enabled by default** when `services.claude-bar.enable = true` |
| Shell completions | **Generate at build time**, flake installs automatically (bash, zsh, fish) |

---

## Technology Stack

| Component | Library | Crate |
|-----------|---------|-------|
| Async runtime | Tokio | `tokio` |
| HTTP client | reqwest | `reqwest` |
| JSON parsing | serde | `serde`, `serde_json` |
| TOML config | toml | `toml` |
| System tray | StatusNotifierItem | `ksni` |
| GUI toolkit | GTK4 + libadwaita | `gtk4`, `libadwaita` |
| D-Bus | zbus | `zbus` |
| CLI | clap | `clap` |
| Logging | tracing | `tracing`, `tracing-subscriber` |
| File watching | notify | `notify` |
| Date/time | chrono | `chrono` |
| Directories | dirs | `dirs` |
| Notifications | notify-rust | `notify-rust` |

---

## Phase 1: Project Setup

### 1.1 Directory Structure

- [x] Create project directory at `/vault/experiments/2026-01-18-claude-bar/`
- [x] Initialize git repository
- [x] Create directory structure:
  ```
  claude-bar/
  ├── flake.nix              # Nix dev environment + package
  ├── flake.lock
  ├── Cargo.toml             # Workspace root
  ├── Cargo.lock
  ├── SPEC.md                # This file
  ├── AGENTS.md              # Agent instructions
  ├── README.md
  ├── config.example.toml    # Example configuration
  ├── assets/
  │   ├── claude-icon.svg    # Claude provider icon
  │   └── codex-icon.svg     # Codex provider icon
  ├── src/
  │   ├── main.rs            # Entry point, subcommand dispatch
  │   ├── lib.rs             # Shared library code
  │   ├── cli/
  │   │   ├── mod.rs
  │   │   ├── status.rs      # status subcommand
  │   │   ├── cost.rs        # cost subcommand
  │   │   └── refresh.rs     # refresh subcommand (D-Bus)
  │   ├── daemon/
  │   │   ├── mod.rs
  │   │   ├── app.rs         # GTK Application setup
  │   │   ├── tray.rs        # SNI tray management
  │   │   ├── dbus.rs        # D-Bus interface
  │   │   └── polling.rs     # Background polling loop
  │   ├── ui/
  │   │   ├── mod.rs
  │   │   ├── popup.rs       # Main popup window
  │   │   ├── progress.rs    # Progress bar widget
  │   │   └── styles.rs      # CSS/styling
  │   ├── core/
  │   │   ├── mod.rs
  │   │   ├── models.rs      # Data models
  │   │   ├── settings.rs    # TOML config + hot-reload
  │   │   ├── store.rs       # UsageStore, CostStore
  │   │   └── notifications.rs
  │   ├── providers/
  │   │   ├── mod.rs         # Provider trait + registry
  │   │   ├── claude.rs      # Claude provider
  │   │   └── codex.rs       # Codex provider
  │   ├── cost/
  │   │   ├── mod.rs
  │   │   ├── scanner.rs     # Log scanning base
  │   │   ├── claude.rs      # Claude log scanner
  │   │   ├── codex.rs       # Codex log scanner
  │   │   └── pricing.rs     # models.dev pricing fetch
  │   └── icons/
  │       ├── mod.rs
  │       └── renderer.rs    # Icon rendering
  └── nix/
      ├── hm-module.nix      # Home Manager module
      └── nixos-module.nix   # NixOS module
  ```

### 1.2 Nix Flake

- [x] Create `flake.nix` with:
  - [x] Rust toolchain (stable, with rust-analyzer)
  - [x] GTK4 and libadwaita development libraries
  - [x] pkg-config for native dependencies
  - [x] D-Bus development files
  - [x] Development tools (cargo-watch, cargo-clippy)
  - [x] Shell completions installation
- [x] Add `direnv` integration (`.envrc` with `use flake`)
- [x] Verify `cargo build` works in dev shell
- [x] Package builds with completions for bash, zsh, fish

**Dependencies to include in flake**:
```nix
# Build inputs
gtk4
libadwaita
pkg-config
openssl
dbus

# Rust
rustToolchain  # or rust-bin.stable.latest.default
rust-analyzer
```

### 1.3 Cargo Configuration

- [x] Create root `Cargo.toml`:
  ```toml
  [package]
  name = "claude-bar"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  tokio = { version = "1", features = ["full"] }
  reqwest = { version = "0.11", features = ["json"] }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  toml = "0.8"
  chrono = { version = "0.4", features = ["serde"] }
  dirs = "5"
  tracing = "0.1"
  tracing-subscriber = { version = "0.3", features = ["json"] }
  thiserror = "1"
  anyhow = "1"

  # GTK/UI
  gtk4 = "0.7"
  libadwaita = "0.5"
  ksni = "0.2"

  # D-Bus
  zbus = "3"

  # CLI
  clap = { version = "4", features = ["derive"] }

  # File watching
  notify = "6"

  # Notifications
  notify-rust = "4"

  # Async traits
  async-trait = "0.1"
  ```
- [x] Verify `cargo check` passes

### 1.4 AGENTS.md

- [x] Create `AGENTS.md` with:
  - [x] Project overview and goals
  - [x] Build instructions (`nix develop`, `cargo build`)
  - [x] Test instructions (`cargo test`)
  - [x] Architecture summary
  - [x] Coding conventions (Rust style, error handling)
  - [x] Reference to this SPEC.md
  - [x] Reference to original CodexBar for implementation details

### 1.5 Basic Scaffolding

- [x] Create minimal `main.rs` with clap subcommand structure
- [x] Create minimal `lib.rs` that compiles
- [x] Add `.gitignore` for Rust/Nix artifacts
- [x] Verify build: `cargo build`

### Phase 1 Notes

**Dependency versions updated from spec:**
- `reqwest` 0.11 → 0.12 (current stable, with `rustls-tls` feature instead of default openssl)
- `gtk4` 0.7 → 0.9 (current stable)
- `libadwaita` 0.5 → 0.7 (matches gtk4 0.9)
- `zbus` 3 → 4 (current stable, significant API changes)
- Added `tracing-journald` for systemd journal logging
- Added `clap_complete` for shell completion generation

**Flake structure:**
- Used `rust-overlay` for reproducible Rust toolchain
- `wrapGAppsHook4` required for GTK4 apps to find schemas/icons at runtime
- `graphene` needed as transitive GTK4 dependency
- Shell completions generated in `postInstall` phase

**Code organization:**
- Kept `lib.rs` minimal (just re-exports modules) to avoid duplicate compilation
- Tests live in their respective modules with `#[cfg(test)]` blocks
- CLI subcommands dispatch to module functions rather than inline logic

**zbus 4.x changes from 3.x:**
- `#[dbus_interface]` → `#[interface]`
- Signal syntax changed to associated functions with `SignalContext`
- Connection methods are now async

---

## Phase 2: Core Data Layer

### 2.1 Data Models

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/UsageFetcher.swift`

- [x] Create `core/models.rs` with:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct RateWindow {
      pub used_percent: f64,        // 0.0 to 1.0
      pub window_minutes: Option<i32>,
      pub resets_at: Option<DateTime<Utc>>,
      pub reset_description: Option<String>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct UsageSnapshot {
      pub primary: Option<RateWindow>,    // 5-hour session
      pub secondary: Option<RateWindow>,  // Weekly quota
      pub opus: Option<RateWindow>,       // Model-specific (shown in popup only)
      pub updated_at: DateTime<Utc>,
      pub identity: ProviderIdentity,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ProviderIdentity {
      pub email: Option<String>,
      pub organization: Option<String>,
      pub plan: Option<String>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct CostSnapshot {
      pub today_cost: f64,
      pub monthly_cost: f64,
      pub currency: String,  // "USD"
      pub daily_breakdown: Vec<DailyCost>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct DailyCost {
      pub date: NaiveDate,
      pub model: String,
      pub cost: f64,
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub enum Provider {
      Claude,
      Codex,
  }
  ```
- [x] Add unit tests for model serialization

### 2.2 Settings Store

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/SettingsStore.swift`

- [x] Create `core/settings.rs` with:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Settings {
      pub providers: ProviderSettings,
      pub display: DisplaySettings,
      pub browser: BrowserSettings,
      pub notifications: NotificationSettings,
      pub debug: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ProviderSettings {
      pub claude: ProviderConfig,
      pub codex: ProviderConfig,
      pub merge_icons: bool,  // default: true
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ProviderConfig {
      pub enabled: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct DisplaySettings {
      pub show_as_remaining: bool,  // Show "remaining" vs "used"
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct BrowserSettings {
      pub preferred: Option<String>,  // None = use xdg-open
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct NotificationSettings {
      pub enabled: bool,  // default: true
      pub threshold: f64, // default: 0.9 (90%)
  }
  ```
- [x] Implement TOML loading from `~/.config/claude-bar/config.toml`
- [x] Implement default settings when config missing
- [x] Create `config.example.toml` with all options documented
- [x] Implement hot-reload with `notify` crate (inotify)
- [x] Add settings validation

### 2.3 Usage Store

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/UsageStore.swift`

- [x] Create `core/store.rs` with:
  ```rust
  pub struct UsageStore {
      snapshots: HashMap<Provider, UsageSnapshot>,
      costs: HashMap<Provider, CostSnapshot>,
      errors: HashMap<Provider, String>,
      last_fetch: HashMap<Provider, Instant>,
      notified_90_percent: HashSet<Provider>,  // Track notification state
  }

  impl UsageStore {
      pub fn get_snapshot(&self, provider: Provider) -> Option<&UsageSnapshot>;
      pub fn get_cost(&self, provider: Provider) -> Option<&CostSnapshot>;
      pub fn get_error(&self, provider: Provider) -> Option<&str>;
      pub fn update_snapshot(&mut self, provider: Provider, snapshot: UsageSnapshot);
      pub fn update_cost(&mut self, provider: Provider, cost: CostSnapshot);
      pub fn set_error(&mut self, provider: Provider, error: String);
      pub fn should_refresh(&self, provider: Provider, cooldown: Duration) -> bool;
      pub fn should_notify(&mut self, provider: Provider, threshold: f64) -> bool;
      pub fn reset_notification(&mut self, provider: Provider);  // Called when usage resets
  }
  ```
- [x] Make thread-safe with `Arc<RwLock<...>>`
- [x] Add change notification mechanism (channels)

### 2.4 Notifications

- [x] Create `core/notifications.rs`:
  ```rust
  pub fn send_high_usage_notification(provider: Provider, percent: f64) -> Result<()>;
  ```
- [x] Use `notify-rust` crate
- [x] Simple text notification (no actions)

### Phase 2 Notes

**SettingsWatcher implementation:**
- Uses `notify` crate with `RecommendedWatcher` for inotify on Linux
- Watches the parent config directory (non-recursive) to detect both creates and modifies
- Debounces file changes with a 100ms delay to avoid duplicate events
- Validates settings before applying changes; keeps old settings if validation fails
- Provides `broadcast::Receiver<Settings>` for components to subscribe to changes
- Both async (`get()`) and blocking (`get_blocking()`) accessors available

**UsageStore change notification:**
- Uses `tokio::sync::broadcast` channel with capacity of 64 messages
- `StoreUpdate` enum variants: `UsageUpdated`, `CostUpdated`, `ErrorOccurred`, `ErrorCleared`
- Subscribers receive updates immediately after state changes
- `ErrorCleared` sent before `UsageUpdated` when a successful fetch clears an error state

**Data models:**
- All models derive `Serialize` and `Deserialize` for JSON/TOML compatibility
- `RateWindow` includes helper methods: `remaining_percent()`, `is_high_usage()`
- `UsageSnapshot::max_usage()` returns the highest usage across all rate windows
- `CostSnapshot` defaults to USD currency with empty daily breakdown

---

## Phase 3: Provider Implementations

### 3.1 Provider Trait

- [x] Create `providers/mod.rs` with trait:
  ```rust
  #[async_trait]
  pub trait UsageProvider: Send + Sync {
      fn name(&self) -> &'static str;
      fn identifier(&self) -> Provider;
      async fn fetch_usage(&self) -> Result<UsageSnapshot>;
      fn dashboard_url(&self) -> &'static str;
      fn has_valid_credentials(&self) -> bool;
      fn credential_error_hint(&self) -> &'static str;
  }
  ```

### 3.2 Claude Provider

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/Providers/Claude/`

- [x] Create `providers/claude.rs`
- [x] Implement credential reading:
  - [x] Read from `~/.claude/.credentials.json`
  - [x] Parse token structure: `{ accessToken, refreshToken, expiresAt, scopes }`
  - [x] Check token expiration (don't refresh - just report error)
  - [x] Reference: `ClaudeOAuthCredentials.swift`
- [x] Implement usage API call:
  - [x] Endpoint: `https://api.anthropic.com/api/oauth/usage`
  - [x] Headers: `Authorization: Bearer <token>`, `anthropic-beta: oauth-2025-04-20`
  - [x] Reference: `ClaudeUsageFetcher.swift`
- [x] Parse response into `UsageSnapshot`:
  - [x] Extract primary (5-hour), secondary (weekly), opus (if present)
  - [x] Extract identity (email, organization, plan)
  - [x] Reference: `ClaudeUsageSnapshot.swift`
- [x] Error handling:
  - [x] Missing credentials file → "Run `claude` to authenticate"
  - [x] Expired token → "Run `claude` to refresh credentials"
  - [x] API errors (rate limit, auth failure)
- [x] Add unit tests with mock responses

### 3.3 Codex Provider

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/Providers/Codex/`

- [x] Create `providers/codex.rs`
- [x] Implement credential reading:
  - [x] Read from `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`)
  - [x] Parse token structure
  - [x] Reference: `CodexOAuthCredentials.swift`
- [x] Implement usage API call:
  - [x] Endpoint: `https://chatgpt.com/backend-api/wham/usage`
  - [x] Headers: `Authorization: Bearer <token>`
  - [x] Reference: `CodexOAuthUsageFetcher.swift`
- [x] Parse response into `UsageSnapshot`
- [x] Error handling:
  - [x] Missing credentials → "Run `codex` to authenticate"
  - [x] Expired/revoked token
  - [x] API errors
- [x] Add unit tests with mock responses

### 3.4 Provider Registry

- [x] Create `providers/registry.rs`:
  ```rust
  pub struct ProviderRegistry {
      providers: Vec<Arc<dyn UsageProvider>>,
  }

  impl ProviderRegistry {
      pub fn new(settings: &Settings) -> Self;
      pub fn enabled_providers(&self) -> impl Iterator<Item = &dyn UsageProvider>;
      pub fn primary_provider(&self) -> Option<&dyn UsageProvider>;
      pub async fn fetch_all(&self) -> HashMap<Provider, Result<UsageSnapshot>>;
  }
  ```
- [x] Initialize based on settings (enabled providers)

### Phase 3 Notes

**Claude provider credential structure:**
- Credentials file wraps OAuth data in a `claudeAiOauth` object
- Access token validity checked by ensuring it's non-empty
- `rateLimitTier` field used to infer plan name (Pro, Max, Team, Enterprise)
- API response uses `utilization` (0-100) which is converted to `used_percent` (0.0-1.0)
- Windows available: `five_hour`, `seven_day`, `seven_day_sonnet`, `seven_day_opus`

**Codex provider credential structure:**
- Credentials stored in `tokens` object with `access_token`, `refresh_token`, `id_token`, `account_id`
- `ChatGPT-Account-Id` header sent if account_id is present
- API response uses integer `used_percent` (0-100) and unix timestamps for `reset_at`
- `limit_window_seconds` converted to `window_minutes` for display

**HTTP client configuration:**
- 30-second timeout on all requests
- User-Agent set to "claude-bar"
- Proper Accept and Content-Type headers for JSON

**Error handling:**
- 401/403 errors provide specific authentication hints
- Other HTTP errors include status code and response body
- Credential parsing errors include context about the file path

**Test coverage:**
- Credential parsing for both providers
- API response parsing with full and minimal responses
- Reset time parsing (ISO8601 for Claude, unix timestamp for Codex)
- Window to RateWindow conversion
- Plan type inference/formatting
- Provider metadata (name, identifier, dashboard URL, error hint)

---

## Phase 4: Cost Tracking

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/Vendored/CostUsage/`

### 4.1 Pricing Fetcher

- [x] Create `cost/pricing.rs`:
  ```rust
  pub struct PricingStore {
      prices: HashMap<String, ModelPricing>,  // model_id -> pricing
      last_fetch: Option<DateTime<Utc>>,
  }

  pub struct ModelPricing {
      pub input_price_per_million: f64,
      pub output_price_per_million: f64,
  }

  impl PricingStore {
      pub async fn fetch_from_models_dev() -> Result<Self>;
      pub fn load_from_cache() -> Option<Self>;
      pub fn load_embedded_defaults() -> Self;
      pub fn save_to_cache(&self) -> Result<()>;
      pub fn get_price(&self, model: &str) -> Option<&ModelPricing>;
  }
  ```
- [x] Fetch from models.dev API
- [x] Cache to `~/.cache/claude-bar/pricing.json`
- [x] Embed defaults in binary (same JSON format)
- [x] Refresh daily

### 4.2 Log Scanner Base

- [x] Create `cost/scanner.rs` with scanner trait:
  ```rust
  pub trait CostScanner: Send + Sync {
      fn scan(&self, since: NaiveDate, until: NaiveDate) -> Result<Vec<DailyCost>>;
  }
  ```

### 4.3 Claude Log Scanner

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner+Claude.swift`

- [x] Create `cost/claude.rs`
- [x] Find Claude project directories:
  - [x] `~/.claude/projects/`
  - [x] `~/.config/claude/projects/`
- [x] Parse JSONL log files:
  - [x] One JSON object per line
  - [x] Extract: timestamp, model, input_tokens, output_tokens
  - [x] Skip malformed lines (log at debug level)
- [x] Calculate costs using pricing store
- [ ] Cache results to `~/.cache/claude-bar/claude-cost.json`

### 4.4 Codex Log Scanner

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner.swift`

- [x] Create `cost/codex.rs`
- [x] Find Codex session directories:
  - [x] `~/.codex/sessions/YYYY/MM/DD/*.jsonl`
  - [x] `$CODEX_HOME/sessions/` if set
- [x] Parse JSONL session logs
- [x] Calculate costs using pricing store
- [ ] Cache results to `~/.cache/claude-bar/codex-cost.json`

### 4.5 Cost Store Integration

- [x] Create `cost/store.rs`:
  ```rust
  pub struct CostStore {
      claude_scanner: ClaudeCostScanner,
      codex_scanner: CodexCostScanner,
      pricing: PricingStore,
  }

  impl CostStore {
      pub async fn refresh_pricing(&mut self) -> Result<()>;
      pub fn scan_all(&mut self) -> HashMap<Provider, CostSnapshot>;
      pub fn get_cost(&self, provider: Provider) -> Option<&CostSnapshot>;
  }
  ```
- [ ] Integrate with main polling loop

### Phase 4 Notes

**Pricing implementation:**
- `ModelPricing` extended to support prompt caching: `cache_creation_price_per_million`, `cache_read_price_per_million`
- Tiered pricing supported for models like Claude Sonnet 4 (different rates above 200k tokens): `threshold_tokens`, `*_above_threshold` fields
- `TokenUsage` struct holds all token counts: input, output, cache_creation, cache_read
- `calculate_cost()` method handles both tiered and flat pricing automatically
- Model name normalization strips prefixes ("anthropic.", "openai/"), suffixes ("-codex", "-v1:0")
- Fuzzy matching for model lookups (partial matches when exact match fails)

**Claude log scanner:**
- Parses `type: "assistant"` entries with `message.usage` data
- Deduplication using `messageId:requestId` key (handles streaming chunks)
- Extracts: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`
- Recursive directory walk to find all `.jsonl` files
- Filters by date from filename if present (YYYY-MM-DD.jsonl) or parses all

**Codex log scanner:**
- Uses directory structure for date filtering: `sessions/YYYY/MM/DD/*.jsonl`
- Two entry types: `turn_context` (sets current model) and `event_msg` (token counts)
- Delta calculation from cumulative totals (Codex reports running totals, not per-request)
- Supports both `cached_input_tokens` and `cache_read_input_tokens` field names

**Cost store:**
- Combines both scanners with shared pricing store
- `scan_all()` returns CostSnapshot per provider with today/monthly aggregation
- `scan_provider()` for single-provider refresh
- Pricing refresh with 24-hour cache validity
- Cached costs retained on scan failure for resilience

**Deferred to future phases:**
- Per-file caching with mtime-based invalidation (not needed for initial release)
- Incremental parsing from byte offset (optimization)
- Integration with main polling loop (Phase 7)

---

## Phase 5: System Tray Integration

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/StatusItemController.swift`

### 5.1 SNI Tray Setup

- [x] Create `daemon/tray.rs` with ksni integration
- [x] Register `StatusNotifierItem` per enabled provider (or single merged)
- [x] Implement tray icon properties:
  - [x] `id`: "claude-bar-claude", "claude-bar-codex", or "claude-bar-merged"
  - [x] `category`: ApplicationStatus
  - [x] `title`: "Claude Code", "Codex", or "Claude Bar"
  - [x] `icon_pixmap`: Dynamic two-bar meter

### 5.2 Icon Rendering

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/IconRenderer.swift`

- [x] Create `icons/renderer.rs`:
  ```rust
  pub struct IconRenderer {
      size: u32,  // 22x22 typical for SNI
      claude_svg: &'static [u8],
      codex_svg: &'static [u8],
  }

  impl IconRenderer {
      pub fn render(&self, provider: Provider, primary: f64, secondary: f64, state: IconState) -> Vec<u8>;
  }

  pub enum IconState {
      Normal,
      Loading,  // Knight Rider animation
      Error,
      Stale,    // Data is old, dim the icon
  }
  ```
- [x] Load embedded SVG assets
- [x] Render two-bar meter:
  - [x] Top bar: Primary usage (session)
  - [x] Bottom bar: Secondary usage (weekly)
  - [x] Fill direction: left-to-right = usage consumed
- [x] Color: Gold/amber (#F5A623)
- [x] Output as RGBA pixel data for ksni

### 5.3 Animation System

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/StatusItemController+Animation.swift`

- [x] Implement Knight Rider loading animation at 15 FPS:
  ```rust
  fn knight_rider_frame(phase: f64) -> (f64, f64) {
      let primary = 0.5 + 0.5 * phase.sin();
      let secondary = 0.5 + 0.5 * (phase + PI).sin();
      (primary, secondary)
  }
  ```
- [x] Create animation timer (15 FPS during loading)

### 5.4 Tray Menu

- [x] Implement right-click context menu:
  - [x] "Refresh Now" - triggers immediate fetch
  - [x] "Open Dashboard" - opens provider dashboard (auth-only)
  - [x] "Quit"
- [x] Use ksni menu API

### 5.5 Click Handler

- [x] Implement left-click to open popup window
- [x] Pass provider identifier to popup
- [x] Trigger instant refresh (with 5s cooldown)

### Phase 5 Notes

**TrayManager architecture:**
- Uses interior mutability pattern with `Arc<RwLock<TrayManagerInner>>` to allow shared state updates
- Event system via `mpsc::UnboundedSender<TrayEvent>` for async event handling
- Separate `TrayState` per provider tracks: percentages, icon state, animation phase, credentials validity, last refresh time

**ksni integration:**
- `ClaudeBarTray` struct implements the `ksni::Tray` trait
- `icon_pixmap()` dynamically generates RGBA pixel data for the icon
- Pixel format conversion: RGBA to network byte order (ARGB) for D-Bus SNI protocol
- `TrayService::spawn()` creates background thread for D-Bus message handling
- `Handle::update()` allows safe updates from async context

**Icon rendering:**
- 22x22 pixel icons (standard SNI size)
- Two horizontal bars (8px each) with 2px gap
- Gold/amber brand color (#F5A623) for filled portions
- Dimmed (1/4 brightness, 50% alpha) for empty portions
- Top bar: primary/session usage, Bottom bar: secondary/weekly usage

**Animation system:**
- Knight Rider effect: sinusoidal oscillation with π phase offset between bars
- 15 FPS update rate (~66ms interval) for smooth animation without excessive CPU
- `run_animation_loop()` async function for tokio-based timing
- Animation phase incremented by π/30 per frame (~1 full cycle per 2 seconds)

**Context menu:**
- "Refresh Now" - sends `TrayEvent::RefreshRequested`
- "Open Dashboard" - only shown when credentials are valid
- "Quit" - sends `TrayEvent::Quit`
- Left click sends `TrayEvent::LeftClick(provider)` for popup

**Deferred to Phase 6:**
- Popup window implementation (currently left click emits event but popup not implemented)
- Actual refresh logic (event is emitted but handling in app.rs not implemented)

---

## Phase 6: GTK Popup UI

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBar/Views/MenuCardView.swift`

### 6.1 Application Setup

- [x] Create `daemon/app.rs`
- [x] Initialize GTK4 + libadwaita application
- [x] Set application ID: `com.github.kabilan.claude-bar`
- [x] Handle single-instance (D-Bus activation)

### 6.2 Window Setup

- [x] Create `ui/popup.rs`
- [x] Create popup window:
  - [x] Undecorated or minimal decoration
  - [x] Position: top-right corner, auto-detect panel via wlr-layer-shell
  - [x] Close on focus loss (click outside)
  - [x] Fixed width (~350px), dynamic height

### 6.3 Popup Layout

- [x] Header section:
  - [x] Provider name + icon
  - [x] Account email (if available)
  - [x] Plan name (e.g., "Claude Pro")
- [x] Primary usage section:
  - [x] Progress bar (0-100%)
  - [x] "78% used" or "22% remaining" based on settings
  - [x] "5-hour window · resets in 2h 14m" (updates every minute)
- [x] Secondary usage section:
  - [x] Progress bar
  - [x] "Weekly · resets in 4d 12h"
- [x] Opus section (if present):
  - [x] Progress bar
  - [x] "Opus/Sonnet · resets in Xd Xh"
- [x] Cost section:
  - [x] "Today: $X.XX"
  - [x] "This month: $X.XX"
- [x] Footer:
  - [x] "Updated 30s ago" (updates live)
  - [ ] Refresh button (deferred - can click tray icon to refresh)
- [x] Error state:
  - [x] Show error message
  - [x] Show troubleshooting hint (e.g., "Run `claude` to authenticate")

### 6.4 Progress Bar Widget

- [x] Create `ui/progress.rs`:
  ```rust
  pub struct UsageProgressBar {
      progress: f64,      // 0.0 to 1.0
      label: String,
  }
  ```
- [x] Use libadwaita styling
- [x] Single brand color (gold/amber)
- [ ] Smooth animation when value changes (deferred - standard GTK progress bar used)

### 6.5 Styling

- [x] Create `ui/styles.rs`
- [x] Use libadwaita styling classes
- [x] Support dark/light mode automatically
- [x] Custom CSS for progress bars if needed
- [x] Consistent spacing and typography

### 6.6 Live Updates

- [x] Update relative time ("30s ago") every second
- [x] Update countdown ("resets in Xh Xm") every minute
- [ ] Use `adw::TimedAnimation` for progress bar transitions (deferred - simple CSS transitions sufficient)

### Phase 6 Notes

**Application architecture:**
- GTK main loop runs on the main thread via `glib::MainContext::iteration()`
- Async operations (tray events, polling) run on tokio runtime
- `tokio::sync::mpsc::unbounded_channel` bridges tokio→GTK communication
- `glib::idle_add_local` processes UI commands from async context

**PopupWindow implementation:**
- Stores provider state in `Rc<RefCell<ProviderState>>` for interior mutability
- Content rebuilt dynamically when data changes via `rebuild_content()`
- Focus controller closes popup when user clicks outside
- Window positioned at monitor top-right with 8px margin and 32px panel offset

**UsageProgressBar custom widget:**
- GTK subclass using `glib::wrapper!` and `ObjectSubclass`
- Custom `snapshot()` renders rounded rectangle progress bar
- Brand color #F5A623 (gold/amber) matches tray icons
- Progress value clamped to 0.0-1.0 range

**CSS styling:**
- `.popup-window` for container styling
- `.usage-progress` for GTK ProgressBar styling
- `.usage-progress-bar` for custom widget
- `.error-hint` for monospace error hints with error color background
- Uses `@theme_*` CSS variables for automatic dark/light mode support

**Live updates:**
- `glib::timeout_add_local` runs every 1000ms while popup is visible
- Updates "Updated Xs ago" labels in-place by traversing widget tree
- Countdown labels recalculated on each rebuild (not live during visibility)
- Source ID tracked in `Rc<Cell<Option<glib::SourceId>>>` for cleanup

**Deferred items:**
- Refresh button in popup footer (clicking tray icon triggers refresh)
- Smooth progress bar animations with `adw::TimedAnimation`
- wlr-layer-shell for accurate panel height detection (uses fixed 32px offset)

---

## Phase 7: CLI Tool

Reference: `/vault/experiments/2026-01-16-steipete-CodexBar/Sources/CodexBarCLI/`

### 7.1 CLI Structure

- [x] Create CLI with clap in `main.rs`:
  ```
  claude-bar

  USAGE:
      claude-bar <COMMAND>

  COMMANDS:
      daemon    Start the tray daemon
      status    Show current usage status
      cost      Show cost summary
      refresh   Trigger daemon refresh via D-Bus
      help      Print help
  ```

### 7.2 Daemon Command

- [x] `claude-bar daemon`:
  - [x] Start GTK application
  - [x] Initialize tray
  - [x] Start polling loop
  - [x] Register D-Bus interface

### 7.3 Status Command

- [x] `claude-bar status`:
  - [x] Fetch directly from provider APIs (standalone)
  - [x] Text format by default
  - [x] `--json` flag for JSON output
  - [x] `--provider <name>` to filter
- [x] Example output:
  ```
  Claude Code
    Session: 78% used (resets in 2h 14m)
    Weekly:  32% used (resets in 4d 12h)
    Opus:    45% used (resets in 4d 12h)

  Codex
    Session: 45% used (resets in 3h 02m)
    Weekly:  28% used (resets in 5d 8h)
  ```

### 7.4 Cost Command

- [x] `claude-bar cost`:
  - [x] Scan local logs (standalone)
  - [x] `--json` flag for JSON output
  - [x] `--days <n>` to specify range (default 30)
- [x] Example output:
  ```
  Claude Code
    Today:      $12.45
    This month: $234.56

  Codex
    Today:      $8.20
    This month: $156.78
  ```

### 7.5 Refresh Command

- [x] `claude-bar refresh`:
  - [x] Connect to daemon via D-Bus
  - [x] Call Refresh method
  - [x] Report success/failure
  - [x] Exit with error if daemon not running

### 7.6 D-Bus Interface

- [x] Create `daemon/dbus.rs`:
  ```rust
  #[dbus_interface(name = "com.github.kabilan.ClaudeBar")]
  impl ClaudeBarService {
      async fn refresh(&self) -> Result<(), Error>;
      #[dbus_interface(property)]
      fn is_refreshing(&self) -> bool;
      #[dbus_interface(signal)]
      fn usage_updated(&self, provider: &str) -> Result<(), Error>;
  }
  ```
- [x] Follow freedesktop patterns
- [x] Register on session bus

### Phase 7 Notes

**CLI architecture:**
- Single binary with clap subcommands: `daemon`, `status`, `cost`, `refresh`, `completions`
- `completions` command added for generating shell completions (bash, zsh, fish)
- CLI commands are standalone (don't require daemon) except for `refresh` which calls daemon via D-Bus

**Status command:**
- Directly instantiates providers and fetches usage from APIs
- Respects `providers.claude.enabled` and `providers.codex.enabled` settings
- `--provider` filter accepts case-insensitive provider names ("claude", "codex")
- JSON output includes `fetched_at` timestamp with all provider status data
- Text output formats percentages to one decimal place with reset time info

**Cost command:**
- Uses `CostStore` to scan JSONL logs and calculate costs
- Refreshes pricing from models.dev before scanning (with cache)
- Shows daily summary for recent days if there's data for multiple days
- JSON output includes full `daily_breakdown` per model

**Refresh command:**
- Uses D-Bus constants from `daemon::dbus` module for consistency
- Simple method call to `com.github.kabilan.ClaudeBar.Refresh`
- Error message includes hint about daemon not running

**D-Bus integration:**
- `ClaudeBarService` uses `mpsc::UnboundedSender<DbusCommand>` to communicate with daemon
- `DbusCommand::Refresh` triggers refresh for all providers
- `is_refreshing` property uses `AtomicBool` for thread-safe status
- Connection builder pattern registers name and serves at path in one chain
- Daemon handles D-Bus commands in a separate tokio task

**Deferred items:**
- Per-file cost caching (deferred from Phase 4)
- Polling loop integration with cost scanning (costs fetched on demand, not polled)

---

## Phase 8: Nix Integration

### 8.1 Home Manager Module

- [x] Create `nix/hm-module.nix`:
  ```nix
  { config, lib, pkgs, ... }:

  with lib;

  let
    cfg = config.services.claude-bar;
    tomlFormat = pkgs.formats.toml { };
  in {
    options.services.claude-bar = {
      enable = mkEnableOption "Claude Bar usage monitor";

      package = mkOption {
        type = types.package;
        default = pkgs.claude-bar;
        description = "The claude-bar package to use.";
      };

      settings = mkOption {
        type = tomlFormat.type;
        default = { };
        description = "Configuration for claude-bar.";
      };
    };

    config = mkIf cfg.enable {
      home.packages = [ cfg.package ];

      xdg.configFile."claude-bar/config.toml".source =
        tomlFormat.generate "claude-bar-config" cfg.settings;

      systemd.user.services.claude-bar = {
        Unit = {
          Description = "Claude Bar usage monitor";
          After = [ "graphical-session-pre.target" ];
          PartOf = [ "graphical-session.target" ];
        };

        Service = {
          ExecStart = "${cfg.package}/bin/claude-bar daemon";
          Restart = "on-failure";
        };

        Install = {
          WantedBy = [ "graphical-session.target" ];
        };
      };
    };
  }
  ```

### 8.2 NixOS Module

- [x] Create `nix/nixos-module.nix`:
  ```nix
  { config, lib, pkgs, ... }:

  with lib;

  let
    cfg = config.programs.claude-bar;
  in {
    options.programs.claude-bar = {
      enable = mkEnableOption "Claude Bar usage monitor";

      package = mkOption {
        type = types.package;
        default = pkgs.claude-bar;
        description = "The claude-bar package to use.";
      };
    };

    config = mkIf cfg.enable {
      environment.systemPackages = [ cfg.package ];
    };
  }
  ```

### 8.3 Example Configuration

- [x] Document usage in README:
  ```nix
  # In home.nix
  services.claude-bar = {
    enable = true;
    settings = {
      providers = {
        claude = { enabled = true; };
        codex = { enabled = true; };
        merge_icons = true;
      };
      display = {
        show_as_remaining = false;
      };
      notifications = {
        enabled = true;
        threshold = 0.9;
      };
      debug = false;
    };
  };
  ```

### 8.4 Flake Outputs

- [x] Add modules to flake outputs:
  ```nix
  outputs = { self, nixpkgs, ... }: {
    packages.x86_64-linux.default = ...;  # claude-bar package

    homeManagerModules.default = import ./nix/hm-module.nix;
    homeManagerModules.claude-bar = self.homeManagerModules.default;

    nixosModules.default = import ./nix/nixos-module.nix;
    nixosModules.claude-bar = self.nixosModules.default;
  };
  ```
- [x] Include shell completions in package

### Phase 8 Notes

**Home Manager module enhancements:**
- Added `literalExpression` for `defaultText` and `example` to improve documentation
- Config file only generated when `settings != {}` to avoid empty file creation
- systemd service uses `Type = "simple"` and `RestartSec = 5` for better restart behavior
- Auto-starts with graphical session via `WantedBy = [ "graphical-session.target" ]`

**NixOS module:**
- Minimal system-level module that just installs the package to `environment.systemPackages`
- User-level configuration should use the Home Manager module

**Flake outputs:**
- Both system-specific (`flake-utils.lib.eachDefaultSystem`) and top-level module exports
- Top-level exports allow `inputs.claude-bar.homeManagerModules.default` in user flakes
- Shell completions generated in `postInstall` phase for bash, zsh, and fish

**Testing notes:**
- GTK tests must run sequentially (`cargo test -- --test-threads=1`) due to GTK's single-thread initialization requirement
- Unused code warnings are expected; Phase 9 will integrate remaining unused functionality

---

## Phase 9: Polish & Testing

### 9.1 Error Handling

- [x] Graceful handling of missing credentials with helpful hints
- [x] Clear error messages in popup
- [x] Exponential backoff for API failures (60s, 120s, 240s, max 10min)
- [x] 30-second request timeout

### 9.2 Logging

- [x] Structured JSONL logging with tracing
- [x] Log to `~/.local/share/claude-bar/claude-bar.log`
- [x] Also log to journald
- [x] Log levels: error, warn, info, debug, trace
- [ ] Debug mode (`debug = true`) enables trace level (deferred - env filter works)

### 9.3 Testing

- [x] Unit tests for:
  - [x] Settings parsing and validation
  - [x] Model serialization
  - [x] Usage calculation
  - [x] Cost calculation
  - [x] Icon rendering
- [x] Integration tests for:
  - [x] Provider API mocking (response parsing tests)
  - [x] Log file parsing
- [ ] Manual testing checklist:
  - [ ] Fresh install (no config)
  - [ ] Missing credentials
  - [ ] Expired credentials
  - [ ] Network failure
  - [ ] Dark mode / light mode
  - [ ] Hyprland window rules work with app ID

### 9.4 Documentation

- [x] README.md with:
  - [x] Installation instructions (Nix flake)
  - [x] Configuration reference
  - [x] CLI usage
  - [x] Hyprland/Sway window rules example
  - [x] Troubleshooting

### Phase 9 Notes

**Error handling with exponential backoff:**
- Added `core/retry.rs` with `RetryState` struct tracking consecutive failures
- Backoff sequence: 60s → 120s → 240s → 480s → 600s (max)
- Polling loop checks every 1 second but respects per-provider backoff delays
- Success resets backoff counter; failure increments it
- Logged with structured fields: `consecutive_failures`, `next_retry_secs`

**Structured logging:**
- Daemon logs to three destinations: console (human-readable), file (JSONL), journald
- CLI commands use compact console-only logging
- Log file path: `~/.local/share/claude-bar/claude-bar.log`
- Log level controlled via `RUST_LOG` environment variable
- Uses tracing-subscriber layers for composable logging

**Test coverage (59 tests):**
- Core models: 8 tests (serialization, rate window calculations)
- Core store: 6 tests (updates, errors, notifications)
- Core settings: 3 tests (defaults, parsing, validation)
- Core retry: 5 tests (backoff, saturation, reset)
- Providers: 14 tests (credentials, API responses, metadata)
- Cost: 18 tests (log parsing, pricing, aggregation)
- UI/Icons: 4 tests (rendering, progress bar)

**Deferred items:**
- Debug mode config option for trace logging (use RUST_LOG instead)
- Manual testing checklist (requires runtime environment)

---

## Future Work

These items are out of scope for the initial implementation but should be considered for future phases.

### Cursor Provider
- Browser cookie extraction (Chrome/Zen)
- Monthly billing model (single bar display)
- No rate windows, just usage percentage

### OpenCode Zen Provider
- Browser cookie extraction
- Cost-only display (no rate limits)
- Daily spend tracking

### Additional Features
- Desktop notifications for quota warnings (configurable thresholds)
- Multiple account support per provider
- Keyboard shortcuts to open popup
- Settings UI (GTK preferences window)
- Auto-update mechanism

---

## Reference Files

Key files in the original CodexBar implementation to reference:

### Core Models & Data
| Purpose | File |
|---------|------|
| Usage snapshot model | `Sources/CodexBarCore/UsageFetcher.swift` |
| Rate window model | `Sources/CodexBarCore/Models/RateWindow.swift` |
| Settings storage | `Sources/CodexBar/SettingsStore.swift` |
| Usage store | `Sources/CodexBar/UsageStore.swift` |

### Claude Provider
| Purpose | File |
|---------|------|
| OAuth credentials | `Sources/CodexBarCore/Providers/Claude/ClaudeOAuthCredentials.swift` |
| Usage fetcher | `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift` |
| Usage snapshot | `Sources/CodexBarCore/Providers/Claude/ClaudeUsageSnapshot.swift` |
| Provider descriptor | `Sources/CodexBarCore/Providers/Claude/ClaudeProviderDescriptor.swift` |

### Codex Provider
| Purpose | File |
|---------|------|
| OAuth credentials | `Sources/CodexBarCore/Providers/Codex/CodexOAuth/CodexOAuthCredentials.swift` |
| Usage fetcher | `Sources/CodexBarCore/Providers/Codex/CodexOAuth/CodexOAuthUsageFetcher.swift` |
| Token refresher | `Sources/CodexBarCore/Providers/Codex/CodexOAuth/CodexTokenRefresher.swift` |
| Provider descriptor | `Sources/CodexBarCore/Providers/Codex/CodexProviderDescriptor.swift` |

### Cost Tracking
| Purpose | File |
|---------|------|
| Scanner base | `Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner.swift` |
| Claude scanner | `Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner+Claude.swift` |
| Cache | `Sources/CodexBarCore/Vendored/CostUsage/CostUsageCache.swift` |

### UI & Tray
| Purpose | File |
|---------|------|
| Status item controller | `Sources/CodexBar/StatusItemController.swift` |
| Menu construction | `Sources/CodexBar/StatusItemController+Menu.swift` |
| Icon rendering | `Sources/CodexBar/IconRenderer.swift` |
| Animations | `Sources/CodexBar/StatusItemController+Animation.swift` |
| Menu card view | `Sources/CodexBar/Views/MenuCardView.swift` |

### CLI
| Purpose | File |
|---------|------|
| CLI entry | `Sources/CodexBarCLI/CLIEntry.swift` |
| Status command | `Sources/CodexBarCLI/CLIUsageCommand.swift` |
| Cost command | `Sources/CodexBarCLI/CLICostCommand.swift` |

---

## Checklist Summary

**Phase 1: Project Setup**
- [x] Directory structure created
- [x] flake.nix working
- [x] Cargo configured
- [x] AGENTS.md written
- [x] Basic scaffolding compiles

**Phase 2: Core Data Layer**
- [x] Data models implemented
- [x] Settings store with hot-reload
- [x] Usage store working
- [x] Notifications working

**Phase 3: Provider Implementations**
- [x] Provider trait defined
- [x] Claude provider working
- [x] Codex provider working
- [x] Provider registry working

**Phase 4: Cost Tracking**
- [x] Pricing fetcher (models.dev) working
- [x] Claude log scanner working
- [x] Codex log scanner working
- [x] Cost store integrated

**Phase 5: System Tray Integration**
- [x] SNI tray setup working
- [x] Icon rendering working
- [x] Animations working (15 FPS)
- [x] Tray menu working
- [x] Click handler working

**Phase 6: GTK Popup UI**
- [x] GTK Application setup with app ID
- [x] Window positioning (top-right, panel-aware)
- [x] Layout implemented
- [x] Progress bars working
- [x] Live updates (countdown, relative time)
- [x] Error states with hints

**Phase 7: CLI Tool**
- [x] Subcommand structure implemented
- [x] `daemon` command working
- [x] `status` command working
- [x] `cost` command working
- [x] `refresh` command (D-Bus) working

**Phase 8: Nix Integration**
- [x] Home Manager module created
- [x] NixOS module created
- [x] Shell completions installed
- [x] Documentation complete

**Phase 9: Polish & Testing**
- [x] Error handling complete (exponential backoff implemented)
- [x] Logging implemented (JSONL + journald)
- [x] Tests written (59 tests passing)
- [x] Documentation complete (README.md)
