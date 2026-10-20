//! Flash configuration persistence for RP2040 external QSPI Flash (2MB).
//!
//! Stores `AppConfig` in the last 4 KB sector (0x001F_F000 .. 0x0020_0000),
//! reserved outside linker memory in `memory.x` (FLASH length set to 2044K - 0x100).

use defmt::{info, warn};
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use sacn_core::config::{AppConfig, CONFIG_BLOCK_SIZE};

pub const FLASH_SIZE: usize = 2 * 1024 * 1024;
pub const CONFIG_FLASH_OFFSET: u32 = 0x001F_F000; // Last 4KB sector starts at 2044 KB
pub const CONFIG_SECTOR_END: u32 = 0x0020_0000;   // Ends at 2048 KB (2MB)

pub static FLASH_MUTEX: Mutex<CriticalSectionRawMutex, Option<Flash<'static, FLASH, Blocking, FLASH_SIZE>>> =
    Mutex::new(None);

/// Initialize the Flash driver and store it in `FLASH_MUTEX`.
pub async fn init_flash(flash: Flash<'static, FLASH, Blocking, FLASH_SIZE>) {
    let mut lock = FLASH_MUTEX.lock().await;
    *lock = Some(flash);
}

/// Load configuration from the last 4KB flash sector.
///
/// Returns `Some(AppConfig)` if a valid record with matching magic, schema version,
/// and CRC32 is found. Returns `None` otherwise.
pub async fn load_config() -> Option<AppConfig> {
    let mut lock = FLASH_MUTEX.lock().await;
    let flash = lock.as_mut()?;

    let mut block = [0u8; CONFIG_BLOCK_SIZE];
    if let Err(e) = flash.blocking_read(CONFIG_FLASH_OFFSET, &mut block) {
        warn!("Failed to read flash config: {}", defmt::Debug2Format(&e));
        return None;
    }

    match AppConfig::decode(&block) {
        Some(cfg) => {
            info!("Loaded valid config from flash (hostname='{}')", cfg.hostname.as_str());
            Some(cfg)
        }
        None => {
            info!("No valid flash config found (first boot or erased); using default settings");
            None
        }
    }
}

/// Save configuration to the last 4KB flash sector.
///
/// Erases the 4KB sector and writes the 512-byte serialized configuration block.
pub async fn save_config(cfg: &AppConfig) -> Result<(), ()> {
    let mut lock = FLASH_MUTEX.lock().await;
    let flash = match lock.as_mut() {
        Some(f) => f,
        None => {
            warn!("Flash not initialized; cannot save config");
            return Err(());
        }
    };

    let mut block = [0u8; CONFIG_BLOCK_SIZE];
    cfg.encode(&mut block);

    info!("Erasing Flash Sector (0x{:08x}..0x{:08x})...", CONFIG_FLASH_OFFSET, CONFIG_SECTOR_END);
    if let Err(e) = flash.blocking_erase(CONFIG_FLASH_OFFSET, CONFIG_SECTOR_END) {
        warn!("Flash erase failed: {}", defmt::Debug2Format(&e));
        return Err(());
    }

    info!("Writing config block (512 bytes) to flash offset 0x{:08x}...", CONFIG_FLASH_OFFSET);
    if let Err(e) = flash.blocking_write(CONFIG_FLASH_OFFSET, &block) {
        warn!("Flash write failed: {}", defmt::Debug2Format(&e));
        return Err(());
    }

    info!("Configuration successfully saved to flash!");
    Ok(())
}
