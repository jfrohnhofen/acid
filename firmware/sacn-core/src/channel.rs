//! Per-output-channel configuration and the pure functions that turn a
//! received DMX universe into pixel/duty data for one channel. Hardware-
//! agnostic: the firmware crate owns the actual GPIO/timer/DMA wiring.

use crate::color::Rgb8;

pub const DMX_UNIVERSE_SIZE: usize = 512;
/// Highest universe number usable by sACN.
pub const MAX_UNIVERSE: u16 = 63999;
/// Maximum pixels on one WS2812 output (up to 1000 pixels supported).
pub const MAX_PIXELS_PER_OUTPUT: u16 = 1000;
/// Allowed PWM frequency range for global PWM.
pub const MIN_PWM_FREQ_HZ: u32 = 100;
pub const MAX_PWM_FREQ_HZ: u32 = 20_000;
pub const DEFAULT_PWM_FREQ_HZ: u32 = 1000;

/// Validates that a PWM frequency is within the allowed hardware range (100 Hz to 20 kHz).
pub fn validate_pwm_freq_hz(freq_hz: u32) -> Result<(), ConfigError> {
    if (MIN_PWM_FREQ_HZ..=MAX_PWM_FREQ_HZ).contains(&freq_hz) {
        Ok(())
    } else {
        Err(ConfigError::PwmFrequencyOutOfRange)
    }
}

/// A single DMX slot: 1-based channel within a universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmxSlot {
    pub universe: u16,
    pub channel: u16,
}

/// How a group of outputs (one timer, 2 outputs) is clocked. Both outputs in
/// a group share the class, because they share the timer's frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupClass {
    /// 800 kHz timer for WS2812B; allows `Ws2812`, `Ws2812Fill`, `Fire`, `Off`.
    Pixel,
    /// Plain PWM at `freq_hz`; allows `Pwm`, `Off`.
    Pwm { freq_hz: u32 },
}

/// How a single physical output channel is driven. The board has 8 of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    /// Output held low; channel unused.
    Off,
    /// A single dimmable load (via a MOSFET gate), driven by one DMX slot.
    /// `dmx_channel` is 1-based (1..=512).
    Pwm { universe: u16, dmx_channel: u16 },
    /// A WS2812B string driven from sACN: 3 consecutive DMX slots
    /// per pixel (R, G, B), starting at `start_channel` (1-based).
    /// Can span multiple consecutive universes, skipping the `(513 - start) % 3`
    /// remaining channels at the end of each universe to avoid splitting pixels.
    Ws2812 {
        universe: u16,
        start_channel: u16,
        pixel_count: u16,
    },
    /// A WS2812B string showing ONE color on every pixel: a single R, G, B
    /// triple (3 DMX slots) starting at `start_channel` (1-based).
    Ws2812Fill { universe: u16, start_channel: u16, pixel_count: u16 },
    /// A WS2812B string driven by the local fire simulation. With
    /// `control = None` the fire is always on at full brightness; with a
    /// control slot, that DMX value is a dimmer (0 = off, 255 = full).
    Fire { pixel_count: u16, control: Option<DmxSlot> },
}

/// Why a configuration was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    UniverseOutOfRange,
    ChannelOutOfRange,
    /// The slots needed would run past channel 512 of the universe.
    RangeExceedsUniverse,
    PixelCountOutOfRange,
    /// The mode isn't allowed in the group's class (e.g. `Pwm` in a pixel group).
    ModeNotAllowedInClass,
    PwmFrequencyOutOfRange,
}

impl GroupClass {
    pub fn validate(&self) -> Result<(), ConfigError> {
        match *self {
            GroupClass::Pixel => Ok(()),
            GroupClass::Pwm { freq_hz } => validate_pwm_freq_hz(freq_hz),
        }
    }
}

/// A contiguous block of DMX slots read by an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmxRange {
    pub universe: u16,
    /// 1-based first slot.
    pub start: u16,
    pub len: u16,
}

/// Computes the span of universes [start_uni, end_uni] inclusive needed to receive
/// `pixel_count` RGB pixels starting at `start_channel` (1-based).
///
/// At each universe boundary, any remainder channels `(513 - current_ch) % 3`
/// are skipped so pixels never cross universe boundaries.
pub fn universes_spanned(
    start_universe: u16,
    start_channel: u16,
    pixel_count: u16,
) -> (u16, u16) {
    if pixel_count == 0 {
        return (start_universe, start_universe);
    }
    let bpp = 3usize;
    let mut cur_uni = start_universe as u32;
    let mut cur_ch = (start_channel as usize).clamp(1, DMX_UNIVERSE_SIZE);
    let mut remaining_pixels = pixel_count as usize;

    while remaining_pixels > 0 {
        let avail = DMX_UNIVERSE_SIZE + 1 - cur_ch;
        let rem = avail % bpp;
        let fit = (avail - rem) / bpp;
        if fit >= remaining_pixels {
            break;
        }
        remaining_pixels -= fit;
        cur_uni = cur_uni.saturating_add(1);
        cur_ch = 1;
    }

    let end_uni = cur_uni.min(MAX_UNIVERSE as u32) as u16;
    (start_universe, end_uni)
}

impl ChannelMode {
    /// Legacy compatibility helper: always returns true since channels are now fully independent.
    pub fn allowed_in(&self, _class: &GroupClass) -> bool {
        true
    }

    /// The DMX slots this mode reads, if single-universe.
    pub fn dmx_range(&self) -> Option<DmxRange> {
        match *self {
            ChannelMode::Off => None,
            ChannelMode::Pwm { universe, dmx_channel } => {
                Some(DmxRange { universe, start: dmx_channel, len: 1 })
            }
            ChannelMode::Ws2812 { universe, start_channel, pixel_count } => {
                Some(DmxRange {
                    universe,
                    start: start_channel,
                    len: pixel_count.saturating_mul(3),
                })
            }
            ChannelMode::Ws2812Fill { universe, start_channel, .. } => {
                Some(DmxRange { universe, start: start_channel, len: 3 })
            }
            ChannelMode::Fire { control, .. } => {
                control.map(|c| DmxRange { universe: c.universe, start: c.channel, len: 1 })
            }
        }
    }

    /// The start universe this mode needs to receive, if any.
    pub fn universe(&self) -> Option<u16> {
        match *self {
            ChannelMode::Off => None,
            ChannelMode::Pwm { universe, .. } => Some(universe),
            ChannelMode::Ws2812 { universe, .. } => Some(universe),
            ChannelMode::Ws2812Fill { universe, .. } => Some(universe),
            ChannelMode::Fire { control, .. } => control.map(|c| c.universe),
        }
    }

    /// Validates the mode configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        match *self {
            ChannelMode::Ws2812 { pixel_count, universe, start_channel } => {
                if pixel_count == 0 || pixel_count > MAX_PIXELS_PER_OUTPUT {
                    return Err(ConfigError::PixelCountOutOfRange);
                }
                if universe == 0 || universe > MAX_UNIVERSE {
                    return Err(ConfigError::UniverseOutOfRange);
                }
                if start_channel == 0 || start_channel as usize > DMX_UNIVERSE_SIZE {
                    return Err(ConfigError::ChannelOutOfRange);
                }
                let (_, end_uni) = universes_spanned(universe, start_channel, pixel_count);
                if end_uni > MAX_UNIVERSE {
                    return Err(ConfigError::UniverseOutOfRange);
                }
                return Ok(());
            }
            ChannelMode::Ws2812Fill { pixel_count, .. }
            | ChannelMode::Fire { pixel_count, .. }
                if pixel_count == 0 || pixel_count > MAX_PIXELS_PER_OUTPUT =>
            {
                return Err(ConfigError::PixelCountOutOfRange);
            }
            _ => {}
        }

        if let Some(range) = self.dmx_range() {
            if range.universe == 0 || range.universe > MAX_UNIVERSE {
                return Err(ConfigError::UniverseOutOfRange);
            }
            if range.start == 0 || range.start as usize > DMX_UNIVERSE_SIZE {
                return Err(ConfigError::ChannelOutOfRange);
            }
            if range.start as usize - 1 + range.len as usize > DMX_UNIVERSE_SIZE {
                return Err(ConfigError::RangeExceedsUniverse);
            }
        }
        Ok(())
    }
}

/// Extracts the PWM brightness (0..=255) for a `Pwm`-mode channel from its
/// universe's latest DMX data. Returns 0 if `dmx_channel` is out of range
/// (e.g. the source is only sending a shorter universe).
pub fn pwm_level_from_universe(universe_data: &[u8], dmx_channel: u16) -> u8 {
    let idx = dmx_channel.saturating_sub(1) as usize;
    universe_data.get(idx).copied().unwrap_or(0)
}

/// Brightness (0..=255) of a `Fire` output. No control slot means always on
/// (255); a control slot with no universe data yet reads as off (0).
pub fn fire_dimmer(control: Option<DmxSlot>, universe_data: Option<&[u8]>) -> u8 {
    match (control, universe_data) {
        (None, _) => 255,
        (Some(_), None) => 0,
        (Some(slot), Some(data)) => pwm_level_from_universe(data, slot.channel),
    }
}

/// Fills `out` with RGB pixel data for a `Ws2812`-mode channel from its
/// universe's latest DMX data (3 slots per pixel, R,G,B). Pixels beyond the
/// available DMX data are written as black, matching "no signal -> off".
pub fn ws2812_from_universe(universe_data: &[u8], start_channel: u16, out: &mut [Rgb8]) {
    ws2812_multi_universe(
        |_| Some(universe_data),
        1,
        start_channel,
        out,
    );
}

/// Fills `out` with RGB pixel data across multiple DMX universes.
///
/// Uses `get_universe(u)` to fetch the latest DMX payload for universe `u`.
/// For each universe:
/// - starts reading from `start_channel` (for `start_universe`) or channel 1 (subsequent universes).
/// - skips the trailing `(513 - current_channel) % 3` channels so pixels never
///   cross universe boundaries.
/// - if `get_universe` returns `None` for a universe, the pixels mapped to that universe are
///   written as black (`Rgb8::BLACK`).
pub fn ws2812_multi_universe<'a, F>(
    mut get_universe: F,
    start_universe: u16,
    start_channel: u16,
    out: &mut [Rgb8],
) where
    F: FnMut(u16) -> Option<&'a [u8]>,
{
    let bpp = 3usize;
    let mut cur_uni = start_universe;
    let mut cur_ch = (start_channel as usize).clamp(1, DMX_UNIVERSE_SIZE);
    let mut px_idx = 0;

    while px_idx < out.len() {
        let avail = DMX_UNIVERSE_SIZE + 1 - cur_ch;
        let rem = avail % bpp;
        let fit = (avail - rem) / bpp;
        let take = fit.min(out.len() - px_idx);

        let uni_data = get_universe(cur_uni);

        for i in 0..take {
            let base = cur_ch - 1 + i * bpp;
            let (r, g, b) = match uni_data {
                Some(data) => {
                    let r = data.get(base).copied().unwrap_or(0);
                    let g = data.get(base + 1).copied().unwrap_or(0);
                    let b = data.get(base + 2).copied().unwrap_or(0);
                    (r, g, b)
                }
                None => (0, 0, 0),
            };
            out[px_idx + i] = Rgb8::new(r, g, b);
        }

        px_idx += take;
        if px_idx >= out.len() {
            break;
        }

        cur_uni = cur_uni.saturating_add(1);
        cur_ch = 1;
    }
}

/// Fills `out` with raw wire bytes across multiple DMX universes (3 bytes per pixel RGB).
/// Returns the number of bytes written to `out`.
pub fn ws2812_multi_universe_bytes<'a, F>(
    mut get_universe: F,
    start_universe: u16,
    start_channel: u16,
    pixel_count: usize,
    out: &mut [u8],
) -> usize
where
    F: FnMut(u16) -> Option<&'a [u8]>,
{
    let bpp = 3usize;
    let mut cur_uni = start_universe;
    let mut cur_ch = (start_channel as usize).clamp(1, DMX_UNIVERSE_SIZE);
    let mut px_idx = 0;
    let mut byte_idx = 0;

    while px_idx < pixel_count {
        let avail = DMX_UNIVERSE_SIZE + 1 - cur_ch;
        let rem = avail % bpp;
        let fit = (avail - rem) / bpp;
        let take = fit.min(pixel_count - px_idx);

        let uni_data = get_universe(cur_uni);

        for i in 0..take {
            let base = cur_ch - 1 + i * bpp;
            for b in 0..bpp {
                let val = match uni_data {
                    Some(data) => data.get(base + b).copied().unwrap_or(0),
                    None => 0,
                };
                if byte_idx < out.len() {
                    out[byte_idx] = val;
                    byte_idx += 1;
                }
            }
        }

        px_idx += take;
        if px_idx >= pixel_count {
            break;
        }

        cur_uni = cur_uni.saturating_add(1);
        cur_ch = 1;
    }
    byte_idx
}

/// Fills `out` with 3 repeated color bytes (RGB) from `universe_data` starting at `start_channel`.
pub fn ws2812_fill_bytes(
    universe_data: &[u8],
    start_channel: u16,
    pixel_count: usize,
    out: &mut [u8],
) -> usize {
    let start_idx = (start_channel as usize).saturating_sub(1);
    let r = universe_data.get(start_idx).copied().unwrap_or(0);
    let g = universe_data.get(start_idx + 1).copied().unwrap_or(0);
    let b = universe_data.get(start_idx + 2).copied().unwrap_or(0);
    let color = [r, g, b];
    let total = (pixel_count * 3).min(out.len());
    for chunk in out[..total].chunks_exact_mut(3) {
        chunk.copy_from_slice(&color);
    }
    total
}

/// The single color for a `Ws2812Fill` channel: 3 slots (R,G,B) starting at
/// `start_channel`. Slots past the end of the data read as 0.
pub fn ws2812_fill_from_universe(universe_data: &[u8], start_channel: u16) -> Rgb8 {
    let mut px = [Rgb8::BLACK; 1];
    ws2812_from_universe(universe_data, start_channel, &mut px);
    px[0]
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn pwm_level_reads_correct_slot() {
        let mut data = [0u8; DMX_UNIVERSE_SIZE];
        data[4] = 200; // DMX channel 5 (1-based)
        assert_eq!(pwm_level_from_universe(&data, 5), 200);
    }

    #[test]
    fn pwm_level_out_of_range_is_zero() {
        let data = [0u8; 10];
        assert_eq!(pwm_level_from_universe(&data, 500), 0);
    }

    #[test]
    fn ws2812_extracts_rgb_triples_in_order() {
        let mut data = [0u8; DMX_UNIVERSE_SIZE];
        // Pixel 0 at channel 1..3 (R,G,B) = (10,20,30)
        data[0] = 10;
        data[1] = 20;
        data[2] = 30;
        // Pixel 1 at channel 4..6 = (40,50,60)
        data[3] = 40;
        data[4] = 50;
        data[5] = 60;

        let mut out = [Rgb8::BLACK; 2];
        ws2812_from_universe(&data, 1, &mut out);

        assert_eq!(out[0], Rgb8::new(10, 20, 30));
        assert_eq!(out[1], Rgb8::new(40, 50, 60));
    }

    #[test]
    fn ws2812_pixels_past_available_data_are_black() {
        let data = [0u8; 6]; // only one pixel's worth of data
        let mut out = [Rgb8::new(255, 255, 255); 3];
        ws2812_from_universe(&data, 1, &mut out);
        assert_eq!(out[0], Rgb8::BLACK);
        assert_eq!(out[1], Rgb8::BLACK);
        assert_eq!(out[2], Rgb8::BLACK);
    }

    #[test]
    fn ws2812_respects_start_channel_offset() {
        let mut data = [0u8; DMX_UNIVERSE_SIZE];
        data[9] = 7; // channel 10 -> R of pixel at start_channel=10
        data[10] = 8;
        data[11] = 9;

        let mut out = [Rgb8::BLACK; 1];
        ws2812_from_universe(&data, 10, &mut out);
        assert_eq!(out[0], Rgb8::new(7, 8, 9));
    }

    #[test]
    fn fill_reads_one_rgb_triple() {
        let mut data = [0u8; DMX_UNIVERSE_SIZE];
        data[19] = 1; // channel 20
        data[20] = 2;
        data[21] = 3;
        assert_eq!(ws2812_fill_from_universe(&data, 20), Rgb8::new(1, 2, 3));
    }

    #[test]
    fn fill_past_end_of_data_is_black() {
        let data = [9u8; 4];
        // channels 3..5: only slots 3 and 4 exist -> (9, 9, 0)
        assert_eq!(ws2812_fill_from_universe(&data, 3), Rgb8::new(9, 9, 0));
        assert_eq!(ws2812_fill_from_universe(&data, 400), Rgb8::BLACK);
    }

    #[test]
    fn fire_dimmer_without_control_is_full() {
        assert_eq!(fire_dimmer(None, None), 255);
        assert_eq!(fire_dimmer(None, Some(&[0u8; 4])), 255);
    }

    #[test]
    fn fire_dimmer_follows_control_slot() {
        let slot = Some(DmxSlot { universe: 1, channel: 3 });
        assert_eq!(fire_dimmer(slot, None), 0);
        assert_eq!(fire_dimmer(slot, Some(&[0, 0, 77, 0])), 77);
        assert_eq!(fire_dimmer(slot, Some(&[0, 0])), 0);
    }

    #[test]
    fn mode_independent_validity() {
        let pwm = ChannelMode::Pwm { universe: 1, dmx_channel: 1 };
        let ws = ChannelMode::Ws2812 { universe: 1, start_channel: 1, pixel_count: 10 };
        let fill = ChannelMode::Ws2812Fill { universe: 1, start_channel: 1, pixel_count: 10 };
        let fire = ChannelMode::Fire { pixel_count: 10, control: None };

        assert_eq!(ChannelMode::Off.validate(), Ok(()));
        assert_eq!(pwm.validate(), Ok(()));
        assert_eq!(ws.validate(), Ok(()));
        assert_eq!(fill.validate(), Ok(()));
        assert_eq!(fire.validate(), Ok(()));
    }

    #[test]
    fn dmx_ranges() {
        assert_eq!(ChannelMode::Off.dmx_range(), None);
        assert_eq!(
            ChannelMode::Pwm { universe: 2, dmx_channel: 7 }.dmx_range(),
            Some(DmxRange { universe: 2, start: 7, len: 1 })
        );
        assert_eq!(
            ChannelMode::Ws2812 { universe: 1, start_channel: 1, pixel_count: 100 }.dmx_range(),
            Some(DmxRange { universe: 1, start: 1, len: 300 })
        );
        assert_eq!(
            ChannelMode::Ws2812Fill { universe: 1, start_channel: 5, pixel_count: 100 }.dmx_range(),
            Some(DmxRange { universe: 1, start: 5, len: 3 })
        );
        assert_eq!(ChannelMode::Fire { pixel_count: 5, control: None }.dmx_range(), None);
        let fire = ChannelMode::Fire {
            pixel_count: 5,
            control: Some(DmxSlot { universe: 9, channel: 2 }),
        };
        assert_eq!(fire.dmx_range(), Some(DmxRange { universe: 9, start: 2, len: 1 }));
        assert_eq!(fire.universe(), Some(9));
    }

    #[test]
    fn universes_spanned_math() {
        // Universe 1, start channel 1, 170 pixels (3 bpp):
        // 512 / 3 = 170 pixels, rem = 2. Ends in Universe 1.
        assert_eq!(universes_spanned(1, 1, 170), (1, 1));
        // 171 pixels: 170 in Uni 1, remaining 1 in Uni 2 -> spans (1, 2)
        assert_eq!(universes_spanned(1, 1, 171), (1, 2));
        // 1000 pixels:
        // Uni 1: 170 px (510 slots, skips 511..512)
        // Uni 2: 170 px (340 cumulative)
        // Uni 3: 170 px (510 cumulative)
        // Uni 4: 170 px (680 cumulative)
        // Uni 5: 170 px (850 cumulative)
        // Uni 6: 150 px (1000 cumulative) -> (1, 6)
        assert_eq!(universes_spanned(1, 1, 1000), (1, 6));

        // Offset start channel: start at 510
        // avail = 513 - 510 = 3 channels -> exactly 1 pixel in Uni 1.
        assert_eq!(universes_spanned(1, 510, 1), (1, 1));
        assert_eq!(universes_spanned(1, 510, 2), (1, 2));
        // start at 511: avail = 513 - 511 = 2 channels. rem = 2 % 3 = 2, fit = 0!
        // 0 pixels in Uni 1, Uni 1 skips 511..512, Uni 2 gets pixel 1 -> spans (1, 2)
        assert_eq!(universes_spanned(1, 511, 1), (1, 2));
    }

    #[test]
    fn multi_universe_pixel_skipping() {
        let mut u1 = [0u8; DMX_UNIVERSE_SIZE];
        let mut u2 = [0u8; DMX_UNIVERSE_SIZE];

        // Uni 1: pixel 0 at ch 508..510 = (1, 2, 3)
        // Channels 511, 512 are unused/skipped remainder.
        u1[507] = 1;
        u1[508] = 2;
        u1[509] = 3;
        u1[510] = 99; // should be skipped
        u1[511] = 99; // should be skipped

        // Uni 2: pixel 1 at ch 1..3 = (4, 5, 6)
        u2[0] = 4;
        u2[1] = 5;
        u2[2] = 6;

        let mut out = [Rgb8::BLACK; 2];
        ws2812_multi_universe(
            |u| match u {
                1 => Some(&u1),
                2 => Some(&u2),
                _ => None,
            },
            1,
            508,
            &mut out,
        );

        assert_eq!(out[0], Rgb8::new(1, 2, 3));
        assert_eq!(out[1], Rgb8::new(4, 5, 6));
    }

    #[test]
    fn multi_universe_bytes_rgb() {
        let mut u1 = [0u8; DMX_UNIVERSE_SIZE];
        let mut u2 = [0u8; DMX_UNIVERSE_SIZE];

        // Uni 1: pixel 0 at ch 508..510 = (10, 20, 30)
        u1[507] = 10;
        u1[508] = 20;
        u1[509] = 30;

        // Uni 2: pixel 1 at ch 1..3 = (50, 60, 70)
        u2[0] = 50;
        u2[1] = 60;
        u2[2] = 70;

        let mut out = [0u8; 6];
        let bytes_written = ws2812_multi_universe_bytes(
            |u| match u {
                1 => Some(&u1),
                2 => Some(&u2),
                _ => None,
            },
            1,
            508,
            2,
            &mut out,
        );

        assert_eq!(bytes_written, 6);
        assert_eq!(out, [10, 20, 30, 50, 60, 70]);
    }

    #[test]
    fn validate_accepts_up_to_1000_pixels() {
        let ws = ChannelMode::Ws2812 { universe: 1, start_channel: 1, pixel_count: 1000 };
        assert_eq!(ws.validate(), Ok(()));

        let ws_overflow = ChannelMode::Ws2812 { universe: 1, start_channel: 1, pixel_count: 1001 };
        assert_eq!(ws_overflow.validate(), Err(ConfigError::PixelCountOutOfRange));
    }

    #[test]
    fn validate_rejects_bad_universe_and_channel() {
        let pwm = |u, c| ChannelMode::Pwm { universe: u, dmx_channel: c };
        assert_eq!(pwm(0, 1).validate(), Err(ConfigError::UniverseOutOfRange));
        assert_eq!(pwm(64000, 1).validate(), Err(ConfigError::UniverseOutOfRange));
        assert_eq!(pwm(63999, 512).validate(), Ok(()));
        assert_eq!(pwm(1, 0).validate(), Err(ConfigError::ChannelOutOfRange));
        assert_eq!(pwm(1, 513).validate(), Err(ConfigError::ChannelOutOfRange));
    }

    #[test]
    fn validate_checks_fire_control_slot() {
        let fire = |c| ChannelMode::Fire {
            pixel_count: 10,
            control: Some(DmxSlot { universe: 1, channel: c }),
        };
        assert_eq!(fire(1).validate(), Ok(()));
        assert_eq!(fire(0).validate(), Err(ConfigError::ChannelOutOfRange));
        let no_control = ChannelMode::Fire { pixel_count: 10, control: None };
        assert_eq!(no_control.validate(), Ok(()));
    }

    #[test]
    fn pwm_frequency_validation() {
        assert_eq!(validate_pwm_freq_hz(100), Ok(()));
        assert_eq!(validate_pwm_freq_hz(1000), Ok(()));
        assert_eq!(validate_pwm_freq_hz(20_000), Ok(()));
        assert_eq!(validate_pwm_freq_hz(50), Err(ConfigError::PwmFrequencyOutOfRange));
        assert_eq!(validate_pwm_freq_hz(100_000), Err(ConfigError::PwmFrequencyOutOfRange));
    }

    #[test]
    fn off_is_always_valid() {
        assert_eq!(ChannelMode::Off.validate(), Ok(()));
    }
}
