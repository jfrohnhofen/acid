use crate::channel::{ChannelMode, GroupClass};
use crate::loss::LossBehavior;


pub const CHANNEL_COUNT: usize = 8;
pub const GROUP_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelView {
    pub mode: ChannelMode,
    pub loss_behavior: LossBehavior,
    pub loss_timeout_ms: u32,
    pub test_mode: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupView {
    pub class: GroupClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniverseView {
    pub universe: u16,
    pub sequence: u8,
    pub packets: u32,
    pub active: bool,
    pub terminated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PageData<'a> {
    pub version: &'a str,
    pub uptime_secs: u64,
    pub hostname: &'a str,
    pub mac: [u8; 6],
    pub link_up: bool,
    pub ip: Option<([u8; 4], u8)>,
    pub is_fallback: bool,
    pub dhcp_enabled: bool,
    pub static_ip: [u8; 4],
    pub static_prefix: u8,
    pub static_gateway: Option<[u8; 4]>,
    pub test_mode: u8,
    pub pwm_freq_hz: u32,
    pub channels: [ChannelView; CHANNEL_COUNT],
    pub universes: &'a [UniverseView],
}

pub const HEAD_AND_STYLES: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>acid &bull; sACN Controller</title>\
<link rel=\"icon\" href=\"data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>⚡</text></svg>\"><style>\
:root{color-scheme:dark;\
--bg:#090d16;--surface:#111827;--surface-card:#162032;--surface-hdr:#1a273e;\
--border:#26354a;--border-light:#334460;--text:#f1f5f9;--text-muted:#94a3b8;\
--primary:#facc15;--primary-hover:#eab308;--primary-text:#0f172a;\
--cyan:#38bdf8;--orange:#f97316;--green:#10b981;--red:#ef4444;--purple:#a855f7;\
}\
*{box-sizing:border-box}body{\
font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,sans-serif;\
background:var(--bg);color:var(--text);margin:0;padding:1.5rem;line-height:1.5;\
}\
.container{max-width:1160px;margin:0 auto}\
.topbar{display:flex;align-items:center;justify-content:space-between;padding:1rem 1.25rem;\
background:var(--surface);border:1px solid var(--border);border-radius:12px;margin-bottom:1.5rem;\
box-shadow:0 4px 12px rgba(0,0,0,0.3);flex-wrap:wrap;gap:1rem}\
.brand{display:flex;align-items:center;gap:0.75rem}\
.brand-logo{font-size:2rem;line-height:1}\
.brand-title{font-size:1.35rem;font-weight:800;letter-spacing:-0.02em;margin:0;display:flex;align-items:center;gap:0.4rem}\
.brand-title span{color:var(--primary)}\
.brand-sub{font-size:0.8rem;color:var(--text-muted);margin:0}\
.topbar-pills{display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap}\
.pill{display:inline-flex;align-items:center;gap:0.4rem;padding:4px 10px;border-radius:9999px;\
font-size:0.75rem;font-weight:600;font-family:monospace;letter-spacing:0.02em}\
.pill-green{background:#064e3b;color:#34d399;border:1px solid #059669}\
.pill-red{background:#450a0a;color:#f87171;border:1px solid #dc2626}\
.pill-yellow{background:#422006;color:#facc15;border:1px solid #ca8a04}\
.pill-cyan{background:#0c4a6e;color:#38bdf8;border:1px solid #0284c7}\
.pill-gray{background:#1e293b;color:#94a3b8;border:1px solid var(--border)}\
.pulse{width:7px;height:7px;border-radius:50%;background:#34d399;animation:blink 1.8s infinite}\
@keyframes blink{0%,100%{opacity:1}50%{opacity:0.3}}\
.grid-stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:1rem;margin-bottom:1.5rem}\
.stat-card{background:var(--surface);border:1px solid var(--border);border-radius:10px;padding:0.9rem 1.15rem;\
box-shadow:0 2px 8px rgba(0,0,0,0.2);display:flex;flex-direction:column;gap:0.65rem;justify-content:center}\
.stat-item{display:flex;flex-direction:column;gap:0.2rem}\
.stat-label{font-size:0.68rem;text-transform:uppercase;color:var(--text-muted);letter-spacing:0.05em;font-weight:700}\
.stat-val{font-size:1rem;font-weight:700;color:#fff;word-break:break-all;display:flex;align-items:center;gap:0.45rem;flex-wrap:wrap}\
.stat-mono{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:0.92rem;font-weight:600;letter-spacing:0.02em}\
.card{background:var(--surface);border:1px solid var(--border);border-radius:12px;margin-bottom:1.5rem;\
box-shadow:0 4px 12px rgba(0,0,0,0.25)}\
.card-hdr{padding:1rem 1.25rem;border-bottom:1px solid var(--border);background:var(--surface-hdr);\
border-top-left-radius:11px;border-top-right-radius:11px;\
display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:0.5rem}\
.card-body{padding:1.25rem}\
.net-form{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:1rem;align-items:end}\
.f-grp label{display:block;font-size:0.75rem;font-weight:600;color:var(--text-muted);margin-bottom:0.35rem;text-transform:uppercase}\
.pwm-freq-wrap{transition:opacity 0.2s}\
.pwm-freq-wrap.is-disabled{opacity:0.35;pointer-events:none}\
input[type=\"text\"],input[type=\"number\"]{\
width:100%;background:var(--bg);color:var(--text);border:1px solid var(--border-light);\
border-radius:6px;padding:7px 10px;font-size:0.875rem;transition:border-color 0.15s,box-shadow 0.15s;\
}\
select{display:none!important}\
.c-select{position:relative;display:inline-block;width:100%}\
.c-select-btn{\
width:100%;background:var(--bg);color:var(--text);border:1px solid var(--border-light);\
border-radius:6px;padding:7px 32px 7px 10px;font-size:0.875rem;font-weight:500;cursor:pointer;\
display:flex;align-items:center;justify-content:space-between;text-align:left;position:relative;\
user-select:none;transition:border-color 0.15s,box-shadow 0.15s;outline:none;overflow:hidden;\
background-image:url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2394a3b8' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E\");\
background-repeat:no-repeat;background-position:right 10px center;\
}\
.c-select-btn span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;display:block}\
.c-select-btn:hover{border-color:var(--border-light);background-color:#111a2c}\
.c-select.open .c-select-btn,.c-select-btn:focus{border-color:var(--primary);box-shadow:0 0 0 2px rgba(250,204,21,0.25);\
background-image:url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23facc15' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='18 15 12 9 6 15'%3E%3C/polyline%3E%3C/svg%3E\")}\
.c-select-menu{\
display:none;position:absolute;top:calc(100% + 4px);left:0;min-width:100%;\
background:#111a2c;border:1px solid var(--border-light);border-radius:8px;\
box-shadow:0 12px 28px rgba(0,0,0,0.65);z-index:100;max-height:220px;overflow-y:auto;padding:4px;\
}\
.c-select.open .c-select-menu{display:block}\
.c-select-item{\
padding:7px 10px;border-radius:5px;font-size:0.85rem;color:var(--text);\
cursor:pointer;display:flex;align-items:center;justify-content:space-between;\
transition:background 0.1s,color 0.1s;white-space:nowrap;\
}\
.c-select-item:hover{background:#1e2d47;color:var(--primary)}\
.c-select-item.selected{background:#1e293b;color:var(--primary);font-weight:700}\
input:disabled{opacity:0.42;cursor:not-allowed;background:#0d121c;border-color:var(--border)}\
input[type=\"number\"]{\
font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;\
font-variant-numeric:tabular-nums;\
font-size:0.92rem;font-weight:600;text-align:center;\
-moz-appearance:textfield;letter-spacing:0.03em;\
}\
input[type=\"number\"]::-webkit-outer-spin-button,\
input[type=\"number\"]::-webkit-inner-spin-button{-webkit-appearance:none;margin:0}\
.btn{background:var(--primary);color:var(--primary-text);border:none;border-radius:6px;padding:7px 14px;\
font-size:0.85rem;font-weight:700;cursor:pointer;transition:all 0.15s;text-align:center}\
.btn:hover{background:var(--primary-hover);transform:translateY(-1px)}\
.btn-sm{padding:4px 10px;font-size:0.78rem}\
.btn-lg{padding:9px 24px;font-size:0.95rem}\
.table{width:100%;border-collapse:collapse;font-size:0.875rem;text-align:left}\
.table-fixed{table-layout:fixed}\
.table th{background:var(--surface-hdr);color:var(--text-muted);font-weight:600;padding:0.75rem 1rem;\
border-bottom:1px solid var(--border);font-size:0.78rem;text-transform:uppercase;letter-spacing:0.04em}\
.table td{padding:0.65rem 1rem;border-bottom:1px solid var(--border);vertical-align:middle;position:relative}\
.table tbody tr:hover td{background:#152238}\
.grp-hdr{background:#182640!important;border-top:10px solid var(--bg)!important;box-shadow:inset 0 1px 0 rgba(255,255,255,0.06)}\
.grp-hdr td{padding:0.75rem 1rem!important}\
.grp-hdr-inner{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:0.75rem}\
.grp-tag{font-weight:700;color:#f8fafc;font-size:0.92rem;display:flex;align-items:center;gap:0.5rem}\
.ch-badge{display:inline-flex;align-items:center;gap:0.4rem;font-weight:700;font-size:0.9rem}\
.dot{width:8px;height:8px;border-radius:50%;display:inline-block}\
.dot-fire{background:var(--orange);box-shadow:0 0 6px var(--orange)}\
.dot-ws{background:var(--cyan);box-shadow:0 0 6px var(--cyan)}\
.dot-fill{background:var(--purple);box-shadow:0 0 6px var(--purple)}\
.dot-pwm{background:var(--primary);box-shadow:0 0 6px var(--primary)}\
.dot-off{background:#475569}\
.ch-row.is-testing td{background:rgba(239,68,68,0.14)!important}\
.ch-row.is-testing:hover td{background:rgba(239,68,68,0.22)!important}\
.ch-row.is-testing .ch-badge{color:#f87171}\
.ch-row.is-testing .dot{background:#ef4444!important;box-shadow:0 0 8px #ef4444!important}\
.cell-na{display:none;color:var(--text-muted);font-size:1.1rem;text-align:center;line-height:34px;height:34px}\
.ch-row.mode-off .cell-uni input,\
.ch-row.mode-off .cell-dmx input,\
.ch-row.mode-off .cell-px input,\
.ch-row.mode-off .cell-loss select,\
.ch-row.mode-off .cell-loss .c-select,\
.ch-row.mode-off .cell-test select,\
.ch-row.mode-off .cell-test .c-select,\
.ch-row.mode-pwm .cell-px input{display:none!important}\
.ch-row.mode-off .cell-uni .cell-na,\
.ch-row.mode-off .cell-dmx .cell-na,\
.ch-row.mode-off .cell-px .cell-na,\
.ch-row.mode-off .cell-loss .cell-na,\
.ch-row.mode-off .cell-test .cell-na,\
.ch-row.mode-pwm .cell-px .cell-na{display:block!important}\
</style></head><body><div class=\"container\">";

pub fn render_head_and_styles<W: core::fmt::Write>(w: &mut W) -> core::fmt::Result {
    w.write_str(HEAD_AND_STYLES)
}

pub fn render_topbar<W: core::fmt::Write>(w: &mut W, data: &PageData<'_>) -> core::fmt::Result {
    let test_pill = if data.test_mode != 0 {
        "<span class=\"pill pill-yellow\" style=\"animation:blink 1.8s infinite\"><span class=\"pulse\"></span> TEST MODE ACTIVE</span> "
    } else {
        ""
    };

    write!(
        w,
        "<div class=\"topbar\">\
         <div class=\"brand\"><div class=\"brand-logo\">🧪</div>\
         <div><div class=\"brand-title\">acid <span>sACN</span></div>\
         <div class=\"brand-sub\">8-Channel sACN E1.31 Output Controller</div></div></div>\
         <div class=\"topbar-pills\">{}<span class=\"pill pill-yellow\">{}.local</span></div></div>",
        test_pill,
        data.hostname
    )
}

pub fn render_stats_grid<W: core::fmt::Write>(w: &mut W, data: &PageData<'_>) -> core::fmt::Result {
    let uptime = data.uptime_secs;
    let m = data.mac;

    write!(
        w,
        "<div class=\"grid-stats\">\
         <div class=\"stat-card\">\
         <div class=\"stat-item\"><div class=\"stat-label\">IPv4 Address</div>\
         <div class=\"stat-val\">"
    )?;

    match data.ip {
        Some((ip, pfx)) => {
            let mode = if data.is_fallback {
                "<span class=\"pill pill-gray\" style=\"color:var(--text-muted);font-weight:600;font-size:0.7rem;padding:2px 7px\">fallback</span>"
            } else if data.dhcp_enabled {
                "<span class=\"pill pill-green\">DHCP</span>"
            } else {
                "<span class=\"pill pill-cyan\">Static</span>"
            };
            write!(w, "{}.{}.{}.{}/{} {}", ip[0], ip[1], ip[2], ip[3], pfx, mode)?;
        }
        None => {
            w.write_str("None <span class=\"pill pill-red\">Disconnected</span>")?;
        }
    }

    write!(
        w,
        "</div></div>\
         <div class=\"stat-item\"><div class=\"stat-label\">MAC Address</div>\
         <div class=\"stat-val stat-mono\">{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}</div></div>\
         </div>\
         <div class=\"stat-card\">\
         <div class=\"stat-item\"><div class=\"stat-label\">mDNS</div>\
         <div class=\"stat-val\">{}.local</div></div>\
         <div class=\"stat-item\"><div class=\"stat-label\">Ports</div>\
         <div class=\"stat-val\" style=\"font-size:0.95rem;font-weight:600\">80 <span style=\"font-size:0.75rem;color:var(--text-muted)\">HTTP</span> &bull; 5568 <span style=\"font-size:0.75rem;color:var(--text-muted)\">sACN</span></div></div>\
         </div>\
         <div class=\"stat-card\">\
         <div class=\"stat-item\"><div class=\"stat-label\">System Uptime</div>\
         <div class=\"stat-val\" id=\"stat-uptime-val\">{}h {:02}m {:02}s</div></div>\
         <div class=\"stat-item\"><div class=\"stat-label\">Firmware Version</div>\
         <div class=\"stat-val stat-mono\" style=\"color:var(--text-muted);font-weight:600\">v{}</div></div>\
         </div>\
         </div>",
        m[0], m[1], m[2], m[3], m[4], m[5],
        data.hostname,
        uptime / 3600,
        (uptime / 60) % 60,
        uptime % 60,
        data.version
    )
}


pub fn render_network_card<W: core::fmt::Write>(w: &mut W, data: &PageData<'_>) -> core::fmt::Result {
    let sel_dhcp = if data.dhcp_enabled { " selected" } else { "" };
    let sel_static = if !data.dhcp_enabled { " selected" } else { "" };
    let s_ip = data.static_ip;
    let static_dis = if data.dhcp_enabled { " disabled" } else { "" };

    write!(
        w,
        "<div class=\"card\"><div class=\"card-hdr\"><h2 class=\"card-title\"><span>🌐</span> Network Settings</h2></div>\
         <form method=\"POST\" action=\"/network\">\
         <div class=\"card-body\">\
         <div class=\"net-form\">\
         <div class=\"f-grp\"><label>Hostname (.local)</label><input name=\"hostname\" type=\"text\" value=\"{}\" maxlength=\"32\"></div>\
         <div class=\"f-grp\"><label>IP Assignment</label><select name=\"dhcp\" id=\"net-dhcp-sel\">\
         <option value=\"true\"{}>DHCP (with link-local fallback)</option>\
         <option value=\"false\"{}>Static IPv4</option></select></div>\
         <div class=\"f-grp\"><label>Static IPv4</label><input name=\"ip\" id=\"net-ip\" type=\"text\" value=\"{}.{}.{}.{}\"{}></div>\
         <div class=\"f-grp\"><label>Subnet Prefix (/N)</label><input name=\"prefix\" id=\"net-prefix\" type=\"number\" min=\"1\" max=\"32\" value=\"{}\"{}></div>\
         </div></div>\
         <div style=\"padding:1rem 1.25rem;background:var(--surface-hdr);border-top:1px solid var(--border);border-bottom-left-radius:11px;border-bottom-right-radius:11px;display:flex;justify-content:flex-end\">\
         <button type=\"submit\" class=\"btn btn-lg\">Save</button>\
         </div></form></div>",
        data.hostname,
        sel_dhcp,
        sel_static,
        s_ip[0], s_ip[1], s_ip[2], s_ip[3],
        static_dis,
        data.static_prefix,
        static_dis
    )
}

pub fn render_settings_header<W: core::fmt::Write>(w: &mut W, data: &PageData<'_>) -> core::fmt::Result {
    render_network_card(w, data)
}

pub fn render_channel_table_header<W: core::fmt::Write>(w: &mut W, data: &PageData<'_>) -> core::fmt::Result {
    let any_pwm = data.channels.iter().any(|ch| matches!(ch.mode, ChannelMode::Pwm { .. }));
    let (pwm_cls, pwm_dis) = if any_pwm {
        ("", "")
    } else {
        (" is-disabled", " disabled")
    };

    write!(
        w,
        "<div class=\"card\">\
         <div class=\"card-hdr\">\
         <div style=\"display:flex;align-items:center;gap:0.75rem\">\
         <h2 class=\"card-title\"><span>🎛️</span> Output Configuration</h2>\
         <span id=\"save-pill\" class=\"pill pill-yellow\" style=\"font-size:0.72rem;padding:3px 10px;opacity:0;transition:opacity 0.25s\"></span>\
         </div>\
         <div id=\"pwm-freq-wrap\" class=\"pwm-freq-wrap{pwm_cls}\" style=\"display:flex;align-items:center;gap:0.6rem\">\
         <label style=\"font-size:0.75rem;font-weight:700;color:var(--text-muted);text-transform:uppercase\">PWM Frequency:</label>\
         <input type=\"number\" id=\"pwm-freq-input\" name=\"pwm_freq\" min=\"100\" max=\"20000\" value=\"{}\" style=\"width:90px;padding:4px 8px;font-size:0.85rem\"{pwm_dis}>\
         <span style=\"font-size:0.75rem;color:var(--text-muted);font-weight:600\">Hz</span>\
         </div></div>\
         <form id=\"outputs-form\" method=\"POST\" action=\"/outputs/save\" onsubmit=\"return saveOutputs(event);\">\
         <table class=\"table table-fixed\"><thead><tr>\
         <th style=\"width:105px\">Output</th>\
         <th style=\"width:200px\">Mode</th>\
         <th style=\"width:85px;text-align:center\">Universe</th>\
         <th style=\"width:85px;text-align:center\">DMX Start</th>\
         <th style=\"width:75px;text-align:center\">Pixels</th>\
         <th style=\"width:125px;text-align:center\">DMX End</th>\
         <th style=\"width:120px\">Loss Behavior</th>\
         <th style=\"width:155px\">Test Pattern</th>\
         </tr></thead><tbody>",
        data.pwm_freq_hz
    )
}

pub fn render_channel_table_footer<W: core::fmt::Write>(w: &mut W) -> core::fmt::Result {
    write!(
        w,
        "</tbody></table>\
         <div style=\"padding:1rem 1.25rem;background:var(--surface-hdr);border-top:1px solid var(--border);border-bottom-left-radius:11px;border-bottom-right-radius:11px;display:flex;justify-content:flex-end\">\
         <button type=\"submit\" id=\"save-outputs-btn\" class=\"btn btn-lg\">Save</button>\
         </div></form></div>"
    )
}

pub fn render_settings_footer<W: core::fmt::Write>(w: &mut W) -> core::fmt::Result {
    render_channel_table_footer(w)
}

pub fn render_test_card<W: core::fmt::Write>(_w: &mut W, _test_mode: u8) -> core::fmt::Result {
    Ok(())
}

pub fn render_group_header<W: core::fmt::Write>(
    _w: &mut W,
    _grp_idx: usize,
    _grp: &GroupView,
) -> core::fmt::Result {
    Ok(())
}

pub fn render_channel_row<W: core::fmt::Write>(
    w: &mut W,
    ch_idx: usize,
    ch: &ChannelView,
) -> core::fmt::Result {
    let ch_num = ch_idx + 1;
    let (uni, dmx, px) = match ch.mode {
        ChannelMode::Ws2812 { universe, start_channel, pixel_count, .. } => {
            (universe, start_channel, pixel_count)
        }
        ChannelMode::Ws2812Fill { universe, start_channel, pixel_count } => {
            (universe, start_channel, pixel_count)
        }
        ChannelMode::Fire { pixel_count, control } => {
            (control.map(|c| c.universe).unwrap_or(1), control.map(|c| c.channel).unwrap_or(1), pixel_count)
        }
        ChannelMode::Pwm { universe, dmx_channel } => (universe, dmx_channel, 30),
        ChannelMode::Off => (1, 1, 30),
    };

    let (dot_class, mode_str) = match ch.mode {
        ChannelMode::Off => ("dot-off", "off"),
        ChannelMode::Ws2812 { .. } => ("dot-ws", "ws2812"),
        ChannelMode::Ws2812Fill { .. } => ("dot-fill", "ws2812fill"),
        ChannelMode::Fire { .. } => ("dot-fire", "fire"),
        ChannelMode::Pwm { .. } => ("dot-pwm", "pwm"),
    };

    let is_off = matches!(ch.mode, ChannelMode::Off);
    let is_ws = matches!(ch.mode, ChannelMode::Ws2812 { .. });
    let is_fill = matches!(ch.mode, ChannelMode::Ws2812Fill { .. });
    let is_fire = matches!(ch.mode, ChannelMode::Fire { .. });
    let is_pwm = matches!(ch.mode, ChannelMode::Pwm { .. });

    let is_blackout = matches!(ch.loss_behavior, LossBehavior::Blackout);
    let is_hold = matches!(ch.loss_behavior, LossBehavior::Hold);
    let is_fade = matches!(ch.loss_behavior, LossBehavior::Fade { .. });

    let is_t0 = ch.test_mode == 0;
    let is_t1 = ch.test_mode == 1;
    let is_t2 = ch.test_mode == 2;
    let is_t3 = ch.test_mode == 3;
    let is_t4 = ch.test_mode == 4;
    let is_t5 = ch.test_mode == 5;
    let test_cls = if is_t0 { "" } else { " is-testing" };

    write!(
        w,
        "<tr class=\"ch-row mode-{mode_str}{test_cls}\" data-ch=\"{ch_num}\">\
         <td><span class=\"ch-badge\"><span class=\"dot {dot_class}\"></span> Output {ch_num}</span></td>\
         <td><select name=\"m{ch_idx}\">\
         <option value=\"off\"{}>Off</option>\
         <option value=\"ws2812\"{}>WS2812B RGB</option>\
         <option value=\"ws2812fill\"{}>WS2812B Fill</option>\
         <option value=\"fire\"{}>Fire Effect</option>\
         <option value=\"pwm\"{}>PWM Dimmer</option>\
         </select></td>\
         <td class=\"cell-uni\"><input type=\"number\" name=\"u{ch_idx}\" min=\"1\" max=\"63999\" value=\"{uni}\"><span class=\"cell-na\">&mdash;</span></td>\
         <td class=\"cell-dmx\"><input type=\"number\" name=\"d{ch_idx}\" min=\"1\" max=\"512\" value=\"{dmx}\"><span class=\"cell-na\">&mdash;</span></td>\
         <td class=\"cell-px\"><input type=\"number\" name=\"p{ch_idx}\" min=\"1\" max=\"1000\" value=\"{px}\"><span class=\"cell-na\">&mdash;</span></td>",
        if is_off { " selected" } else { "" },
        if is_ws { " selected" } else { "" },
        if is_fill { " selected" } else { "" },
        if is_fire { " selected" } else { "" },
        if is_pwm { " selected" } else { "" }
    )?;

    // DMX End calculation
    let (end_u, end_ch, ch_count) = match ch.mode {
        ChannelMode::Off => (0, 0, 0),
        ChannelMode::Pwm { universe, dmx_channel } => (universe, dmx_channel, 1),
        ChannelMode::Fire { control, .. } => {
            if let Some(c) = control {
                (c.universe, (c.channel + 2).min(512), 3)
            } else {
                (0, 0, 0)
            }
        }
        ChannelMode::Ws2812Fill { universe, start_channel, .. } => {
            (universe, (start_channel + 2).min(512), 3)
        }
        ChannelMode::Ws2812 { universe, start_channel, pixel_count } => {
            let total = (pixel_count as usize) * 3;
            let mut cur_u = universe;
            let mut cur_c = (start_channel as usize).clamp(1, 512);
            let mut rem = pixel_count as usize;
            let mut res = (universe, start_channel, total);
            while rem > 0 {
                let avail = 512 + 1 - cur_c;
                let fit = avail / 3;
                if fit >= rem {
                    let end_c = (cur_c + rem * 3 - 1) as u16;
                    res = (cur_u, end_c, total);
                    break;
                }
                rem -= fit;
                cur_u = cur_u.saturating_add(1);
                cur_c = 1;
            }
            res
        }
    };

    if ch_count > 0 {
        write!(
            w,
            "<td class=\"cell-end\" style=\"text-align:center\"><span class=\"cell-end-val stat-mono\" style=\"font-size:0.82rem\">U{end_u}:{end_ch} <span style=\"font-size:0.72rem;color:var(--text-muted)\">({ch_count}ch)</span></span></td>"
        )?;
    } else {
        w.write_str(
            "<td class=\"cell-end\" style=\"text-align:center\"><span class=\"cell-end-val stat-mono\" style=\"font-size:0.82rem\"><span class=\"cell-na\" style=\"display:inline\">&mdash;</span></span></td>"
        )?;
    }

    write!(
        w,
        "<td class=\"cell-loss\"><select name=\"l{ch_idx}\">\
         <option value=\"blackout\"{}>Blackout</option>\
         <option value=\"hold\"{}>Hold</option>\
         <option value=\"fade\"{}>Fade</option>\
         </select><span class=\"cell-na\">&mdash;</span></td>\
         <td class=\"cell-test\"><select name=\"t{ch_idx}\" class=\"test-sel\" data-ch=\"{ch_num}\">",
        if is_blackout { " selected" } else { "" },
        if is_hold { " selected" } else { "" },
        if is_fade { " selected" } else { "" }
    )?;

    write!(
        w,
        "<option value=\"0\"{}>Live (sACN)</option>",
        if is_t0 { " selected" } else { "" }
    )?;

    match ch.mode {
        ChannelMode::Ws2812 { .. } | ChannelMode::Ws2812Fill { .. } | ChannelMode::Fire { .. } => {
            write!(
                w,
                "<option value=\"3\"{}>Rainbow Wave</option>\
                 <option value=\"2\"{}>Color Cycle</option>\
                 <option value=\"5\"{}>Pixel Tracer</option>\
                 <option value=\"1\"{}>100% White</option>\
                 <option value=\"4\"{}>Breathe</option>",
                if is_t3 { " selected" } else { "" },
                if is_t2 { " selected" } else { "" },
                if is_t5 { " selected" } else { "" },
                if is_t1 { " selected" } else { "" },
                if is_t4 { " selected" } else { "" },
            )?;
        }
        ChannelMode::Pwm { .. } => {
            write!(
                w,
                "<option value=\"1\"{}>100% Full On</option>\
                 <option value=\"4\"{}>Breathe</option>",
                if is_t1 { " selected" } else { "" },
                if is_t4 { " selected" } else { "" },
            )?;
        }
        ChannelMode::Off => {}
    }

    w.write_str("</select><span class=\"cell-na\">&mdash;</span></td></tr>")
}


pub fn render_universes_card<W: core::fmt::Write>(w: &mut W, universes: &[UniverseView]) -> core::fmt::Result {
    w.write_str(
        "<div class=\"card\"><div class=\"card-hdr\">\
         <h2 class=\"card-title\"><span>📡</span> Received sACN Universes</h2></div>\
         <table class=\"table\"><thead><tr>\
         <th>Universe</th><th>Last Sequence</th><th>Packets Received</th><th>Live Status</th>\
         </tr></thead><tbody id=\"uni-table-body\">"
    )?;

    if universes.is_empty() {
        w.write_str("<tr><td colspan=\"4\" style=\"color:var(--text-muted);text-align:center;padding:1.5rem\">\
                     Listening on UDP port 5568 &mdash; send sACN packets to see live stream data</td></tr>")?;
    } else {
        for u in universes {
            let pill = if u.terminated {
                "<span class=\"pill pill-yellow\">Terminated</span>"
            } else if u.active {
                "<span class=\"pill pill-green\"><span class=\"pulse\"></span> Active</span>"
            } else {
                "<span class=\"pill pill-gray\">Inactive</span>"
            };
            write!(
                w,
                "<tr><td><strong>Universe {}</strong></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                u.universe, u.sequence, u.packets, pill
            )?;
        }
    }

    w.write_str("</tbody></table></div>")
}

pub static SCRIPT: &str = "<script>\
function initCustomSelect(sel){\
if(!sel)return;\
if(sel._c){sel._c();return;}\
sel.style.display='none';\
var wrap=document.createElement('div');wrap.className='c-select';\
if(sel.style.width==='auto')wrap.style.width='auto';\
var btn=document.createElement('button');btn.type='button';btn.className='c-select-btn';\
var lbl=document.createElement('span');btn.appendChild(lbl);\
var menu=document.createElement('div');menu.className='c-select-menu';\
wrap.appendChild(btn);wrap.appendChild(menu);\
sel.parentNode.insertBefore(wrap,sel.nextSibling);\
var sync=function(){\
menu.innerHTML='';\
var cur=sel.value,curTxt='';\
for(var i=0;i<sel.options.length;i++){\
var opt=sel.options[i];\
if(opt.value===cur||(!curTxt&&i===0)){curTxt=opt.text;}\
var itm=document.createElement('div');\
itm.className='c-select-item'+(opt.value===cur?' selected':'');\
itm.textContent=opt.text;\
(function(val){\
itm.addEventListener('click',function(e){\
e.stopPropagation();sel.value=val;\
wrap.classList.remove('open');\
sel.dispatchEvent(new Event('change',{bubbles:true}));\
sel.dispatchEvent(new Event('input',{bubbles:true}));\
sync();\
});\
})(opt.value);\
menu.appendChild(itm);\
}\
lbl.textContent=curTxt||(sel.options[0]?sel.options[0].text:'');\
};\
btn.addEventListener('click',function(e){\
e.stopPropagation();var isOp=wrap.classList.contains('open');\
document.querySelectorAll('.c-select.open').forEach(function(c){c.classList.remove('open');});\
if(!isOp){sync();wrap.classList.add('open');}\
});\
sel._c=sync;\
sync();\
}\
function syncTest(tr){\
if(!tr)return;\
var ts=tr.querySelector('select[name^=\"t\"]');\
if(!ts)return;\
var ch=ts.getAttribute('data-ch')||(parseInt(ts.name.replace('t',''))+1);\
var test=ts.value;\
var ms=tr.querySelector('select[name^=\"m\"]');\
var ps=tr.querySelector('input[name^=\"p\"]');\
var cm=ms?ms.value:'';\
var px=ps?ps.value:'';\
fetch('/api/test/channel',{\
method:'POST',\
headers:{'Content-Type':'application/x-www-form-urlencoded'},\
body:'ch='+encodeURIComponent(ch)+'&test='+encodeURIComponent(test)+'&ch_mode='+encodeURIComponent(cm)+'&pixels='+encodeURIComponent(px)\
}).catch(function(){});\
}\
function getTestOpts(m){\
var n='<option value=\"0\">Live (sACN)</option>';\
if(m==='ws2812'||m==='ws2812fill'||m==='fire'){\
return n+'<option value=\"3\">Rainbow Wave</option><option value=\"2\">Color Cycle</option><option value=\"5\">Pixel Tracer</option><option value=\"1\">100% White</option><option value=\"4\">Breathe</option>';\
}else if(m==='pwm'){\
return n+'<option value=\"1\">100% Full On</option><option value=\"4\">Breathe</option>';\
}\
return n;\
}\
function calcDmxEnd(m,u,d,p){\
if(m==='off')return'<span class=\"cell-na\" style=\"display:inline\">&mdash;</span>';\
if(m==='pwm')return'U'+u+':'+d+' <span style=\"font-size:0.72rem;color:var(--text-muted)\">(1ch)</span>';\
if(m==='fire'){var end=Math.min(d+2,512);return'U'+u+':'+end+' <span style=\"font-size:0.72rem;color:var(--text-muted)\">(3ch)</span>';}\
if(m==='ws2812fill'){var end=Math.min(d+2,512);return'U'+u+':'+end+' <span style=\"font-size:0.72rem;color:var(--text-muted)\">(3ch)</span>';}\
if(m==='ws2812'){\
var rem=p,curU=u,curCh=Math.min(Math.max(d,1),512);\
while(rem>0){\
var avail=512+1-curCh,fit=Math.floor(avail/3);\
if(fit>=rem){\
var endCh=curCh+rem*3-1;\
return'U'+curU+':'+endCh+' <span style=\"font-size:0.72rem;color:var(--text-muted)\">('+(p*3)+'ch)</span>';\
}\
rem-=fit;curU++;curCh=1;\
}\
}\
return'<span class=\"cell-na\" style=\"display:inline\">&mdash;</span>';\
}\
function updDmxEnd(tr){\
if(!tr)return;\
var ms=tr.querySelector('select[name^=\"m\"]');\
var us=tr.querySelector('input[name^=\"u\"]');\
var ds=tr.querySelector('input[name^=\"d\"]');\
var ps=tr.querySelector('input[name^=\"p\"]');\
var endEl=tr.querySelector('.cell-end-val');\
if(!endEl)return;\
var m=ms?ms.value:'off';\
var u=us?parseInt(us.value)||1:1;\
var d=ds?parseInt(ds.value)||1:1;\
var p=ps?parseInt(ps.value)||30:30;\
endEl.innerHTML=calcDmxEnd(m,u,d,p);\
}\
function updPwmFreq(){\
var hasPwm=Array.from(document.querySelectorAll('select[name^=\"m\"]')).some(function(s){return s.value==='pwm';});\
var wrap=document.getElementById('pwm-freq-wrap');\
var inp=document.getElementById('pwm-freq-input');\
if(wrap&&inp){\
if(hasPwm){wrap.classList.remove('is-disabled');inp.disabled=false;}\
else{wrap.classList.add('is-disabled');inp.disabled=true;}\
}\
}\
var _pending=false;\
function markPending(p){\
_pending=p;\
var pill=document.getElementById('save-pill');\
if(p){\
if(pill){\
pill.className='pill pill-yellow';\
pill.innerHTML='<span class=\"pulse\" style=\"background:#facc15\"></span> Unsaved changes';\
pill.style.opacity='1';\
}\
}\
}\
async function saveOutputs(e){\
if(e&&e.preventDefault)e.preventDefault();\
var promises=[];\
for(var c in _syncTimers){\
clearTimeout(_syncTimers[c]);\
var r=document.querySelector('tr[data-ch=\"'+c+'\"]');\
if(r)promises.push(syncChannel(r));\
}\
_syncTimers={};\
if(_pwmTimer){\
clearTimeout(_pwmTimer);\
_pwmTimer=null;\
var pInp=document.getElementById('pwm-freq-input');\
if(pInp)promises.push(syncPwmFreq(pInp.value));\
}\
var pill=document.getElementById('save-pill');\
var btn=document.getElementById('save-outputs-btn');\
if(btn){btn.disabled=true;btn.textContent='Saving...';}\
if(pill){\
pill.className='pill pill-cyan';\
pill.innerHTML='<span class=\"pulse\" style=\"background:#38bdf8\"></span> Saving...';\
pill.style.opacity='1';\
}\
try{\
if(promises.length>0)await Promise.all(promises);\
var r=await fetch('/api/outputs/save',{method:'POST'});\
if(r.ok){\
markPending(false);\
if(pill){\
pill.className='pill pill-green';\
pill.innerHTML='<span class=\"pulse\" style=\"background:#34d399\"></span> Saved';\
pill.style.opacity='1';\
setTimeout(function(){if(!_pending)pill.style.opacity='0';},3000);\
}\
}else{\
if(pill){pill.className='pill pill-red';pill.textContent='Save Error ('+r.status+')';pill.style.opacity='1';}\
}\
}catch(err){\
if(pill){pill.className='pill pill-red';pill.textContent='Network Error';pill.style.opacity='1';}\
}finally{\
if(btn){btn.disabled=false;btn.textContent='Save';}\
}\
return false;\
}\
function syncChannel(tr){\
if(!tr)return Promise.resolve();\
var ch=tr.getAttribute('data-ch');\
if(!ch)return Promise.resolve();\
var ms=tr.querySelector('select[name^=\"m\"]');\
var us=tr.querySelector('input[name^=\"u\"]');\
var ds=tr.querySelector('input[name^=\"d\"]');\
var ps=tr.querySelector('input[name^=\"p\"]');\
var ls=tr.querySelector('select[name^=\"l\"]');\
var m=ms?ms.value:'off';\
var u=us?us.value:'1';\
var d=ds?ds.value:'1';\
var p=ps?ps.value:'30';\
var l=ls?ls.value:'blackout';\
return fetch('/api/channel',{\
method:'POST',\
headers:{'Content-Type':'application/x-www-form-urlencoded'},\
body:'ch='+encodeURIComponent(ch)+'&mode='+encodeURIComponent(m)+'&universe='+encodeURIComponent(u)+'&dmx_channel='+encodeURIComponent(d)+'&pixel_count='+encodeURIComponent(p)+'&loss='+encodeURIComponent(l)\
}).catch(function(){});\
}\
function syncPwmFreq(val){\
return fetch('/api/pwm_freq',{\
method:'POST',\
headers:{'Content-Type':'application/x-www-form-urlencoded'},\
body:'freq='+encodeURIComponent(val)\
}).catch(function(){});\
}\
var _syncTimers={};\
function debounceSyncChannel(tr){\
if(!tr)return;\
var ch=tr.getAttribute('data-ch');\
clearTimeout(_syncTimers[ch]);\
_syncTimers[ch]=setTimeout(function(){syncChannel(tr);},350);\
}\
var _pwmTimer;\
function debounceSyncPwm(val){\
clearTimeout(_pwmTimer);\
_pwmTimer=setTimeout(function(){syncPwmFreq(val);},350);\
}\
function updRow(s){\
var tr=s.closest('tr');if(!tr)return;\
var m=s.value;\
tr.className='ch-row mode-'+m;\
var d=tr.querySelector('.dot');\
if(d){d.className='dot '+(m==='off'?'dot-off':m==='pwm'?'dot-pwm':m==='fire'?'dot-fire':m==='ws2812fill'?'dot-fill':'dot-ws');}\
updPwmFreq();\
updDmxEnd(tr);\
var ts=tr.querySelector('select[name^=\"t\"]');\
if(ts){\
var curT=ts.value;\
ts.innerHTML=getTestOpts(m);\
var valid=false;\
for(var i=0;i<ts.options.length;i++){\
if(ts.options[i].value===curT){valid=true;break;}\
}\
ts.value=valid?curT:'0';\
if(ts._c)ts._c();\
tr.classList.toggle('is-testing',ts.value!=='0');\
if(m==='off'||!valid){\
if(curT!=='0'){\
var ch=ts.getAttribute('data-ch')||(parseInt(ts.name.replace('t',''))+1);\
fetch('/api/test/channel',{\
method:'POST',\
headers:{'Content-Type':'application/x-www-form-urlencoded'},\
body:'ch='+encodeURIComponent(ch)+'&test=0'\
}).catch(function(){});\
}\
}else if(ts.value!=='0'){\
syncTest(tr);\
}\
}\
syncChannel(tr);\
markPending(true);\
}\
function updNet(){\
var d=document.getElementById('net-dhcp-sel');\
if(!d)return;\
var isDhcp=d.value==='true';\
var ip=document.getElementById('net-ip');\
var pre=document.getElementById('net-prefix');\
if(ip)ip.disabled=isDhcp;\
if(pre)pre.disabled=isDhcp;\
}\
document.addEventListener('change',function(e){\
var t=e.target;if(!t)return;\
if(t.id==='net-dhcp-sel'){updNet();}\
else if(t.name&&t.name.indexOf('m')===0){updRow(t);markPending(true);}\
else if(t.name&&t.name.indexOf('l')===0){syncChannel(t.closest('tr'));markPending(true);}\
else if(t.name&&t.name.indexOf('t')===0){\
var tr=t.closest('tr');\
if(tr)tr.classList.toggle('is-testing',t.value!=='0');\
syncTest(tr);\
}\
else if(t.name&&(t.name.indexOf('u')===0||t.name.indexOf('d')===0||t.name.indexOf('p')===0)){\
var tr=t.closest('tr');\
if(tr){\
clearTimeout(_syncTimers[tr.getAttribute('data-ch')]);\
updDmxEnd(tr);\
syncChannel(tr);\
markPending(true);\
}\
}\
else if(t.name==='pwm_freq'){\
clearTimeout(_pwmTimer);\
syncPwmFreq(t.value);\
markPending(true);\
}\
if(t._c)t._c();\
});\
document.addEventListener('input',function(e){\
var t=e.target;if(!t)return;\
if(t.id==='net-dhcp-sel'){updNet();}\
else if(t.name&&t.name.indexOf('m')===0){updRow(t);markPending(true);}\
else if(t.name&&(t.name.indexOf('u')===0||t.name.indexOf('d')===0||t.name.indexOf('p')===0)){\
var tr=t.closest('tr');\
if(tr){\
updDmxEnd(tr);\
debounceSyncChannel(tr);\
markPending(true);\
if(t.name.indexOf('p')===0){\
var ts=tr.querySelector('select[name^=\"t\"]');\
if(ts&&ts.value!=='0'){syncTest(tr);}\
}\
}\
}\
else if(t.name==='pwm_freq'){\
debounceSyncPwm(t.value);\
markPending(true);\
}\
if(t._c)t._c();\
});\
document.addEventListener('click',function(){\
document.querySelectorAll('.c-select.open').forEach(function(c){c.classList.remove('open');});\
});\
var _polling=false;\
function pollStatus(){\
if(_polling)return;\
_polling=true;\
fetch('/api/status').then(function(r){return r.json();}).then(function(d){\
var up=document.getElementById('stat-uptime-val');\
if(up&&typeof d.uptime==='number'){\
var h=Math.floor(d.uptime/3600),m=Math.floor((d.uptime%3600)/60),s=d.uptime%60;\
up.textContent=h+'h '+(m<10?'0':'')+m+'m '+(s<10?'0':'')+s+'s';\
}\
var tb=document.getElementById('uni-table-body');\
if(tb&&Array.isArray(d.universes)){\
if(d.universes.length===0){\
tb.innerHTML='<tr><td colspan=\"4\" style=\"color:var(--text-muted);text-align:center;padding:1.5rem\">Listening on UDP port 5568 &mdash; send sACN packets to see live stream data</td></tr>';\
}else{\
var h='';\
for(var i=0;i<d.universes.length;i++){\
var u=d.universes[i];\
var pill=u.terminated?'<span class=\"pill pill-yellow\">Terminated</span>':(u.active?'<span class=\"pill pill-green\"><span class=\"pulse\"></span> Active</span>':'<span class=\"pill pill-gray\">Inactive</span>');\
h+='<tr><td><strong>Universe '+u.universe+'</strong></td><td>'+u.sequence+'</td><td>'+u.packets+'</td><td>'+pill+'</td></tr>';\
}\
tb.innerHTML=h;\
}\
}\
}).catch(function(){}).finally(function(){_polling=false;});\
}\
setInterval(pollStatus,1500);\
document.querySelectorAll('select').forEach(initCustomSelect);\
document.querySelectorAll('tr.ch-row').forEach(updDmxEnd);\
updNet();\
updPwmFreq();\
</script></div></body></html>";

pub fn render_script<W: core::fmt::Write>(w: &mut W) -> core::fmt::Result {
    w.write_str(SCRIPT)
}

pub fn render_status_json<W: core::fmt::Write>(
    w: &mut W,
    uptime_secs: u64,
    link_up: bool,
    test_mode: u8,
    universes: &[UniverseView],
) -> core::fmt::Result {
    write!(
        w,
        "{{\"uptime\":{},\"link\":{},\"test_mode\":{},\"universes\":[",
        uptime_secs, link_up, test_mode
    )?;
    for (i, u) in universes.iter().enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }
        write!(
            w,
            "{{\"universe\":{},\"sequence\":{},\"packets\":{},\"active\":{},\"terminated\":{}}}",
            u.universe, u.sequence, u.packets, u.active, u.terminated
        )?;
    }
    w.write_str("]}")
}
