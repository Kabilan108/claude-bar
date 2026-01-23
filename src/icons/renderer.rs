use crate::core::models::Provider;
use crate::ui::colors;

const ICON_SIZE: u32 = 22;
const BACKGROUND_ALPHA_DARK: u8 = 70;
const BACKGROUND_ALPHA_LIGHT: u8 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    Loading,
    Error,
    #[allow(dead_code)]
    Stale,
}

pub struct IconRenderer {
    size: u32,
}

impl IconRenderer {
    pub fn new() -> Self {
        Self { size: ICON_SIZE }
    }

    #[allow(dead_code)]
    pub fn with_size(size: u32) -> Self {
        Self { size }
    }

    pub fn render(
        &self,
        provider: Provider,
        primary: f64,
        secondary: f64,
        state: IconState,
        is_dark: bool,
    ) -> Vec<u8> {
        let width = self.size as usize;
        let height = self.size as usize;
        let mut pixels = vec![0u8; width * height * 4]; // RGBA

        let (r, g, b) = match state {
            IconState::Normal => colors::provider_rgb(provider),
            IconState::Loading => colors::provider_rgb(provider),
            IconState::Error => (128, 128, 128), // Gray
            IconState::Stale => (180, 180, 180), // Light gray
        };
        let muted = colors::muted_rgb((r, g, b));

        let background_alpha = if is_dark {
            BACKGROUND_ALPHA_DARK
        } else {
            BACKGROUND_ALPHA_LIGHT
        };
        let background_color = if is_dark {
            (240, 240, 240, background_alpha)
        } else {
            (0, 0, 0, background_alpha)
        };
        self.draw_rounded_rect(&mut pixels, width, height, 5.0, background_color);

        // Draw two horizontal bars
        let bar_height = (height as f64 * 0.35) as usize;
        let bar_gap = 2;
        let bar_width = width - 4;
        let bar_x = 2;

        // Primary bar (top)
        let primary_y = 2;
        let primary_fill = ((bar_width as f64) * primary.clamp(0.0, 1.0)) as usize;
        self.draw_bar(
            &mut pixels,
            width,
            bar_x,
            primary_y,
            bar_width,
            bar_height,
            primary_fill,
            (r, g, b),
            muted,
        );

        // Secondary bar (bottom)
        let secondary_y = primary_y + bar_height + bar_gap;
        let secondary_fill = ((bar_width as f64) * secondary.clamp(0.0, 1.0)) as usize;
        self.draw_bar(
            &mut pixels,
            width,
            bar_x,
            secondary_y,
            bar_width,
            bar_height,
            secondary_fill,
            (r, g, b),
            muted,
        );

        pixels
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bar(
        &self,
        pixels: &mut [u8],
        stride: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        fill: usize,
        color: (u8, u8, u8),
        empty_color: (u8, u8, u8),
    ) {
        let (r, g, b) = color;
        let (er, eg, eb) = empty_color;

        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                let idx = (py * stride + px) * 4;

                if idx + 3 < pixels.len() {
                    if dx < fill {
                        // Filled portion
                        pixels[idx] = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b;
                        pixels[idx + 3] = 255;
                    } else {
                        // Empty portion (dimmed)
                        pixels[idx] = er;
                        pixels[idx + 1] = eg;
                        pixels[idx + 2] = eb;
                        pixels[idx + 3] = 140;
                    }
                }
            }
        }
    }

    fn draw_rounded_rect(
        &self,
        pixels: &mut [u8],
        width: usize,
        height: usize,
        radius: f32,
        color: (u8, u8, u8, u8),
    ) {
        let (r, g, b, a) = color;
        for y in 0..height {
            for x in 0..width {
                if !inside_rounded_rect(x, y, width, height, radius) {
                    continue;
                }
                let idx = (y * width + x) * 4;
                if idx + 3 < pixels.len() {
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                    pixels[idx + 3] = a;
                }
            }
        }
    }

    pub fn knight_rider_frame(phase: f64) -> (f64, f64) {
        use std::f64::consts::PI;
        let primary = 0.5 + 0.5 * phase.sin();
        let secondary = 0.5 + 0.5 * (phase + PI).sin();
        (primary, secondary)
    }
}

impl Default for IconRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn inside_rounded_rect(x: usize, y: usize, width: usize, height: usize, radius: f32) -> bool {
    let x = x as f32;
    let y = y as f32;
    let width = width as f32;
    let height = height as f32;
    let r = radius.max(0.0);

    if x >= r && x < width - r {
        return true;
    }
    if y >= r && y < height - r {
        return true;
    }

    let cx = if x < r { r } else { width - r };
    let cy = if y < r { r } else { height - r };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_produces_correct_size() {
        let renderer = IconRenderer::new();
        let pixels = renderer.render(Provider::Claude, 0.5, 0.5, IconState::Normal, false);
        assert_eq!(pixels.len(), 22 * 22 * 4);
    }

    #[test]
    fn test_knight_rider_animation() {
        let (p1, s1) = IconRenderer::knight_rider_frame(0.0);
        let (p2, s2) = IconRenderer::knight_rider_frame(std::f64::consts::PI);

        // At phase 0, primary should be at 0.5
        assert!((p1 - 0.5).abs() < 0.01);

        // At phase PI, values should be inverted
        assert!((p2 - 0.5).abs() < 0.01);
        assert!((s1 - s2).abs() < 0.01);
    }
}
