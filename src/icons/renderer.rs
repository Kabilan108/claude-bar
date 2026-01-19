use crate::core::models::Provider;

const ICON_SIZE: u32 = 22;
const BRAND_COLOR: (u8, u8, u8) = (245, 166, 35); // #F5A623

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
        _provider: Provider,
        primary: f64,
        secondary: f64,
        state: IconState,
    ) -> Vec<u8> {
        let width = self.size as usize;
        let height = self.size as usize;
        let mut pixels = vec![0u8; width * height * 4]; // RGBA

        let (r, g, b) = match state {
            IconState::Normal => BRAND_COLOR,
            IconState::Loading => BRAND_COLOR,
            IconState::Error => (128, 128, 128), // Gray
            IconState::Stale => (180, 180, 180), // Light gray
        };

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
    ) {
        let (r, g, b) = color;

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
                        pixels[idx] = r / 4;
                        pixels[idx + 1] = g / 4;
                        pixels[idx + 2] = b / 4;
                        pixels[idx + 3] = 128;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_produces_correct_size() {
        let renderer = IconRenderer::new();
        let pixels = renderer.render(Provider::Claude, 0.5, 0.5, IconState::Normal);
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
