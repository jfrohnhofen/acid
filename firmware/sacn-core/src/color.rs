#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const BLACK: Rgb8 = Rgb8 { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Scales every component by `level / 255` (rounded to nearest).
    /// `255` leaves the color unchanged, `0` gives black.
    pub fn scaled(self, level: u8) -> Self {
        let s = |v: u8| ((v as u16 * level as u16 + 127) / 255) as u8;
        Self { r: s(self.r), g: s(self.g), b: s(self.b) }
    }

    /// The three bytes to clock out to a strip with the given color order,
    /// in transmission order.
    pub fn wire_bytes(self, order: ColorOrder) -> [u8; 3] {
        let Rgb8 { r, g, b } = self;
        match order {
            ColorOrder::Rgb => [r, g, b],
            ColorOrder::Rbg => [r, b, g],
            ColorOrder::Grb => [g, r, b],
            ColorOrder::Gbr => [g, b, r],
            ColorOrder::Brg => [b, r, g],
            ColorOrder::Bgr => [b, g, r],
        }
    }
}

/// Byte order a strip expects on its data line. sACN/DMX pixel data is always
/// R,G,B; this is a per-output setting that only affects the wire format.
/// WS2812B is `Grb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorOrder {
    Rgb,
    Rbg,
    #[default]
    Grb,
    Gbr,
    Brg,
    Bgr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_full_is_identity_and_zero_is_black() {
        let c = Rgb8::new(10, 128, 255);
        assert_eq!(c.scaled(255), c);
        assert_eq!(c.scaled(0), Rgb8::BLACK);
    }

    #[test]
    fn scaled_half_is_roughly_half() {
        assert_eq!(Rgb8::new(255, 100, 0).scaled(128), Rgb8::new(128, 50, 0));
    }

    #[test]
    fn wire_bytes_orders() {
        let c = Rgb8::new(1, 2, 3);
        assert_eq!(c.wire_bytes(ColorOrder::Rgb), [1, 2, 3]);
        assert_eq!(c.wire_bytes(ColorOrder::Grb), [2, 1, 3]);
        assert_eq!(c.wire_bytes(ColorOrder::Bgr), [3, 2, 1]);
        assert_eq!(c.wire_bytes(ColorOrder::Rbg), [1, 3, 2]);
        assert_eq!(c.wire_bytes(ColorOrder::Gbr), [2, 3, 1]);
        assert_eq!(c.wire_bytes(ColorOrder::Brg), [3, 1, 2]);
    }

    #[test]
    fn default_order_is_ws2812_grb() {
        assert_eq!(ColorOrder::default(), ColorOrder::Grb);
    }
}
