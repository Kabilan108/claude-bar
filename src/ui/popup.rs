use crate::core::models::{CostSnapshot, Provider, UsageSnapshot};

pub struct PopupWindow {
    // TODO: GTK window and widgets
}

impl PopupWindow {
    pub fn new() -> Self {
        Self {}
    }

    pub fn show(&self, _provider: Provider) {
        // TODO: Show popup window positioned at top-right
    }

    pub fn hide(&self) {
        // TODO: Hide popup window
    }

    pub fn update_usage(&self, _provider: Provider, _snapshot: &UsageSnapshot) {
        // TODO: Update usage display
    }

    pub fn update_cost(&self, _provider: Provider, _cost: &CostSnapshot) {
        // TODO: Update cost display
    }

    pub fn show_error(&self, _provider: Provider, _error: &str, _hint: &str) {
        // TODO: Show error state with troubleshooting hint
    }
}

impl Default for PopupWindow {
    fn default() -> Self {
        Self::new()
    }
}
