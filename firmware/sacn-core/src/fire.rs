//! Locally-rendered fire effect for a 1D WS2812B strip.
//!
//! Classic "heat diffusion + random sparking" algorithm, in the style of
//! FastLED's `Fire2012` by Mark Kriegsman. Runs entirely on-device with no
//! sACN input required (an optional sACN dimmer can scale brightness and
//! sparking, see `sparking_for_level`).

use crate::color::Rgb8;

/// How much each cell cools down per step, scaled against strip length.
/// Higher = shorter flames.
pub const DEFAULT_COOLING: u8 = 55;
/// Chance (out of 255) of a new spark at the base of the strip each step.
/// Higher = more roaring fire.
pub const DEFAULT_SPARKING: u8 = 120;

/// Small, fast, no_std PRNG (xorshift32). Not cryptographic - just needs to
/// look organic.
#[derive(Debug, Clone, Copy)]
struct Xorshift32(u32);

impl Xorshift32 {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform-ish value in `0..=max` (inclusive).
    fn next_up_to(&mut self, max: u32) -> u32 {
        if max == 0 {
            0
        } else {
            self.next_u32() % (max + 1)
        }
    }
}

pub struct Fire<const N: usize> {
    heat: [u8; N],
    rng: Xorshift32,
    pub cooling: u8,
    pub sparking: u8,
}

impl<const N: usize> Fire<N> {
    pub const fn pixel_count() -> usize {
        N
    }

    pub fn new(seed: u32) -> Self {
        Self {
            heat: [0; N],
            rng: Xorshift32::new(seed),
            cooling: DEFAULT_COOLING,
            sparking: DEFAULT_SPARKING,
        }
    }

    /// Advance the simulation by one step.
    pub fn step(&mut self) {
        if N == 0 {
            return;
        }

        // Step 1: cool down every cell a little.
        let cooldown_max = ((self.cooling as u32 * 10) / N as u32) + 2;
        for cell in self.heat.iter_mut() {
            let cooldown = self.rng.next_up_to(cooldown_max) as u8;
            *cell = cell.saturating_sub(cooldown);
        }

        // Step 2: heat drifts up and diffuses a little.
        for i in (2..N).rev() {
            self.heat[i] = ((self.heat[i - 1] as u16 + self.heat[i - 2] as u16 + self.heat[i - 2] as u16) / 3) as u8;
        }

        // Step 3: randomly ignite a new spark near the bottom.
        if self.rng.next_up_to(255) < self.sparking as u32 {
            let spark_range = if N >= 7 { 7 } else { N };
            let y = self.rng.next_up_to((spark_range - 1) as u32) as usize;
            let boost = 160 + self.rng.next_up_to(95) as u8;
            self.heat[y] = self.heat[y].saturating_add(boost);
        }
    }

    /// Render the current heat map into `out` (must be exactly `N` long).
    pub fn render(&self, out: &mut [Rgb8; N]) {
        for (cell, px) in self.heat.iter().zip(out.iter_mut()) {
            *px = heat_to_color(*cell);
        }
    }

    /// Access the raw heat values.
    pub fn heat(&self) -> &[u8; N] {
        &self.heat
    }
}

/// Sparking probability for a dimmer `level` (0..=255): `DEFAULT_SPARKING`
/// scaled by `level / 255`, so flames also shrink as the fire is dimmed down
/// (brightness itself is scaled separately with `Rgb8::scaled`).
pub fn sparking_for_level(level: u8) -> u8 {
    ((DEFAULT_SPARKING as u16 * level as u16 + 127) / 255) as u8
}

fn hash2d(x: u32, y: u32) -> u8 {
    let mut h = x.wrapping_mul(0x3747_6557) ^ y.wrapping_mul(0x68E3_1DA4);
    h = (h ^ (h >> 13)).wrapping_mul(0x27D4_EB2D);
    (h ^ (h >> 15)) as u8
}

/// 2D smooth integer Perlin/Value noise in range 0..=255.
pub fn inoise8(x: u32, y: u32) -> u8 {
    let gx = x >> 8;
    let gy = y >> 8;
    let fx = (x & 0xFF) as u32;
    let fy = (y & 0xFF) as u32;

    // Smoothstep: s(t) = 3*t^2 - 2*t^3
    let sx = (fx * fx * (768 - 2 * fx)) >> 16;
    let sy = (fy * fy * (768 - 2 * fy)) >> 16;

    let v00 = hash2d(gx, gy) as i32;
    let v10 = hash2d(gx.wrapping_add(1), gy) as i32;
    let v01 = hash2d(gx, gy.wrapping_add(1)) as i32;
    let v11 = hash2d(gx.wrapping_add(1), gy.wrapping_add(1)) as i32;

    let sx = sx as i32;
    let sy = sy as i32;

    let top = v00 + (((v10 - v00) * sx) >> 8);
    let bot = v01 + (((v11 - v01) * sx) >> 8);
    let val = top + (((bot - top) * sy) >> 8);

    val.clamp(0, 255) as u8
}

/// Quadratic ease-in-out curve for natural, organic acceleration.
pub fn quadratic_ease_in_out(p: u8) -> u8 {
    let p = p as u32;
    if p < 128 {
        ((2 * p * p) / 255).min(255) as u8
    } else {
        let inv = 255 - p;
        (255 - (2 * inv * inv) / 255).min(255) as u8
    }
}

/// Render procedural fire effect across all pixels in the strip into RGB pixels.
/// `dimmer`: 0..=255 brightness scaling.
/// `speed`: 0..=255 animation speed (128 = default natural speed).
/// `yellow_to_red`: 0..=255 color tint (0 = bright yellow/gold, 128 = balanced natural fire, 255 = deep crimson red).
pub fn render_fire_pixels(
    now_ms: u64,
    dimmer: u8,
    speed: u8,
    yellow_to_red: u8,
    out: &mut [Rgb8],
) {
    if dimmer == 0 {
        out.fill(Rgb8::BLACK);
        return;
    }

    let speed_mult = 30 + ((speed as u64) * 225) / 255;
    let time = (now_ms * speed_mult) / 128;
    let max_green = ((255 - yellow_to_red as u32) * 130) / 255;

    for (i, px) in out.iter_mut().enumerate() {
        let n = inoise8((i as u32).wrapping_mul(250), (time.wrapping_add(i as u64) / 5) as u32);
        let ni = inoise8((i as u32).wrapping_mul(500), (time.wrapping_add(i as u64) / 3) as u32);

        let v = quadratic_ease_in_out(n) as u32;
        let vi_raw = quadratic_ease_in_out(ni) as u32;

        let vi = 38 + (vi_raw * (255 - 38)) / 255;
        let red_base = 140 + (v * 115) / 255;
        let mut red = ((vi * red_base) / 255) as u8;
        let mut yellow = ((vi * ((v * max_green) / 255)) / 255) as u8;

        if dimmer < 255 {
            red = ((red as u16 * dimmer as u16) / 255) as u8;
            yellow = ((yellow as u16 * dimmer as u16) / 255) as u8;
        }

        *px = Rgb8::new(red, yellow, 0);
    }
}

/// Render procedural fire effect directly into raw wire bytes (GRB order for WS2812).
/// `dimmer`: 0..=255 brightness scaling.
/// `speed`: 0..=255 animation speed (128 = default natural speed).
/// `yellow_to_red`: 0..=255 color tint (0 = bright yellow/gold, 128 = balanced natural fire, 255 = deep crimson red).
pub fn render_fire_wire_bytes(
    now_ms: u64,
    pixel_count: usize,
    dimmer: u8,
    speed: u8,
    yellow_to_red: u8,
    out: &mut [u8],
) -> usize {
    let bytes_needed = pixel_count * 3;
    let total = bytes_needed.min(out.len());
    let pixels = total / 3;

    if dimmer == 0 {
        out[..total].fill(0);
        return total;
    }

    let speed_mult = 30 + ((speed as u64) * 225) / 255;
    let time = (now_ms * speed_mult) / 128;
    let max_green = ((255 - yellow_to_red as u32) * 130) / 255;

    for i in 0..pixels {
        let n = inoise8((i as u32).wrapping_mul(250), (time.wrapping_add(i as u64) / 5) as u32);
        let ni = inoise8((i as u32).wrapping_mul(500), (time.wrapping_add(i as u64) / 3) as u32);

        let v = quadratic_ease_in_out(n) as u32;
        let vi_raw = quadratic_ease_in_out(ni) as u32;

        let vi = 38 + (vi_raw * (255 - 38)) / 255;
        let red_base = 140 + (v * 115) / 255;
        let mut red = ((vi * red_base) / 255) as u8;
        let mut yellow = ((vi * ((v * max_green) / 255)) / 255) as u8;

        if dimmer < 255 {
            red = ((red as u16 * dimmer as u16) / 255) as u8;
            yellow = ((yellow as u16 * dimmer as u16) / 255) as u8;
        }

        let base = i * 3;
        out[base] = yellow;   // G on WS2812 wire
        out[base + 1] = red;  // R on WS2812 wire
        out[base + 2] = 0;    // B on WS2812 wire
    }

    total
}

/// Maps a heat value (0..=255) to a black -> red -> yellow -> white gradient,
/// matching FastLED's `HeatColor`.
pub fn heat_to_color(heat: u8) -> Rgb8 {
    let t192 = ((heat as u16 * 191) / 255) as u8;
    let heat_ramp = (t192 & 0x3F) << 2; // expand low 6 bits to 0..252

    if t192 > 128 {
        Rgb8::new(255, 255, heat_ramp)
    } else if t192 > 64 {
        Rgb8::new(255, heat_ramp, 0)
    } else {
        Rgb8::new(heat_ramp, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparking_scales_with_level() {
        assert_eq!(sparking_for_level(255), DEFAULT_SPARKING);
        assert_eq!(sparking_for_level(0), 0);
        assert!(sparking_for_level(128) < DEFAULT_SPARKING);
    }

    #[test]
    fn heat_to_color_is_black_at_zero() {
        assert_eq!(heat_to_color(0), Rgb8::new(0, 0, 0));
    }

    #[test]
    fn heat_to_color_is_bright_near_max() {
        let c = heat_to_color(255);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
    }

    #[test]
    fn simulation_runs_without_panicking_for_many_steps() {
        let mut fire: Fire<60> = Fire::new(12345);
        let mut out = [Rgb8::BLACK; 60];
        for _ in 0..1000 {
            fire.step();
            fire.render(&mut out);
        }
    }

    #[test]
    fn sparking_eventually_produces_nonzero_heat() {
        let mut fire: Fire<30> = Fire::new(1);
        let mut out = [Rgb8::BLACK; 30];
        for _ in 0..50 {
            fire.step();
        }
        fire.render(&mut out);
        assert!(out.iter().any(|px| *px != Rgb8::BLACK), "fire should have ignited by now");
    }

    #[test]
    fn zero_length_strip_does_not_panic() {
        let mut fire: Fire<0> = Fire::new(1);
        let mut out: [Rgb8; 0] = [];
        fire.step();
        fire.render(&mut out);
    }

    #[test]
    fn deterministic_given_same_seed() {
        let mut a: Fire<40> = Fire::new(999);
        let mut b: Fire<40> = Fire::new(999);
        let mut out_a = [Rgb8::BLACK; 40];
        let mut out_b = [Rgb8::BLACK; 40];
        for _ in 0..200 {
            a.step();
            b.step();
        }
        a.render(&mut out_a);
        b.render(&mut out_b);
        assert_eq!(out_a, out_b);
    }

    #[test]
    fn test_noise_fire_renders_all_pixels() {
        let mut pixels = [Rgb8::BLACK; 60];
        render_fire_pixels(1000, 255, 128, 128, &mut pixels);
        for (i, px) in pixels.iter().enumerate() {
            assert!(
                px.r > 0,
                "pixel {} should have non-zero red component, got {:?}",
                i, px
            );
        }
    }

    #[test]
    fn test_noise_fire_wire_bytes() {
        let mut wire = [0u8; 90];
        let written = render_fire_wire_bytes(2500, 30, 255, 128, 128, &mut wire);
        assert_eq!(written, 90);
        // Ensure every pixel has non-zero red (wire byte 1 for GRB)
        for i in 0..30 {
            let r = wire[i * 3 + 1];
            assert!(r > 0, "pixel {} wire red must be > 0", i);
        }
    }

    #[test]
    fn test_noise_fire_dimmer_zero_is_black() {
        let mut wire = [255u8; 30];
        let written = render_fire_wire_bytes(1000, 10, 0, 128, 128, &mut wire);
        assert_eq!(written, 30);
        assert!(wire.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_noise_fire_yellow_to_red_tint() {
        let mut yellow_wire = [0u8; 30];
        let mut red_wire = [0u8; 30];
        // yellow_to_red = 0 (yellow)
        render_fire_wire_bytes(1000, 10, 255, 128, 0, &mut yellow_wire);
        // yellow_to_red = 255 (red)
        render_fire_wire_bytes(1000, 10, 255, 128, 255, &mut red_wire);

        // At 255 (red), green (byte 0 in GRB) must be 0
        for i in 0..10 {
            assert_eq!(red_wire[i * 3], 0, "pixel {} green should be 0 for pure red", i);
        }
        // At 0 (yellow), green (byte 0 in GRB) must be greater than red_wire's green
        let total_green_yellow: u32 = (0..10).map(|i| yellow_wire[i * 3] as u32).sum();
        assert!(total_green_yellow > 0, "yellow mode should have prominent green component");
    }
}
