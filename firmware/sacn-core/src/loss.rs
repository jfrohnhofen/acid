//! sACN source-loss handling: what an output does when its universe stops
//! receiving packets. Pure and hardware-independent; the firmware supplies
//! "milliseconds since the last frame" and applies the returned gain.
//!
//! The gain multiplies the output: PWM level, pixel colors (`Rgb8::scaled`),
//! or the fire dimmer.

/// E1.31 "network data loss" timeout.
pub const DEFAULT_LOSS_TIMEOUT_MS: u32 = 2500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossBehavior {
    /// Keep showing the last received data indefinitely.
    Hold,
    /// Go dark as soon as the timeout elapses.
    Blackout,
    /// Fade linearly to dark over `ms`, starting when the timeout elapses.
    /// `ms == 0` behaves like `Blackout`.
    Fade { ms: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LossConfig {
    pub timeout_ms: u32,
    pub behavior: LossBehavior,
}

impl Default for LossConfig {
    fn default() -> Self {
        Self { timeout_ms: DEFAULT_LOSS_TIMEOUT_MS, behavior: LossBehavior::Blackout }
    }
}

impl LossConfig {
    /// Output gain (0..=255) given the time since the universe last received
    /// a frame and whether the stream was marked terminated.
    ///
    /// If `ms_since_last_frame` is `None` (nothing was ever received), the gain is 0
    /// for all behaviors since there is no data to hold.
    ///
    /// If `terminated` is true (e.g. sACN Stream_Terminated packet received),
    /// loss processing triggers immediately without waiting for `timeout_ms`.
    /// When `self.behavior == LossBehavior::Hold`, `gain` returns 255 so the last
    /// received frame is held indefinitely.
    pub fn gain(&self, ms_since_last_frame: Option<u32>, terminated: bool) -> u8 {
        let Some(since) = ms_since_last_frame else {
            return 0;
        };
        if !terminated && since < self.timeout_ms {
            return 255;
        }
        match self.behavior {
            LossBehavior::Hold => 255,
            LossBehavior::Blackout => 0,
            LossBehavior::Fade { ms } => {
                let elapsed = if terminated {
                    since as u64
                } else {
                    (since - self.timeout_ms) as u64
                };
                let ms = ms as u64;
                if ms == 0 || elapsed >= ms {
                    0
                } else {
                    (255 - elapsed * 255 / ms) as u8
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(behavior: LossBehavior) -> LossConfig {
        LossConfig { timeout_ms: 1000, behavior }
    }

    #[test]
    fn full_gain_before_timeout_for_all_behaviors() {
        for b in [LossBehavior::Hold, LossBehavior::Blackout, LossBehavior::Fade { ms: 500 }] {
            assert_eq!(cfg(b).gain(Some(0), false), 255);
            assert_eq!(cfg(b).gain(Some(999), false), 255);
        }
    }

    #[test]
    fn never_received_is_dark_for_all_behaviors() {
        for b in [LossBehavior::Hold, LossBehavior::Blackout, LossBehavior::Fade { ms: 500 }] {
            assert_eq!(cfg(b).gain(None, false), 0);
            assert_eq!(cfg(b).gain(None, true), 0);
        }
    }

    #[test]
    fn hold_keeps_full_gain_forever() {
        assert_eq!(cfg(LossBehavior::Hold).gain(Some(1000), false), 255);
        assert_eq!(cfg(LossBehavior::Hold).gain(Some(u32::MAX), false), 255);
        // And when stream is terminated!
        assert_eq!(cfg(LossBehavior::Hold).gain(Some(0), true), 255);
        assert_eq!(cfg(LossBehavior::Hold).gain(Some(5000), true), 255);
    }

    #[test]
    fn blackout_goes_dark_at_timeout_and_immediately_on_termination() {
        assert_eq!(cfg(LossBehavior::Blackout).gain(Some(1000), false), 0);
        assert_eq!(cfg(LossBehavior::Blackout).gain(Some(5000), false), 0);
        assert_eq!(cfg(LossBehavior::Blackout).gain(Some(0), true), 0);
    }

    #[test]
    fn fade_ramps_down_after_timeout() {
        let c = cfg(LossBehavior::Fade { ms: 1000 });
        assert_eq!(c.gain(Some(1000), false), 255);
        assert_eq!(c.gain(Some(1500), false), 128);
        assert_eq!(c.gain(Some(1999), false), 1);
        assert_eq!(c.gain(Some(2000), false), 0);
        assert_eq!(c.gain(Some(9999), false), 0);
    }

    #[test]
    fn fade_ramps_down_immediately_on_termination() {
        let c = cfg(LossBehavior::Fade { ms: 1000 });
        assert_eq!(c.gain(Some(0), true), 255);
        assert_eq!(c.gain(Some(500), true), 128);
        assert_eq!(c.gain(Some(999), true), 1);
        assert_eq!(c.gain(Some(1000), true), 0);
    }

    #[test]
    fn fade_is_monotonic_non_increasing() {
        let c = cfg(LossBehavior::Fade { ms: 777 });
        let mut prev = 255u8;
        for t in (1000..2000).step_by(7) {
            let g = c.gain(Some(t), false);
            assert!(g <= prev, "gain rose at t={t}");
            prev = g;
        }
    }

    #[test]
    fn zero_length_fade_acts_like_blackout() {
        assert_eq!(cfg(LossBehavior::Fade { ms: 0 }).gain(Some(1000), false), 0);
        assert_eq!(cfg(LossBehavior::Fade { ms: 0 }).gain(Some(0), true), 0);
    }

    #[test]
    fn default_is_e131_timeout_with_blackout() {
        let d = LossConfig::default();
        assert_eq!(d.timeout_ms, 2500);
        assert_eq!(d.behavior, LossBehavior::Blackout);
    }
}
