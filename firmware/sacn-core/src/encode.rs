//! WS2812B bit encoding for timer burst-DMA output.
//!
//! One timer drives up to `CH` strips at once. On every timer update event
//! (one WS2812 bit slot, 1.25 us) the DMA writes one *row* of compare values,
//! one per channel, into the timer's CCR registers (see embassy's
//! `waveform_up_multi_channel`). So the output buffer is a flattened
//! `rows x CH` array: `out[row * CH + channel]`.
//!
//! A WS2812 bit is a pulse whose high time encodes the value: ~0.4 us for 0,
//! ~0.8 us for 1, inside a 1.25 us slot. Compare value = duty in timer counts.
//! A duty of 0 keeps the line low (used for padding and the reset gap).

use crate::color::{ColorOrder, Rgb8};

pub const BITS_PER_PIXEL: usize = 24;

/// Compare value for one WS2812 bit given the timer `period` in counts
/// (the timer's max duty, i.e. ARR + 1). Uses 32% for a 0 and 64% for a 1 of
/// the slot, which sits in the middle of the datasheet's +-150 ns tolerance.
/// Verify with a logic analyzer: cheap clones are fussier about timing.
pub fn bit_duty(bit: bool, period: u16) -> u16 {
    let p = period as u32;
    if bit {
        (p * 16 / 25) as u16
    } else {
        (p * 8 / 25) as u16
    }
}

/// Number of DMA rows for a frame whose longest strip has `max_pixels`
/// pixels, followed by `reset_rows` all-low rows (>= 50 us; 48 rows = 60 us).
pub const fn frame_rows(max_pixels: usize, reset_rows: usize) -> usize {
    max_pixels * BITS_PER_PIXEL + reset_rows
}

/// Number of DMA rows for a frame whose longest strip has `max_bytes` wire bytes,
/// followed by `reset_rows` all-low rows (60 us reset gap).
pub const fn frame_rows_for_bytes(max_bytes: usize, reset_rows: usize) -> usize {
    max_bytes * 8 + reset_rows
}

/// Encodes raw wire bytes for `CH` strips into `out`, returning the number of rows
/// written. Supports any byte length per channel (e.g. 3-byte RGB or 4-byte RGBW).
pub fn encode_frame_bytes<const CH: usize>(
    channels: [&[u8]; CH],
    period: u16,
    reset_rows: usize,
    out: &mut [u16],
) -> Option<usize> {
    let max_bytes = channels.iter().map(|c| c.len()).max().unwrap_or(0);
    let max_bits = max_bytes * 8;
    let rows = max_bits + reset_rows;
    let needed = rows * CH;
    if out.len() < needed {
        return None;
    }

    for bit_idx in 0..max_bits {
        let byte_pos = bit_idx / 8;
        let bit_shift = 7 - (bit_idx % 8);
        for ch in 0..CH {
            let duty = match channels[ch].get(byte_pos) {
                Some(&byte) => {
                    let bit = (byte >> bit_shift) & 1 == 1;
                    bit_duty(bit, period)
                }
                None => 0,
            };
            out[bit_idx * CH + ch] = duty;
        }
    }
    out[max_bits * CH..needed].fill(0);
    Some(rows)
}

/// Encodes one frame for `CH` strips into `out`, returning the number of rows
/// written (`out[..rows * CH]` is the DMA buffer). Returns `None` if `out` is
/// too small. Strips shorter than the longest are padded with low (duty 0), an
/// empty slice keeps that channel low for the whole frame.
pub fn encode_frame<const CH: usize>(
    channels: [&[Rgb8]; CH],
    orders: [ColorOrder; CH],
    period: u16,
    reset_rows: usize,
    out: &mut [u16],
) -> Option<usize> {
    let max_pixels = channels.iter().map(|c| c.len()).max().unwrap_or(0);
    let rows = frame_rows(max_pixels, reset_rows);
    let needed = rows * CH;
    if out.len() < needed {
        return None;
    }

    for pixel in 0..max_pixels {
        for ch in 0..CH {
            let bytes = channels[ch].get(pixel).map(|px| px.wire_bytes(orders[ch]));
            for bit_idx in 0..BITS_PER_PIXEL {
                let duty = match bytes {
                    Some(b) => {
                        let byte = b[bit_idx / 8];
                        let bit = (byte >> (7 - bit_idx % 8)) & 1 == 1;
                        bit_duty(bit, period)
                    }
                    None => 0,
                };
                out[(pixel * BITS_PER_PIXEL + bit_idx) * CH + ch] = duty;
            }
        }
    }
    out[max_pixels * BITS_PER_PIXEL * CH..needed].fill(0);
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: u16 = 125; // 100 MHz / 800 kHz
    const ZERO: u16 = 40; // 0.32 * 125
    const ONE: u16 = 80; // 0.64 * 125

    #[test]
    fn bit_duty_at_125_counts() {
        assert_eq!(bit_duty(false, PERIOD), ZERO);
        assert_eq!(bit_duty(true, PERIOD), ONE);
    }

    #[test]
    fn single_pixel_msb_first_in_wire_order() {
        // Grb order: wire bytes = [g, r, b] = [0x01, 0x80, 0x00].
        let px = [Rgb8::new(0x80, 0x01, 0x00)];
        let mut out = [0xFFFFu16; 24 + 4];
        let rows = encode_frame([&px[..]], [ColorOrder::Grb], PERIOD, 4, &mut out).unwrap();
        assert_eq!(rows, 28);

        // Byte 0 = 0x01: seven 0 bits then a 1 bit.
        assert!(out[0..7].iter().all(|&d| d == ZERO));
        assert_eq!(out[7], ONE);
        // Byte 1 = 0x80: a 1 bit then seven 0 bits.
        assert_eq!(out[8], ONE);
        assert!(out[9..16].iter().all(|&d| d == ZERO));
        // Byte 2 = 0x00: eight 0 bits.
        assert!(out[16..24].iter().all(|&d| d == ZERO));
        // Reset gap is low.
        assert!(out[24..28].iter().all(|&d| d == 0));
    }

    #[test]
    fn two_channels_are_interleaved_per_row() {
        let a = [Rgb8::new(0xFF, 0xFF, 0xFF)]; // all ones
        let b = [Rgb8::BLACK]; // all zeros
        let mut out = [0u16; 24 * 2 + 2 * 2];
        let rows = encode_frame([&a[..], &b[..]], [ColorOrder::Rgb; 2], PERIOD, 2, &mut out).unwrap();
        assert_eq!(rows, 26);
        for row in 0..24 {
            assert_eq!(out[row * 2], ONE, "ch0 row {row}");
            assert_eq!(out[row * 2 + 1], ZERO, "ch1 row {row}");
        }
        assert!(out[48..52].iter().all(|&d| d == 0));
    }

    #[test]
    fn shorter_strip_and_empty_channel_are_padded_low() {
        let long = [Rgb8::new(0xFF, 0xFF, 0xFF); 2];
        let short = [Rgb8::new(0xFF, 0xFF, 0xFF); 1];
        let empty: [Rgb8; 0] = [];
        let mut out = [0xFFFFu16; 2 * 24 * 3];
        let rows = encode_frame(
            [&long[..], &short[..], &empty[..]],
            [ColorOrder::Rgb; 3],
            PERIOD,
            0,
            &mut out,
        )
        .unwrap();
        assert_eq!(rows, 48);
        for row in 0..48 {
            assert_eq!(out[row * 3], ONE);
            // Short strip: pixel 0 (rows 0..24) lit, pixel 1 (rows 24..48) padded.
            let expect_short = if row < 24 { ONE } else { 0 };
            assert_eq!(out[row * 3 + 1], expect_short, "short row {row}");
            assert_eq!(out[row * 3 + 2], 0, "empty row {row}");
        }
    }

    #[test]
    fn all_channels_empty_gives_only_reset_rows() {
        let empty: [Rgb8; 0] = [];
        let mut out = [0xFFFFu16; 5 * 2];
        let rows = encode_frame([&empty[..], &empty[..]], [ColorOrder::Grb; 2], PERIOD, 5, &mut out).unwrap();
        assert_eq!(rows, 5);
        assert!(out.iter().all(|&d| d == 0));
    }

    #[test]
    fn too_small_buffer_is_rejected() {
        let px = [Rgb8::BLACK];
        let mut out = [0u16; 24 + 3]; // one row short of 24 + 4
        assert_eq!(encode_frame([&px[..]], [ColorOrder::Grb], PERIOD, 4, &mut out), None);
    }

    #[test]
    fn frame_rows_math() {
        assert_eq!(frame_rows(30, 48), 30 * 24 + 48);
        assert_eq!(frame_rows(0, 48), 48);
        assert_eq!(frame_rows_for_bytes(10, 48), 10 * 8 + 48);
    }

    #[test]
    fn encode_frame_bytes_interleaved_rgbw() {
        // Channel 0: 1 RGBW pixel (4 bytes: 0x80, 0x01, 0x00, 0xFF) = 32 bits
        let ch0 = [0x80, 0x01, 0x00, 0xFF];
        // Channel 1: 1 RGB pixel (3 bytes: 0xFF, 0x00, 0xAA) = 24 bits
        let ch1 = [0xFF, 0x00, 0xAA];
        let mut out = [0xFFFFu16; (32 + 4) * 2];
        let rows = encode_frame_bytes([&ch0[..], &ch1[..]], PERIOD, 4, &mut out).unwrap();
        assert_eq!(rows, 36);

        // Bit 0 of ch0: 0x80 -> 1 (ONE)
        assert_eq!(out[0 * 2], ONE);
        // Bit 0 of ch1: 0xFF -> 1 (ONE)
        assert_eq!(out[0 * 2 + 1], ONE);

        // Bit 1 of ch0: 0x80 -> 0 (ZERO)
        assert_eq!(out[1 * 2], ZERO);
        // Bit 1 of ch1: 0xFF -> 1 (ONE)
        assert_eq!(out[1 * 2 + 1], ONE);

        // Bit 24 of ch0: 0xFF (first bit of 4th byte) -> ONE
        assert_eq!(out[24 * 2], ONE);
        // Bit 24 of ch1: beyond 24 bits -> padded low (0)
        assert_eq!(out[24 * 2 + 1], 0);

        // Reset gap rows 32..36 are low for both channels
        for r in 32..36 {
            assert_eq!(out[r * 2], 0);
            assert_eq!(out[r * 2 + 1], 0);
        }
    }
}
