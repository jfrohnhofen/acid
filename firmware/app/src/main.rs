//! ACID: 8-channel sACN (E1.31) output controller for Raspberry Pi Pico (RP2040)
//! + W5500 SPI Ethernet.
//!
//! Features:
//! - 8 outputs organized into 4 groups:
//!   - Channel 0 (Ch 1): GP15 (PWM7 B, PIO0 SM0)
//!   - Channel 1 (Ch 2): GP14 (PWM7 A, PIO0 SM1)
//!   - Channel 2 (Ch 3): GP13 (PWM6 B, PIO0 SM2)
//!   - Channel 3 (Ch 4): GP12 (PWM6 A, PIO0 SM3)
//!   - Channel 4 (Ch 5): GP11 (PWM5 B, PIO1 SM0)
//!   - Channel 5 (Ch 6): GP10 (PWM5 A, PIO1 SM1)
//!   - Channel 6 (Ch 7): GP9  (PWM4 B, PIO1 SM2)
//!   - Channel 7 (Ch 8): GP8  (PWM4 A, PIO1 SM3)
//! - WS2812 parallel drive via RP2040 PIO at 800 kHz, or hardware PWM dimmer.
//! - FastLED Fire2012 local fire effect per channel with sACN dimmer slot.
//! - W5500 10/100 Ethernet on SPI0:
//!   - GP0: MISO (SPI0 RX)
//!   - GP1: RSTn (reset output)
//!   - GP2: SCK (SPI0 SCK)
//!   - GP3: MOSI (SPI0 TX)
//!   - GP4: INTn (interrupt input)
//!   - GP5: CSn (chip select output)
//! - DHCP with fallback to deterministic 169.254.x.y link-local IP.
//! - mDNS announcement: `<hostname>.local`, `_http._tcp`, and `_acid._tcp`.
//! - Web status & configuration UI at `http://<hostname>.local/` (port 80).
//! - sACN E1.31 receiver on UDP port 5568 with IGMP multicast group join.

#![no_std]
#![no_main]

use core::net::{Ipv4Addr, Ipv6Addr};
use core::sync::atomic::{AtomicBool, Ordering};

use defmt::{info, warn};
use defmt_rtt as _;
use edge_mdns::buf::VecBufAccess;
use edge_mdns::domain::base::Ttl;
use edge_mdns::host::{Host, Service, ServiceAnswers};
use edge_mdns::io::{self, IPV4_DEFAULT_SOCKET};
use edge_mdns::{ChainedHostAnswers, HostAnswersMdnsHandler};
use edge_nal::UdpSplit;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_executor::Spawner;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{
    ConfigV4, DhcpConfig, Ipv4Address, Ipv4Cidr, Stack, StackResources, StaticConfigV4,
};
use embassy_net_wiznet::chip::W5500;
use embassy_net_wiznet::State;
use embassy_rp::bind_interrupts;
use embassy_rp::dma::Channel;
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::*;
use embassy_rp::pio::Pio;
use embassy_rp::spi::Spi;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::once_lock::OnceLock;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Delay, Duration, Instant, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use panic_probe as _;
use static_cell::StaticCell;

use firmware::clocks;
use firmware::output::{OutputDriver, MAX_FIRE_PIXELS};
use firmware::rand::Xorshift;
use firmware::state::{
    AppConfig, NetStatus, APP_CONFIG, NET_STATUS, UNIVERSE_STORE,
};
use sacn_core::channel::ChannelMode;
use sacn_core::identity::Identity;
use sacn_core::sacn::DataPacket;

bind_interrupts!(struct Irqs {
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>,
                 embassy_rp::dma::InterruptHandler<DMA_CH1>,
                 embassy_rp::dma::InterruptHandler<DMA_CH2>,
                 embassy_rp::dma::InterruptHandler<DMA_CH3>,
                 embassy_rp::dma::InterruptHandler<DMA_CH4>,
                 embassy_rp::dma::InterruptHandler<DMA_CH5>,
                 embassy_rp::dma::InterruptHandler<DMA_CH6>,
                 embassy_rp::dma::InterruptHandler<DMA_CH7>,
                 embassy_rp::dma::InterruptHandler<DMA_CH8>,
                 embassy_rp::dma::InterruptHandler<DMA_CH9>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    PIO1_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO1>;
});

const DHCP_TIMEOUT: Duration = Duration::from_secs(15);
const HTTP_PORT: u16 = 80;
const HTTP_TASKS: usize = 1;
const SACN_PORT: u16 = 5568;

type SpiDev = ExclusiveDevice<
    Spi<'static, SPI0, embassy_rp::spi::Async>,
    Output<'static>,
    Delay,
>;
type EthRunner =
    embassy_net_wiznet::Runner<'static, W5500, SpiDev, Input<'static>, Output<'static>>;
type EthDevice = embassy_net_wiznet::Device<'static>;

static IDENTITY: OnceLock<Identity> = OnceLock::new();
static USING_FALLBACK: AtomicBool = AtomicBool::new(false);
static MDNS_RELOAD_SIGNAL: Signal<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, ()> =
    Signal::new();

static mut CORE1_STACK: embassy_rp::multicore::Stack<4096> = embassy_rp::multicore::Stack::new();
static EXECUTOR_CORE1: StaticCell<embassy_executor::Executor> = StaticCell::new();

#[embassy_executor::task]
async fn eth_task(runner: EthRunner) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, EthDevice>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_manager_task(stack: Stack<'static>, fallback_ip: [u8; 4]) {
    use embassy_futures::select::{select, select3, Either, Either3};

    let mut current_config = {
        let cfg = APP_CONFIG.lock().await;
        (
            cfg.net.dhcp,
            cfg.hostname,
            cfg.net.static_ip,
            cfg.net.static_prefix,
            cfg.net.static_gateway,
        )
    };

    loop {
        let (is_dhcp, hostname, static_ip, static_prefix, static_gateway) = current_config;

        if !is_dhcp {
            apply_static(stack, static_ip, static_prefix, static_gateway);
            MDNS_RELOAD_SIGNAL.signal(());

            // In static mode, stay until user changes settings in web UI
            let cmd = firmware::state::NET_UPDATE_CHANNEL.receive().await;
            current_config = (
                cmd.dhcp,
                cmd.hostname,
                cmd.static_ip,
                cmd.static_prefix,
                cmd.static_gateway,
            );
            continue;
        }

        // --- DHCP Mode ---
        // 1. If physical link is not up yet, wait for link
        if !stack.is_link_up() {
            info!("Ethernet link is down. Waiting for cable to be connected...");
            // Temporarily assign fallback IP while link is down so stack is valid
            USING_FALLBACK.store(true, Ordering::Relaxed);
            stack.set_config_v4(ConfigV4::Static(StaticConfigV4 {
                address: Ipv4Cidr::new(
                    Ipv4Address::new(fallback_ip[0], fallback_ip[1], fallback_ip[2], fallback_ip[3]),
                    16,
                ),
                gateway: None,
                dns_servers: Default::default(),
            }));

            // Wait for link up OR web UI reconfig
            match select(stack.wait_link_up(), firmware::state::NET_UPDATE_CHANNEL.receive()).await {
                Either::First(()) => {
                    info!("Ethernet link detected! Starting DHCP...");
                }
                Either::Second(cmd) => {
                    current_config = (
                        cmd.dhcp,
                        cmd.hostname,
                        cmd.static_ip,
                        cmd.static_prefix,
                        cmd.static_gateway,
                    );
                    continue;
                }
            }
        }

        // 2. Physical link is UP: start DHCP discovery
        USING_FALLBACK.store(false, Ordering::Relaxed);
        let mut dhcp = DhcpConfig::default();
        dhcp.hostname = hostname.as_str().try_into().ok();
        info!("Requesting DHCP lease for '{}'...", hostname.as_str());
        stack.set_config_v4(ConfigV4::Dhcp(dhcp));

        // Wait for lease (up to 15s) OR link loss OR web UI reconfig
        let dhcp_wait = async { with_timeout(DHCP_TIMEOUT, stack.wait_config_up()).await };

        match select3(
            dhcp_wait,
            stack.wait_link_down(),
            firmware::state::NET_UPDATE_CHANNEL.receive(),
        )
        .await
        {
            Either3::First(Ok(())) => {
                // DHCP lease acquired!
                USING_FALLBACK.store(false, Ordering::Relaxed);
                if let Some(cfg) = stack.config_v4() {
                    let a = cfg.address.address().octets();
                    info!(
                        "DHCP lease acquired: {}.{}.{}.{}/{}",
                        a[0], a[1], a[2], a[3], cfg.address.prefix_len()
                    );
                }
                MDNS_RELOAD_SIGNAL.signal(());

                // Keep lease until link is lost OR web UI reconfig arrives
                match select(stack.wait_link_down(), firmware::state::NET_UPDATE_CHANNEL.receive()).await {
                    Either::First(()) => {
                        warn!("Ethernet link lost!");
                    }
                    Either::Second(cmd) => {
                        current_config = (
                            cmd.dhcp,
                            cmd.hostname,
                            cmd.static_ip,
                            cmd.static_prefix,
                            cmd.static_gateway,
                        );
                    }
                }
            }
            Either3::First(Err(_)) => {
                // DHCP timed out after 15s of link UP (e.g. ad-hoc network with no DHCP server)
                warn!(
                    "No DHCP lease after {}s of active link; falling back to link-local {}.{}.{}.{}",
                    DHCP_TIMEOUT.as_secs(),
                    fallback_ip[0],
                    fallback_ip[1],
                    fallback_ip[2],
                    fallback_ip[3]
                );
                USING_FALLBACK.store(true, Ordering::Relaxed);
                stack.set_config_v4(ConfigV4::Static(StaticConfigV4 {
                    address: Ipv4Cidr::new(
                        Ipv4Address::new(fallback_ip[0], fallback_ip[1], fallback_ip[2], fallback_ip[3]),
                        16,
                    ),
                    gateway: None,
                    dns_servers: Default::default(),
                }));
                MDNS_RELOAD_SIGNAL.signal(());

                // While on fallback, stay until link drop or web UI reconfig
                match select(
                    stack.wait_link_down(),
                    firmware::state::NET_UPDATE_CHANNEL.receive(),
                )
                .await
                {
                    Either::First(()) => {
                        warn!("Ethernet link lost!");
                    }
                    Either::Second(cmd) => {
                        current_config = (
                            cmd.dhcp,
                            cmd.hostname,
                            cmd.static_ip,
                            cmd.static_prefix,
                            cmd.static_gateway,
                        );
                    }
                }
            }
            Either3::Second(()) => {
                warn!("Ethernet link lost during DHCP discovery!");
            }
            Either3::Third(cmd) => {
                current_config = (
                    cmd.dhcp,
                    cmd.hostname,
                    cmd.static_ip,
                    cmd.static_prefix,
                    cmd.static_gateway,
                );
            }
        }
    }
}

fn apply_static(stack: Stack<'static>, ip: [u8; 4], prefix: u8, gateway: Option<[u8; 4]>) {
    USING_FALLBACK.store(false, Ordering::Relaxed);
    info!("Applying static IPv4: {}.{}.{}.{}/{}", ip[0], ip[1], ip[2], ip[3], prefix);
    stack.set_config_v4(ConfigV4::Static(StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]), prefix),
        gateway: gateway.map(|g| Ipv4Address::new(g[0], g[1], g[2], g[3])),
        dns_servers: Default::default(),
    }));
}

#[embassy_executor::task]
async fn mdns_task(stack: Stack<'static>) {
    let identity = IDENTITY.get().await;
    static UDP_BUFFERS: StaticCell<UdpBuffers<1, 1024, 1024, 4>> = StaticCell::new();
    let udp = Udp::new(stack, UDP_BUFFERS.init(UdpBuffers::new()));
    let mut rng = Xorshift::new(u64::from_le_bytes([
        identity.mac[0],
        identity.mac[1],
        identity.mac[2],
        identity.mac[3],
        identity.mac[4],
        identity.mac[5],
        0x6d,
        0x64,
    ]));

    loop {
        stack.wait_config_up().await;
        let Some(cfg) = stack.config_v4() else { continue };
        let our_ip: Ipv4Addr = cfg.address.address();
        let hostname_buf = {
            let app = APP_CONFIG.lock().await;
            app.hostname
        };
        let hostname_str = hostname_buf.as_str();
        info!(
            "mDNS: announcing {}.local (_http._tcp, _acid._tcp) at {}.{}.{}.{}",
            hostname_str,
            our_ip.octets()[0],
            our_ip.octets()[1],
            our_ip.octets()[2],
            our_ip.octets()[3]
        );

        let mut socket =
            match io::bind(&udp, IPV4_DEFAULT_SOCKET, Some(Ipv4Addr::UNSPECIFIED), None).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("mDNS bind failed: {}", defmt::Debug2Format(&e));
                    Timer::after_secs(5).await;
                    continue;
                }
            };
        let (recv, send) = socket.split();

        let host = Host {
            hostname: hostname_str,
            ipv4: our_ip,
            ipv6: Ipv6Addr::UNSPECIFIED,
            ttl: Ttl::from_secs(60),
        };
        let http_service = Service {
            name: hostname_str,
            priority: 0,
            weight: 0,
            service: "_http",
            protocol: "_tcp",
            port: HTTP_PORT,
            service_subtypes: &[],
            txt_kvs: &[("path", "/")],
        };
        let acid_service = Service {
            name: hostname_str,
            priority: 0,
            weight: 0,
            service: "_acid",
            protocol: "_tcp",
            port: HTTP_PORT,
            service_subtypes: &[],
            txt_kvs: &[
                ("model", "acid"),
                ("proto", "sacn"),
                ("channels", "8"),
                ("path", "/"),
            ],
        };

        let signal = Signal::<NoopRawMutex, ()>::new();
        let mdns = io::Mdns::new(
            Some(Ipv4Addr::UNSPECIFIED),
            None,
            recv,
            send,
            VecBufAccess::<NoopRawMutex, 1500>::new(),
            VecBufAccess::<NoopRawMutex, 1500>::new(),
            &mut rng,
            &signal,
        );
        let answers = ChainedHostAnswers::new(
            ServiceAnswers::new(&host, &http_service),
            ServiceAnswers::new(&host, &acid_service),
        );
        let run_fut = mdns.run(HostAnswersMdnsHandler::new(answers));
        match embassy_futures::select::select(run_fut, MDNS_RELOAD_SIGNAL.wait()).await {
            embassy_futures::select::Either::First(res) => {
                if let Err(e) = res {
                    warn!("mDNS stopped: {}", defmt::Debug2Format(&e));
                }
                Timer::after_secs(5).await;
            }
            embassy_futures::select::Either::Second(()) => {
                info!("mDNS: reload signaled, refreshing announcement");
            }
        }
    }
}

#[embassy_executor::task(pool_size = HTTP_TASKS)]
async fn web_task(id: usize, stack: Stack<'static>) -> ! {
    let app = firmware::web::router();
    let config = picoserve::Config::const_default();
    let mut tcp_rx = [0u8; 1024];
    let mut tcp_tx = [0u8; 2048];
    let mut http_buf = [0u8; 2048];

    picoserve::Server::new(&app, &config, &mut http_buf)
        .listen_and_serve(id, stack, HTTP_PORT, &mut tcp_rx, &mut tcp_tx)
        .await
        .into_never()
}

fn collect_active_universes(cfg: &AppConfig) -> heapless::Vec<u16, 64> {
    let mut list = heapless::Vec::new();
    let _ = list.push(1);
    for ch in &cfg.channels {
        match ch.mode {
            ChannelMode::Ws2812 { universe, start_channel, pixel_count } => {
                let (start_u, end_u) = sacn_core::channel::universes_spanned(universe, start_channel, pixel_count);
                for u in start_u..=end_u {
                    if !list.contains(&u) {
                        let _ = list.push(u);
                    }
                }
            }
            ChannelMode::Ws2812Fill { universe, .. } => {
                if !list.contains(&universe) {
                    let _ = list.push(universe);
                }
            }
            ChannelMode::Fire { control: Some(slot), .. } => {
                if !list.contains(&slot.universe) {
                    let _ = list.push(slot.universe);
                }
            }
            ChannelMode::Pwm { universe, .. } => {
                if !list.contains(&universe) {
                    let _ = list.push(universe);
                }
            }
            _ => {}
        }
    }
    list
}

#[embassy_executor::task]
async fn sacn_task(stack: Stack<'static>) -> ! {
    static SACN_RX_META: StaticCell<[PacketMetadata; 8]> = StaticCell::new();
    static SACN_RX_BUF: StaticCell<[u8; 2048]> = StaticCell::new();
    static SACN_TX_META: StaticCell<[PacketMetadata; 1]> = StaticCell::new();
    static SACN_TX_BUF: StaticCell<[u8; 128]> = StaticCell::new();

    let rx_meta = SACN_RX_META.init([PacketMetadata::EMPTY; 8]);
    let rx_buf = SACN_RX_BUF.init([0u8; 2048]);
    let tx_meta = SACN_TX_META.init([PacketMetadata::EMPTY; 1]);
    let tx_buf = SACN_TX_BUF.init([0u8; 128]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    if let Err(e) = socket.bind(SACN_PORT) {
        defmt::error!("sACN bind error: {}", defmt::Debug2Format(&e));
    }
    info!("sACN receiver bound on UDP {}", SACN_PORT);

    let mut joined_universes: heapless::Vec<u16, 64> = heapless::Vec::new();
    let mut buf = [0u8; 700];

    loop {
        if stack.is_config_up() {
            let active = {
                let cfg = APP_CONFIG.lock().await;
                collect_active_universes(&cfg)
            };

            let mut i = 0;
            while i < joined_universes.len() {
                let u = joined_universes[i];
                if !active.contains(&u) {
                    let hi = (u >> 8) as u8;
                    let lo = (u & 0xFF) as u8;
                    let addr = Ipv4Address::new(239, 255, hi, lo);
                    let _ = stack.leave_multicast_group(addr);
                    info!("sACN: left multicast group 239.255.{}.{} (uni {})", hi, lo, u);
                    joined_universes.swap_remove(i);
                } else {
                    i += 1;
                }
            }

            for &u in &active {
                if !joined_universes.contains(&u) {
                    let hi = (u >> 8) as u8;
                    let lo = (u & 0xFF) as u8;
                    let addr = Ipv4Address::new(239, 255, hi, lo);
                    match stack.join_multicast_group(addr) {
                        Ok(()) => {
                            info!("sACN: joined multicast group 239.255.{}.{} (uni {})", hi, lo, u);
                            let _ = joined_universes.push(u);
                        }
                        Err(e) => {
                            warn!("sACN: failed to join multicast group for uni {}: {}", u, defmt::Debug2Format(&e));
                        }
                    }
                }
            }
        }

        match with_timeout(Duration::from_millis(500), socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _from))) => {
                match DataPacket::parse(&buf[..len]) {
                    Ok(pkt) => {
                        if pkt.start_code == 0x00 {
                            let now = Instant::now();
                            UNIVERSE_STORE.lock(|s| {
                                let mut store = s.borrow_mut();
                                store.update(pkt.universe, pkt.sequence_number, pkt.dmx_data, now);
                                if pkt.is_stream_terminated() {
                                    store.mark_terminated(pkt.universe);
                                }
                            });
                        }
                    }
                    Err(_e) => {}
                }
            }
            Ok(Err(_e)) => {}
            Err(_) => {}
        }
    }
}

#[embassy_executor::task]
async fn output_task(mut driver: OutputDriver) -> ! {
    let mut fire_sims: [Option<sacn_core::fire::Fire<{ MAX_FIRE_PIXELS }>>; 8] = [
        None, None, None, None, None, None, None, None,
    ];

    loop {
        let frame_start = Instant::now();
        let now_ms = frame_start.as_millis() as u64;
        let cfg = {
            let app = APP_CONFIG.lock().await;
            *app
        };

        // Output the frame with PIO / PWM concurrently on Core 1
        driver.process_frame(&cfg, now_ms, &mut fire_sims).await;

        let elapsed = Instant::now().duration_since(frame_start);
        let target = Duration::from_millis(25); // 40 Hz refresh rate
        if elapsed < target {
            Timer::after(target - elapsed).await;
        } else {
            embassy_futures::yield_now().await;
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(clocks::config());
    info!("ACID: starting Raspberry Pi Pico application");

    // 1. Flash persistence initialization & UID extraction
    let mut flash = Flash::<_, Blocking, { firmware::flash::FLASH_SIZE }>::new_blocking(p.FLASH);
    let mut uid = [0u8; 8];
    if let Err(e) = flash.blocking_unique_id(&mut uid) {
        warn!("Failed to read flash UID: {}", defmt::Debug2Format(&e));
    }
    let identity = Identity::from_uid(&uid);
    let m = identity.mac;
    info!(
        "identity: {} mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} fallback {}.{}.{}.{}",
        identity.hostname(),
        m[0],
        m[1],
        m[2],
        m[3],
        m[4],
        m[5],
        identity.fallback_ip[0],
        identity.fallback_ip[1],
        identity.fallback_ip[2],
        identity.fallback_ip[3]
    );
    let _ = IDENTITY.init(identity);

    firmware::flash::init_flash(flash).await;

    // Load saved configuration from Flash sector if present
    if let Some(loaded) = firmware::flash::load_config().await {
        let mut cfg = APP_CONFIG.lock().await;
        *cfg = loaded;
        if cfg.hostname.len == 0 {
            cfg.hostname = firmware::state::CustomHostname::from_str(identity.hostname());
        }
    } else {
        let mut cfg = APP_CONFIG.lock().await;
        cfg.hostname = firmware::state::CustomHostname::from_str(identity.hostname());
        cfg.net.static_ip = identity.fallback_ip;
    }

    // Initial NetStatus with MAC address
    NET_STATUS.lock(|s| {
        s.set(NetStatus {
            link_up: false,
            ip: None,
            mac: identity.mac,
            is_fallback: false,
        })
    });

    info!("Starting output initialization (8 channels, PIO WS2812 & hardware PWM)");

    let pio0 = Pio::new(p.PIO0, Irqs);
    let pio1 = Pio::new(p.PIO1, Irqs);

    let dma_channels = [
        Channel::new(p.DMA_CH2, Irqs),
        Channel::new(p.DMA_CH3, Irqs),
        Channel::new(p.DMA_CH4, Irqs),
        Channel::new(p.DMA_CH5, Irqs),
        Channel::new(p.DMA_CH6, Irqs),
        Channel::new(p.DMA_CH7, Irqs),
        Channel::new(p.DMA_CH8, Irqs),
        Channel::new(p.DMA_CH9, Irqs),
    ];

    let driver = OutputDriver::new(
        pio0.common,
        pio0.sm0,
        pio0.sm1,
        pio0.sm2,
        pio0.sm3,
        p.PIN_15,
        p.PIN_14,
        p.PIN_13,
        p.PIN_12,
        pio1.common,
        pio1.sm0,
        pio1.sm1,
        pio1.sm2,
        pio1.sm3,
        p.PIN_11,
        p.PIN_10,
        p.PIN_9,
        p.PIN_8,
        dma_channels,
    );

    embassy_rp::multicore::spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR_CORE1.init(embassy_executor::Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(output_task(driver).unwrap());
            });
        },
    );
    info!("Output coordinator task spawned on Core 1");

    // 2. W5500 on SPI0: GP0 (MISO), GP1 (RSTn), GP2 (SCK), GP3 (MOSI), GP4 (INTn), GP5 (CSn)
    let mut spi_config = embassy_rp::spi::Config::default();
    spi_config.frequency = 10_000_000;
    let spi = embassy_rp::spi::Spi::new(
        p.SPI0,
        p.PIN_2,
        p.PIN_3,
        p.PIN_0,
        p.DMA_CH0,
        p.DMA_CH1,
        Irqs,
        spi_config,
    );
    let cs = Output::new(p.PIN_5, Level::High);
    let spi_dev = ExclusiveDevice::new(spi, cs, Delay).unwrap();
    let w5500_int = Input::new(p.PIN_4, Pull::Up);
    let w5500_reset = Output::new(p.PIN_1, Level::High);

    static ETH_STATE: StaticCell<State<2, 2>> = StaticCell::new();
    let (device, eth_runner) = match embassy_net_wiznet::new::<2, 2, W5500, _, _, _>(
        identity.mac,
        ETH_STATE.init(State::new()),
        spi_dev,
        w5500_int,
        w5500_reset,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            defmt::error!("W5500 init failed: {}", defmt::Debug2Format(&e));
            loop {
                Timer::after_secs(1).await;
            }
        }
    };

    info!("W5500 initialised");
    spawner.spawn(eth_task(eth_runner).unwrap());

    // 3. Network stack
    static RESOURCES: StaticCell<StackResources<6>> = StaticCell::new();
    let seed = u64::from_le_bytes([m[0], m[1], m[2], m[3], m[4], m[5], 0x5a, 0xc1]);
    let (stack, net_runner) = embassy_net::new(
        device,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(net_task(net_runner).unwrap());
    spawner.spawn(net_manager_task(stack, identity.fallback_ip).unwrap());
    spawner.spawn(mdns_task(stack).unwrap());
    spawner.spawn(sacn_task(stack).unwrap());

    for id in 0..HTTP_TASKS {
        spawner.spawn(web_task(id, stack).unwrap());
    }

    // 4. Main monitoring loop
    let mut last = NetStatus::default();
    loop {
        let is_fb = USING_FALLBACK.load(Ordering::Relaxed);
        let ip = stack.config_v4().map(|c| {
            let a = c.address.address().octets();
            (a, c.address.prefix_len())
        });
        let status = NetStatus {
            link_up: stack.is_link_up(),
            ip,
            mac: identity.mac,
            is_fallback: is_fb,
        };
        NET_STATUS.lock(|s| s.set(status));

        let changed = status.link_up != last.link_up || status.ip != last.ip;
        if changed {
            match status.ip {
                Some((a, pfx)) => info!(
                    "link {} ip {}.{}.{}.{}/{} ({})",
                    status.link_up,
                    a[0],
                    a[1],
                    a[2],
                    a[3],
                    pfx,
                    if is_fb { "fallback" } else { "DHCP" }
                ),
                None => info!("link {} ip none", status.link_up),
            }
        }
        last = status;
        Timer::after_secs(1).await;
    }
}
