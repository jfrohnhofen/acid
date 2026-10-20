//! Status and configuration web UI served over HTTP.
//!
//! Exposes:
//! - `GET /` -> Status & unified configuration table.
//! - `POST /settings` -> Configure network, output channels, and PWM frequency.
//! - `POST /channel` -> Configure single channel mode, universe, DMX address, pixel count, and loss behavior.
//! - `POST /api/test/channel` -> Interactive channel test mode selection.

use embassy_time::Instant;
use heapless::String;
use picoserve::response::chunked::{ChunkWriter, ChunkedResponse, Chunks, ChunksWritten};
use picoserve::response::IntoResponse;
use picoserve::routing::{get, post};
use sacn_core::channel::{ChannelMode, DmxSlot};
use sacn_core::loss::{LossBehavior, LossConfig};
use sacn_core::web::{
    render_channel_row, render_channel_table_footer, render_channel_table_header,
    render_network_card, render_stats_grid, render_status_json,
    render_topbar, render_universes_card, ChannelView, PageData,
    UniverseView,
};

use crate::state::{
    CustomHostname, NetConfig, NetUpdateCmd, APP_CONFIG, CHANNEL_COUNT,
    NET_UPDATE_CHANNEL, UNIVERSE_STORE,
};

pub struct StatusPage;

impl Chunks for StatusPage {
    fn content_type(&self) -> &'static str {
        "text/html; charset=utf-8"
    }

    async fn write_chunks<W: picoserve::io::Write>(
        self,
        mut writer: ChunkWriter<W>,
    ) -> Result<ChunksWritten, W::Error> {
        let mut buf: String<2048> = String::new();
        let cfg = { *APP_CONFIG.lock().await };
        let uptime = Instant::now().as_secs();
        let net = crate::state::NET_STATUS.lock(|s| s.get());
        let uni_info = UNIVERSE_STORE.lock(|s| s.borrow().get_info());

        let mut ch_views = [ChannelView {
            mode: ChannelMode::Off,
            loss_behavior: LossBehavior::Blackout,
            loss_timeout_ms: 2500,
            test_mode: 0,
        }; CHANNEL_COUNT];
        for i in 0..CHANNEL_COUNT {
            ch_views[i] = ChannelView {
                mode: cfg.channels[i].mode,
                loss_behavior: cfg.channels[i].loss.behavior,
                loss_timeout_ms: cfg.channels[i].loss.timeout_ms,
                test_mode: crate::state::CHANNEL_TEST_MODES[i].load(core::sync::atomic::Ordering::Relaxed),
            };
        }

        let mut uni_views: heapless::Vec<UniverseView, 16> = heapless::Vec::new();
        for u in &uni_info {
            let active = if let Some(last) = u.last_received {
                Instant::now().duration_since(last).as_millis() <= 2500
            } else {
                false
            };
            let _ = uni_views.push(UniverseView {
                universe: u.universe,
                sequence: u.sequence_number,
                packets: u.packets_received,
                active,
                terminated: u.terminated,
            });
        }

        let test_mode = if crate::state::CHANNEL_TEST_MODES.iter().any(|m| m.load(core::sync::atomic::Ordering::Relaxed) != 0) { 1 } else { 0 };

        let page_data = PageData {
            version: env!("CARGO_PKG_VERSION"),
            uptime_secs: uptime,
            hostname: cfg.hostname.as_str(),
            mac: net.mac,
            link_up: net.link_up,
            ip: net.ip,
            is_fallback: net.is_fallback,
            dhcp_enabled: cfg.net.dhcp,
            static_ip: cfg.net.static_ip,
            static_prefix: cfg.net.static_prefix,
            static_gateway: cfg.net.static_gateway,
            test_mode,
            pwm_freq_hz: cfg.pwm_freq_hz,
            channels: ch_views,
            universes: &uni_views,
        };

        // 1. Head & Styles (streamed directly from Flash)
        writer.write_chunk(sacn_core::web::HEAD_AND_STYLES.as_bytes()).await?;

        // 2. Topbar
        buf.clear();
        let _ = render_topbar(&mut buf, &page_data);
        writer.write_chunk(buf.as_bytes()).await?;

        // 3. Stats Grid
        buf.clear();
        let _ = render_stats_grid(&mut buf, &page_data);
        writer.write_chunk(buf.as_bytes()).await?;

        // 4. Network Settings Card
        buf.clear();
        let _ = render_network_card(&mut buf, &page_data);
        writer.write_chunk(buf.as_bytes()).await?;

        // 5. Output Channels Card Header
        buf.clear();
        let _ = render_channel_table_header(&mut buf, &page_data);
        writer.write_chunk(buf.as_bytes()).await?;

        for ch_idx in 0..CHANNEL_COUNT {
            buf.clear();
            let _ = render_channel_row(&mut buf, ch_idx, &ch_views[ch_idx]);
            writer.write_chunk(buf.as_bytes()).await?;
        }

        buf.clear();
        let _ = render_channel_table_footer(&mut buf);
        writer.write_chunk(buf.as_bytes()).await?;

        // 6. Universes Card
        buf.clear();
        let _ = render_universes_card(&mut buf, &uni_views);
        writer.write_chunk(buf.as_bytes()).await?;

        // 7. Client Script (streamed directly from Flash)
        writer.write_chunk(sacn_core::web::SCRIPT.as_bytes()).await?;

        writer.finalize().await
    }
}

pub async fn get_status() -> ChunkedResponse<StatusPage> {
    ChunkedResponse::new(StatusPage)
}

pub struct StatusJson;

impl Chunks for StatusJson {
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    async fn write_chunks<W: picoserve::io::Write>(
        self,
        mut writer: ChunkWriter<W>,
    ) -> Result<ChunksWritten, W::Error> {
        let uptime = Instant::now().as_secs();
        let net = crate::state::NET_STATUS.lock(|s| s.get());
        let uni_info = UNIVERSE_STORE.lock(|s| s.borrow().get_info());
        let mut uni_views: heapless::Vec<UniverseView, 16> = heapless::Vec::new();
        for u in &uni_info {
            let active = if let Some(last) = u.last_received {
                Instant::now().duration_since(last).as_millis() <= 2500
            } else {
                false
            };
            let _ = uni_views.push(UniverseView {
                universe: u.universe,
                sequence: u.sequence_number,
                packets: u.packets_received,
                active,
                terminated: u.terminated,
            });
        }

        let test_mode = if crate::state::CHANNEL_TEST_MODES.iter().any(|m| m.load(core::sync::atomic::Ordering::Relaxed) != 0) { 1 } else { 0 };
        let mut buf: String<1024> = String::new();
        let _ = render_status_json(&mut buf, uptime, net.link_up, test_mode, &uni_views);
        writer.write_chunk(buf.as_bytes()).await?;
        writer.finalize().await
    }
}

pub async fn get_status_json() -> ChunkedResponse<StatusJson> {
    ChunkedResponse::new(StatusJson)
}

fn default_one() -> u16 {
    1
}
fn default_thirty() -> u16 {
    30
}
fn default_prefix() -> u8 {
    24
}

#[derive(serde::Deserialize, Default)]
pub struct NetworkForm {
    #[serde(default)]
    pub hostname: heapless::String<32>,
    #[serde(default)]
    pub dhcp: heapless::String<8>,
    #[serde(default)]
    pub ip: heapless::String<16>,
    #[serde(default = "default_prefix")]
    pub prefix: u8,
    #[serde(default)]
    pub gateway: heapless::String<16>,
}

#[derive(serde::Deserialize, Default)]
pub struct PwmFreqForm {
    #[serde(default)]
    pub freq: u32,
    #[serde(default)]
    pub pwm_freq: u32,
}

fn parse_channel(
    mode_str: &str,
    uni: u16,
    dmx: u16,
    px: u16,
    loss_str: &str,
) -> (ChannelMode, LossConfig) {
    let mode = match mode_str {
        "ws2812" => ChannelMode::Ws2812 {
            universe: uni.clamp(1, 63999),
            start_channel: dmx.clamp(1, 512),
            pixel_count: px.clamp(1, 1000),
        },
        "ws2812fill" => ChannelMode::Ws2812Fill {
            universe: uni.clamp(1, 63999),
            start_channel: dmx.clamp(1, 512),
            pixel_count: px.clamp(1, 1000),
        },
        "fire" => ChannelMode::Fire {
            pixel_count: px.clamp(1, 1000),
            control: if dmx > 0 {
                Some(DmxSlot {
                    universe: uni.clamp(1, 63999),
                    channel: dmx.clamp(1, 510),
                })
            } else {
                None
            },
        },
        "pwm" => ChannelMode::Pwm {
            universe: uni.clamp(1, 63999),
            dmx_channel: dmx.clamp(1, 512),
        },
        _ => ChannelMode::Off,
    };

    let behavior = match loss_str {
        "hold" => LossBehavior::Hold,
        "fade" => LossBehavior::Fade { ms: 1000 },
        _ => LossBehavior::Blackout,
    };
    let loss = LossConfig {
        timeout_ms: 2500,
        behavior,
    };

    (mode, loss)
}

fn apply_net_html(target: &str) -> (
    [(&'static str, &'static str); 2],
    heapless::String<1200>,
) {
    let mut html = heapless::String::<1200>::new();
    use core::fmt::Write;
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\">\
        <meta http-equiv=\"refresh\" content=\"2; url=http://{target}/\">\
        <title>Applying Network Settings &bull; acid</title>\
        <style>:root{{color-scheme:dark}}body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#090d16;color:#f8fafc;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}}\
        .card{{background:#111827;padding:2rem;border-radius:12px;border:1px solid #1f293d;text-align:center;max-width:440px;box-shadow:0 10px 25px rgba(0,0,0,0.5)}}\
        .spinner{{width:36px;height:36px;border:3px solid #1f293d;border-top-color:#38bdf8;border-radius:50%;animation:spin 1s linear infinite;margin:1.5rem auto}}\
        @keyframes spin{{to{{transform:rotate(360deg)}}}}\
        h1{{font-size:1.25rem;margin:0 0 0.5rem;color:#38bdf8}}\
        p{{font-size:0.9rem;color:#94a3b8;line-height:1.5}}\
        .btn{{display:inline-block;margin-top:1.25rem;padding:0.6rem 1.25rem;background:#0284c7;color:#fff;border-radius:6px;text-decoration:none;font-weight:600;font-size:0.85rem}}\
        .btn:hover{{background:#0369a1}}</style>\
        <script>setTimeout(function(){{location.href=\"http://{target}/\";}},2000);</script>\
        </head><body><div class=\"card\">\
        <h1>Applying Settings</h1>\
        <div class=\"spinner\"></div>\
        <p>Configuring network interface...<br>Redirecting to <strong>http://{target}/</strong></p>\
        <a class=\"btn\" href=\"http://{target}/\">Click here if not redirected</a>\
        </div></body></html>"
    );
    (
        [
            ("Content-Type", "text/html; charset=utf-8"),
            ("Cache-Control", "no-cache, no-store, must-revalidate"),
        ],
        html,
    )
}

pub async fn post_network(
    picoserve::extract::Form(form): picoserve::extract::Form<NetworkForm>,
) -> impl IntoResponse {
    let mut net_changed = false;
    let mut net_cmd = None;
    let mut redirect_target: Option<heapless::String<64>> = None;

    let cfg_to_save = {
        let mut cfg = APP_CONFIG.lock().await;

        let dhcp = form.dhcp.as_str() == "true";
        let static_ip = parse_ipv4(form.ip.as_str()).unwrap_or(cfg.net.static_ip);
        let static_prefix = if form.prefix >= 1 && form.prefix <= 32 {
            form.prefix
        } else {
            cfg.net.static_prefix
        };
        let static_gateway = parse_ipv4(form.gateway.as_str()).or(cfg.net.static_gateway);
        let custom_host = CustomHostname::from_str(form.hostname.as_str());

        if cfg.net.dhcp != dhcp
            || cfg.net.static_ip != static_ip
            || cfg.net.static_prefix != static_prefix
            || cfg.net.static_gateway != static_gateway
            || cfg.hostname != custom_host
        {
            net_changed = true;
            net_cmd = Some(NetUpdateCmd {
                dhcp,
                static_ip,
                static_prefix,
                static_gateway,
                hostname: custom_host,
            });

            if !dhcp {
                let mut s = heapless::String::<64>::new();
                use core::fmt::Write;
                let _ = write!(s, "{}.{}.{}.{}", static_ip[0], static_ip[1], static_ip[2], static_ip[3]);
                redirect_target = Some(s);
            } else {
                let mut s = heapless::String::<64>::new();
                use core::fmt::Write;
                let _ = write!(s, "{}.local", custom_host.as_str());
                redirect_target = Some(s);
            }
        }

        cfg.net = NetConfig {
            dhcp,
            static_ip,
            static_prefix,
            static_gateway,
        };
        cfg.hostname = custom_host;

        *cfg
    };

    let _ = crate::flash::save_config(&cfg_to_save).await;

    if net_changed {
        if let Some(cmd) = net_cmd {
            let _ = NET_UPDATE_CHANNEL.try_send(cmd);
        }
        if let Some(target) = redirect_target {
            return Ok(apply_net_html(target.as_str()));
        }
    }

    Err(picoserve::response::Redirect::to("/"))
}

pub async fn post_pwm_freq(
    picoserve::extract::Form(form): picoserve::extract::Form<PwmFreqForm>,
) -> impl IntoResponse {
    let freq = if form.freq > 0 { form.freq } else { form.pwm_freq };
    if freq >= sacn_core::channel::MIN_PWM_FREQ_HZ && freq <= sacn_core::channel::MAX_PWM_FREQ_HZ {
        let mut cfg = APP_CONFIG.lock().await;
        cfg.pwm_freq_hz = freq;
    }
    "ok"
}

#[derive(serde::Deserialize)]
pub struct ChannelForm {
    pub ch: usize,
    #[serde(default)]
    pub mode: heapless::String<16>,
    #[serde(default = "default_one")]
    pub universe: u16,
    #[serde(default = "default_one")]
    pub dmx_channel: u16,
    #[serde(default = "default_thirty")]
    pub pixel_count: u16,
    #[serde(default)]
    pub loss: heapless::String<16>,
}

pub async fn post_channel(
    picoserve::extract::Form(form): picoserve::extract::Form<ChannelForm>,
) -> impl IntoResponse {
    if form.ch >= 1 && form.ch <= CHANNEL_COUNT {
        let ch_idx = form.ch - 1;
        let mut cfg = APP_CONFIG.lock().await;
        let (mode, loss) = parse_channel(
            form.mode.as_str(),
            form.universe,
            form.dmx_channel,
            form.pixel_count,
            form.loss.as_str(),
        );
        cfg.channels[ch_idx].mode = mode;
        cfg.channels[ch_idx].loss = loss;
    }
    "ok"
}

pub async fn post_save_outputs() -> impl IntoResponse {
    let cfg = {
        let cfg_guard = APP_CONFIG.lock().await;
        *cfg_guard
    };
    match crate::flash::save_config(&cfg).await {
        Ok(()) => (picoserve::response::StatusCode::OK, "ok"),
        Err(()) => (picoserve::response::StatusCode::INTERNAL_SERVER_ERROR, "error"),
    }
}

pub async fn post_save_outputs_form() -> impl IntoResponse {
    let cfg = {
        let cfg_guard = APP_CONFIG.lock().await;
        *cfg_guard
    };
    let _ = crate::flash::save_config(&cfg).await;
    picoserve::response::Redirect::to("/")
}

pub async fn post_group() -> impl IntoResponse {
    picoserve::response::Redirect::to("/")
}

pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in s.trim().split('.') {
        if count >= 4 {
            return None;
        }
        let val = part.parse::<u8>().ok()?;
        octets[count] = val;
        count += 1;
    }
    if count == 4 {
        Some(octets)
    } else {
        None
    }
}

#[derive(serde::Deserialize, Default)]
pub struct ChannelTestForm {
    #[serde(default)]
    pub ch: usize,
    #[serde(default)]
    pub test: u8,
    #[serde(default)]
    pub mode: u8,
    #[serde(default)]
    pub ch_mode: heapless::String<16>,
    #[serde(default)]
    pub pixels: Option<u16>,
}

pub async fn post_channel_test(
    picoserve::extract::Form(form): picoserve::extract::Form<ChannelTestForm>,
) -> impl IntoResponse {
    if form.ch >= 1 && form.ch <= CHANNEL_COUNT {
        let raw_mode = if form.test != 0 { form.test } else { form.mode };
        let mut pattern = sacn_core::test_pattern::TestPattern::from_u8(raw_mode);
        let ch_idx = form.ch - 1;

        let ch_mode_str = form.ch_mode.as_str();
        let applicable = match ch_mode_str {
            "ws2812" | "ws2812fill" | "fire" => true,
            "pwm" => matches!(
                pattern,
                sacn_core::test_pattern::TestPattern::Off
                    | sacn_core::test_pattern::TestPattern::FullWhite
                    | sacn_core::test_pattern::TestPattern::Breathe
            ),
            "off" => false,
            _ => true,
        };
        if !applicable {
            pattern = sacn_core::test_pattern::TestPattern::Off;
        }

        crate::state::CHANNEL_TEST_MODES[ch_idx]
            .store(pattern.as_u8(), core::sync::atomic::Ordering::Relaxed);

        if pattern == sacn_core::test_pattern::TestPattern::Off {
            crate::state::CHANNEL_TEST_PIXELS[ch_idx].store(0, core::sync::atomic::Ordering::Relaxed);
        } else if let Some(px) = form.pixels {
            if px > 0 {
                crate::state::CHANNEL_TEST_PIXELS[ch_idx].store(px, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    "ok"
}

#[derive(serde::Deserialize, Default)]
pub struct TestForm {
    #[serde(default)]
    pub mode: u8,
}

pub async fn post_test(
    picoserve::extract::Form(form): picoserve::extract::Form<TestForm>,
) -> impl IntoResponse {
    let pattern = sacn_core::test_pattern::TestPattern::from_u8(form.mode);
    for i in 0..CHANNEL_COUNT {
        crate::state::CHANNEL_TEST_MODES[i].store(pattern.as_u8(), core::sync::atomic::Ordering::Relaxed);
        if pattern == sacn_core::test_pattern::TestPattern::Off {
            crate::state::CHANNEL_TEST_PIXELS[i].store(0, core::sync::atomic::Ordering::Relaxed);
        }
    }
    picoserve::response::Redirect::to("/")
}

pub fn router() -> picoserve::Router<impl picoserve::routing::PathRouter> {
    picoserve::Router::new()
        .route("/", get(get_status))
        .route("/api/status", get(get_status_json))
        .route("/api/test/channel", post(post_channel_test))
        .route("/api/channel", post(post_channel))
        .route("/api/pwm_freq", post(post_pwm_freq))
        .route("/api/outputs/save", post(post_save_outputs))
        .route("/outputs/save", post(post_save_outputs_form))
        .route("/outputs", post(post_save_outputs_form))
        .route("/network", post(post_network))
        .route("/channel", post(post_channel))
        .route("/group", post(post_group))
        .route("/test", post(post_test))
}
