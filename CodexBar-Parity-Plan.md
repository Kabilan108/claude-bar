# CodexBar Parity Plan (Claude Bar)

Goal: Achieve feature + UI parity with CodexBar (SwiftUI/AppKit) for Claude and Codex providers, in the Rust GTK4/libadwaita port.
This plan is self-contained and references CodexBar files so an agent can follow the original implementation patterns.

Update instructions for implementer:
- After completing each task, replace `- [ ]` with `- [x]`.
- If you discover new sub-tasks, add them under the relevant section.
- If a task is blocked, add a short note under that task with the blocker.

---

## 0) Baseline references (do not edit)

Use these CodexBar files as implementation references while coding:
- Menu card UI + sections: `Sources/CodexBar/MenuCardView.swift`
- Provider tab switcher: `Sources/CodexBar/StatusItemController+SwitcherViews.swift`
- Menu descriptor + actions (Add Account, Dashboard, Status, Settings, About, Quit): `Sources/CodexBar/MenuDescriptor.swift`
- Progress bar with pace indicator: `Sources/CodexBar/UsageProgressBar.swift`
- Pace computation & display text: `Sources/CodexBarCore/UsagePace.swift`, `Sources/CodexBar/UsagePaceText.swift`
- Usage snapshot + identity model: `Sources/CodexBarCore/UsageFetcher.swift`
- Extra usage (provider cost) model: `Sources/CodexBarCore/ProviderCostSnapshot.swift`
- Token cost snapshot model: `Sources/CodexBarCore/CostUsageModels.swift`
- Claude OAuth + extra usage mapping: `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthUsageFetcher.swift` and `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift`
- Codex OAuth usage mapping + identity: `Sources/CodexBarCore/Providers/Codex/CodexProviderDescriptor.swift`
- Preferences window + keyboard shortcut: `Sources/CodexBar/PreferencesView.swift`, `Sources/CodexBar/PreferencesAdvancedPane.swift`, `Sources/CodexBar/CodexbarApp.swift`
- About panel: `Sources/CodexBar/About.swift`

---

## 1) Data model parity (core models)

- [x] Add `tertiary` (or `opus`) window support in `UsageSnapshot` to match CodexBar’s three-window model (primary/secondary/tertiary).
  - Reference: `Sources/CodexBarCore/UsageFetcher.swift` (struct `UsageSnapshot` has `primary`, `secondary`, `tertiary`).
- [x] Add `provider_cost` (extra usage/quota) to `UsageSnapshot`, modeled after CodexBar’s `ProviderCostSnapshot`.
  - Reference: `Sources/CodexBarCore/ProviderCostSnapshot.swift` and `Sources/CodexBarCore/UsageFetcher.swift`.
- [x] Add token cost snapshot model (session tokens + last 30 days tokens + daily breakdown) to mirror CodexBar’s `CostUsageTokenSnapshot`.
  - Reference: `Sources/CodexBarCore/CostUsageModels.swift`.
- [x] Extend identity model to include provider-scoped account identity (email, organization, plan/login method).
  - Reference: `ProviderIdentitySnapshot` in `Sources/CodexBarCore/UsageFetcher.swift`.

**Tricky diff (data models):** add new structs + fields carefully to preserve serialization.
```diff
*** Begin Patch
*** Update File: src/core/models.rs
@@
 pub struct UsageSnapshot {
     pub primary: Option<RateWindow>,
     pub secondary: Option<RateWindow>,
+    pub tertiary: Option<RateWindow>,
+    pub provider_cost: Option<ProviderCostSnapshot>,
     #[serde(default)]
     pub carveouts: Vec<ModelWindow>,
     pub updated_at: DateTime<Utc>,
     pub identity: ProviderIdentity,
 }
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct ProviderCostSnapshot {
+    pub used: f64,
+    pub limit: f64,
+    pub currency_code: String,
+    pub period: Option<String>,
+    pub resets_at: Option<DateTime<Utc>>,
+    pub updated_at: DateTime<Utc>,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct CostUsageTokenSnapshot {
+    pub session_tokens: Option<u64>,
+    pub session_cost_usd: Option<f64>,
+    pub last_30_days_tokens: Option<u64>,
+    pub last_30_days_cost_usd: Option<f64>,
+    pub daily: Vec<DailyTokenUsage>,
+    pub updated_at: DateTime<Utc>,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct DailyTokenUsage {
+    pub date: NaiveDate,
+    pub total_tokens: Option<u64>,
+    pub cost_usd: Option<f64>,
+}
*** End Patch
```

---

## 2) Claude provider parity (OAuth + extra usage + identity)

- [x] Extend OAuth response parsing to include extra usage fields (`extra_usage`) and model-specific windows (Sonnet/Opus).
  - Reference: `OAuthUsageResponse` in `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthUsageFetcher.swift`.
- [x] Normalize extra usage amounts (minor units -> major units), and handle possible 100x scaling for non-enterprise plans.
  - Reference: `ClaudeUsageFetcher.oauthExtraUsageCost` + `rescaleClaudeExtraUsageCostIfNeeded` in `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift`.
- [x] Populate `provider_cost` on the `UsageSnapshot` for Claude.
- [x] Map plan from `rate_limit_tier` into human-friendly labels (Max/Pro/Team/Enterprise) and attach to identity.
  - Reference: `inferPlan(rateLimitTier:)` in `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift`.
- [ ] If desired later: add Web/CLI fallback strategies (not required for parity MVP but matches CodexBar’s behavior).

**Tricky diff (Claude extra usage parsing + normalization):**
```diff
*** Begin Patch
*** Update File: src/providers/claude.rs
@@
 struct OAuthUsageResponse {
     five_hour: Option<UsageWindow>,
     seven_day: Option<UsageWindow>,
     #[serde(rename = "seven_day_sonnet")]
     seven_day_sonnet: Option<UsageWindow>,
     #[serde(rename = "seven_day_opus")]
     seven_day_opus: Option<UsageWindow>,
+    #[serde(rename = "extra_usage")]
+    extra_usage: Option<OAuthExtraUsage>,
 }
+
+#[derive(Debug, Deserialize)]
+struct OAuthExtraUsage {
+    #[serde(rename = "is_enabled")]
+    is_enabled: Option<bool>,
+    #[serde(rename = "monthly_limit")]
+    monthly_limit: Option<f64>,
+    #[serde(rename = "used_credits")]
+    used_credits: Option<f64>,
+    currency: Option<String>,
+}
@@
-        Ok(UsageSnapshot {
+        let provider_cost = map_extra_usage(&usage.extra_usage, plan.as_deref());
+
+        Ok(UsageSnapshot {
             primary,
             secondary,
+            tertiary: None,
             carveouts,
             updated_at: Utc::now(),
             identity: ProviderIdentity {
                 email: None,
                 organization: None,
                 plan,
             },
+            provider_cost,
         })
     }
*** End Patch
```

**Note:** implement `map_extra_usage` with cents->dollars normalization and 100x rescale for non-enterprise, following CodexBar’s `normalizeClaudeExtraUsageAmounts` and `rescaleClaudeExtraUsageCostIfNeeded`.

---

## 3) Codex provider parity (identity + plan)

- [x] Parse account email and plan from id_token JWT, if present, to populate identity (CodexBar uses JWT claims).
  - Reference: `resolveAccountEmail` and `resolvePlan` in `Sources/CodexBarCore/Providers/Codex/CodexProviderDescriptor.swift`.
- [x] Ensure `tertiary` is None (Codex doesn’t use it) and populate `provider_cost` if applicable in future.
- [x] Consider the `chatgpt_base_url` resolution logic for usage endpoint (CodexBar resolves to backend-api vs codex endpoint).
  - Reference: `CodexOAuthUsageFetcher.resolveUsageURL` in `Sources/CodexBarCore/Providers/Codex/CodexOAuth/CodexOAuthUsageFetcher.swift`.

---

## 4) Cost tracking parity (tokens + 30-day rollups)

- [x] Extend cost scanning to aggregate token totals (input+output+cache read+cache creation) per day.
  - Current logs already include tokens: `src/cost/claude.rs` / `src/cost/codex.rs`.
- [x] Add a `CostUsageTokenSnapshot` computed from daily totals (today + last 30 days), similar to CodexBar’s `CostUsageFetcher.tokenSnapshot`.
  - Reference: `Sources/CodexBarCore/CostUsageFetcher.swift`.
- [x] Store this new snapshot in `UsageStore` and surface it to the UI.

---

## 5) Popup UI layout parity (core menu card structure)

- [x] Replace the current linear popup layout with CodexBar’s card-like hierarchy:
  - Header (provider name + email + subtitle + plan badge)
  - Usage section(s) (Session / Weekly / Sonnet or Opus) with progress + percent + reset
  - Extra usage section (provider cost)
  - Cost section (Today / Last 30 days with tokens)
  - Footer actions
  - Reference: `Sources/CodexBar/MenuCardView.swift`.
- [x] Add provider tab switcher at the top of the popup, with icon + name and selected underline.
  - Reference: `Sources/CodexBar/StatusItemController+SwitcherViews.swift`.
- [x] Add provider-colored progress bars and use a consistent section spacing/typography.
  - Reference: `Sources/CodexBar/MenuCardView.swift:90` and `Sources/CodexBar/UsageProgressBar.swift`.
- [x] Add pace indicator overlay to progress bars and a “Pace: …” line under weekly usage.
  - Reference: `Sources/CodexBar/UsagePaceText.swift`.

**Tricky diff (pace overlay in progress bar):** add a marker/stripe overlay when pace is behind/ahead.
```diff
*** Begin Patch
*** Update File: src/ui/progress.rs
@@
 pub struct UsageProgressBar(ObjectSubclass<imp::UsageProgressBarPriv>)
@@
 impl UsageProgressBar {
@@
+    pub fn set_pace_marker(&self, marker_progress: Option<f64>, is_deficit: bool) {
+        let imp = self.imp();
+        imp.pace_marker.set(marker_progress.unwrap_or(-1.0));
+        imp.pace_deficit.set(is_deficit);
+        self.queue_draw();
+    }
 }
@@
 pub struct UsageProgressBarPriv {
     pub progress: Cell<f64>,
     pub label: RefCell<String>,
     pub accent: RefCell<gdk::RGBA>,
     pub trough: RefCell<gdk::RGBA>,
+    pub pace_marker: Cell<f64>,
+    pub pace_deficit: Cell<bool>,
 }
@@
 impl Default for UsageProgressBarPriv {
@@
         Self {
             progress: Cell::new(0.0),
             label: RefCell::new(String::new()),
             accent: RefCell::new(gdk::RGBA::new(0.96, 0.65, 0.14, 1.0)),
             trough: RefCell::new(gdk::RGBA::new(0.2, 0.2, 0.2, 0.3)),
+            pace_marker: Cell::new(-1.0),
+            pace_deficit: Cell::new(false),
         }
     }
 }
@@
 impl WidgetImpl for UsageProgressBarPriv {
     fn snapshot(&self, snapshot: &gtk4::Snapshot) {
@@
         draw_rounded_bar(...);
         if progress > 0.0 { ... }
+
+        // Pace marker (draw a thin stripe at marker position if set)
+        let marker = self.pace_marker.get();
+        if marker >= 0.0 && marker <= 1.0 {
+            let x = (width * marker) as f32;
+            let color = if self.pace_deficit.get() {
+                gdk::RGBA::new(1.0, 0.0, 0.0, 0.8)
+            } else {
+                gdk::RGBA::new(0.0, 0.7, 0.2, 0.8)
+            };
+            let rect = gtk4::graphene::Rect::new(x - 1.0, 0.0, 2.0, height as f32);
+            snapshot.append_color(&color, &rect);
+        }
     }
 }
*** End Patch
```

---

## 6) Footer actions & menu links (Add Account, Dashboard, Status, Settings, About, Quit)

- [x] Add action rows at bottom of the popup similar to CodexBar’s `MenuDescriptor` sections.
  - Reference: `Sources/CodexBar/MenuDescriptor.swift`.
- [x] Implement Add Account / Switch Account actions for Claude and Codex, using CLI login flows.
  - Reference: `Sources/CodexBar/ClaudeLoginRunner.swift`, `Sources/CodexBar/CodexLoginRunner.swift`.
- [x] Implement “Usage Dashboard” and “Status Page” actions for each provider.
  - Reference: `Sources/CodexBar/StatusItemController+Actions.swift`.
- [x] Add Settings, About, Quit actions (Settings should open a GTK settings window).
  - Reference: `Sources/CodexBar/MenuDescriptor.swift:290`.

---

## 7) Settings UI + show-used toggle wiring

- [x] Create a GTK preferences window with at least:
  - show used vs remaining
  - merge icons
  - theme mode
  - notifications threshold
- [x] Wire settings changes to live update popup (call `PopupWindow::set_show_as_remaining` and rebuild).
  - Current setter exists but is unused: `src/ui/popup.rs`.
- [x] Persist settings to TOML (already supported in `src/core/settings.rs`).

---

## 8) Keyboard shortcut to open popup

- [x] Add a global shortcut binding (use portal/hotkey crate) to open the popup.
  - Reference: `Sources/CodexBar/PreferencesAdvancedPane.swift` + `Sources/CodexBar/CodexbarApp.swift`.
- [x] Expose the shortcut in settings UI.

---

## 9) About dialog parity

- [x] Implement an About window with version + build info.
  - Reference: `Sources/CodexBar/About.swift`.

---

## 10) Polish & parity checks

- [x] Verify padding/typography/section separators match CodexBar’s density and hierarchy.
- [x] Ensure provider colors are used for progress bars and accents.
- [x] Validate that UI updates gracefully when usage is missing or in error state.
- [x] Ensure “refresh now” exists in popup (not only tray menu).

---

## Implementation order recommendation (optional)

1) Data model parity (Section 1)
2) Provider parsing (Sections 2–3)
3) Cost tokens (Section 4)
4) Popup UI layout & provider switcher (Section 5)
5) Footer actions (Section 6)
6) Settings + shortcut (Sections 7–8)
7) About dialog + polish (Sections 9–10)
