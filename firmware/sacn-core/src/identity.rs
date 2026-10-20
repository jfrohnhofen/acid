//! Device identity derived from unique chip/flash ID: MAC address,
//! hostname and link-local fallback IP. Pure and hardware-independent; the
//! firmware passes in unique ID bytes.
//!
//! The UID is *not* used raw: its bytes encode lot / wafer / x-y position, so
//! boards from the same batch would otherwise share long prefixes. It is
//! hashed first (FNV-1a followed by a splitmix64 finalizer).

/// Length of the generated hostname: `"acid-"` + 6 hex digits.
pub const HOSTNAME_LEN: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Locally administered, unicast MAC (first byte `x2`, `x6`, `xA` or `xE`).
    pub mac: [u8; 6],
    hostname: [u8; HOSTNAME_LEN],
    /// Link-local `169.254.x.y` address used when DHCP fails.
    pub fallback_ip: [u8; 4],
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// splitmix64 finalizer: spreads small input differences over all bits.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Identity {
    pub fn from_uid(uid: &[u8]) -> Self {
        let h1 = mix(fnv1a64(uid));
        let h2 = mix(h1 ^ 0x9e37_79b9_7f4a_7c15);

        let b = h1.to_le_bytes();
        // Clear the multicast bit, set the locally-administered bit.
        let mac = [(b[0] & 0xFC) | 0x02, b[1], b[2], b[3], b[4], b[5]];

        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut hostname = *b"acid-000000";
        for (i, byte) in mac[3..6].iter().enumerate() {
            hostname[5 + i * 2] = HEX[(byte >> 4) as usize];
            hostname[6 + i * 2] = HEX[(byte & 0x0F) as usize];
        }

        let c = h2.to_le_bytes();
        // 169.254.1.0 ..= 169.254.254.255 (RFC 3927 reserves .0.x and .255.x).
        let fallback_ip = [169, 254, 1 + (c[0] % 254), c[1]];

        Self { mac, hostname, fallback_ip }
    }

    /// The hostname, e.g. `acid-a1b2c3` (reachable as `acid-a1b2c3.local`).
    pub fn hostname(&self) -> &str {
        // Always ASCII by construction.
        core::str::from_utf8(&self.hostname).unwrap_or("acid-device")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UID_A: [u8; 12] = [0x12, 0x00, 0x34, 0x00, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22];

    #[test]
    fn deterministic() {
        assert_eq!(Identity::from_uid(&UID_A), Identity::from_uid(&UID_A));
    }

    #[test]
    fn mac_is_unicast_and_locally_administered() {
        for seed in 0u8..=255 {
            let mut uid = UID_A;
            uid[11] = seed;
            let id = Identity::from_uid(&uid);
            assert_eq!(id.mac[0] & 0x01, 0, "multicast bit set for seed {seed}");
            assert_eq!(id.mac[0] & 0x02, 0x02, "LAA bit clear for seed {seed}");
        }
    }

    #[test]
    fn hostname_matches_last_three_mac_bytes() {
        let id = Identity::from_uid(&UID_A);
        let h = id.hostname();
        assert_eq!(h.len(), HOSTNAME_LEN);
        assert!(h.starts_with("acid-"));
        let expect = [id.mac[3], id.mac[4], id.mac[5]];
        let mut parsed = [0u8; 3];
        for i in 0..3 {
            parsed[i] = u8::from_str_radix(&h[5 + i * 2..7 + i * 2], 16).unwrap();
        }
        assert_eq!(parsed, expect);
        assert!(h[5..].chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn fallback_ip_is_valid_link_local() {
        for seed in 0u8..=255 {
            let mut uid = UID_A;
            uid[0] = seed;
            let ip = Identity::from_uid(&uid).fallback_ip;
            assert_eq!(ip[0], 169);
            assert_eq!(ip[1], 254);
            assert!((1..=254).contains(&ip[2]), "third octet {} out of range", ip[2]);
        }
    }

    #[test]
    fn one_bit_change_changes_everything() {
        let a = Identity::from_uid(&UID_A);
        let mut uid = UID_A;
        uid[5] ^= 0x01;
        let b = Identity::from_uid(&uid);
        assert_ne!(a.mac, b.mac);
        assert_ne!(a.hostname(), b.hostname());
        // Strong mixing: most MAC bytes should differ, not just the last.
        let differing = a.mac.iter().zip(b.mac.iter()).filter(|(x, y)| x != y).count();
        assert!(differing >= 4, "only {differing} MAC bytes differ");
    }

    #[test]
    fn similar_uids_do_not_collide() {
        // Boards from the same lot differ only in a few low bytes.
        let mut macs = std::collections::HashSet::new();
        for i in 0u16..=1000 {
            let mut uid = UID_A;
            uid[10] = (i >> 8) as u8;
            uid[11] = i as u8;
            assert!(macs.insert(Identity::from_uid(&uid).mac), "MAC collision at {i}");
        }
    }
}
