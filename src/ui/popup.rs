use crate::core::models::{
    CostSnapshot, CostUsageTokenSnapshot, Provider, ProviderCostSnapshot, RateWindow, UsageSnapshot,
};
use crate::core::settings::{PopupAnchor, PopupSettings, ThemeMode};
use crate::ui::{colors, styles, UsagePaceStage, UsagePaceText, UsageProgressBar};
use chrono::{DateTime, Utc};
use gtk4::gdk;
use gtk4::glib::{self, clone};
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const POPUP_WIDTH: i32 = 350;
const UPDATE_INTERVAL_MS: u32 = 1000;

fn label(text: &str, css_class: &str, align: gtk4::Align) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class(css_class);
    label.set_halign(align);
    label
}

fn separator() -> gtk4::Separator {
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(12);
    sep.set_margin_bottom(12);
    sep.add_css_class("section-separator");
    sep
}

fn build_content_box() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content
}

fn provider_rgba(provider: Provider, alpha: f32) -> gdk::RGBA {
    let (r, g, b) = colors::provider_rgb(provider);
    gdk::RGBA::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        alpha,
    )
}

#[derive(Clone)]
pub struct PopupWindow {
    window: adw::Window,
    stack: gtk4::Stack,
    content_primary: gtk4::Box,
    content_secondary: gtk4::Box,
    active_primary: Rc<Cell<bool>>,
    provider_state: Rc<RefCell<ProviderState>>,
    update_source: Rc<Cell<Option<glib::SourceId>>>,
    dismiss_source: Rc<Cell<Option<glib::SourceId>>>,
    dismiss_timeout_ms: Rc<Cell<u64>>,
    css_provider: gtk4::CssProvider,
}

struct ProviderState {
    provider: Provider,
    snapshots: HashMap<Provider, UsageSnapshot>,
    costs: HashMap<Provider, CostSnapshot>,
    token_snapshots: HashMap<Provider, CostUsageTokenSnapshot>,
    errors: HashMap<Provider, (String, String)>,
    show_as_remaining: bool,
    showing_provider_menu: bool,
}

struct UsageRow<'a> {
    title: String,
    window: &'a RateWindow,
    show_pace: bool,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self {
            provider: Provider::Claude,
            snapshots: HashMap::new(),
            costs: HashMap::new(),
            token_snapshots: HashMap::new(),
            errors: HashMap::new(),
            show_as_remaining: false,
            showing_provider_menu: false,
        }
    }
}

impl PopupWindow {
    pub fn new(app: &adw::Application, theme_mode: ThemeMode, popup_settings: &PopupSettings) -> Self {
        let window = adw::Window::builder()
            .application(app)
            .title("Claude Bar")
            .default_width(POPUP_WIDTH)
            .resizable(false)
            .deletable(true)
            .decorated(false)
            .hide_on_close(true)
            .build();

        window.add_css_class("popup-window");

        if gtk4_layer_shell::is_supported() {
            window.init_layer_shell();
            window.set_layer(gtk4_layer_shell::Layer::Top);
            window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
            window.set_namespace(Some("claude-bar-popup"));
            window.set_exclusive_zone(-1);
            apply_layer_shell_position(&window, popup_settings);
        }

        let css_provider = gtk4::CssProvider::new();
        let css = styles::css_for_provider(Provider::Claude);
        css_provider.load_from_data(&css);

        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let content_primary = build_content_box();
        let content_secondary = build_content_box();
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_transition_duration(150);
        stack.add_named(&content_primary, Some("primary"));
        stack.add_named(&content_secondary, Some("secondary"));
        stack.set_visible_child(&content_primary);

        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.add_css_class("popup-frame");
        frame.append(&stack);
        window.set_content(Some(&frame));

        let provider_state = Rc::new(RefCell::new(ProviderState::default()));
        let update_source = Rc::new(Cell::new(None));
        let active_primary = Rc::new(Cell::new(true));
        let dismiss_source = Rc::new(Cell::new(None));
        let dismiss_timeout_ms = Rc::new(Cell::new(popup_settings.dismiss_timeout_ms));

        let focus_controller = gtk4::EventControllerFocus::new();
        {
            let window_close = window.clone();
            let dismiss_src = Rc::clone(&dismiss_source);
            let timeout_ms = Rc::clone(&dismiss_timeout_ms);
            focus_controller.connect_leave(move |_| {
                let ms = timeout_ms.get();
                if ms == 0 {
                    window_close.close();
                    return;
                }

                let window_deferred = window_close.clone();
                let dismiss_src_inner = Rc::clone(&dismiss_src);
                let source_id = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(ms),
                    move || {
                        dismiss_src_inner.set(None);
                        window_deferred.close();
                    },
                );
                dismiss_src.set(Some(source_id));
            });
        }
        {
            let dismiss_src = Rc::clone(&dismiss_source);
            focus_controller.connect_enter(move |_| {
                if let Some(source_id) = dismiss_src.take() {
                    source_id.remove();
                }
            });
        }
        window.add_controller(focus_controller);

        let popup = Self {
            window,
            stack,
            content_primary,
            content_secondary,
            active_primary,
            provider_state,
            update_source,
            dismiss_source,
            dismiss_timeout_ms,
            css_provider,
        };

        popup.apply_theme_mode(theme_mode);
        popup.install_key_controller();
        popup
    }

    pub fn apply_popup_settings(&self, settings: &PopupSettings) {
        self.dismiss_timeout_ms.set(settings.dismiss_timeout_ms);
        if gtk4_layer_shell::is_supported() {
            apply_layer_shell_position(&self.window, settings);
        }
    }

    pub fn show(&self, provider: Provider) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.provider = provider;
            state.showing_provider_menu = false;
        }

        self.cancel_pending_dismiss();
        self.apply_provider_styles(provider);
        self.rebuild_content();

        self.window.set_visible(true);
        self.window.present();

        self.start_live_updates();
    }

    pub fn show_provider_menu(&self, providers: &[Provider]) {
        self.stop_live_updates();
        {
            let mut state = self.provider_state.borrow_mut();
            state.showing_provider_menu = true;
        }

        self.cancel_pending_dismiss();
        let content = self.current_content();
        self.rebuild_provider_menu_in(&content, providers);

        self.window.set_visible(true);
        self.window.present();
    }

    #[allow(dead_code)]
    pub fn hide(&self) {
        self.stop_live_updates();
        self.window.close();
    }

    pub fn update_usage(&self, provider: Provider, snapshot: &UsageSnapshot) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.snapshots.insert(provider, snapshot.clone());
            state.errors.remove(&provider);
        }
        self.rebuild_if_visible();
    }

    pub fn update_cost(&self, provider: Provider, cost: &CostSnapshot) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.costs.insert(provider, cost.clone());
        }
        self.rebuild_if_visible();
    }

    pub fn update_tokens(&self, provider: Provider, tokens: &CostUsageTokenSnapshot) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.token_snapshots.insert(provider, tokens.clone());
        }
        self.rebuild_if_visible();
    }

    pub fn show_error(&self, provider: Provider, error: &str, hint: &str) {
        {
            let mut state = self.provider_state.borrow_mut();
            state
                .errors
                .insert(provider, (error.to_string(), hint.to_string()));
            state.snapshots.remove(&provider);
        }
        self.rebuild_if_visible();
    }

    #[allow(dead_code)]
    pub fn set_show_as_remaining(&self, show_as_remaining: bool) {
        self.provider_state.borrow_mut().show_as_remaining = show_as_remaining;
        self.rebuild_if_visible();
    }

    pub fn set_theme_mode(&self, mode: ThemeMode) {
        self.apply_theme_mode(mode);
    }

    fn rebuild_if_visible(&self) {
        let showing_menu = self.provider_state.borrow().showing_provider_menu;
        if self.window.is_visible() && !showing_menu {
            self.rebuild_content();
        }
    }

    fn current_content(&self) -> gtk4::Box {
        if self.active_primary.get() {
            self.content_primary.clone()
        } else {
            self.content_secondary.clone()
        }
    }

    fn swap_content(&self) -> gtk4::Box {
        let next_primary = !self.active_primary.get();
        self.active_primary.set(next_primary);
        if next_primary {
            self.content_primary.clone()
        } else {
            self.content_secondary.clone()
        }
    }

    fn cancel_pending_dismiss(&self) {
        if let Some(source_id) = self.dismiss_source.take() {
            source_id.remove();
        }
    }

    fn install_key_controller(&self) {
        let popup = self.clone();
        let controller = gtk4::EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, state| {
            match key {
                gdk::Key::Escape => {
                    popup.hide();
                    glib::Propagation::Stop
                }
                gdk::Key::Tab => {
                    let backwards = state.contains(gdk::ModifierType::SHIFT_MASK);
                    popup.switch_provider(backwards);
                    glib::Propagation::Stop
                }
                gdk::Key::ISO_Left_Tab => {
                    popup.switch_provider(true);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(controller);
    }

    fn switch_provider(&self, backwards: bool) {
        let next = next_provider(self.provider_state.borrow().provider, backwards);
        {
            let mut state = self.provider_state.borrow_mut();
            if state.provider == next {
                return;
            }
            state.provider = next;
            state.showing_provider_menu = false;
        }

        self.apply_provider_styles(next);
        let content = self.swap_content();
        self.rebuild_content_in(&content);
        self.stack.set_visible_child(&content);
        self.start_live_updates();
    }

    fn apply_provider_styles(&self, provider: Provider) {
        let css = styles::css_for_provider(provider);
        self.css_provider.load_from_data(&css);
    }

    fn apply_theme_mode(&self, mode: ThemeMode) {
        let scheme = match mode {
            ThemeMode::System => adw::ColorScheme::Default,
            ThemeMode::Light => adw::ColorScheme::ForceLight,
            ThemeMode::Dark => adw::ColorScheme::ForceDark,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
    }

    fn rebuild_content(&self) {
        let content = self.current_content();
        self.rebuild_content_in(&content);
    }

    fn rebuild_content_in(&self, content: &gtk4::Box) {
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }

        let state = self.provider_state.borrow();
        let snapshot = state.snapshots.get(&state.provider);
        let cost = state.costs.get(&state.provider);
        let tokens = state.token_snapshots.get(&state.provider);
        let error = state.errors.get(&state.provider);

        self.build_provider_switcher(content, &state);
        self.build_header(content, &state, snapshot, error);
        content.append(&separator());

        if let Some((error, hint)) = error {
            self.build_error_section(content, error, hint);
        } else if let Some(snapshot) = snapshot {
            let usage_rows = collect_usage_rows(state.provider, snapshot);
            let accent = provider_rgba(state.provider, 0.75);
            let trough = provider_rgba(state.provider, 0.12);
            self.build_usage_sections(
                content,
                state.provider,
                &usage_rows,
                state.show_as_remaining,
                &accent,
                &trough,
            );

            if let Some(provider_cost) = snapshot.provider_cost.as_ref() {
                self.build_provider_cost_section(content, provider_cost, &accent, &trough);
            }

            if cost.is_some() || tokens.is_some() {
                content.append(&separator());
                self.build_cost_section(content, cost, tokens);
            }
        } else {
            content.append(&label("No usage data yet", "dim-label", gtk4::Align::Start));
        }

        let updated_at = snapshot.map(|s| s.updated_at);
        self.build_footer_actions(content, updated_at);
        self.resize_to_content(content);
    }

    fn rebuild_provider_menu_in(&self, content: &gtk4::Box, providers: &[Provider]) {
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }

        content.append(&label("Select provider", "heading", gtk4::Align::Start));
        content.append(&separator());

        for provider in providers {
            let button = gtk4::Button::with_label(provider.name());
            button.add_css_class("provider-choice");
            button.set_halign(gtk4::Align::Start);
            let popup = self.clone();
            let provider = *provider;
            button.connect_clicked(move |_| {
                popup.show(provider);
            });
            content.append(&button);
        }

        self.resize_to_content(content);
    }

    fn resize_to_content(&self, content: &gtk4::Box) {
        let (_, natural, _, _) = content.measure(gtk4::Orientation::Vertical, POPUP_WIDTH);
        self.window.set_default_height(natural);
    }

    fn build_header(
        &self,
        content: &gtk4::Box,
        state: &ProviderState,
        snapshot: Option<&UsageSnapshot>,
        error: Option<&(String, String)>,
    ) {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        header_box.set_margin_bottom(4);

        let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let provider_name = label(state.provider.name(), "title-3", gtk4::Align::Start);
        provider_name.set_hexpand(true);
        title_row.append(&provider_name);

        if let Some(plan) = snapshot.and_then(|s| s.identity.plan.as_ref()) {
            let plan_badge = label(plan, "plan-badge", gtk4::Align::End);
            plan_badge.set_valign(gtk4::Align::Center);
            title_row.append(&plan_badge);
        }

        header_box.append(&title_row);

        let subtitle_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let updated_text = if error.is_some() {
            "Unable to load usage".to_string()
        } else if let Some(snapshot) = snapshot {
            format_relative_time(snapshot.updated_at)
        } else {
            "Loading\u{2026}".to_string()
        };
        let updated_label = label(&updated_text, "header-updated", gtk4::Align::Start);
        updated_label.set_hexpand(true);
        subtitle_row.append(&updated_label);

        if let Some(email) = snapshot.and_then(|s| s.identity.email.as_ref()) {
            subtitle_row.append(&label(email, "dim-label", gtk4::Align::End));
        }

        header_box.append(&subtitle_row);
        content.append(&header_box);
    }

    fn build_provider_switcher(&self, content: &gtk4::Box, state: &ProviderState) {
        let switcher = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        switcher.add_css_class("provider-switcher");

        for provider in [Provider::Claude, Provider::Codex] {
            let button = gtk4::Button::new();
            button.add_css_class("provider-tab");
            button.set_hexpand(true);
            if provider == state.provider {
                button.add_css_class("selected");
            }

            let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
            let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            dot.set_size_request(6, 6);
            dot.add_css_class("provider-dot");
            match provider {
                Provider::Claude => dot.add_css_class("provider-dot-claude"),
                Provider::Codex => dot.add_css_class("provider-dot-codex"),
            }

            let name = label(provider.name(), "provider-tab-label", gtk4::Align::Start);
            inner.append(&dot);
            inner.append(&name);
            button.set_child(Some(&inner));

            let popup = self.clone();
            button.connect_clicked(move |_| {
                popup.show(provider);
            });

            switcher.append(&button);
        }

        content.append(&switcher);
    }

    fn build_usage_sections(
        &self,
        content: &gtk4::Box,
        provider: Provider,
        usage_rows: &[UsageRow<'_>],
        show_as_remaining: bool,
        accent: &gdk::RGBA,
        trough: &gdk::RGBA,
    ) {
        for row in usage_rows {
            self.build_usage_row(
                content,
                provider,
                row.title.as_str(),
                row.window,
                show_as_remaining,
                accent,
                trough,
                row.show_pace,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_usage_row(
        &self,
        content: &gtk4::Box,
        provider: Provider,
        title: &str,
        window: &RateWindow,
        show_as_remaining: bool,
        accent: &gdk::RGBA,
        trough: &gdk::RGBA,
        show_pace: bool,
    ) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        section.set_margin_top(10);
        section.append(&label(title, "heading", gtk4::Align::Start));

        let progress_bar = UsageProgressBar::new();
        progress_bar.set_hexpand(true);
        let display_percent = if show_as_remaining {
            window.remaining_percent()
        } else {
            window.used_percent
        };
        progress_bar.set_progress(display_percent.clamp(0.0, 1.0));
        progress_bar.set_colors(*accent, *trough);
        if show_pace {
            if let Some(detail) = UsagePaceText::weekly_detail(provider, window, Utc::now()) {
                let marker = detail.expected_used_percent / 100.0;
                let is_deficit = matches!(
                    detail.stage,
                    UsagePaceStage::SlightlyAhead
                        | UsagePaceStage::Ahead
                        | UsagePaceStage::FarAhead
                );
                progress_bar.set_pace_marker(Some(marker), is_deficit);
            }
        }
        section.append(&progress_bar);

        let details_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let percent_text = if show_as_remaining {
            format!("{:.0}% remaining", window.remaining_percent() * 100.0)
        } else {
            format!("{:.0}% used", window.used_percent * 100.0)
        };
        let percent_label = label(&percent_text, "usage-label", gtk4::Align::Start);
        percent_label.set_hexpand(true);
        details_row.append(&percent_label);

        if let Some(resets_at) = &window.resets_at {
            details_row.append(&label(&format_reset_time(*resets_at), "countdown-label", gtk4::Align::End));
        }

        section.append(&details_row);

        if show_pace {
            if let Some(summary) = UsagePaceText::weekly_summary(provider, window, Utc::now()) {
                section.append(&label(&summary, "pace-label", gtk4::Align::Start));
            }
        }
        content.append(&section);
    }

    fn build_cost_section(
        &self,
        content: &gtk4::Box,
        cost: Option<&CostSnapshot>,
        tokens: Option<&CostUsageTokenSnapshot>,
    ) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        section.set_margin_top(4);
        section.append(&label("Cost", "heading", gtk4::Align::Start));

        if let Some(cost) = cost {
            if cost.log_error {
                let error_label = label("Error reading logs", "cost-error", gtk4::Align::Start);
                attach_log_copy_handler(&error_label);
                section.append(&error_label);
                content.append(&section);
                return;
            }
        }

        if let Some(tokens) = tokens {
            let prefix = cost.map_or("", |c| if c.pricing_estimate { "~" } else { "" });
            let session_cost = tokens
                .session_cost_usd
                .or_else(|| cost.map(|c| c.today_cost))
                .map(|v| format!("{}{}", prefix, format_currency(v)));
            let month_cost = tokens
                .last_30_days_cost_usd
                .or_else(|| cost.map(|c| c.monthly_cost))
                .map(|v| format!("{}{}", prefix, format_currency(v)));

            let session_tokens = tokens.session_tokens.map(format_token_count);
            let session_line = if let Some(cost_text) = session_cost {
                if let Some(tokens_text) = session_tokens {
                    format!("Today: {} · {} tokens", cost_text, tokens_text)
                } else {
                    format!("Today: {}", cost_text)
                }
            } else {
                "Today: —".to_string()
            };

            let month_tokens = tokens.last_30_days_tokens.map(format_token_count);
            let month_line = if let Some(cost_text) = month_cost {
                if let Some(tokens_text) = month_tokens {
                    format!("Last 30 days: {} · {} tokens", cost_text, tokens_text)
                } else {
                    format!("Last 30 days: {}", cost_text)
                }
            } else {
                "Last 30 days: —".to_string()
            };

            section.append(&label(&session_line, "cost-line", gtk4::Align::Start));
            section.append(&label(&month_line, "cost-line", gtk4::Align::Start));
        } else if let Some(cost) = cost {
            let prefix = if cost.pricing_estimate { "~" } else { "" };
            let today = format!("Today: {}{}", prefix, format_currency(cost.today_cost));
            let month = format!("Last 30 days: {}{}", prefix, format_currency(cost.monthly_cost));
            section.append(&label(&today, "cost-line", gtk4::Align::Start));
            section.append(&label(&month, "cost-line", gtk4::Align::Start));
        } else {
            section.append(&label("No cost data yet", "dim-label", gtk4::Align::Start));
        }

        content.append(&section);
    }

    fn build_error_section(&self, content: &gtk4::Box, error: &str, hint: &str) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let error_label = label(error, "error", gtk4::Align::Start);
        error_label.set_wrap(true);
        section.append(&error_label);

        let hint_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        hint_box.add_css_class("error-hint");
        let hint_label = gtk4::Label::new(Some(hint));
        hint_label.set_selectable(true);
        hint_label.set_halign(gtk4::Align::Start);
        hint_box.append(&hint_label);
        section.append(&hint_box);

        content.append(&section);
    }

    fn build_provider_cost_section(
        &self,
        content: &gtk4::Box,
        cost: &ProviderCostSnapshot,
        accent: &gdk::RGBA,
        trough: &gdk::RGBA,
    ) {
        if cost.limit <= 0.0 {
            return;
        }

        let title = if cost.currency_code == "Quota" {
            "Quota usage".to_string()
        } else {
            "Extra usage".to_string()
        };

        let used = if cost.currency_code == "Quota" {
            format!("{:.0}", cost.used)
        } else {
            format_currency_with_code(cost.used, &cost.currency_code)
        };
        let limit = if cost.currency_code == "Quota" {
            format!("{:.0}", cost.limit)
        } else {
            format_currency_with_code(cost.limit, &cost.currency_code)
        };
        let period = cost.period.as_deref().unwrap_or("This month");
        let spend_line = format!("{}: {} / {}", period, used, limit);

        let percent_used = (cost.used / cost.limit).clamp(0.0, 1.0);

        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        section.set_margin_top(14);
        section.append(&label(&title, "heading", gtk4::Align::Start));

        let progress_bar = UsageProgressBar::new();
        progress_bar.set_hexpand(true);
        progress_bar.set_progress(percent_used);
        progress_bar.set_colors(*accent, *trough);
        section.append(&progress_bar);

        let details = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let spend_label = label(&spend_line, "cost-line", gtk4::Align::Start);
        spend_label.set_hexpand(true);
        details.append(&spend_label);
        details.append(&label(
            &format!("{:.0}% used", percent_used * 100.0),
            "countdown-label",
            gtk4::Align::End,
        ));

        section.append(&details);
        content.append(&section);
    }

    fn build_footer_actions(&self, content: &gtk4::Box, _updated_at: Option<DateTime<Utc>>) {
        content.append(&separator());

        let actions = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        actions.add_css_class("footer-actions");

        let provider = self.provider_state.borrow().provider;
        let has_error = self.provider_state.borrow().errors.contains_key(&provider);
        let login_label = if has_error { "Add Account" } else { "Switch Account" };

        actions.append(&self.action_button(login_label, move || {
            crate::daemon::login::spawn_provider_login(provider);
        }));
        actions.append(&self.action_button("Usage Dashboard", move || {
            open::that(provider.dashboard_url()).ok();
        }));
        actions.append(&self.action_button("Status Page", move || {
            open::that(provider.status_url()).ok();
        }));
        actions.append(&self.action_button("Refresh Now", move || {
            trigger_refresh();
        }));
        actions.append(&self.action_button("Settings", {
            let popup = self.clone();
            move || {
                popup.open_settings_window();
            }
        }));
        content.append(&actions);

        let version_label = label(
            &format!("Claude Bar v{}", env!("CARGO_PKG_VERSION")),
            "version-footer",
            gtk4::Align::Center,
        );
        version_label.set_margin_top(8);
        content.append(&version_label);
    }

    fn action_button<F>(&self, label_text: &str, action: F) -> gtk4::Button
    where
        F: Fn() + 'static,
    {
        let button = gtk4::Button::with_label(label_text);
        button.add_css_class("footer-action");
        button.set_halign(gtk4::Align::Fill);
        if let Some(child) = button.child().and_then(|c| c.downcast::<gtk4::Label>().ok()) {
            child.set_halign(gtk4::Align::Start);
        }
        button.connect_clicked(move |_| {
            action();
        });
        button
    }

    fn open_settings_window(&self) {
        let settings = crate::core::settings::Settings::load().unwrap_or_default();
        let settings = Rc::new(RefCell::new(settings));

        let window = adw::PreferencesWindow::builder()
            .transient_for(&self.window)
            .title("Settings")
            .default_width(360)
            .default_height(420)
            .build();

        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        group.set_title("Display");

        let show_remaining_row = adw::ActionRow::builder()
            .title("Show remaining usage")
            .build();
        let show_remaining_switch = gtk4::Switch::new();
        show_remaining_switch.set_active(settings.borrow().display.show_as_remaining);
        show_remaining_row.add_suffix(&show_remaining_switch);
        show_remaining_row.set_activatable_widget(Some(&show_remaining_switch));
        {
            let settings = Rc::clone(&settings);
            let popup = self.clone();
            show_remaining_switch.connect_state_set(move |_, state| {
                {
                    let mut settings = settings.borrow_mut();
                    settings.display.show_as_remaining = state;
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
                popup.set_show_as_remaining(state);
                glib::Propagation::Proceed
            });
        }
        group.add(&show_remaining_row);

        let merge_icons_row = adw::ActionRow::builder()
            .title("Merge tray icons")
            .build();
        let merge_icons_switch = gtk4::Switch::new();
        merge_icons_switch.set_active(settings.borrow().providers.merge_icons);
        merge_icons_row.add_suffix(&merge_icons_switch);
        merge_icons_row.set_activatable_widget(Some(&merge_icons_switch));
        {
            let settings = Rc::clone(&settings);
            merge_icons_switch.connect_state_set(move |_, state| {
                {
                    let mut settings = settings.borrow_mut();
                    settings.providers.merge_icons = state;
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
                glib::Propagation::Proceed
            });
        }
        group.add(&merge_icons_row);

        let theme_row = adw::ComboRow::new();
        theme_row.set_title("Theme");
        let theme_model = gtk4::StringList::new(&["System", "Light", "Dark"]);
        theme_row.set_model(Some(&theme_model));
        theme_row.set_selected(match settings.borrow().theme.mode {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        });
        {
            let settings = Rc::clone(&settings);
            let popup = self.clone();
            theme_row.connect_selected_notify(move |row| {
                let mode = match row.selected() {
                    1 => ThemeMode::Light,
                    2 => ThemeMode::Dark,
                    _ => ThemeMode::System,
                };
                {
                    let mut settings = settings.borrow_mut();
                    settings.theme.mode = mode.clone();
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
                popup.set_theme_mode(mode);
            });
        }
        group.add(&theme_row);

        let notifications_group = adw::PreferencesGroup::new();
        notifications_group.set_title("Notifications");
        let threshold_row = adw::ActionRow::builder()
            .title("Usage threshold")
            .subtitle("Notify when usage exceeds this percent")
            .build();
        let threshold_spin = gtk4::SpinButton::with_range(0.0, 1.0, 0.05);
        threshold_spin.set_value(settings.borrow().notifications.threshold);
        threshold_row.add_suffix(&threshold_spin);
        threshold_row.set_activatable_widget(Some(&threshold_spin));
        {
            let settings = Rc::clone(&settings);
            threshold_spin.connect_value_changed(move |spin| {
                {
                    let mut settings = settings.borrow_mut();
                    settings.notifications.threshold = spin.value();
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
            });
        }
        notifications_group.add(&threshold_row);

        let shortcuts_group = adw::PreferencesGroup::new();
        shortcuts_group.set_title("Shortcuts");
        let shortcut_row = adw::ActionRow::builder()
            .title("Open popup")
            .build();
        let shortcut_entry = gtk4::Entry::new();
        shortcut_entry.set_text(&settings.borrow().shortcuts.popup);
        shortcut_entry.set_width_chars(12);
        shortcut_row.add_suffix(&shortcut_entry);
        shortcut_row.set_activatable_widget(Some(&shortcut_entry));
        {
            let settings = Rc::clone(&settings);
            shortcut_entry.connect_changed(move |entry| {
                {
                    let mut settings = settings.borrow_mut();
                    settings.shortcuts.popup = entry.text().to_string();
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
            });
        }
        let shortcut_switch = gtk4::Switch::new();
        shortcut_switch.set_active(settings.borrow().shortcuts.enabled);
        shortcut_row.add_suffix(&shortcut_switch);
        {
            let settings = Rc::clone(&settings);
            shortcut_switch.connect_state_set(move |_, state| {
                {
                    let mut settings = settings.borrow_mut();
                    settings.shortcuts.enabled = state;
                    if let Err(e) = settings.save() {
                        tracing::warn!(error = %e, "Failed to save settings");
                    }
                }
                glib::Propagation::Proceed
            });
        }
        shortcuts_group.add(&shortcut_row);

        page.add(&group);
        page.add(&notifications_group);
        page.add(&shortcuts_group);
        window.add(&page);
        window.present();
    }

    fn start_live_updates(&self) {
        self.stop_live_updates();

        let state = Rc::clone(&self.provider_state);
        let content = self.current_content();

        let source_id = glib::timeout_add_local(
            std::time::Duration::from_millis(UPDATE_INTERVAL_MS.into()),
            clone!(
                #[weak]
                state,
                #[weak]
                content,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    update_dynamic_labels(&state, &content);
                    glib::ControlFlow::Continue
                }
            ),
        );

        self.update_source.set(Some(source_id));
    }

    fn stop_live_updates(&self) {
        if let Some(source_id) = self.update_source.take() {
            source_id.remove();
        }
    }
}

impl Drop for PopupWindow {
    fn drop(&mut self) {
        self.stop_live_updates();
        self.cancel_pending_dismiss();
    }
}

fn apply_layer_shell_position(window: &adw::Window, settings: &PopupSettings) {
    let (anchor_v, anchor_h) = match settings.anchor {
        PopupAnchor::TopLeft => (gtk4_layer_shell::Edge::Top, gtk4_layer_shell::Edge::Left),
        PopupAnchor::TopRight => (gtk4_layer_shell::Edge::Top, gtk4_layer_shell::Edge::Right),
        PopupAnchor::BottomLeft => (gtk4_layer_shell::Edge::Bottom, gtk4_layer_shell::Edge::Left),
        PopupAnchor::BottomRight => (gtk4_layer_shell::Edge::Bottom, gtk4_layer_shell::Edge::Right),
    };

    window.set_anchor(gtk4_layer_shell::Edge::Top, false);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);
    window.set_anchor(gtk4_layer_shell::Edge::Left, false);
    window.set_anchor(gtk4_layer_shell::Edge::Right, false);

    window.set_anchor(anchor_v, true);
    window.set_anchor(anchor_h, true);

    window.set_margin(gtk4_layer_shell::Edge::Top, settings.margin_top);
    window.set_margin(gtk4_layer_shell::Edge::Right, settings.margin_right);
    window.set_margin(gtk4_layer_shell::Edge::Bottom, settings.margin_bottom);
    window.set_margin(gtk4_layer_shell::Edge::Left, settings.margin_left);
}

fn collect_usage_rows(provider: Provider, snapshot: &UsageSnapshot) -> Vec<UsageRow<'_>> {
    let mut rows = Vec::new();

    if let Some(primary) = &snapshot.primary {
        let label = match provider {
            Provider::Claude => "5-hour session",
            Provider::Codex => "Session",
        };
        rows.push(UsageRow {
            title: label.to_string(),
            window: primary,
            show_pace: false,
        });
    }

    if let Some(secondary) = &snapshot.secondary {
        let label = match provider {
            Provider::Claude => "Weekly quota",
            Provider::Codex => "Weekly",
        };
        rows.push(UsageRow {
            title: label.to_string(),
            window: secondary,
            show_pace: true,
        });
    }

    if let Some(tertiary) = &snapshot.tertiary {
        let label = resolve_tertiary_label(snapshot, provider);
        rows.push(UsageRow {
            title: label,
            window: tertiary,
            show_pace: false,
        });
    }

    rows
}

fn resolve_tertiary_label(snapshot: &UsageSnapshot, provider: Provider) -> String {
    let Some(tertiary) = snapshot.tertiary.as_ref() else {
        return "Model".to_string();
    };

    for carveout in &snapshot.carveouts {
        if windows_match(&carveout.window, tertiary) {
            return carveout
                .label
                .trim_end_matches(" Weekly")
                .to_string();
        }
    }

    match provider {
        Provider::Claude => "Model".to_string(),
        Provider::Codex => "Additional".to_string(),
    }
}

fn windows_match(left: &RateWindow, right: &RateWindow) -> bool {
    let percent_close = (left.used_percent - right.used_percent).abs() < 0.001;
    let reset_same = left.resets_at == right.resets_at;
    let window_same = left.window_minutes == right.window_minutes;
    percent_close && reset_same && window_same
}

fn attach_log_copy_handler(label: &gtk4::Label) {
    let Some(path) = daemon_log_path() else {
        return;
    };

    label.set_tooltip_text(Some(&path));
    label.set_can_target(true);
    let path = Rc::new(path);

    let click = gtk4::GestureClick::new();
    let label_clone = label.clone();
    let path_clone = Rc::clone(&path);
    click.connect_released(move |_, _, _, _| {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&path_clone);
        }

        label_clone.set_tooltip_text(Some("Copied!"));
        let label_reset = label_clone.clone();
        let path_reset = Rc::clone(&path_clone);
        glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
            label_reset.set_tooltip_text(Some(&path_reset));
        });
    });

    label.add_controller(click);
}

fn daemon_log_path() -> Option<String> {
    dirs::data_local_dir().map(|d| {
        d.join("claude-bar")
            .join("claude-bar.log")
            .display()
            .to_string()
    })
}

fn next_provider(current: Provider, backwards: bool) -> Provider {
    let providers = [Provider::Claude, Provider::Codex];
    let current_idx = providers
        .iter()
        .position(|p| *p == current)
        .unwrap_or(0);
    let next_idx = if backwards {
        (current_idx + providers.len() - 1) % providers.len()
    } else {
        (current_idx + 1) % providers.len()
    };
    providers[next_idx]
}

fn update_dynamic_labels(state: &Rc<RefCell<ProviderState>>, content: &gtk4::Box) {
    let state_ref = state.borrow();
    let snapshot = state_ref.snapshots.get(&state_ref.provider);

    if let Some(snapshot) = snapshot {
        let mut child = content.first_child();
        while let Some(widget) = child {
            if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                let text = label.text();
                if text.starts_with("Updated ") {
                    let new_text = format_relative_time(snapshot.updated_at);
                    label.set_text(&new_text);
                }
            }

            if let Some(box_widget) = widget.downcast_ref::<gtk4::Box>() {
                update_labels_in_box(box_widget, snapshot);
            }

            child = widget.next_sibling();
        }
    }
}

fn update_labels_in_box(box_widget: &gtk4::Box, snapshot: &UsageSnapshot) {
    let mut child = box_widget.first_child();
    while let Some(widget) = child {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            let text = label.text();
            if text.starts_with("Updated ") {
                let new_text = format_relative_time(snapshot.updated_at);
                label.set_text(&new_text);
            }
        }

        if let Some(inner_box) = widget.downcast_ref::<gtk4::Box>() {
            update_labels_in_box(inner_box, snapshot);
        }

        child = widget.next_sibling();
    }
}

fn format_relative_time(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);

    let seconds = duration.num_seconds();
    if seconds < 0 {
        return "Updated just now".to_string();
    }

    if seconds < 60 {
        return format!("Updated {}s ago", seconds);
    }

    let minutes = duration.num_minutes();
    if minutes < 60 {
        return format!("Updated {}m ago", minutes);
    }

    let hours = duration.num_hours();
    if hours < 24 {
        return format!("Updated {}h ago", hours);
    }

    let days = duration.num_days();
    format!("Updated {}d ago", days)
}

fn format_reset_time(reset_at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = reset_at.signed_duration_since(now);

    if duration.num_seconds() <= 0 {
        return "resets now".to_string();
    }

    let total_minutes = duration.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    if hours > 24 {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        format!("resets in {}d {}h", days, remaining_hours)
    } else if hours > 0 {
        format!("resets in {}h {}m", hours, minutes)
    } else {
        format!("resets in {}m", minutes)
    }
}

fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}

fn format_currency_with_code(value: f64, code: &str) -> String {
    if code == "USD" {
        return format_currency(value);
    }
    format!("{} {:.2}", code, value)
}

fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn trigger_refresh() {
    tokio::spawn(async {
        let connection = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to connect to D-Bus session");
                return;
            }
        };
        let result: zbus::Result<()> = connection
            .call_method(
                Some(crate::daemon::DBUS_NAME),
                crate::daemon::DBUS_PATH,
                Some(crate::daemon::DBUS_NAME),
                "Refresh",
                &(),
            )
            .await
            .map(|reply| reply.body().deserialize().unwrap_or(()));
        if let Err(e) = result {
            tracing::warn!(error = %e, "Failed to trigger refresh");
        }
    });
}
