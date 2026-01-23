use crate::core::models::Provider;

pub const CLAUDE_HEX: &str = "#F5A623";
pub const CODEX_HEX: &str = "#10A37F";

pub const CLAUDE_RGB: (u8, u8, u8) = (245, 166, 35);
pub const CODEX_RGB: (u8, u8, u8) = (16, 163, 127);

pub fn provider_hex(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => CLAUDE_HEX,
        Provider::Codex => CODEX_HEX,
    }
}

pub fn provider_rgb(provider: Provider) -> (u8, u8, u8) {
    match provider {
        Provider::Claude => CLAUDE_RGB,
        Provider::Codex => CODEX_RGB,
    }
}

pub fn muted_rgb(color: (u8, u8, u8)) -> (u8, u8, u8) {
    let (r, g, b) = color;
    (
        (r as f32 * 0.35) as u8,
        (g as f32 * 0.35) as u8,
        (b as f32 * 0.35) as u8,
    )
}
