# Implementation Specification: Claude Bar Refinements

This specification addresses refinements to the Claude Bar prototype based on user feedback after initial testing.

---

## Phase 0: Fix Pricing Parse & Display Bugs

**Problem**: The `claude-bar cost` command shows two bugs:
1. "Failed to parse models.dev response" warning - pricing fetch fails
2. "$-0.00" displayed for Codex Today - negative zero display bug

**Implementation Tasks**:

- [x] **0.1** Investigate models.dev API response format - the parsing may be broken due to API changes
- [x] **0.2** Fix the models.dev response parser in `src/cost/pricing.rs`
- [x] **0.3** Fix negative zero display - ensure costs display as "$0.00" not "$-0.00"
- [x] **0.4** Add better error logging to show what part of parsing failed

**Files to modify**:
- `src/cost/pricing.rs` - Fix parsing logic
- `src/cost/store.rs` or display code - Fix negative zero formatting

**Verification**: Run `claude-bar cost` without the parse warning, and "$0.00" displays correctly.

---

## Phase 1: Display Billing/Spend in Popup Window

**Problem**: The popup window has the UI structure to display costs (`build_cost_section`), but the `CostStore` is never instantiated or connected to the daemon.

### Cost Data Flow

1. **Daemon startup**: Fetch pricing table from models.dev
2. **Ongoing**: Scan local session logs, calculate costs using cached pricing table
3. **Manual refresh**: `claude-bar refresh-pricing` command for when users know new pricing exists

### Pricing Refresh Behavior

- Refresh pricing at daemon startup
- If startup fetch fails (network issue), retry every **5 minutes** until successful
- Add `RefreshPricing` D-Bus method (follows existing `Refresh` pattern)
- Add `claude-bar refresh-pricing` CLI subcommand

### Stale/Error Handling

- **Stale pricing data**: Only show tilde prefix (~$X.XX) if the initial fetch *actually failed*, not based on age. Pricing changes infrequently (models.dev), so cached data from startup is fine indefinitely.
- **Log parse errors**: Display "Error reading logs" in the cost section. Clicking the error message copies the daemon log file path to clipboard. Tooltip briefly changes to "Copied!" then reverts.

### Cost Section Display

- **Layout**: Inline format - "Today: $X.XX • This month: $X.XX"
- **Position**: Below all progress bars, between two horizontal rules, above "Updated Xs ago"
- **Zero values**: Always show "$0.00" even if no usage
- **Currency**: Always USD ($), no configuration needed
- **Timezone**: Use local system timezone for day/month boundaries
- **Visibility**: Only show cost section if CostStore has data for that provider (derived from data, not declared capability)

**Implementation Tasks**:

- [x] **1.1** Instantiate `CostStore` in daemon `run()` function alongside `UsageStore`
- [x] **1.2** Add cost scanning to the polling loop - call `CostStore::scan_all()` periodically (e.g., every 5 minutes)
- [x] **1.3** Wire scanned costs into `UsageStore.update_cost()` for each provider
- [x] **1.4** Add pricing refresh at daemon startup with 5-minute retry on failure
- [x] **1.5** Add `RefreshPricing` D-Bus method to `src/daemon/dbus.rs`
- [x] **1.6** Add `claude-bar refresh-pricing` CLI subcommand in `src/cli/`
- [x] **1.7** Ensure `ShowPopup` command includes cost data from `store.get_cost(provider)`
- [x] **1.8** Update popup UI to show inline cost format with proper error handling
- [x] **1.9** Implement click-to-copy for error messages with tooltip feedback

**Files to modify**:
- `src/daemon/app.rs` - Add CostStore integration, pricing retry logic
- `src/daemon/dbus.rs` - Add RefreshPricing method
- `src/cli/mod.rs` - Add refresh-pricing subcommand
- `src/cli/refresh_pricing.rs` (new) - CLI implementation
- `src/ui/popup.rs` - Update cost section layout, click-to-copy behavior

**Verification**:
- Open popup after using Claude Code, verify cost shows "Today: $X.XX • This month: $X.XX"
- Run `claude-bar refresh-pricing` and verify it triggers pricing refresh via D-Bus

---

## Phase 2: Dynamic Progress Bars

**Problem**: The current implementation assumes fixed bar counts per provider. The third "opus" bar actually shows Sonnet data with Opus as fallback, and the bar count should be dynamic based on what the API returns.

### API Response Structure (Claude)

The Anthropic API returns:
- `five_hour` - main 5-hour session rate limit (always show in popup + tray)
- `seven_day` - main weekly quota (always show in popup + tray)
- `seven_day_sonnet` - *optional* Sonnet-specific weekly carve-out (show in popup if present)
- `seven_day_opus` - *optional* Opus-specific weekly carve-out (show in popup if present)

### API Response Structure (Codex)

- Session window (primary)
- Weekly window (secondary)
- No model-specific carve-outs

### Display Rules

**Tray icon**: Only shows main limits (session + weekly), never model-specific carve-outs.

**Popup**: Shows all available windows dynamically:
- Claude: 2-4 bars depending on API response (session, weekly, optionally sonnet, optionally opus)
- Codex: Always 2 bars (session, weekly)

**Height calculation**: Provider-specific heights calculated dynamically from actual data at render time, not from static capabilities.

**Bar labels**:
- "Session" / "5-hour session"
- "Weekly" / "Weekly quota"
- "Sonnet Weekly" (if seven_day_sonnet present)
- "Opus Weekly" (if seven_day_opus present)

**Implementation Tasks**:

- [x] **2.1** Refactor `UsageSnapshot` to support multiple optional model carve-out windows (not just single `opus` field)
- [x] **2.2** Update Claude provider to populate separate `seven_day_sonnet` and `seven_day_opus` fields (not either/or)
- [x] **2.3** Update popup rendering to dynamically create progress bars based on available data
- [x] **2.4** Calculate popup height dynamically: `base_height + (num_bars × bar_height) + (has_cost × cost_height)`
- [x] **2.5** Update tray icon to only use primary (session) and secondary (weekly) windows
- [x] **2.6** Handle missing data gracefully - show available bars only (if only weekly exists, show one bar)

**Files to modify**:
- `src/core/models.rs` - Refactor UsageSnapshot structure
- `src/providers/claude.rs` - Update to populate separate carve-out fields
- `src/ui/popup.rs` - Dynamic bar rendering and height calculation
- `src/icons/renderer.rs` - Ensure tray only uses primary/secondary

**Verification**:
- Open Claude popup - shows bars for all windows API returned
- Open Codex popup - shows exactly 2 bars
- No height animation or flicker when switching between providers

---

## Phase 3: Provider Colors and Light/Dark Mode

### Provider Brand Colors

| Provider | Color | Hex |
|----------|-------|-----|
| Claude | Orange/Amber | `#F5A623` |
| Codex | Teal (OpenAI) | `#10A37F` |

### Color Usage

**Tray icon bars**:
- Filled portion: Provider brand color
- Unfilled portion: Computed muted version (apply consistent transformation, e.g., 30% opacity or desaturated)

**Popup progress bars**: Provider brand color for filled portion

**Tray icon background**: Rounded rectangle with subtle background that adapts to light/dark mode (light background in dark mode, dark in light mode)

### Loading State

When daemon starts with no cached data, or during refresh:
- Show animated knight rider loading pattern (existing implementation)
- Animation runs continuously until data arrives

### Error State

When both providers have fetch errors:
- Show last known state (keep displaying previous successful data)
- Do not show error icon or empty bars

### Theme Configuration

```toml
[theme]
# Options: "system", "light", "dark"
mode = "system"
```

**Hot-reload**: Not supported. Theme changes require daemon restart. This is acceptable since config changes typically happen via NixOS rebuild which restarts the service anyway.

**Home Manager module**: Only expose `theme.mode`, keep polling intervals as internal implementation details.

```nix
services.claude-bar = {
  theme.mode = "system";  # or "light" or "dark"
};
```

**Implementation Tasks**:

- [x] **3.1** Create `src/ui/colors.rs` with provider color constants and muted color computation
- [x] **3.2** Add `ThemeSettings` struct to `src/core/settings.rs` with `mode: ThemeMode` enum
- [x] **3.3** Update `src/ui/styles.rs` to generate CSS with provider-specific accent colors
- [x] **3.4** Update `PopupWindow` to apply `AdwStyleManager::set_color_scheme()` based on theme mode
- [x] **3.5** Update tray icon rendering to use provider colors for bars and computed muted for unfilled
- [x] **3.6** Add rounded rectangle background to tray icon that adapts to light/dark
- [x] **3.7** Update `nix/hm-module.nix` to add `theme.mode` option only
- [x] **3.8** Update `config.example.toml` with theme section

**Files to modify**:
- `src/ui/colors.rs` (new) - Provider color constants and computation
- `src/core/settings.rs` - Add ThemeSettings
- `src/ui/styles.rs` - Provider-aware CSS generation
- `src/ui/popup.rs` - Apply color scheme and provider CSS
- `src/ui/mod.rs` - Export colors module
- `src/icons/renderer.rs` - Provider colors in tray icon, rounded rect background
- `nix/hm-module.nix` - Add theme.mode option
- `config.example.toml` - Document theme options

**Verification**:
1. Open Claude popup - progress bars are orange (#F5A623)
2. Open Codex popup - progress bars are teal (#10A37F)
3. Tray icon shows colored bars with muted unfilled portions
4. Set `mode = "dark"` with light system theme - popup forces dark mode

---

## Phase 4: Tray Icon Behavior Changes

### Default Configuration

Change default `merge = false` (separate tray icons per provider). Users who want a combined icon can opt-in.

### Separate Icons (merge = false)

- **Left-click**: Opens that provider's popup directly (no ambiguity)
- **Right-click**: Context menu with Refresh, Open Dashboard (for that provider), Quit

### Merged Icon (merge = true)

- **Left-click**: Shows provider selection menu only ("Claude Code", "Codex")
- **Right-click**: Full menu with:
  - Refresh Now
  - Open Claude Dashboard
  - Open Codex Dashboard
  - Quit

This removes ambiguity - with a merged icon, user must explicitly choose which provider to view.

**Implementation Tasks**:

- [x] **4.1** Change default `merge` setting to `false` in config defaults
- [x] **4.2** Implement left-click provider selection menu for merged icon
- [x] **4.3** Update right-click menu for merged icon to include dashboard options for all configured providers
- [x] **4.4** Ensure separate icons maintain current left-click → popup behavior

**Files to modify**:
- `src/core/settings.rs` - Change merge default to false
- `src/daemon/tray.rs` - Implement differentiated click behavior for merged vs separate

**Verification**:
- With merge=false: Two tray icons, left-click opens respective popup
- With merge=true: One tray icon, left-click shows provider menu, right-click shows full menu

---

## Phase 5: Popup UX Improvements

### Keyboard Navigation

- **Escape**: Closes popup
- **Tab**: Switches to next provider (cycles forward)
- **Shift+Tab**: Switches to previous provider (cycles backward)
- Provider switch happens **in-place** (smooth content transition, popup stays open)

### Error Display

When cost log parsing fails:
- Show "Error reading logs" text in cost section
- **Click behavior**: Copies daemon log file path to clipboard
- **Tooltip feedback**: Tooltip briefly shows "Copied!" then reverts to default

**Implementation Tasks**:

- [x] **5.1** Add keyboard event handling to popup (Escape, Tab, Shift+Tab)
- [x] **5.2** Implement in-place provider switching with smooth content transition
- [x] **5.3** Implement click-to-copy on error messages with tooltip feedback

**Files to modify**:
- `src/ui/popup.rs` - Keyboard handling, provider switching, error click behavior

**Verification**:
- Press Tab in popup - content switches to next provider smoothly
- Press Escape - popup closes
- Click error message - log path copied, tooltip shows "Copied!"

---

## Phase 6: Polish and Testing

- [ ] **6.1** Test all phases together end-to-end
- [ ] **6.2** Verify cost display for both Claude and Codex
- [ ] **6.3** Verify dynamic progress bars render correctly for all API response variations
- [ ] **6.4** Verify smooth popup transitions between providers
- [ ] **6.5** Test theme modes (system, light, dark)
- [ ] **6.6** Test merged vs separate tray icon behaviors
- [ ] **6.7** Test keyboard navigation
- [ ] **6.8** Run `cargo clippy` and fix any warnings
- [ ] **6.9** Run `cargo test` and ensure all tests pass
- [x] **6.10** Update README.md if needed

---

## Implementation Order

1. **Phase 0** (Bugs) - Fix existing issues first
2. **Phase 1** (Billing) - Enables cost visibility
3. **Phase 2** (Dynamic bars) - Correct API data representation
4. **Phase 3** (Theme) - Visual polish, provider identity
5. **Phase 4** (Tray behavior) - UX improvement for merged icon
6. **Phase 5** (Popup UX) - Keyboard navigation and error handling
7. **Phase 6** (Polish) - Final integration testing

Phases 1, 2, and 3 can be worked on in parallel as they touch mostly different files.

---

## Commit Guidelines

**After completing each phase**, create a git commit with the phase work.

### Pre-commit Checklist (MANDATORY)

Before committing, you MUST complete these steps. Do NOT commit until all pass:

1. **Build**: Run `cargo build --release` - must compile with zero errors
2. **Lint**: Run `cargo clippy` - must pass with zero warnings. Fix all warnings before proceeding.
3. **Test**: Run `cargo test` - all tests must pass. If tests fail due to intentional changes, update the tests first.

If any of these steps fail, fix the issues before committing. Do not commit broken or warning-laden code.

### Commit Format

Stage all changes for the phase and commit with this format:

```
feat(phase-N): <brief description>

<bullet points of what was implemented>

Phase N of claude-bar refinements.
```

**Examples:**

```
fix(phase-0): resolve pricing parse and negative zero bugs

- Fix models.dev response parser for updated API format
- Fix negative zero display in cost formatting
- Add detailed error logging for parse failures

Phase 0 of claude-bar refinements.
```

```
feat(phase-1): add cost display to popup window

- Instantiate CostStore in daemon alongside UsageStore
- Add RefreshPricing D-Bus method and CLI command
- Implement inline cost display with click-to-copy errors
- Add 5-minute pricing retry on startup failure

Phase 1 of claude-bar refinements.
```

This ensures each phase is a reviewable, revertible unit of work.

---

## Summary of Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Stale pricing indicator | Tilde prefix (~$X.XX) only on fetch failure | Pricing rarely changes, age-based staleness not useful |
| Theme hot-reload | Requires restart | Simpler implementation, NixOS rebuilds restart anyway |
| Height calculation | Dynamic from actual data | Future-proof, adapts to API changes |
| Cost section visibility | Derived from data | No cost data = no cost section |
| Pricing retry | Every 5 minutes | Handles network issues at boot |
| Tray bars unfilled color | Computed (reduced opacity) | Consistent transformation, fewer constants |
| Error click behavior | Copy log path to clipboard | Actionable without cluttering UI |
| Default merge | false (separate icons) | Only two providers, less ambiguity |
| Merged icon left-click | Provider selection menu | Removes ambiguity of "which provider?" |
| Tab navigation | In-place transition | Smoother UX than close/reopen |
| Cost display | Inline with separator | Compact layout |
| Currency | Always USD | Anthropic/OpenAI bill in USD |
| Timezone | Local system | Intuitive for users |
