//! Drives the 8 output channels on Raspberry Pi Pico (RP2040).
//!
//! Pin mapping (West-to-East ordering matching PCB IDC headers):
//! - Channel 0 (Ch 1): GP15 (PWM7 B, PIO0 SM0)
//! - Channel 1 (Ch 2): GP14 (PWM7 A, PIO0 SM1)
//! - Channel 2 (Ch 3): GP13 (PWM6 B, PIO0 SM2)
//! - Channel 3 (Ch 4): GP12 (PWM6 A, PIO0 SM3)
//! - Channel 4 (Ch 5): GP11 (PWM5 B, PIO1 SM0)
//! - Channel 5 (Ch 6): GP10 (PWM5 A, PIO1 SM1)
//! - Channel 6 (Ch 7): GP9  (PWM4 B, PIO1 SM2)
//! - Channel 7 (Ch 8): GP8  (PWM4 A, PIO1 SM3)
//!
//! Groups:
//! - Group 0: Channels 0 & 1 -> PWM Slice 7
//! - Group 1: Channels 2 & 3 -> PWM Slice 6
//! - Group 2: Channels 4 & 5 -> PWM Slice 5
//! - Group 3: Channels 6 & 7 -> PWM Slice 4
//!
//! When in `GroupClass::Pixel`:
//!   Driven at 800 kHz via RP2040 PIO (PIO0 for Ch 0..3, PIO1 for Ch 4..7).
//!   All active channels transmit in parallel via hardware DMA.
//! When in `GroupClass::Pwm`:
//!   Driven by RP2040 hardware PWM slice.

use embassy_futures::join::join4;
use embassy_rp::dma::Channel;
use embassy_rp::pac;
use embassy_rp::peripherals::{PIO0, PIO1};
use embassy_rp::pio::{Common, Config as PioConfig, Direction, Instance, ShiftDirection, StateMachine};
use embassy_time::{Instant, Timer};
use fixed::traits::ToFixed;
use pio::{Assembler, SideSet};

use sacn_core::channel::{
    fire_dimmer, pwm_level_from_universe, ws2812_fill_bytes, ws2812_multi_universe_bytes,
    ChannelMode,
};
use sacn_core::config::AppConfig;
use sacn_core::fire::{sparking_for_level, Fire};

use crate::state::UNIVERSE_STORE;

pub const MAX_PIXELS: usize = 1000;
pub const MAX_WIRE_BYTES: usize = 3000; // 1000 * 3 (RGB)
pub const MAX_FIRE_PIXELS: usize = 170; // Local fire simulation limit (saves RAM)
pub const CHANNEL_COUNT: usize = 8;

pub const CHANNEL_GPIOS: [usize; CHANNEL_COUNT] = [15, 14, 13, 12, 11, 10, 9, 8];
pub const CHANNEL_PWM_SLICES: [usize; CHANNEL_COUNT] = [7, 7, 6, 6, 5, 5, 4, 4];

static STRIP_BUFFERS: static_cell::StaticCell<[[u8; MAX_WIRE_BYTES]; CHANNEL_COUNT]> =
    static_cell::StaticCell::new();

/// Assembles the standard 800 kHz WS2812 PIO program.
///
/// 10 cycles per bit at 125 MHz sysclk (divider = 15.625).
/// Bit 1: 7 cycles high (875 ns), 3 cycles low (375 ns).
/// Bit 0: 3 cycles high (375 ns), 7 cycles low (875 ns).
pub fn ws2812_program() -> pio::Program<32> {
    let mut a = Assembler::<32>::new_with_side_set(SideSet::new(false, 1, false));
    let mut bitloop = a.label();
    let mut do_zero = a.label();

    // bitloop:
    a.bind(&mut bitloop);
    // out x, 1 side 0 [2]
    a.out_with_delay_and_side_set(pio::OutDestination::X, 1, 2, 0);
    // jmp !x do_zero side 1 [1]
    a.jmp_with_delay_and_side_set(pio::JmpCondition::XIsZero, &mut do_zero, 1, 1);
    // do_one: jmp bitloop side 1 [4]
    a.jmp_with_delay_and_side_set(pio::JmpCondition::Always, &mut bitloop, 4, 1);
    // do_zero: nop side 0 [4]
    a.bind(&mut do_zero);
    a.nop_with_delay_and_side_set(4, 0);

    a.assemble_program()
}

pub struct OutputDriver {
    pub sm0: StateMachine<'static, PIO0, 0>,
    pub sm1: StateMachine<'static, PIO0, 1>,
    pub sm2: StateMachine<'static, PIO0, 2>,
    pub sm3: StateMachine<'static, PIO0, 3>,
    pub sm4: StateMachine<'static, PIO1, 0>,
    pub sm5: StateMachine<'static, PIO1, 1>,
    pub sm6: StateMachine<'static, PIO1, 2>,
    pub sm7: StateMachine<'static, PIO1, 3>,

    pub dma: [Channel<'static>; CHANNEL_COUNT],
    pub strips: &'static mut [[u8; MAX_WIRE_BYTES]; CHANNEL_COUNT],
    pub last_pwm_freq_hz: Option<u32>,
    pub last_pwm_top: u16,
    pub pin_is_pwm: [Option<bool>; CHANNEL_COUNT],
}

impl OutputDriver {
    pub fn new(
        mut common0: Common<'static, PIO0>,
        mut sm0: StateMachine<'static, PIO0, 0>,
        mut sm1: StateMachine<'static, PIO0, 1>,
        mut sm2: StateMachine<'static, PIO0, 2>,
        mut sm3: StateMachine<'static, PIO0, 3>,
        pin15: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_15>,
        pin14: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_14>,
        pin13: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_13>,
        pin12: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_12>,

        mut common1: Common<'static, PIO1>,
        mut sm4: StateMachine<'static, PIO1, 0>,
        mut sm5: StateMachine<'static, PIO1, 1>,
        mut sm6: StateMachine<'static, PIO1, 2>,
        mut sm7: StateMachine<'static, PIO1, 3>,
        pin11: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_11>,
        pin10: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_10>,
        pin9: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_9>,
        pin8: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_8>,

        dma: [Channel<'static>; CHANNEL_COUNT],
    ) -> Self {
        let prg = ws2812_program();
        let loaded0 = common0.load_program(&prg);
        let loaded1 = common1.load_program(&prg);

        // Configure PIO0 state machines for Ch 0..3 (GP15, 14, 13, 12)
        let mut p15 = common0.make_pio_pin(pin15);
        let mut p14 = common0.make_pio_pin(pin14);
        let mut p13 = common0.make_pio_pin(pin13);
        let mut p12 = common0.make_pio_pin(pin12);

        setup_sm(&mut sm0, &mut p15, &loaded0);
        setup_sm(&mut sm1, &mut p14, &loaded0);
        setup_sm(&mut sm2, &mut p13, &loaded0);
        setup_sm(&mut sm3, &mut p12, &loaded0);

        // Configure PIO1 state machines for Ch 4..7 (GP11, 10, 9, 8)
        let mut p11 = common1.make_pio_pin(pin11);
        let mut p10 = common1.make_pio_pin(pin10);
        let mut p9 = common1.make_pio_pin(pin9);
        let mut p8 = common1.make_pio_pin(pin8);

        setup_sm(&mut sm4, &mut p11, &loaded1);
        setup_sm(&mut sm5, &mut p10, &loaded1);
        setup_sm(&mut sm6, &mut p9, &loaded1);
        setup_sm(&mut sm7, &mut p8, &loaded1);

        let strips = STRIP_BUFFERS.init([[0u8; MAX_WIRE_BYTES]; CHANNEL_COUNT]);

        Self {
            sm0,
            sm1,
            sm2,
            sm3,
            sm4,
            sm5,
            sm6,
            sm7,
            dma,
            strips,
            last_pwm_freq_hz: None,
            last_pwm_top: 25500,
            pin_is_pwm: [None; CHANNEL_COUNT],
        }
    }

    pub async fn process_frame(
        &mut self,
        cfg: &AppConfig,
        now_ms: u64,
        fire_sims: &mut [Option<Fire<MAX_FIRE_PIXELS>>; CHANNEL_COUNT],
    ) {
        // 1. Configure global PWM frequency on slices 4..7 if changed
        let freq_hz = cfg.pwm_freq_hz.clamp(
            sacn_core::channel::MIN_PWM_FREQ_HZ,
            sacn_core::channel::MAX_PWM_FREQ_HZ,
        );
        if self.last_pwm_freq_hz != Some(freq_hz) {
            let div_u8 = ((125_000_000 / (freq_hz * 65535)) + 1).clamp(1, 255) as u8;
            let top = ((125_000_000 / (div_u8 as u32 * freq_hz)).saturating_sub(1)).min(65535) as u16;
            self.last_pwm_top = top;
            for &slice in &[4, 5, 6, 7] {
                pac::PWM.ch(slice).top().write(|w| w.set_top(top));
                pac::PWM.ch(slice).div().write(|w| w.set_int(div_u8));
                pac::PWM.ch(slice).csr().write(|w| w.set_en(true));
            }
            self.last_pwm_freq_hz = Some(freq_hz);
        }

        // 2. Render all 8 channels
        let mut strip_lens = [0usize; CHANNEL_COUNT];
        let now_instant = Instant::now();

        for i in 0..CHANNEL_COUNT {
            let gp = CHANNEL_GPIOS[i];
            let slice = CHANNEL_PWM_SLICES[i];
            let ch_cfg = cfg.channels[i];
            let test = sacn_core::test_pattern::TestPattern::from_u8(
                crate::state::CHANNEL_TEST_MODES[i].load(core::sync::atomic::Ordering::Relaxed),
            );

            let is_pwm_mode = matches!(ch_cfg.mode, ChannelMode::Pwm { .. });

            if is_pwm_mode {
                if self.pin_is_pwm[i] != Some(true) {
                    pac::IO_BANK0.gpio(gp).ctrl().write(|w| w.set_funcsel(4.into())); // PWM funcsel 4
                    self.pin_is_pwm[i] = Some(true);
                }

                let duty = if test != sacn_core::test_pattern::TestPattern::Off {
                    test.render_pwm(now_ms)
                } else {
                    render_pwm_channel(ch_cfg.mode, ch_cfg.loss, now_instant)
                };

                let compare = ((duty as u32 * self.last_pwm_top as u32 + 127) / 255) as u16;

                if i % 2 == 1 {
                    // Channel A (odd index)
                    pac::PWM.ch(slice).cc().modify(|w| w.set_a(compare));
                } else {
                    // Channel B (even index)
                    pac::PWM.ch(slice).cc().modify(|w| w.set_b(compare));
                }
                strip_lens[i] = 0;
            } else {
                if self.pin_is_pwm[i] != Some(false) {
                    let funcsel = if i < 4 { 6.into() } else { 7.into() }; // PIO0 = 6, PIO1 = 7
                    pac::IO_BANK0.gpio(gp).ctrl().write(|w| w.set_funcsel(funcsel));
                    self.pin_is_pwm[i] = Some(false);
                }

                if matches!(ch_cfg.mode, ChannelMode::Off) && test == sacn_core::test_pattern::TestPattern::Off {
                    strip_lens[i] = 0;
                } else {
                    let len = if test != sacn_core::test_pattern::TestPattern::Off {
                        let px = get_test_pixel_info(i, ch_cfg.mode);
                        test.render_pixel_bytes(now_ms, px, &mut self.strips[i])
                    } else {
                        render_pixel_channel_bytes(
                            ch_cfg.mode,
                            ch_cfg.loss,
                            now_instant,
                            &mut fire_sims[i],
                            &mut self.strips[i],
                        )
                    };
                    strip_lens[i] = len;
                }
            }
        }

        // 3. Transmit pixel channels concurrently via DMA to PIO
        let [dma0, dma1, dma2, dma3, dma4, dma5, dma6, dma7] = &mut self.dma;
        let strips = &*self.strips;
        let f0 = push_strip(&mut self.sm0, dma0, &strips[0][..strip_lens[0]]);
        let f1 = push_strip(&mut self.sm1, dma1, &strips[1][..strip_lens[1]]);
        let f2 = push_strip(&mut self.sm2, dma2, &strips[2][..strip_lens[2]]);
        let f3 = push_strip(&mut self.sm3, dma3, &strips[3][..strip_lens[3]]);
        let f4 = push_strip(&mut self.sm4, dma4, &strips[4][..strip_lens[4]]);
        let f5 = push_strip(&mut self.sm5, dma5, &strips[5][..strip_lens[5]]);
        let f6 = push_strip(&mut self.sm6, dma6, &strips[6][..strip_lens[6]]);
        let f7 = push_strip(&mut self.sm7, dma7, &strips[7][..strip_lens[7]]);

        join4(
            embassy_futures::join::join(f0, f1),
            embassy_futures::join::join(f2, f3),
            embassy_futures::join::join(f4, f5),
            embassy_futures::join::join(f6, f7),
        )
        .await;

        // 4. Latch reset gap for WS2812 (>80 µs)
        if strip_lens.iter().any(|&l| l > 0) {
            Timer::after_micros(100).await;
        }
    }
}

fn setup_sm<'d, T: Instance, const SM: usize>(
    sm: &mut StateMachine<'d, T, SM>,
    pin: &mut embassy_rp::pio::Pin<'d, T>,
    loaded: &embassy_rp::pio::LoadedProgram<'d, T>,
) {
    let mut cfg = PioConfig::default();
    cfg.use_program(loaded, &[pin]);
    cfg.clock_divider = 15.625_f32.to_fixed();
    cfg.shift_out.auto_fill = true;
    cfg.shift_out.threshold = 8;
    cfg.shift_out.direction = ShiftDirection::Left; // MSB first

    sm.set_config(&cfg);
    sm.set_pin_dirs(Direction::Out, &[pin]);
    sm.set_enable(true);
}

async fn push_strip<T: Instance, const SM: usize>(
    sm: &mut StateMachine<'_, T, SM>,
    dma: &mut Channel<'_>,
    data: &[u8],
) {
    if !data.is_empty() {
        sm.tx().dma_push(dma, data, true).await;
    }
}

fn test_pixel_info(mode: ChannelMode) -> usize {
    match mode {
        ChannelMode::Ws2812 { pixel_count, .. } => {
            (pixel_count as usize).min(MAX_PIXELS)
        }
        ChannelMode::Ws2812Fill { pixel_count, .. } => {
            (pixel_count as usize).min(MAX_PIXELS)
        }
        ChannelMode::Fire { pixel_count, .. } => {
            (pixel_count as usize).min(MAX_FIRE_PIXELS)
        }
        ChannelMode::Off => 30,
        _ => 0,
    }
}

fn get_test_pixel_info(ch_idx: usize, mode: ChannelMode) -> usize {
    let cfg_px = test_pixel_info(mode);
    let override_px = crate::state::CHANNEL_TEST_PIXELS[ch_idx].load(core::sync::atomic::Ordering::Relaxed) as usize;

    if override_px > 0 {
        override_px.min(MAX_PIXELS)
    } else {
        cfg_px
    }
}

fn get_universe_data(universe: u16) -> Option<([u8; sacn_core::channel::DMX_UNIVERSE_SIZE], Option<u32>)> {
    UNIVERSE_STORE.lock(|s| {
        let store = s.borrow();
        store.get(universe).map(|(data, last, term)| {
            let ms = if term {
                None
            } else {
                last.map(|t| Instant::now().duration_since(t).as_millis() as u32)
            };
            (data, ms)
        })
    })
}

fn render_pixel_channel_bytes(
    mode: ChannelMode,
    loss: sacn_core::loss::LossConfig,
    _now: Instant,
    fire_sim: &mut Option<Fire<MAX_FIRE_PIXELS>>,
    out: &mut [u8; MAX_WIRE_BYTES],
) -> usize {
    match mode {
        ChannelMode::Off => {
            *fire_sim = None;
            0
        }
        ChannelMode::Ws2812 { universe, start_channel, pixel_count } => {
            *fire_sim = None;
            let n = (pixel_count as usize).min(MAX_PIXELS);
            let uni_data = get_universe_data(universe);
            let gain = loss.gain(uni_data.as_ref().and_then(|(_, ms)| *ms));

            let bytes_written = UNIVERSE_STORE.lock(|s| {
                let store = s.borrow();
                ws2812_multi_universe_bytes(
                    |u| store.get_data(u),
                    universe,
                    start_channel,
                    n,
                    out,
                )
            });

            if gain < 255 {
                for b in out[..bytes_written].iter_mut() {
                    *b = ((*b as u16 * gain as u16 + 127) / 255) as u8;
                }
            }
            bytes_written
        }
        ChannelMode::Ws2812Fill { universe, start_channel, pixel_count } => {
            *fire_sim = None;
            let n = (pixel_count as usize).min(MAX_PIXELS);
            let uni_data = get_universe_data(universe);
            let gain = loss.gain(uni_data.as_ref().and_then(|(_, ms)| *ms));
            let bytes_written = if let Some((data, _)) = uni_data {
                ws2812_fill_bytes(&data, start_channel, n, out)
            } else {
                let total = (n * 3).min(out.len());
                out[..total].fill(0);
                total
            };
            if gain < 255 {
                for b in out[..bytes_written].iter_mut() {
                    *b = ((*b as u16 * gain as u16 + 127) / 255) as u8;
                }
            }
            bytes_written
        }
        ChannelMode::Fire { pixel_count, control } => {
            let n = (pixel_count as usize).min(MAX_FIRE_PIXELS);
            let sim = fire_sim.get_or_insert_with(|| Fire::new(0xCAFE_BABE ^ (pixel_count as u32)));

            let dimmer_raw = match control {
                Some(slot) => {
                    let uni_data = get_universe_data(slot.universe);
                    let gain = loss.gain(uni_data.as_ref().and_then(|(_, ms)| *ms));
                    let level = fire_dimmer(Some(slot), uni_data.as_ref().map(|(d, _)| d.as_slice()));
                    ((level as u16 * gain as u16) / 255) as u8
                }
                None => 255,
            };

            sim.sparking = sparking_for_level(dimmer_raw);
            sim.step();

            let mut byte_idx = 0;
            for &cell in sim.heat()[..n].iter() {
                let rgb = sacn_core::fire::heat_to_color(cell);
                let scaled = if dimmer_raw < 255 { rgb.scaled(dimmer_raw) } else { rgb };
                if byte_idx + 3 <= out.len() {
                    out[byte_idx] = scaled.r;
                    out[byte_idx + 1] = scaled.g;
                    out[byte_idx + 2] = scaled.b;
                    byte_idx += 3;
                }
            }
            byte_idx
        }
        ChannelMode::Pwm { .. } => {
            *fire_sim = None;
            0
        }
    }
}

fn render_pwm_channel(
    mode: ChannelMode,
    loss: sacn_core::loss::LossConfig,
    _now: Instant,
) -> u8 {
    match mode {
        ChannelMode::Pwm { universe, dmx_channel } => {
            let uni_data = get_universe_data(universe);
            let gain = loss.gain(uni_data.as_ref().and_then(|(_, ms)| *ms));
            let raw_level = uni_data
                .map(|(d, _)| pwm_level_from_universe(&d, dmx_channel))
                .unwrap_or(0);
            ((raw_level as u16 * gain as u16) / 255) as u8
        }
        _ => 0,
    }
}
