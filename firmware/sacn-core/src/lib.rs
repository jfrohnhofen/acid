//! Hardware-independent core: sACN (E1.31) parsing, channel/output modeling,
//! and the local fire effect renderer. No MCU HAL dependency on
//! purpose, so all of it runs and is tested on the host with plain `cargo test`.

#![cfg_attr(not(test), no_std)]

pub mod channel;
pub mod color;
pub mod config;
pub mod encode;
pub mod fire;
pub mod identity;
pub mod loss;
pub mod sacn;
pub mod test_pattern;
pub mod web;

pub use color::{ColorOrder, Rgb8};
pub use test_pattern::TestPattern;

