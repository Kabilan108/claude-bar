use crate::core::models::Provider;
use crate::ui::colors;

pub fn css_for_provider(provider: Provider) -> String {
    let accent = colors::provider_hex(provider);
    format!(
        r#"
@define-color provider_accent {accent};

.popup-window {{
    background-color: @window_bg_color;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}}

.provider-switcher {{
    margin-bottom: 6px;
}}

.provider-tab {{
    padding: 4px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
}}

.provider-tab.selected {{
    border-bottom: 2px solid @provider_accent;
}}

.provider-tab-label {{
    font-size: 0.9em;
    font-weight: 600;
}}

.provider-dot {{
    border-radius: 999px;
}}

.provider-dot-claude {{
    background-color: #F5A623;
}}

.provider-dot-codex {{
    background-color: #10A37F;
}}

.subtitle {{
    font-size: 0.85em;
    color: @theme_unfocused_fg_color;
}}

.plan-badge {{
    font-size: 0.8em;
    padding: 2px 6px;
    border-radius: 6px;
    background-color: alpha(@provider_accent, 0.15);
    color: @theme_fg_color;
}}

.usage-label {{
    font-size: 0.95em;
    color: @theme_fg_color;
}}

.countdown-label {{
    font-size: 0.85em;
    color: @theme_unfocused_fg_color;
}}

.pace-label {{
    font-size: 0.8em;
    color: @theme_unfocused_fg_color;
}}

.cost-line {{
    font-size: 0.9em;
    color: @theme_fg_color;
}}

.cost-amount {{
    font-weight: 400;
    font-size: 0.85em;
    color: @provider_accent;
}}

.cost-period {{
    font-weight: 400;
    font-size: 0.9em;
    color: @theme_unfocused_fg_color;
}}

.cost-error {{
    color: @error_color;
    font-weight: 500;
}}

.footer-label {{
    font-size: 0.8em;
    color: @theme_unfocused_fg_color;
}}

.footer-actions {{
    margin-top: 4px;
}}

.footer-action {{
    padding: 4px 8px;
}}

.error-hint {{
    font-family: monospace;
    font-size: 0.85em;
    padding: 8px 12px;
    background-color: alpha(@error_color, 0.1);
    border-radius: 6px;
    border: 1px solid alpha(@error_color, 0.2);
}}

.error {{
    color: @error_color;
}}

.heading {{
    font-weight: 600;
    font-size: 1.0em;
}}

.title-3 {{
    font-weight: 700;
    font-size: 1.2em;
}}

.section-separator {{
    margin-top: 6px;
    margin-bottom: 6px;
}}

.provider-choice {{
    padding: 6px 8px;
}}
"#
    )
}
