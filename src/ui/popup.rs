use crate::core::models::{CostSnapshot, Provider, RateWindow, UsageSnapshot};
use crate::core::settings::ThemeMode;
use crate::ui::{colors, styles, UsageProgressBar};
use chrono::{DateTime, Utc};
use gtk4::gdk;
use gtk4::glib::{self, clone};
use gtk4::prelude::*;
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
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    sep
}

fn build_content_box() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_top(8);
    content.set_margin_bottom(0);
    content.set_margin_start(16);
    content.set_margin_end(16);
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
    css_provider: gtk4::CssProvider,
}

struct ProviderState {
    provider: Provider,
    snapshots: HashMap<Provider, UsageSnapshot>,
    costs: HashMap<Provider, CostSnapshot>,
    errors: HashMap<Provider, (String, String)>,
    show_as_remaining: bool,
    showing_provider_menu: bool,
}

struct UsageRow<'a> {
    title: String,
    window: &'a RateWindow,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self {
            provider: Provider::Claude,
            snapshots: HashMap::new(),
            costs: HashMap::new(),
            errors: HashMap::new(),
            show_as_remaining: false,
            showing_provider_menu: false,
        }
    }
}

impl PopupWindow {
    pub fn new(app: &adw::Application, theme_mode: ThemeMode) -> Self {
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

        window.set_content(Some(&stack));

        let provider_state = Rc::new(RefCell::new(ProviderState::default()));
        let update_source = Rc::new(Cell::new(None));
        let active_primary = Rc::new(Cell::new(true));

        let focus_controller = gtk4::EventControllerFocus::new();
        let window_clone = window.clone();
        focus_controller.connect_leave(move |_| {
            window_clone.close();
        });
        window.add_controller(focus_controller);

        let popup = Self {
            window,
            stack,
            content_primary,
            content_secondary,
            active_primary,
            provider_state,
            update_source,
            css_provider,
        };

        popup.apply_theme_mode(theme_mode);
        popup.install_key_controller();
        popup
    }

    pub fn show(&self, provider: Provider) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.provider = provider;
            state.showing_provider_menu = false;
        }

        self.apply_provider_styles(provider);
        self.rebuild_content();

        self.position_window();
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

        let content = self.current_content();
        self.rebuild_provider_menu_in(&content, providers);

        self.position_window();
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

    fn position_window(&self) {
        // Positioning is handled by the compositor on Wayland.
        // Layer-shell protocols would be needed for precise placement.
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
        let error = state.errors.get(&state.provider);

        self.build_header(content, &state, snapshot);
        content.append(&separator());

        if let Some((error, hint)) = error {
            self.build_error_section(content, error, hint);
            self.resize_to_content(content);
        } else if let Some(snapshot) = snapshot {
            let usage_rows = collect_usage_rows(state.provider, snapshot);
            let accent = provider_rgba(state.provider, 1.0);
            let trough = provider_rgba(state.provider, 0.25);
            self.build_usage_sections(content, &usage_rows, state.show_as_remaining, &accent, &trough);

            if let Some(cost) = cost {
                content.append(&separator());
                self.build_cost_section(content, cost);
            } else {
                content.append(&separator());
            }

            self.build_footer(content, snapshot.updated_at);
            self.resize_to_content(content);
        } else {
            content.append(&label("No usage data yet", "dim-label", gtk4::Align::Start));
            self.resize_to_content(content);
        }
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
    ) {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

        let provider_name = label(state.provider.name(), "title-3", gtk4::Align::Start);
        provider_name.set_hexpand(true);
        title_row.append(&provider_name);

        if let Some(email) = snapshot.and_then(|s| s.identity.email.as_ref()) {
            title_row.append(&label(email, "dim-label", gtk4::Align::End));
        }

        header_box.append(&title_row);

        if let Some(plan) = snapshot.and_then(|s| s.identity.plan.as_ref()) {
            header_box.append(&label(plan, "dim-label", gtk4::Align::Start));
        }

        content.append(&header_box);
    }

    fn build_usage_sections(
        &self,
        content: &gtk4::Box,
        usage_rows: &[UsageRow<'_>],
        show_as_remaining: bool,
        accent: &gdk::RGBA,
        trough: &gdk::RGBA,
    ) {
        for row in usage_rows {
            self.build_usage_row(content, row.title.as_str(), row.window, show_as_remaining, accent, trough);
        }
    }

    fn build_usage_row(
        &self,
        content: &gtk4::Box,
        title: &str,
        window: &RateWindow,
        show_as_remaining: bool,
        accent: &gdk::RGBA,
        trough: &gdk::RGBA,
    ) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        section.set_margin_top(8);
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
        content.append(&section);
    }

    fn build_cost_section(&self, content: &gtk4::Box, cost: &CostSnapshot) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        if cost.log_error {
            let error_label = label("Error reading logs", "cost-error", gtk4::Align::Start);
            attach_log_copy_handler(&error_label);
            section.append(&error_label);
        } else {
            let prefix = if cost.pricing_estimate { "~" } else { "" };
            let cost_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

            let today_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let today_amount = label(&format!("{}${:.2}", prefix, cost.today_cost), "cost-amount", gtk4::Align::Start);
            let today_period = label("today", "cost-period", gtk4::Align::Start);
            today_box.append(&today_amount);
            today_box.append(&today_period);
            today_box.set_hexpand(true);

            let month_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let month_amount = label(&format!("{}${:.2}", prefix, cost.monthly_cost), "cost-amount", gtk4::Align::End);
            let month_period = label("this month", "cost-period", gtk4::Align::End);
            month_box.append(&month_amount);
            month_box.append(&month_period);

            cost_row.append(&today_box);
            cost_row.append(&month_box);
            section.append(&cost_row);
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

    fn build_footer(&self, content: &gtk4::Box, updated_at: DateTime<Utc>) {
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        footer.set_margin_top(8);
        footer.set_margin_bottom(6);
        let updated_label = label(&format_relative_time(updated_at), "footer-label", gtk4::Align::Start);
        updated_label.set_hexpand(true);
        footer.append(&updated_label);
        content.append(&footer);
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
    }
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
        });
    }

    for carveout in &snapshot.carveouts {
        rows.push(UsageRow {
            title: carveout.label.clone(),
            window: &carveout.window,
        });
    }

    rows
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
