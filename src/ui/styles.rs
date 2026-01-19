#[allow(dead_code)]
pub const BRAND_COLOR: &str = "#F5A623";

pub const CSS: &str = r#"
.popup-window {
    background-color: @window_bg_color;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.usage-progress {
    min-height: 8px;
    border-radius: 4px;
    background-color: alpha(@window_fg_color, 0.1);
}

.usage-progress progress {
    background-color: #F5A623;
    border-radius: 4px;
}

.usage-progress trough {
    min-height: 8px;
    border-radius: 4px;
    background-color: alpha(@window_fg_color, 0.1);
}

.usage-progress-bar {
    min-height: 8px;
}

.usage-label {
    font-size: 0.9em;
    color: @theme_fg_color;
}

.countdown-label {
    font-size: 0.8em;
    color: @theme_unfocused_fg_color;
}

.cost-label {
    font-weight: 500;
    font-size: 0.9em;
}

.error-hint {
    font-family: monospace;
    font-size: 0.85em;
    padding: 8px 12px;
    background-color: alpha(@error_color, 0.1);
    border-radius: 6px;
    border: 1px solid alpha(@error_color, 0.2);
}

.error {
    color: @error_color;
}

.heading {
    font-weight: 600;
    font-size: 0.95em;
}

.title-3 {
    font-weight: 700;
    font-size: 1.1em;
}
"#;
