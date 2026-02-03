use crate::core::models::Provider;
use crate::ui::colors;

pub fn css_for_provider(provider: Provider) -> String {
    let accent = colors::provider_hex(provider);
    format!(
        r#"
@define-color provider_accent {accent};

.popup-frame {{
    background-color: #242424;
    background-color: @window_bg_color;
    border-radius: 14px;
    border: 1px solid alpha(@theme_fg_color, 0.06);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.28), 0 2px 8px rgba(0, 0, 0, 0.12);
    padding: 4px;
}}

.provider-switcher {{
    margin-bottom: 12px;
    padding: 4px;
    background-color: alpha(@theme_fg_color, 0.04);
    border-radius: 10px;
}}

.provider-tab {{
    padding: 6px 12px;
    background: transparent;
    border: none;
    border-radius: 8px;
    transition: background-color 150ms ease;
}}

.provider-tab.selected {{
    background-color: alpha(@provider_accent, 0.12);
}}

.provider-tab-label {{
    font-size: 0.85em;
    font-weight: 500;
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
    font-size: 0.8em;
    font-weight: 400;
    color: @theme_unfocused_fg_color;
}}

.plan-badge {{
    font-size: 0.75em;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 8px;
    background-color: alpha(@theme_fg_color, 0.06);
    color: @theme_unfocused_fg_color;
}}

.usage-label {{
    font-size: 0.9em;
    font-weight: 500;
    color: @theme_fg_color;
}}

.countdown-label {{
    font-size: 0.8em;
    font-weight: 400;
    color: alpha(@theme_unfocused_fg_color, 0.7);
}}

.pace-label {{
    font-size: 0.78em;
    font-weight: 400;
    color: alpha(@theme_unfocused_fg_color, 0.7);
    margin-top: 2px;
}}

.cost-line {{
    font-size: 0.85em;
    font-weight: 400;
    color: @theme_fg_color;
}}

.cost-amount {{
    font-weight: 500;
    font-size: 0.85em;
    color: @theme_fg_color;
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

.header-updated {{
    font-size: 0.75em;
    font-weight: 400;
    color: alpha(@theme_unfocused_fg_color, 0.5);
}}

.footer-label {{
    font-size: 0.78em;
    font-weight: 400;
    color: alpha(@theme_unfocused_fg_color, 0.6);
}}

.footer-actions {{
    margin-top: 8px;
    padding-top: 4px;
}}

.footer-action {{
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: 8px;
    font-size: 0.88em;
    font-weight: 400;
    color: @theme_fg_color;
    transition: background-color 150ms ease;
}}

.footer-action:hover {{
    background-color: alpha(@theme_fg_color, 0.06);
}}

.version-footer {{
    font-size: 0.72em;
    font-weight: 400;
    color: alpha(@theme_unfocused_fg_color, 0.4);
    margin-top: 8px;
    margin-bottom: 2px;
}}

.error-hint {{
    font-family: monospace;
    font-size: 0.82em;
    padding: 10px 14px;
    background-color: alpha(@error_color, 0.08);
    border-radius: 8px;
    border: 1px solid alpha(@error_color, 0.15);
}}

.error {{
    color: @error_color;
}}

.heading {{
    font-weight: 500;
    font-size: 0.85em;
    color: @theme_unfocused_fg_color;
    text-transform: uppercase;
    letter-spacing: 0.03em;
}}

.title-3 {{
    font-weight: 600;
    font-size: 1.35em;
}}

.section-separator {{
    margin-top: 10px;
    margin-bottom: 10px;
    opacity: 0.3;
}}

.usage-progress-bar {{
    margin-top: 4px;
    margin-bottom: 2px;
}}

.provider-choice {{
    padding: 6px 8px;
}}
"#
    )
}
