//! System clock configuration for Raspberry Pi Pico (RP2040).

use embassy_rp::config::Config;

/// Default 125 MHz sysclk from 12 MHz crystal oscillator on Raspberry Pi Pico.
pub fn config() -> Config {
    Config::default()
}
