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
}
