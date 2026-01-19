use crate::core::models::{CostSnapshot, Provider, RateWindow, UsageSnapshot};
use crate::ui::styles;
use chrono::{DateTime, Utc};
use gtk4::glib::{self, clone};
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
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

pub struct PopupWindow {
    window: adw::Window,
    content: gtk4::Box,
    provider_state: Rc<RefCell<ProviderState>>,
    update_source: Rc<Cell<Option<glib::SourceId>>>,
}

struct ProviderState {
    provider: Provider,
    snapshot: Option<UsageSnapshot>,
    cost: Option<CostSnapshot>,
    error: Option<(String, String)>,
    show_as_remaining: bool,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self {
            provider: Provider::Claude,
            snapshot: None,
            cost: None,
            error: None,
            show_as_remaining: false,
        }
    }
}

impl PopupWindow {
    pub fn new(app: &adw::Application) -> Self {
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
        css_provider.load_from_data(styles::CSS);

        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(16);
        content.set_margin_end(16);

        window.set_content(Some(&content));

        let provider_state = Rc::new(RefCell::new(ProviderState::default()));
        let update_source = Rc::new(Cell::new(None));

        let focus_controller = gtk4::EventControllerFocus::new();
        let window_clone = window.clone();
        focus_controller.connect_leave(move |_| {
            window_clone.close();
        });
        window.add_controller(focus_controller);

        Self {
            window,
            content,
            provider_state,
            update_source,
        }
    }

    pub fn show(&self, provider: Provider) {
        {
            let mut state = self.provider_state.borrow_mut();
            state.provider = provider;
        }

        self.rebuild_content();

        self.position_window();
        self.window.set_visible(true);
        self.window.present();

        self.start_live_updates();
    }

    #[allow(dead_code)]
    pub fn hide(&self) {
        self.stop_live_updates();
        self.window.close();
    }

    pub fn update_usage(&self, provider: Provider, snapshot: &UsageSnapshot) {
        self.update_state(provider, |state| {
            state.snapshot = Some(snapshot.clone());
            state.error = None;
        });
    }

    pub fn update_cost(&self, provider: Provider, cost: &CostSnapshot) {
        self.update_state(provider, |state| {
            state.cost = Some(cost.clone());
        });
    }

    pub fn show_error(&self, provider: Provider, error: &str, hint: &str) {
        self.update_state(provider, |state| {
            state.error = Some((error.to_string(), hint.to_string()));
            state.snapshot = None;
        });
    }

    #[allow(dead_code)]
    pub fn set_show_as_remaining(&self, show_as_remaining: bool) {
        self.provider_state.borrow_mut().show_as_remaining = show_as_remaining;
        self.rebuild_if_visible();
    }

    fn update_state(&self, provider: Provider, f: impl FnOnce(&mut ProviderState)) {
        let mut state = self.provider_state.borrow_mut();
        if state.provider == provider {
            f(&mut state);
        }
        drop(state);
        self.rebuild_if_visible();
    }

    fn rebuild_if_visible(&self) {
        if self.window.is_visible() {
            self.rebuild_content();
        }
    }

    fn position_window(&self) {
        // Positioning is handled by the compositor on Wayland.
        // Layer-shell protocols would be needed for precise placement.
    }

    fn rebuild_content(&self) {
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }

        let state = self.provider_state.borrow();

        self.build_header(&state);
        self.content.append(&separator());

        if let Some((error, hint)) = &state.error {
            self.build_error_section(error, hint);
        } else if let Some(snapshot) = &state.snapshot {
            self.build_usage_sections(snapshot, state.show_as_remaining);

            if state.cost.is_some() || snapshot.primary.is_some() {
                self.content.append(&separator());
            }

            if let Some(cost) = &state.cost {
                self.build_cost_section(cost);
            }

            self.content.append(&separator());
            self.build_footer(snapshot.updated_at);
        } else {
            self.content.append(&label("No usage data yet", "dim-label", gtk4::Align::Start));
        }
    }

    fn build_header(&self, state: &ProviderState) {
        let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

        let provider_name = label(state.provider.name(), "title-3", gtk4::Align::Start);
        provider_name.set_hexpand(true);
        title_row.append(&provider_name);

        if let Some(email) = state.snapshot.as_ref().and_then(|s| s.identity.email.as_ref()) {
            title_row.append(&label(email, "dim-label", gtk4::Align::End));
        }

        header_box.append(&title_row);

        if let Some(plan) = state.snapshot.as_ref().and_then(|s| s.identity.plan.as_ref()) {
            header_box.append(&label(plan, "dim-label", gtk4::Align::Start));
        }

        self.content.append(&header_box);
    }

    fn build_usage_sections(&self, snapshot: &UsageSnapshot, show_as_remaining: bool) {
        if let Some(primary) = &snapshot.primary {
            self.build_usage_row("Session (5-hour)", primary, show_as_remaining);
        }

        if let Some(secondary) = &snapshot.secondary {
            self.build_usage_row("Weekly", secondary, show_as_remaining);
        }

        if let Some(opus) = &snapshot.opus {
            self.build_usage_row("Opus/Sonnet", opus, show_as_remaining);
        }
    }

    fn build_usage_row(&self, title: &str, window: &RateWindow, show_as_remaining: bool) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        section.set_margin_top(8);
        section.append(&label(title, "heading", gtk4::Align::Start));

        let progress_bar = gtk4::ProgressBar::new();
        progress_bar.add_css_class("usage-progress");
        let display_percent = if show_as_remaining {
            window.remaining_percent()
        } else {
            window.used_percent
        };
        progress_bar.set_fraction(display_percent.clamp(0.0, 1.0));
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
        self.content.append(&section);
    }

    fn build_cost_section(&self, cost: &CostSnapshot) {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        section.append(&label("Cost", "heading", gtk4::Align::Start));
        section.append(&label(&format!("Today: ${:.2}", cost.today_cost), "cost-label", gtk4::Align::Start));
        section.append(&label(&format!("This month: ${:.2}", cost.monthly_cost), "cost-label", gtk4::Align::Start));
        self.content.append(&section);
    }

    fn build_error_section(&self, error: &str, hint: &str) {
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

        self.content.append(&section);
    }

    fn build_footer(&self, updated_at: DateTime<Utc>) {
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let updated_label = label(&format_relative_time(updated_at), "dim-label", gtk4::Align::Start);
        updated_label.set_hexpand(true);
        footer.append(&updated_label);
        self.content.append(&footer);
    }

    fn start_live_updates(&self) {
        self.stop_live_updates();

        let state = Rc::clone(&self.provider_state);
        let content = self.content.clone();

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

fn update_dynamic_labels(state: &Rc<RefCell<ProviderState>>, content: &gtk4::Box) {
    let state_ref = state.borrow();

    if let Some(snapshot) = &state_ref.snapshot {
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
