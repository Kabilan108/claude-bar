use crate::core::models::Provider;

pub struct TrayManager {
    // TODO: ksni StatusNotifierItem instances
}

impl TrayManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn update_icon(&mut self, _provider: Provider, _primary: f64, _secondary: f64) {
        // TODO: Update tray icon with new usage percentages
    }

    pub fn set_loading(&mut self, _provider: Provider) {
        // TODO: Show Knight Rider loading animation
    }

    pub fn set_error(&mut self, _provider: Provider) {
        // TODO: Show error state (grayed icon)
    }
}

impl Default for TrayManager {
    fn default() -> Self {
        Self::new()
    }
}
