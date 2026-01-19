pub struct UsageProgressBar {
    progress: f64,
    label: String,
}

impl UsageProgressBar {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            progress: 0.0,
            label: label.into(),
        }
    }

    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    pub fn progress(&self) -> f64 {
        self.progress
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}
