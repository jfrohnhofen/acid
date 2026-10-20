//! Shared firmware code used by the application and the bring-up test
//! binaries in `src/bin/`.

#![no_std]

pub mod clocks;
pub mod output;
pub mod rand;
pub mod state;
pub mod web;
pub mod flash;

