pub const BRAND_COLOR: &str = "#F5A623";

pub const CSS: &str = r#"
.usage-progress {
    min-height: 8px;
}

.usage-progress progress {
    background-color: #F5A623;
    border-radius: 4px;
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
    font-weight: bold;
}

.error-hint {
    font-family: monospace;
    font-size: 0.85em;
    padding: 8px;
    background-color: alpha(@error_color, 0.1);
    border-radius: 4px;
}
"#;
