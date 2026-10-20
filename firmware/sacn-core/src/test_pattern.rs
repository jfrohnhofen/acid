//! Hardware test patterns for WS2812 pixel strips and PWM dimmers.
//!
//! Provides pure pattern generators for verifying physical wiring, LED strips,
//! and MOSFET dimmers without needing an active sACN stream.

use crate::color::Rgb8;

/// Available test patterns for hardware output testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TestPattern {
    /// Normal operation: outputs driven by sACN stream and channel configurations.
    #[default]
    Off = 0,
    /// 100% full brightness: solid white for WS2812 pixels, 100% duty cycle for PWM dimmers.
    FullWhite = 1,
    /// Steps through Red -> Green -> Blue -> White every 1000ms.
    RgbCycle = 2,
    /// Smooth moving rainbow hue gradient across pixel strips.
    Rainbow = 3,
    /// Smooth breathing triangle pulse (0% -> 100% -> 0%) over 2000ms.
    Breathe = 4,
    /// Single white pixel tracing along the strip to easily verify pixel order and count.
    Tracer = 5,
}

impl TestPattern {
    /// Convert raw integer (from HTTP form or API) to a `TestPattern`.
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::FullWhite,
            2 => Self::RgbCycle,
            3 => Self::Rainbow,
            4 => Self::Breathe,
            5 => Self::Tracer,
            _ => Self::Off,
        }
    }

    /// Return integer ID for this pattern.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Human-readable label for UI select options.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Disabled (Normal sACN)",
            Self::FullWhite => "100% Full On (PWM & Pixels)",
            Self::RgbCycle => "Smooth Color Cycle",
            Self::Rainbow => "Rainbow Wave",
            Self::Breathe => "Breathe / Pulse (0-100%)",
            Self::Tracer => "Pixel Tracer",
        }
    }

    /// Check if this test pattern is applicable to the given channel mode.
    pub fn is_applicable_to(self, mode: &crate::channel::ChannelMode) -> bool {
        use crate::channel::ChannelMode;
        match mode {
            ChannelMode::Off => false,
            ChannelMode::Ws2812 { .. }
            | ChannelMode::Ws2812Fill { .. }
            | ChannelMode::Fire { .. } => true,
            ChannelMode::Pwm { .. } => matches!(self, Self::Off | Self::FullWhite | Self::Breathe),
        }
    }

    /// Pure rendering function for WS2812 pixel strip buffer.
    pub fn render_pixels(self, now_ms: u64, out: &mut [Rgb8]) {
        match self {
            Self::Off => {
                out.fill(Rgb8::BLACK);
            }
            Self::FullWhite => {
                out.fill(Rgb8::new(255, 255, 255));
            }
            Self::RgbCycle => {
                // Smooth continuous hue rotation over 6 seconds
                let hue = ((now_ms % 6000) * 256 / 6000) as u8;
                out.fill(color_wheel(hue));
            }
            Self::Rainbow => {
                let n = out.len().max(1);
                let offset = ((now_ms % 4000) * 256 / 4000) as u8;
                for (i, px) in out.iter_mut().enumerate() {
                    let hue = ((i * 256 / n) as u8).wrapping_add(offset);
                    *px = color_wheel(hue);
                }
            }
            Self::Breathe => {
                let cycle_ms = 2000;
                let t = (now_ms % cycle_ms) as u32;
                let duty = if t < (cycle_ms / 2) as u32 {
                    (t * 255 / (cycle_ms as u32 / 2)) as u8
                } else {
                    ((cycle_ms as u32 - t) * 255 / (cycle_ms as u32 / 2)) as u8
                };
                out.fill(Rgb8::new(duty, duty, duty));
            }
            Self::Tracer => {
                out.fill(Rgb8::BLACK);
                if !out.is_empty() {
                    let idx = ((now_ms / 35) as usize) % out.len();
                    out[idx] = Rgb8::new(255, 255, 255);
                }
            }
        }
    }

    /// Pure rendering function for raw wire bytes (3-byte RGB in GRB wire order).
    pub fn render_pixel_bytes(self, now_ms: u64, pixel_count: usize, out: &mut [u8]) -> usize {
        let bytes_needed = pixel_count * 3;
        let total = bytes_needed.min(out.len());
        let pixels = total / 3;

        match self {
            Self::Off => {
                out[..total].fill(0);
            }
            Self::FullWhite => {
                out[..total].fill(255);
            }
            Self::RgbCycle => {
                let hue = ((now_ms % 6000) * 256 / 6000) as u8;
                let rgb = color_wheel(hue);
                for chunk in out[..total].chunks_exact_mut(3) {
                    chunk[0] = rgb.g; // GRB wire order
                    chunk[1] = rgb.r;
                    chunk[2] = rgb.b;
                }
            }
            Self::Rainbow => {
                let n = pixels.max(1);
                let offset = ((now_ms % 4000) * 256 / 4000) as u8;
                for i in 0..pixels {
                    let hue = ((i * 256 / n) as u8).wrapping_add(offset);
                    let rgb = color_wheel(hue);
                    let base = i * 3;
                    out[base] = rgb.g; // GRB wire order
                    out[base + 1] = rgb.r;
                    out[base + 2] = rgb.b;
                }
            }
            Self::Breathe => {
                let cycle_ms = 2000;
                let t = (now_ms % cycle_ms) as u32;
                let duty = if t < (cycle_ms / 2) as u32 {
                    (t * 255 / (cycle_ms as u32 / 2)) as u8
                } else {
                    ((cycle_ms as u32 - t) * 255 / (cycle_ms as u32 / 2)) as u8
                };
                out[..total].fill(duty);
            }
            Self::Tracer => {
                out[..total].fill(0);
                if pixels > 0 {
                    let idx = ((now_ms / 35) as usize) % pixels;
                    let base = idx * 3;
                    if base + 2 < total {
                        out[base] = 255;     // G
                        out[base + 1] = 255; // R
                        out[base + 2] = 255; // B
                    }
                }
            }
        }
        total
    }

    /// Pure rendering function for PWM duty cycle (0..=255).
    pub fn render_pwm(self, now_ms: u64) -> u8 {
        match self {
            Self::Off => 0,
            Self::FullWhite => 255,
            Self::RgbCycle => {
                // Smooth 3s pulse for PWM dimmers
                let cycle_ms = 3000;
                let t = (now_ms % cycle_ms) as u32;
                if t < (cycle_ms / 2) as u32 {
                    (t * 255 / (cycle_ms as u32 / 2)) as u8
                } else {
                    ((cycle_ms as u32 - t) * 255 / (cycle_ms as u32 / 2)) as u8
                }
            }
            Self::Rainbow => 255,
            Self::Breathe => {
                let cycle_ms = 2000;
                let t = (now_ms % cycle_ms) as u32;
                if t < (cycle_ms / 2) as u32 {
                    (t * 255 / (cycle_ms as u32 / 2)) as u8
                } else {
                    ((cycle_ms as u32 - t) * 255 / (cycle_ms as u32 / 2)) as u8
                }
            }
            Self::Tracer => {
                if (now_ms % 1000) < 100 {
                    255
                } else {
                    0
                }
            }
        }
    }
}

/// Seamless 256-step color wheel with zero wrap discontinuities.
pub fn color_wheel(pos: u8) -> Rgb8 {
    if pos < 85 {
        let p = pos * 3;
        Rgb8::new(255 - p, p, 0)
    } else if pos < 170 {
        let p = (pos - 85) * 3;
        Rgb8::new(0, 255 - p, p)
    } else {
        let p = (pos - 170) * 3;
        Rgb8::new(p, 0, 255 - p)
    }
}

/// Fast HSV to RGB conversion alias for compatibility.
pub fn hsv_to_rgb(h: u8) -> Rgb8 {
    color_wheel(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_conversion() {
        assert_eq!(TestPattern::from_u8(0), TestPattern::Off);
        assert_eq!(TestPattern::from_u8(1), TestPattern::FullWhite);
        assert_eq!(TestPattern::from_u8(2), TestPattern::RgbCycle);
        assert_eq!(TestPattern::from_u8(3), TestPattern::Rainbow);
        assert_eq!(TestPattern::from_u8(4), TestPattern::Breathe);
        assert_eq!(TestPattern::from_u8(5), TestPattern::Tracer);
        assert_eq!(TestPattern::from_u8(99), TestPattern::Off);

        assert_eq!(TestPattern::FullWhite.as_u8(), 1);
        assert_eq!(TestPattern::Rainbow.as_u8(), 3);
        assert_eq!(TestPattern::Tracer.as_u8(), 5);
    }

    #[test]
    fn test_render_pixels_tracer() {
        let mut pixels = [Rgb8::BLACK; 10];
        TestPattern::Tracer.render_pixels(0, &mut pixels);
        assert_eq!(pixels[0], Rgb8::new(255, 255, 255));
        for px in &pixels[1..] {
            assert_eq!(*px, Rgb8::BLACK);
        }

        // After 35ms, pixel 1 is lit
        TestPattern::Tracer.render_pixels(35, &mut pixels);
        assert_eq!(pixels[0], Rgb8::BLACK);
        assert_eq!(pixels[1], Rgb8::new(255, 255, 255));
    }

    #[test]
    fn test_render_pixels_full_white() {
        let mut pixels = [Rgb8::BLACK; 10];
        TestPattern::FullWhite.render_pixels(0, &mut pixels);
        for px in pixels {
            assert_eq!(px, Rgb8::new(255, 255, 255));
        }
    }

    #[test]
    fn test_render_pixels_rgb_cycle() {
        let mut pixels = [Rgb8::BLACK; 5];

        // t=0: Pure Red (hue 0)
        TestPattern::RgbCycle.render_pixels(0, &mut pixels);
        assert_eq!(pixels[0], Rgb8::new(255, 0, 0));

        // t=2000ms (1/3 of 6s): Pure Green (hue 85)
        TestPattern::RgbCycle.render_pixels(2000, &mut pixels);
        assert_eq!(pixels[0], Rgb8::new(0, 255, 0));

        // t=4000ms (2/3 of 6s): Pure Blue (hue 170)
        TestPattern::RgbCycle.render_pixels(4000, &mut pixels);
        assert_eq!(pixels[0], Rgb8::new(0, 0, 255));
    }

    #[test]
    fn test_render_pixels_rainbow() {
        let mut pixels = [Rgb8::BLACK; 60];
        TestPattern::Rainbow.render_pixels(0, &mut pixels);
        // Ensure not all pixels are identical (it's a gradient)
        assert_ne!(pixels[0], pixels[30]);
    }

    #[test]
    fn test_render_pwm() {
        assert_eq!(TestPattern::Off.render_pwm(0), 0);
        assert_eq!(TestPattern::FullWhite.render_pwm(0), 255);
        assert_eq!(TestPattern::FullWhite.render_pwm(500), 255);

        // Breathe at t=0 is 0, at t=1000 is 255
        assert_eq!(TestPattern::Breathe.render_pwm(0), 0);
        assert_eq!(TestPattern::Breathe.render_pwm(1000), 255);
    }

    #[test]
    fn test_pattern_applicability() {
        use crate::channel::ChannelMode;

        let off = ChannelMode::Off;
        assert!(!TestPattern::FullWhite.is_applicable_to(&off));
        assert!(!TestPattern::Rainbow.is_applicable_to(&off));

        let ws = ChannelMode::Ws2812 { universe: 1, start_channel: 1, pixel_count: 30 };
        assert!(TestPattern::Rainbow.is_applicable_to(&ws));
        assert!(TestPattern::RgbCycle.is_applicable_to(&ws));
        assert!(TestPattern::FullWhite.is_applicable_to(&ws));
        assert!(TestPattern::Breathe.is_applicable_to(&ws));

        let fill = ChannelMode::Ws2812Fill { universe: 1, start_channel: 1, pixel_count: 30 };
        assert!(TestPattern::Rainbow.is_applicable_to(&fill));
        assert!(TestPattern::RgbCycle.is_applicable_to(&fill));
        assert!(TestPattern::FullWhite.is_applicable_to(&fill));
        assert!(TestPattern::Breathe.is_applicable_to(&fill));

        let fire = ChannelMode::Fire { pixel_count: 30, control: None };
        assert!(TestPattern::Rainbow.is_applicable_to(&fire));
        assert!(TestPattern::RgbCycle.is_applicable_to(&fire));
        assert!(TestPattern::Tracer.is_applicable_to(&fire));
        assert!(TestPattern::FullWhite.is_applicable_to(&fire));
        assert!(TestPattern::Breathe.is_applicable_to(&fire));

        let pwm = ChannelMode::Pwm { universe: 1, dmx_channel: 1 };
        assert!(!TestPattern::Rainbow.is_applicable_to(&pwm));
        assert!(!TestPattern::RgbCycle.is_applicable_to(&pwm));
        assert!(TestPattern::FullWhite.is_applicable_to(&pwm));
        assert!(TestPattern::Breathe.is_applicable_to(&pwm));
    }
}
