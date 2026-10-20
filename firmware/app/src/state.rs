//! Shared runtime state: per-group and per-channel configuration and the
//! universe store (holding received DMX data with timestamps for loss tracking).

use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_time::Instant;
use sacn_core::channel::DMX_UNIVERSE_SIZE;

pub use sacn_core::config::{
    AppConfig, ChannelSetting, CustomHostname, GroupSetting, NetConfig, CHANNEL_COUNT,
    CHANNELS_PER_GROUP, GROUP_COUNT,
};

pub const MAX_TRACKED_UNIVERSES: usize = 32;

#[derive(Clone, Copy)]
pub struct NetUpdateCmd {
    pub dhcp: bool,
    pub static_ip: [u8; 4],
    pub static_prefix: u8,
    pub static_gateway: Option<[u8; 4]>,
    pub hostname: CustomHostname,
}

pub static NET_UPDATE_CHANNEL: embassy_sync::channel::Channel<
    CriticalSectionRawMutex,
    NetUpdateCmd,
    2,
> = embassy_sync::channel::Channel::new();

pub static APP_CONFIG: AsyncMutex<CriticalSectionRawMutex, AppConfig> =
    AsyncMutex::new(AppConfig::const_default());


#[derive(Clone, Copy)]
pub struct UniverseSlot {
    pub universe: u16,
    pub data: [u8; DMX_UNIVERSE_SIZE],
    pub sequence_number: u8,
    pub packets_received: u32,
    pub last_received: Option<Instant>,
    pub terminated: bool,
}

pub struct UniverseStore {
    slots: [Option<UniverseSlot>; MAX_TRACKED_UNIVERSES],
}

impl UniverseStore {
    pub const fn new() -> Self {
        Self {
            slots: [None; MAX_TRACKED_UNIVERSES],
        }
    }

    /// Update universe data.
    pub fn update(&mut self, universe: u16, sequence_number: u8, data: &[u8], now: Instant) {
        if let Some(pos) = self
            .slots
            .iter()
            .position(|s| matches!(s, Some(slot) if slot.universe == universe))
        {
            let slot = self.slots[pos].as_mut().unwrap();
            slot.data.fill(0);
            let len = data.len().min(DMX_UNIVERSE_SIZE);
            slot.data[..len].copy_from_slice(&data[..len]);
            slot.sequence_number = sequence_number;
            slot.packets_received = slot.packets_received.wrapping_add(1);
            slot.last_received = Some(now);
            slot.terminated = false;
            return;
        }

        // Find empty slot or evict the least recently updated
        let target_pos = if let Some(empty) = self.slots.iter().position(|s| s.is_none()) {
            empty
        } else {
            // Find slot with oldest last_received
            let mut oldest_idx = 0;
            let mut oldest_time = Instant::MAX;
            for (idx, slot) in self.slots.iter().enumerate() {
                if let Some(s) = slot {
                    if let Some(t) = s.last_received {
                        if t < oldest_time {
                            oldest_time = t;
                            oldest_idx = idx;
                        }
                    } else {
                        oldest_idx = idx;
                        break;
                    }
                }
            }
            oldest_idx
        };

        let mut new_data = [0u8; DMX_UNIVERSE_SIZE];
        let len = data.len().min(DMX_UNIVERSE_SIZE);
        new_data[..len].copy_from_slice(&data[..len]);
        self.slots[target_pos] = Some(UniverseSlot {
            universe,
            data: new_data,
            sequence_number,
            packets_received: 1,
            last_received: Some(now),
            terminated: false,
        });
    }

    pub fn mark_terminated(&mut self, universe: u16) {
        if let Some(slot) = self.slots.iter_mut().flatten().find(|s| s.universe == universe) {
            slot.terminated = true;
        }
    }

    pub fn get(&self, universe: u16) -> Option<([u8; DMX_UNIVERSE_SIZE], Option<Instant>, bool)> {
        self.slots
            .iter()
            .flatten()
            .find(|s| s.universe == universe)
            .map(|s| (s.data, s.last_received, s.terminated))
    }

    pub fn get_data(&self, universe: u16) -> Option<&[u8]> {
        self.slots
            .iter()
            .flatten()
            .find(|s| s.universe == universe)
            .map(|s| s.data.as_slice())
    }

    pub fn status(&self) -> impl Iterator<Item = &UniverseSlot> + '_ {
        self.slots.iter().flatten()
    }

    pub fn get_info(&self) -> heapless::Vec<UniverseInfo, MAX_TRACKED_UNIVERSES> {
        let mut list = heapless::Vec::new();
        for slot in self.slots.iter().flatten() {
            let _ = list.push(UniverseInfo {
                universe: slot.universe,
                sequence_number: slot.sequence_number,
                packets_received: slot.packets_received,
                last_received: slot.last_received,
                terminated: slot.terminated,
            });
        }
        list
    }
}

#[derive(Clone, Copy)]
pub struct UniverseInfo {
    pub universe: u16,
    pub sequence_number: u8,
    pub packets_received: u32,
    pub last_received: Option<Instant>,
    pub terminated: bool,
}


pub static UNIVERSE_STORE: BlockingMutex<CriticalSectionRawMutex, RefCell<UniverseStore>> =
    BlockingMutex::new(RefCell::new(UniverseStore::new()));

#[derive(Clone, Copy, Default)]
pub struct NetStatus {
    pub link_up: bool,
    pub ip: Option<([u8; 4], u8)>,
    pub mac: [u8; 6],
    pub is_fallback: bool,
}

pub static NET_STATUS: BlockingMutex<CriticalSectionRawMutex, core::cell::Cell<NetStatus>> =
    BlockingMutex::new(core::cell::Cell::new(NetStatus {
        link_up: false,
        ip: None,
        mac: [0; 6],
        is_fallback: false,
    }));

/// Per-channel runtime test mode overrides (0 = Off / normal sACN, 1..=4 = test patterns).
pub static CHANNEL_TEST_MODES: [core::sync::atomic::AtomicU8; CHANNEL_COUNT] = [
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
];

/// Live pixel count override for active test mode (0 = follow channel config).
pub static CHANNEL_TEST_PIXELS: [core::sync::atomic::AtomicU16; CHANNEL_COUNT] = [
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
    core::sync::atomic::AtomicU16::new(0),
];



