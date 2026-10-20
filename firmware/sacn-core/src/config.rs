//! Persistent configuration schema and binary serialization for acid.
//!
//! Stores:
//! - Global PWM frequency
//! - 8 channel settings (mode, DMX mapping, color order, loss config)
//! - Network settings (DHCP vs static IP, prefix, gateway)
//! - Hostname (.local)
//!
//! Stored in persistent flash storage as a 512-byte block with a 16-byte
//! header:
//! - [0..4]   "ACID" (magic)
//! - [4..6]   version (u16 LE, currently 1)
//! - [6..8]   payload_len (u16 LE)
//! - [8..12]  crc32 (u32 LE of payload bytes [16..16+payload_len])
//! - [12..16] reserved
//! - [16..16+len] payload
//! - [16+len..512] padding (0xFF)

use crate::channel::{ChannelMode, DmxSlot, GroupClass};
use crate::color::ColorOrder;
use crate::loss::{LossBehavior, LossConfig};

pub const GROUP_COUNT: usize = 4;
pub const CHANNELS_PER_GROUP: usize = 2;
pub const CHANNEL_COUNT: usize = 8;

pub const CONFIG_MAGIC: [u8; 4] = *b"ACID";
pub const CONFIG_VERSION: u16 = 1;
pub const CONFIG_BLOCK_SIZE: usize = 512;

/// Standard IEEE 802.3 CRC32 (Ethernet polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelSetting {
    pub mode: ChannelMode,
    pub order: ColorOrder,
    pub loss: LossConfig,
}

impl Default for ChannelSetting {
    fn default() -> Self {
        Self {
            mode: ChannelMode::Off,
            order: ColorOrder::Grb,
            loss: LossConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupSetting {
    pub class: GroupClass,
}

impl Default for GroupSetting {
    fn default() -> Self {
        Self {
            class: GroupClass::Pixel,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomHostname {
    pub bytes: [u8; 32],
    pub len: u8,
}

impl CustomHostname {
    pub const fn empty() -> Self {
        Self { bytes: [0; 32], len: 0 }
    }

    pub fn from_str(s: &str) -> Self {
        let mut bytes = [0u8; 32];
        let s = s.trim();
        let len = s.len().min(32);
        bytes[..len].copy_from_slice(&s.as_bytes()[..len]);
        Self { bytes, len: len as u8 }
    }

    pub fn as_str(&self) -> &str {
        if self.len == 0 {
            "acid"
        } else {
            core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("acid")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetConfig {
    pub dhcp: bool,
    pub static_ip: [u8; 4],
    pub static_prefix: u8,
    pub static_gateway: Option<[u8; 4]>,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            dhcp: true,
            static_ip: [169, 254, 246, 176],
            static_prefix: 16,
            static_gateway: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfig {
    pub pwm_freq_hz: u32,
    pub channels: [ChannelSetting; CHANNEL_COUNT],
    pub net: NetConfig,
    pub hostname: CustomHostname,
}

impl AppConfig {
    pub const fn const_default() -> Self {
        Self {
            pwm_freq_hz: crate::channel::DEFAULT_PWM_FREQ_HZ,
            channels: [
                ChannelSetting {
                    mode: ChannelMode::Fire { pixel_count: 30, control: None },
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
                ChannelSetting {
                    mode: ChannelMode::Off,
                    order: ColorOrder::Grb,
                    loss: LossConfig { timeout_ms: 2500, behavior: LossBehavior::Blackout },
                },
            ],
            net: NetConfig {
                dhcp: true,
                static_ip: [169, 254, 246, 176],
                static_prefix: 16,
                static_gateway: None,
            },
            hostname: CustomHostname::empty(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::const_default()
    }
}

impl AppConfig {
    /// Encode this configuration into a 512-byte flash block with header and CRC32.
    pub fn encode(&self, out: &mut [u8; CONFIG_BLOCK_SIZE]) {
        out.fill(0xFF);
        out[0..4].copy_from_slice(&CONFIG_MAGIC);
        out[4..6].copy_from_slice(&CONFIG_VERSION.to_le_bytes());

        let mut idx = 16;

        // 1. Global PWM Frequency
        out[idx..idx + 4].copy_from_slice(&self.pwm_freq_hz.to_le_bytes());
        idx += 4;

        // 2. Channels (8 channels)
        for ch in &self.channels {
            match ch.mode {
                ChannelMode::Off => {
                    out[idx] = 0;
                    out[idx + 1..idx + 9].fill(0);
                }
                ChannelMode::Ws2812 { universe, start_channel, pixel_count } => {
                    out[idx] = 1;
                    out[idx + 1..idx + 3].copy_from_slice(&universe.to_le_bytes());
                    out[idx + 3..idx + 5].copy_from_slice(&start_channel.to_le_bytes());
                    out[idx + 5..idx + 7].copy_from_slice(&pixel_count.to_le_bytes());
                    out[idx + 7..idx + 9].fill(0);
                }
                ChannelMode::Ws2812Fill { universe, start_channel, pixel_count } => {
                    out[idx] = 2;
                    out[idx + 1..idx + 3].copy_from_slice(&universe.to_le_bytes());
                    out[idx + 3..idx + 5].copy_from_slice(&start_channel.to_le_bytes());
                    out[idx + 5..idx + 7].copy_from_slice(&pixel_count.to_le_bytes());
                    out[idx + 7..idx + 9].fill(0);
                }
                ChannelMode::Fire { pixel_count, control } => {
                    out[idx] = 3;
                    out[idx + 1..idx + 3].copy_from_slice(&pixel_count.to_le_bytes());
                    if let Some(ctrl) = control {
                        out[idx + 3] = 1;
                        out[idx + 4..idx + 6].copy_from_slice(&ctrl.universe.to_le_bytes());
                        out[idx + 6..idx + 8].copy_from_slice(&ctrl.channel.to_le_bytes());
                        out[idx + 8] = 0;
                    } else {
                        out[idx + 3] = 0;
                        out[idx + 4..idx + 9].fill(0);
                    }
                }
                ChannelMode::Pwm { universe, dmx_channel } => {
                    out[idx] = 4;
                    out[idx + 1..idx + 3].copy_from_slice(&universe.to_le_bytes());
                    out[idx + 3..idx + 5].copy_from_slice(&dmx_channel.to_le_bytes());
                    out[idx + 5..idx + 9].fill(0);
                }
            }
            idx += 9;

            out[idx] = match ch.order {
                ColorOrder::Rgb => 0,
                ColorOrder::Rbg => 1,
                ColorOrder::Grb => 2,
                ColorOrder::Gbr => 3,
                ColorOrder::Brg => 4,
                ColorOrder::Bgr => 5,
            };
            idx += 1;

            match ch.loss.behavior {
                LossBehavior::Blackout => {
                    out[idx] = 0;
                    out[idx + 1..idx + 5].fill(0);
                }
                LossBehavior::Hold => {
                    out[idx] = 1;
                    out[idx + 1..idx + 5].fill(0);
                }
                LossBehavior::Fade { ms } => {
                    out[idx] = 2;
                    out[idx + 1..idx + 5].copy_from_slice(&ms.to_le_bytes());
                }
            }
            idx += 5;

            out[idx..idx + 4].copy_from_slice(&ch.loss.timeout_ms.to_le_bytes());
            idx += 4;
        }

        // 3. Network config
        out[idx] = if self.net.dhcp { 1 } else { 0 };
        idx += 1;
        out[idx..idx + 4].copy_from_slice(&self.net.static_ip);
        idx += 4;
        out[idx] = self.net.static_prefix;
        idx += 1;
        if let Some(gw) = self.net.static_gateway {
            out[idx] = 1;
            out[idx + 1..idx + 5].copy_from_slice(&gw);
        } else {
            out[idx] = 0;
            out[idx + 1..idx + 5].fill(0);
        }
        idx += 5;

        // 4. Hostname
        out[idx] = self.hostname.len;
        idx += 1;
        out[idx..idx + 32].copy_from_slice(&self.hostname.bytes);
        idx += 32;

        let payload_len = (idx - 16) as u16;
        out[6..8].copy_from_slice(&payload_len.to_le_bytes());
        let crc = crc32(&out[16..idx]);
        out[8..12].copy_from_slice(&crc.to_le_bytes());
        out[12..16].fill(0);
    }

    /// Decode a 512-byte flash block. Returns `None` if magic, version, or CRC32 fails.
    pub fn decode(block: &[u8; CONFIG_BLOCK_SIZE]) -> Option<Self> {
        if block[0..4] != CONFIG_MAGIC {
            return None;
        }
        let version = u16::from_le_bytes([block[4], block[5]]);
        if version != CONFIG_VERSION {
            return None;
        }
        let payload_len = u16::from_le_bytes([block[6], block[7]]) as usize;
        if payload_len == 0 || payload_len > (CONFIG_BLOCK_SIZE - 16) {
            return None;
        }
        let stored_crc = u32::from_le_bytes([block[8], block[9], block[10], block[11]]);
        let calc_crc = crc32(&block[16..16 + payload_len]);
        if stored_crc != calc_crc {
            return None;
        }

        let mut idx = 16;
        let freq = u32::from_le_bytes(block[idx..idx + 4].try_into().ok()?);
        idx += 4;
        let pwm_freq_hz = if (crate::channel::MIN_PWM_FREQ_HZ..=crate::channel::MAX_PWM_FREQ_HZ).contains(&freq) {
            freq
        } else {
            crate::channel::DEFAULT_PWM_FREQ_HZ
        };

        let mut channels = [ChannelSetting::default(); CHANNEL_COUNT];
        for i in 0..CHANNEL_COUNT {
            let mode_tag = block[idx];
            let mode = match mode_tag {
                1 => {
                    let u = u16::from_le_bytes(block[idx + 1..idx + 3].try_into().ok()?);
                    let d = u16::from_le_bytes(block[idx + 3..idx + 5].try_into().ok()?);
                    let p = u16::from_le_bytes(block[idx + 5..idx + 7].try_into().ok()?);
                    ChannelMode::Ws2812 {
                        universe: u,
                        start_channel: d,
                        pixel_count: p,
                    }
                }
                2 => {
                    let u = u16::from_le_bytes(block[idx + 1..idx + 3].try_into().ok()?);
                    let d = u16::from_le_bytes(block[idx + 3..idx + 5].try_into().ok()?);
                    let p = u16::from_le_bytes(block[idx + 5..idx + 7].try_into().ok()?);
                    ChannelMode::Ws2812Fill { universe: u, start_channel: d, pixel_count: p }
                }
                3 => {
                    let p = u16::from_le_bytes(block[idx + 1..idx + 3].try_into().ok()?);
                    let has_ctrl = block[idx + 3] != 0;
                    let control = if has_ctrl {
                        let cu = u16::from_le_bytes(block[idx + 4..idx + 6].try_into().ok()?);
                        let cc = u16::from_le_bytes(block[idx + 6..idx + 8].try_into().ok()?);
                        Some(DmxSlot { universe: cu, channel: cc })
                    } else {
                        None
                    };
                    ChannelMode::Fire { pixel_count: p, control }
                }
                4 => {
                    let u = u16::from_le_bytes(block[idx + 1..idx + 3].try_into().ok()?);
                    let d = u16::from_le_bytes(block[idx + 3..idx + 5].try_into().ok()?);
                    ChannelMode::Pwm { universe: u, dmx_channel: d }
                }
                _ => ChannelMode::Off,
            };
            idx += 9;

            let order = match block[idx] {
                0 => ColorOrder::Rgb,
                1 => ColorOrder::Rbg,
                2 => ColorOrder::Grb,
                3 => ColorOrder::Gbr,
                4 => ColorOrder::Brg,
                5 => ColorOrder::Bgr,
                _ => ColorOrder::Grb,
            };
            idx += 1;

            let behavior = match block[idx] {
                1 => LossBehavior::Hold,
                2 => {
                    let ms = u32::from_le_bytes(block[idx + 1..idx + 5].try_into().ok()?);
                    LossBehavior::Fade { ms }
                }
                _ => LossBehavior::Blackout,
            };
            idx += 5;

            let timeout_ms = u32::from_le_bytes(block[idx..idx + 4].try_into().ok()?);
            idx += 4;

            channels[i] = ChannelSetting {
                mode,
                order,
                loss: LossConfig { timeout_ms, behavior },
            };
        }

        let dhcp = block[idx] != 0;
        idx += 1;
        let static_ip: [u8; 4] = block[idx..idx + 4].try_into().ok()?;
        idx += 4;
        let static_prefix = block[idx];
        idx += 1;
        let has_gateway = block[idx] != 0;
        let static_gateway = if has_gateway {
            Some(block[idx + 1..idx + 5].try_into().ok()?)
        } else {
            None
        };
        idx += 5;

        let host_len = block[idx].min(32);
        idx += 1;
        let mut host_bytes = [0u8; 32];
        host_bytes.copy_from_slice(&block[idx..idx + 32]);

        Some(Self {
            pwm_freq_hz,
            channels,
            net: NetConfig {
                dhcp,
                static_ip,
                static_prefix,
                static_gateway,
            },
            hostname: CustomHostname { bytes: host_bytes, len: host_len },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_check() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn roundtrip_default_config() {
        let original = AppConfig::default();
        let mut block = [0u8; CONFIG_BLOCK_SIZE];
        original.encode(&mut block);

        assert_eq!(&block[0..4], b"ACID");
        assert_eq!(u16::from_le_bytes([block[4], block[5]]), CONFIG_VERSION);

        let decoded = AppConfig::decode(&block).expect("failed to decode valid block");
        assert_eq!(original, decoded);
    }

    #[test]
    fn roundtrip_custom_config() {
        let mut original = AppConfig::default();
        original.pwm_freq_hz = 2000;
        original.channels[1].mode = ChannelMode::Ws2812 {
            universe: 42,
            start_channel: 100,
            pixel_count: 50,
        };
        original.channels[2].mode = ChannelMode::Pwm {
            universe: 5,
            dmx_channel: 12,
        };
        original.channels[3].mode = ChannelMode::Fire {
            pixel_count: 60,
            control: Some(DmxSlot { universe: 10, channel: 1 }),
        };
        original.channels[3].loss.behavior = LossBehavior::Fade { ms: 500 };
        original.net.dhcp = false;
        original.net.static_ip = [192, 168, 1, 99];
        original.net.static_prefix = 24;
        original.net.static_gateway = Some([192, 168, 1, 1]);
        original.hostname = CustomHostname::from_str("my-stage-lights");

        let mut block = [0u8; CONFIG_BLOCK_SIZE];
        original.encode(&mut block);

        let decoded = AppConfig::decode(&block).expect("failed to decode valid block");
        assert_eq!(original, decoded);
        assert_eq!(decoded.hostname.as_str(), "my-stage-lights");
    }

    #[test]
    fn rejects_erased_flash() {
        let erased = [0xFFu8; CONFIG_BLOCK_SIZE];
        assert!(AppConfig::decode(&erased).is_none());
    }

    #[test]
    fn rejects_corrupted_crc() {
        let original = AppConfig::default();
        let mut block = [0u8; CONFIG_BLOCK_SIZE];
        original.encode(&mut block);

        // Corrupt one payload byte
        block[20] ^= 0x55;
        assert!(AppConfig::decode(&block).is_none());
    }

    #[test]
    fn rejects_wrong_magic() {
        let original = AppConfig::default();
        let mut block = [0u8; CONFIG_BLOCK_SIZE];
        original.encode(&mut block);

        block[0] = b'X';
        assert!(AppConfig::decode(&block).is_none());
    }

    #[test]
    fn rejects_wrong_version() {
        let original = AppConfig::default();
        let mut block = [0u8; CONFIG_BLOCK_SIZE];
        original.encode(&mut block);

        block[4] = 99;
        assert!(AppConfig::decode(&block).is_none());
    }
}
