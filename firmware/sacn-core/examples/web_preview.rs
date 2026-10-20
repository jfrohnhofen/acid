//! Host-side web preview server for rapid UI development and styling.
//!
//! Run with: `cargo run -p sacn-core --example web_preview`
//! Then open: `http://127.0.0.1:8080/` in your browser.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Instant;

use sacn_core::channel::{ChannelMode, DmxSlot};
use sacn_core::loss::LossBehavior;

use sacn_core::web::{
    render_channel_row, render_channel_table_footer, render_channel_table_header,
    render_head_and_styles, render_network_card, render_script, render_stats_grid,
    render_topbar, render_universes_card, ChannelView, PageData,
    UniverseView, CHANNEL_COUNT,
};

struct ServerState {
    start_time: Instant,
    hostname: String,
    dhcp_enabled: bool,
    static_ip: [u8; 4],
    static_prefix: u8,
    static_gateway: Option<[u8; 4]>,
    test_mode: u8,
    pwm_freq_hz: u32,
    channels: [ChannelView; CHANNEL_COUNT],
    universes: Vec<UniverseView>,
}

static STATE: Mutex<Option<ServerState>> = Mutex::new(None);


fn main() {
    let mut initial_channels = [ChannelView {
        mode: ChannelMode::Off,
        loss_behavior: LossBehavior::Blackout,
        loss_timeout_ms: 2500,
        test_mode: 0,
    }; CHANNEL_COUNT];

    // Out-of-the-box: Channel 1 has Fire effect
    initial_channels[0].mode = ChannelMode::Fire {
        pixel_count: 30,
        control: None,
    };
    initial_channels[1].mode = ChannelMode::Ws2812 {
        universe: 1,
        start_channel: 1,
        pixel_count: 30,
    };

    let initial_universes = vec![
        UniverseView {
            universe: 1,
            sequence: 42,
            packets: 1250,
            active: true,
            terminated: false,
        },
        UniverseView {
            universe: 2,
            sequence: 18,
            packets: 340,
            active: false,
            terminated: false,
        },
    ];

    *STATE.lock().unwrap() = Some(ServerState {
        start_time: Instant::now(),
        hostname: "acid-3498cb".to_string(),
        dhcp_enabled: true,
        static_ip: [169, 254, 246, 176],
        static_prefix: 16,
        static_gateway: None,
        test_mode: 0,
        pwm_freq_hz: 1000,
        channels: initial_channels,
        universes: initial_universes,
    });

    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).expect("failed to bind address");
    println!("\n🧪 ACID Web Preview running on http://{}\n", addr);

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            handle_connection(stream);
        }
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0u8; 4096];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let req = String::from_utf8_lossy(&buffer[..bytes_read]);
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    if method == "POST" {
        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
        handle_post(path, body);
        let resp = if path.starts_with("/api/") {
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
        } else {
            "HTTP/1.1 303 See Other\r\nLocation: /\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        };
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    if path == "/api/status" {
        let mut state_guard = STATE.lock().unwrap();
        let state = state_guard.as_mut().unwrap();
        let uptime = state.start_time.elapsed().as_secs();
        let mut json = String::new();
        let _ = sacn_core::web::render_status_json(&mut json, uptime, true, state.test_mode, &state.universes);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            json.len(),
            json
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    // GET / -> render page
    let mut state_guard = STATE.lock().unwrap();
    let state = state_guard.as_mut().unwrap();
    let uptime = state.start_time.elapsed().as_secs();

    let page_data = PageData {
        version: "1.0.0-preview",
        uptime_secs: uptime,
        hostname: &state.hostname,
        mac: [0x12, 0x08, 0x8f, 0x34, 0x98, 0xcb],
        link_up: true,
        ip: Some((state.static_ip, state.static_prefix)),
        is_fallback: state.dhcp_enabled,
        dhcp_enabled: state.dhcp_enabled,
        static_ip: state.static_ip,
        static_prefix: state.static_prefix,
        static_gateway: state.static_gateway,
        test_mode: state.test_mode,
        pwm_freq_hz: state.pwm_freq_hz,
        channels: state.channels,
        universes: &state.universes,
    };

    let mut html = String::new();
    let _ = render_head_and_styles(&mut html);
    let _ = render_topbar(&mut html, &page_data);
    let _ = render_stats_grid(&mut html, &page_data);
    let _ = render_network_card(&mut html, &page_data);
    let _ = render_channel_table_header(&mut html, &page_data);

    for ch_idx in 0..CHANNEL_COUNT {
        let _ = render_channel_row(&mut html, ch_idx, &state.channels[ch_idx]);
    }
    let _ = render_channel_table_footer(&mut html);

    let _ = render_universes_card(&mut html, &state.universes);
    let _ = render_script(&mut html);

    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(resp.as_bytes());
}

fn handle_post(path: &str, body: &str) {
    let mut params = std::collections::HashMap::new();
    for pair in body.split('&') {
        let mut kv = pair.split('=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            params.insert(k, v);
        }
    }

    let mut state_guard = STATE.lock().unwrap();
    let state = state_guard.as_mut().unwrap();

    match path {
        "/api/pwm_freq" => {
            if let Some(freq_str) = params.get("freq").or_else(|| params.get("pwm_freq")) {
                if let Ok(freq) = freq_str.parse::<u32>() {
                    state.pwm_freq_hz = freq.clamp(100, 20000);
                    println!("Global PWM freq updated: {} Hz", state.pwm_freq_hz);
                }
            }
        }
        "/api/channel" | "/channel" => {
            let ch_num: usize = params.get("ch").and_then(|v| v.parse().ok()).unwrap_or(1);
            if ch_num >= 1 && ch_num <= CHANNEL_COUNT {
                let ch_idx = ch_num - 1;
                let mode_str = params.get("mode").copied().unwrap_or("off");
                let uni: u16 = params.get("universe").and_then(|v| v.parse().ok()).unwrap_or(1);
                let dmx: u16 = params.get("dmx_channel").and_then(|v| v.parse().ok()).unwrap_or(1);
                let px: u16 = params.get("pixel_count").and_then(|v| v.parse().ok()).unwrap_or(30);
                let loss_str = params.get("loss").copied().unwrap_or("blackout");

                let mode = match mode_str {
                    "ws2812" => ChannelMode::Ws2812 { universe: uni, start_channel: dmx, pixel_count: px },
                    "ws2812fill" => ChannelMode::Ws2812Fill { universe: uni, start_channel: dmx, pixel_count: px },
                    "fire" => ChannelMode::Fire { pixel_count: px, control: if dmx > 0 { Some(DmxSlot { universe: uni, channel: dmx }) } else { None } },
                    "pwm" => ChannelMode::Pwm { universe: uni, dmx_channel: dmx },
                    _ => ChannelMode::Off,
                };
                let loss_behavior = match loss_str {
                    "hold" => LossBehavior::Hold,
                    "fade" => LossBehavior::Fade { ms: 1000 },
                    _ => LossBehavior::Blackout,
                };
                let prev_test = state.channels[ch_idx].test_mode;
                state.channels[ch_idx] = ChannelView {
                    mode,
                    loss_behavior,
                    loss_timeout_ms: 2500,
                    test_mode: prev_test,
                };
                println!("Channel {} updated: {:?}", ch_num, mode);
            }
        }
        "/api/outputs/save" | "/outputs/save" => {
            println!("Outputs configuration persisted to flash!");
        }
        "/network" => {
            if let Some(host) = params.get("hostname") {
                state.hostname = host.to_string();
            }
            if let Some(dhcp) = params.get("dhcp") {
                state.dhcp_enabled = *dhcp == "true";
            }
            if let Some(ip_str) = params.get("ip") {
                let parts: Vec<u8> = ip_str.split('.').filter_map(|s| s.parse().ok()).collect();
                if parts.len() == 4 {
                    state.static_ip = [parts[0], parts[1], parts[2], parts[3]];
                }
            }
            if let Some(p_str) = params.get("prefix") {
                if let Ok(p) = p_str.parse::<u8>() {
                    state.static_prefix = p.clamp(1, 32);
                }
            }
            println!("Network updated: hostname={}, dhcp={}", state.hostname, state.dhcp_enabled);
        }
        "/test" => {
            let mode: u8 = params.get("mode").and_then(|v| v.parse().ok()).unwrap_or(0);
            state.test_mode = mode;
            for ch in &mut state.channels {
                ch.test_mode = mode;
            }
            println!("Test mode updated to {}", mode);
        }
        "/api/test/channel" => {
            let ch_num: usize = params.get("ch").and_then(|v| v.parse().ok()).unwrap_or(1);
            let mode: u8 = params.get("test").or_else(|| params.get("mode")).and_then(|v| v.parse().ok()).unwrap_or(0);
            if ch_num >= 1 && ch_num <= CHANNEL_COUNT {
                state.channels[ch_num - 1].test_mode = mode;
                println!("Channel {} test mode updated to {}", ch_num, mode);
            }
        }
        _ => {}
    }
}
