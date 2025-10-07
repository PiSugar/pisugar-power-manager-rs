use std::collections::HashMap;
use std::convert::TryInto;
use std::fs::remove_file;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::exit;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Instant, SystemTime};
use std::{env, fs};

use anyhow::{anyhow, bail, Result};
use chrono::prelude::*;
use clap::{Arg, ArgAction, Command};
use cmds::{ButtonMode, Cmds};
use digest_auth::{AuthContext, AuthorizationHeader, Charset, Qop, WwwAuthenticateHeader};
use env_logger::Env;
use futures::prelude::*;
use futures::SinkExt;
use futures_channel::mpsc::unbounded;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response};
use hyper::{Request, Server};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use lazy_static::lazy_static;
use log::LevelFilter;
use rand::RngCore;
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::time::Duration;
use tokio_util::codec::{BytesCodec, Framed};

use pisugar_core::{
    execute_shell, get_ntp_datetime, notify_shutdown_soon, sys_write_time, Error, Model, PiSugarConfig, PiSugarCore,
    RTCRawTime, I2C_READ_INTERVAL,
};

mod cmds;

/// Websocket info
const WS_JSON: &str = "_ws.json";

lazy_static! {
    /// WS addr
    static ref WS_ADDR: Mutex<Option<SocketAddr>> = Mutex::new(None);
}

/// Tap event tx
type EventTx = tokio::sync::watch::Sender<String>;

/// Tap event rx
type EventRx = tokio::sync::watch::Receiver<String>;

/// Poll pisugar status
async fn poll_pisugar_status(core: &mut PiSugarCore, tx: &EventTx) {
    log::debug!("Polling state");
    let now = Instant::now();
    match core.poll(now).await {
        Ok(Some(tap_type)) => {
            let _ = tx.send(format!("{}\n", tap_type));
        }
        Err(e) => {
            log::warn!("Poll error: {}, retry after 1s", e);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        _ => {}
    }
}

/// Handle request
fn handle_request(core: Arc<Mutex<PiSugarCore>>, req: &str) -> String {
    let parts: Vec<String> = req.split(' ').map(|s| s.to_string()).collect();
    let err = "Invalid request.\n".to_string();

    if !req.contains("set_auth") {
        log::debug!("Request: {}", req);
    }

    if req.starts_with("help") {
        let help = Cmds::from_str(req).expect_err("");
        return help.to_string();
    }

    let cmd = match Cmds::from_str(req) {
        Ok(cmd) => cmd,
        Err(e) => {
            log::warn!("Invalid cmd: {}", e);
            return err;
        }
    };

    let core_cloned = core.clone();
    let mut core = core_cloned.lock().unwrap();
    let r = match &cmd {
        Cmds::Get(get_cmd) => {
            let r = match get_cmd {
                cmds::GetCmds::Version => Ok(env!("CARGO_PKG_VERSION").to_string()),
                cmds::GetCmds::Model => Ok(core.model()),
                cmds::GetCmds::FirmwareVersion => core.version(),
                cmds::GetCmds::Battery => core.level().map(|l| l.to_string()),
                cmds::GetCmds::BatteryI => core.intensity_avg().map(|i| i.to_string()),
                cmds::GetCmds::BatteryV => core.voltage_avg().map(|v| v.to_string()),
                cmds::GetCmds::BatteryKeepInput => core.keep_input().map(|k| k.to_string()),
                cmds::GetCmds::BatteryLedAmount => core.led_amount().map(|n| n.to_string()),
                cmds::GetCmds::BatteryPowerPlugged => core.power_plugged().map(|p| p.to_string()),
                cmds::GetCmds::BatteryAllowCharging => core.allow_charging().map(|a| a.to_string()),
                cmds::GetCmds::BatteryChargingRange => core
                    .charging_range()
                    .map(|r| r.map_or("".to_string(), |r| format!("{},{}", r.0, r.1))),
                cmds::GetCmds::BatteryCharging => core.charging().map(|c| c.to_string()),
                cmds::GetCmds::BatteryInputProtectEnabled => core.input_protected().map(|c| c.to_string()),
                cmds::GetCmds::BatteryOutputEnabled => core.output_enabled().map(|o| o.to_string()),
                cmds::GetCmds::FullChargeDuration => Ok(core
                    .config()
                    .full_charge_duration
                    .map_or("".to_string(), |d| d.to_string())),
                cmds::GetCmds::SystemTime => Ok(Local::now().to_rfc3339_opts(SecondsFormat::Millis, false)),
                cmds::GetCmds::RtcAddr => core.read_rtc_addr().map(|a| format!("0x{:02x}", a)),
                cmds::GetCmds::RtcTime => core
                    .read_time()
                    .map(|t| t.to_rfc3339_opts(SecondsFormat::Millis, false)),
                cmds::GetCmds::RtcTimeList => core.read_raw_time().map(|r| r.to_string()),
                cmds::GetCmds::RtcAlarmFlag => core.read_alarm_flag().map(|f| f.to_string()),
                cmds::GetCmds::RtcAlarmTime => {
                    let t = core
                        .read_alarm_time()
                        .and_then(|r| r.try_into().map_err(|_| Error::Other("Invalid".to_string())));
                    t.map(|t: DateTime<Utc>| {
                        t.with_timezone(Local::now().offset())
                            .to_rfc3339_opts(SecondsFormat::Millis, false)
                    })
                }
                cmds::GetCmds::RtcAlarmTimeList => core.read_alarm_time().map(|r| r.to_string()),
                cmds::GetCmds::RtcAlarmEnabled => core.read_alarm_enabled().map(|e| e.to_string()),
                cmds::GetCmds::RtcAdjustPpm => Ok(core.config().rtc_adj_ppm.unwrap_or_default().to_string()),
                cmds::GetCmds::AlarmRepeat => Ok(core.config().auto_wake_repeat.to_string()),
                cmds::GetCmds::SafeShutdownLevel => Ok(core.config().auto_shutdown_level.unwrap_or(0.0).to_string()),
                cmds::GetCmds::SafeShutdownDelay => Ok(core.config().auto_shutdown_delay.unwrap_or(0.0).to_string()),
                cmds::GetCmds::ButtonEnable { mode } => Ok(match mode {
                    cmds::ButtonMode::Single => core.config().single_tap_enable,
                    cmds::ButtonMode::Double => core.config().double_tap_enable,
                    cmds::ButtonMode::Long => core.config().long_tap_enable,
                })
                .map(|b| format!("{} {}", parts[2], b.to_string())),
                cmds::GetCmds::ButtonShell { mode } => Ok(match mode {
                    cmds::ButtonMode::Single => core.config().single_tap_shell.clone(),
                    cmds::ButtonMode::Double => core.config().double_tap_shell.clone(),
                    cmds::ButtonMode::Long => core.config().long_tap_shell.clone(),
                })
                .map(|x| format!("{} {}", parts[2], x)),
                cmds::GetCmds::AutoPowerOn => Ok(core.config().auto_power_on.unwrap_or(false).to_string()),
                cmds::GetCmds::AuthUsername => Ok(core.config().auth_user.clone().unwrap_or_default()),
                cmds::GetCmds::AntiMistouch => Ok(core.config().anti_mistouch.unwrap_or(true).to_string()),
                cmds::GetCmds::SoftPoweroff => Ok(core.config().soft_poweroff.unwrap_or(false).to_string()),
                cmds::GetCmds::SoftPoweroffShell => Ok(core.config().soft_poweroff_shell.clone().unwrap_or_default()),
                cmds::GetCmds::Temperature => core.get_temperature().map(|x| x.to_string()),
                cmds::GetCmds::InputProtect => core.input_protected().map(|x| x.to_string()),
            };
            r.map(|x| format!("{}: {}", parts[1], x))
        }
        Cmds::SetBatteryKeepInput(b) => core.set_keep_input(b.value()).map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetBatteryChargingRange { range } => {
            let charging_range = if range.len() == 2 {
                Some((range[0], range[1]))
            } else {
                None
            };
            core.set_charging_range(charging_range)
                .map(|_| format!("{}: done\n", parts[0]))
        }
        Cmds::SetBatteryInputProtect(b) => core
            .toggle_input_protected(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetBatteryOutput(b) => core
            .toggle_output_enabled(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetFullChargeDuration { seconds } => {
            core.config_mut().full_charge_duration = Some(*seconds);
            core.save_config().map(|_| format!("{}: done\n", parts[0]))
        }
        Cmds::SetAllowCharging(b) => core
            .toggle_allow_charging(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetRtcAddr { addr } => {
            if let Err(e) = core.set_rtc_addr(*addr) {
                log::warn!("Set RTC addr error: {}", e);
            }
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::RtcClearFlag => core.clear_alarm_flag().map(|_| format!("{}: done\n", parts[0])),
        Cmds::RtcPi2rtc => core.write_time(Local::now()).map(|_| format!("{}: done\n", parts[0])),
        Cmds::RtcRtc2pi => core.read_time().map(|t| {
            sys_write_time(t);
            format!("{}: done\n", parts[0])
        }),
        Cmds::RtcWeb => {
            let core_cloned = core_cloned.clone();
            tokio::spawn(async move {
                match get_ntp_datetime().await {
                    Ok(ntp_datetime) => {
                        sys_write_time(ntp_datetime.into());
                        if let Ok(core) = core_cloned.lock() {
                            let _ = core.write_time(ntp_datetime.into());
                        }
                    }
                    Err(e) => log::warn!("Sync NTP time error: {}", e),
                }
            });
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::RtcAlarmSet { datetime, weekdays } => {
            let datetime: DateTime<Local> = datetime.clone().into();
            let sd3078_time: RTCRawTime = datetime.clone().into();
            core.write_alarm(sd3078_time, *weekdays).map(|_| {
                core.config_mut().auto_wake_repeat = *weekdays;
                core.config_mut().auto_wake_time = Some(datetime.clone());
                if let Err(e) = core.save_config() {
                    log::warn!("{}", e);
                }
                format!("{}: done\n", parts[0])
            })
        }
        Cmds::RtcAlarmDisable => core.disable_alarm().map(|_| {
            core.config_mut().auto_wake_time = None;
            if let Err(e) = core.save_config() {
                log::warn!("{}", e);
            }
            format!("{}: done\n", parts[0])
        }),
        Cmds::RtcAdjustPpm { ppm } => {
            let ppm = if *ppm > 500.0 { 500.0 } else { *ppm };
            let ppm = if ppm < -500.0 { -500.0 } else { ppm };
            core.write_rtc_adjust_ppm(ppm).map(|_| {
                core.config_mut().rtc_adj_ppm = Some(ppm);
                if let Err(e) = core.save_config() {
                    log::warn!("{}", e);
                }
                format!("{}: done\n", parts[0])
            })
        }
        Cmds::SetSafeShutdownLevel { level } => {
            // level between <30，level < 0 means do not shutdown
            let level = if *level > 30.0 { 30.0 } else { *level };
            core.config_mut().auto_shutdown_level = Some(level);
            if let Err(e) = core.save_config() {
                log::error!("{}", e);
            }
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::SetSafeShutdownDelay { delay } => {
            // delay between 0-30
            let delay = if *delay < 0.0 { 0.0 } else { *delay };
            let delay = if delay > 120.0 { 120.0 } else { delay };
            core.config_mut().auto_shutdown_delay = Some(delay);
            if let Err(e) = core.save_config() {
                log::error!("{}", e);
            }
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::RtcTestWake => core
            .test_wake()
            .map(|_| format!("{}: wakeup after 1 min 30 sec\n", parts[0])),
        Cmds::SetButtonEnable { mode, enable } => {
            match *mode {
                ButtonMode::Single => core.config_mut().single_tap_enable = enable.0,
                ButtonMode::Double => core.config_mut().double_tap_enable = enable.0,
                ButtonMode::Long => core.config_mut().long_tap_enable = enable.0,
            }
            if let Err(e) = core.save_config() {
                log::error!("{}", e);
            }
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::SetButtonShell { mode, shell } => {
            let cmd = shell.join(" ");
            match mode {
                ButtonMode::Single => core.config_mut().single_tap_shell = cmd,
                ButtonMode::Double => core.config_mut().double_tap_shell = cmd,
                ButtonMode::Long => core.config_mut().long_tap_shell = cmd,
            }
            if let Err(e) = core.save_config() {
                log::error!("{}", e);
            }
            Ok(format!("{}: done\n", parts[0]))
        }
        Cmds::SetAutoPowerOn(b) => core
            .toggle_auto_power_on(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetAuth { username, password } => {
            if let (Some(username), Some(password)) = (username, password) {
                core.config_mut().auth_user = Some(username.to_string());
                core.config_mut().auth_password = Some(password.to_string());
            } else {
                core.config_mut().auth_user = None;
                core.config_mut().auth_password = None;
            }
            core.save_config().map(|_| format!("{}: done\n", parts[0]))
        }
        Cmds::ForceShutdown => core.force_shutdown().map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetAntiMistouch(b) => core
            .toggle_anti_mistouch(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetSoftPoweroff(b) => core
            .toggle_soft_poweroff(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
        Cmds::SetSoftPoweroffShell { shell } => {
            let script = shell.join(" ");
            core.config_mut().soft_poweroff_shell = if script.len() > 0 {
                Some(script.to_string())
            } else {
                None
            };
            core.save_config().map(|_| format!("{}: done\n", parts[0]))
        }
        Cmds::SetInputProtect(b) => core
            .toggle_input_protected(b.value())
            .map(|_| format!("{}: done\n", parts[0])),
    };

    match r {
        Ok(mut r) => {
            if !r.ends_with("\n") {
                r += "\n";
            }
            r
        }
        Err(e) => {
            log::warn!("Request: {}, error: {}", req, e);
            err
        }
    }
}

async fn _handle_stream<T>(core: Arc<Mutex<PiSugarCore>>, stream: T, mut event_rx: EventRx) -> io::Result<()>
where
    T: 'static + AsyncRead + AsyncWrite + Send,
{
    let framed = Framed::new(stream, BytesCodec::new());
    let (sink, mut stream) = framed.split();
    let (mut tx, rx) = unbounded();

    // handle request
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(buf)) = stream.next().await {
            let reqs = String::from_utf8_lossy(buf.as_ref());
            let reqs = reqs.trim_end_matches('\n');
            for req in reqs.split('\n') {
                log::debug!("Req: {}", req);
                let req = req.replace('\r', "");
                let resp = handle_request(core.clone(), req.as_str());
                log::debug!("Resp: {}", resp);
                tx_cloned.send(Some(resp)).await.expect("Channel failed");
            }
        }
        // delay for 100 millis
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::debug!("Stream close");
    });

    // button event
    tokio::spawn(async move {
        let _ = event_rx.borrow_and_update();
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            tx.send(Some(s)).await.expect("Channel failed");
        }
        log::debug!("Event watcher close");
        tx.send(None).await.expect("Channel failed");
    });

    // send back
    tokio::spawn(
        rx.map(|s| match s {
            Some(s) => {
                log::debug!("Sink send: {}", s);
                Ok(hyper::body::Bytes::from(s))
            }
            None => {
                log::debug!("Sink close");
                Err(io::ErrorKind::Other.into())
            }
        })
        .forward(sink),
    );

    Ok(())
}

/// Handle tcp stream
async fn handle_tcp_stream(core: Arc<Mutex<PiSugarCore>>, stream: TcpStream, event_rx: EventRx) -> io::Result<()> {
    log::info!("Incoming tcp connection from: {}", stream.peer_addr()?);
    _handle_stream(core, stream, event_rx).await
}

/// Handle websocket request
async fn handle_ws_connection(
    core: Arc<Mutex<PiSugarCore>>,
    stream: TcpStream,
    mut event_rx: EventRx,
) -> io::Result<()> {
    log::info!("Incoming ws connection from: {}", stream.peer_addr()?);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .await?;
    log::info!("WS connection established");

    let (mut tx, rx) = unbounded();
    let (sink, mut stream) = ws_stream.split();

    // handle request
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok(msg) = msg.to_text() {
                let req = msg.replace('\n', "");
                log::debug!("Req: {}", req);
                let resp = handle_request(core.clone(), req.as_str());
                log::debug!("Resp: {}", resp);
                tx_cloned.send(Some(resp)).await.expect("Channel failed");
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::debug!("WS stream close")
    });

    // button event
    tokio::spawn(async move {
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            tx.send(Some(s)).await.expect("Channel failed");
        }
        log::debug!("Event watcher close");
        tx.send(None).await.expect("Channel failed");
    });

    // send back
    tokio::spawn(
        rx.map(|s| match s {
            Some(s) => {
                log::debug!("WS sink send: {}", s);
                Ok(s.into())
            }
            None => {
                log::debug!("WS sink close");
                Err(tokio_tungstenite::tungstenite::Error::AlreadyClosed)
            }
        })
        .forward(sink),
    );

    Ok(())
}

/// Handle uds
async fn handle_uds_stream(core: Arc<Mutex<PiSugarCore>>, stream: UnixStream, event_rx: EventRx) -> io::Result<()> {
    log::info!("Incoming uds stream: {:?}", stream.peer_addr()?);
    _handle_stream(core, stream, event_rx).await
}

/// Clean up before exit
fn clean_up(uds: Option<String>, web_dir: Option<String>) {
    if let Some(uds) = uds {
        let p: &Path = Path::new(uds.as_str());
        if p.exists() {
            match remove_file(p) {
                Ok(_) => {}
                Err(e) => {
                    log::warn!("Failed to remove uds file: {}", e);
                }
            }
        }
    }

    if let Some(web_dir) = web_dir {
        let p: &Path = Path::new(web_dir.as_str());
        let p = p.join(WS_JSON);
        if p.exists() {
            match remove_file(p) {
                Ok(_) => {}
                Err(e) => {
                    log::warn!("Failed to remove ws json file: {}", e);
                }
            }
        }
    }

    exit(0)
}

async fn on_ws_client(
    websocket: HyperWebsocket,
    core: Arc<Mutex<PiSugarCore>>,
    mut event_rx: EventRx,
) -> Result<(), io::Error> {
    let websocket = websocket.await;
    let websocket = websocket.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let (mut tx, mut rx) = unbounded();
    let (mut sink, mut s) = websocket.split();

    // req
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = s.next().await {
            let resp_msg = match msg {
                Message::Text(req) => {
                    let resp = handle_request(core.clone(), &req);
                    Some(Message::text(resp))
                }
                Message::Binary(_) => Some(Message::Close(None)),
                Message::Ping(data) => Some(Message::Pong(data)),
                Message::Close(_) => Some(Message::Close(None)),
                Message::Pong(_) => None,
                Message::Frame(_) => None,
            };
            if resp_msg.is_some() {
                tx_cloned.send(resp_msg).await.expect("Channel failed");
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::info!("Websocket closed");
    });

    // button event
    tokio::spawn(async move {
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            tx.send(Some(Message::text(s))).await.expect("Channel failed");
        }
        log::debug!("Event watcher close");
        tx.send(None).await.expect("Channel failed");
    });

    // send back
    while let Some(Some(rsp)) = rx.next().await {
        sink.send(rsp)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
    }

    Ok(())
}

type SecurityRecord = (Option<String>, u32, SystemTime);

lazy_static! {
    static ref SECURITY_CTX: Mutex<HashMap<String, SecurityRecord>> = Mutex::new(HashMap::default());
}
const SECURITY_TIMEOUT_SECONDS: u64 = 30 * 60;

fn build_realm(req: &Request<Body>, user: &str) -> String {
    let host = req
        .headers()
        .get(&hyper::header::HOST)
        .map(|v| v.to_str().unwrap_or(""))
        .map(|v| v.to_string())
        .unwrap_or_else(|| "localhost".to_string());
    let port = req.uri().port_u16();
    if let Some(port) = port {
        format!("{}@{}:{}", user, host, port)
    } else {
        format!("{}@{}", user, host)
    }
}

fn build_www_header(req: &Request<Body>, user: &str, session_timeout: Duration) -> Result<String> {
    let now = SystemTime::now();
    let realm = build_realm(req, user);

    // nonce, should unchange during the session
    let mut nonce = [0; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let nonce = base64::encode(nonce);

    // use a random number as opaque
    let mut opaque = [0; 12];
    rand::thread_rng().fill_bytes(&mut opaque);
    let opaque = base64::encode(opaque);

    log::info!("Http auth create opaque {}, nonce: {}", opaque, nonce);

    // clean timout sessions
    let mut ctx = SECURITY_CTX
        .lock()
        .map_err(|e| anyhow!("Lock security ctx errro: {}", e))?;
    ctx.retain(|opaque, v| {
        if v.2 + session_timeout < now {
            log::debug!("opaque={}, cnonce={:?} timeout", opaque, v.0);
            false
        } else {
            true
        }
    });

    // new session
    ctx.insert(opaque.clone(), (None, 0, now));
    let header = digest_auth::WwwAuthenticateHeader {
        domain: Some(vec!["/".to_string()]),
        realm,
        nonce,
        opaque: Some(opaque),
        stale: false,
        algorithm: Default::default(),
        qop: Some(vec![Qop::AUTH, Qop::AUTH_INT]),
        userhash: false,
        charset: Charset::UTF8,
        nc: 0,
    };
    Ok(header.to_string())
}

fn rebuild_www_header(
    req: &Request<Body>,
    auth_header: &AuthorizationHeader,
    session_timeout: Duration,
) -> Result<WwwAuthenticateHeader> {
    let opaque = auth_header
        .opaque
        .clone()
        .ok_or_else(|| anyhow!("Rebuild www header, server opaque not exist"))?;

    let mut ctx = SECURITY_CTX
        .lock()
        .map_err(|e| anyhow!("Lock SECURITY_CTX error: {}", e))?;

    let (cnonce, nc, last_time) = ctx
        .get(&opaque)
        .ok_or_else(|| anyhow!("Rebuild www header, server opaque not in SECURITY_CTX"))?;

    let now = SystemTime::now();
    let duration = now.duration_since(*last_time)?;

    // session timeout
    if duration > session_timeout {
        bail!("SECURITY ERROR: session timout, {}s", duration.as_secs());
    }

    // cnonce replay
    let auth_cnonce = auth_header
        .cnonce
        .clone()
        .ok_or_else(|| anyhow!("SECURITY ERROR: empty cnonce"))?;
    if let Some(cnonce) = cnonce {
        if auth_cnonce != *cnonce {
            log::debug!("SECURITY: cnonce changed, current {}", auth_cnonce);
        } else if auth_header.nc < *nc {
            bail!("SECURITY ERROR: nc replay");
        }
    }

    let realm = build_realm(req, &auth_header.username);
    let new_nc = auth_header.nc;

    let www_header = WwwAuthenticateHeader {
        domain: Some(vec!["/".to_string()]),
        realm,
        nonce: auth_header.nonce.clone(),
        opaque: auth_header.opaque.clone(),
        stale: false,
        algorithm: auth_header.algorithm,
        qop: auth_header.qop.map(|x| vec![x]),
        userhash: auth_header.userhash,
        charset: Charset::UTF8,
        nc: auth_header.nc - 1,
    };

    // update security ctx
    ctx.insert(opaque, (Some(auth_cnonce), new_nc, now));

    Ok(www_header)
}

/// Handle http request, /ws to websocket
async fn handle_http_req(
    req: Request<Body>,
    static_: hyper_staticfile::Static,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: EventRx,
) -> Result<Response<Body>> {
    log::info!("request: {} {}", req.method(), req.uri());
    // check for http auth
    if let Ok(config) = core.lock() {
        if let (Some(auth_user), Some(auth_pass)) =
            (config.config().auth_user.clone(), config.config().auth_password.clone())
        {
            let auth_user = auth_user.trim().to_string();
            let auth_password = auth_pass.trim().to_string();
            if !auth_user.is_empty() && !auth_password.is_empty() {
                let mut auth_context = AuthContext::new(auth_user.clone(), auth_pass, req.uri().to_string());
                let mut auth_ok = false;
                for (name, value) in req.headers() {
                    if name.eq(&hyper::header::AUTHORIZATION) {
                        if let Ok(value) = value.to_str() {
                            let auth_header = match AuthorizationHeader::parse(value) {
                                Err(e) => {
                                    log::warn!("Invalid authentication header {}", e);
                                    continue;
                                }
                                Ok(h) => h,
                            };
                            auth_context.set_custom_cnonce(auth_header.cnonce.clone().unwrap_or_default());

                            match rebuild_www_header(&req, &auth_header, Duration::from_secs(SECURITY_TIMEOUT_SECONDS))
                            {
                                Ok(mut www_header) => {
                                    let auth_header2 = AuthorizationHeader::from_prompt(&mut www_header, &auth_context)
                                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                                    auth_ok = auth_header2.response == auth_header.response;
                                }
                                Err(e) => {
                                    log::error!("Rebuid auth header error: {}", e);
                                }
                            };
                        }
                    }
                }
                if !auth_ok {
                    let www_header = build_www_header(&req, &auth_user, Duration::from_secs(SECURITY_TIMEOUT_SECONDS))?;
                    let resp = Response::builder()
                        .status(hyper::StatusCode::UNAUTHORIZED)
                        .header(hyper::header::WWW_AUTHENTICATE, www_header) // fix chrome digest auth
                        .body(Body::empty())?;
                    return Ok(resp);
                }
            }
        }
    }
    // _ws.json
    if req.uri().path().contains(WS_JSON) {
        if let Some(ws_addr) = *WS_ADDR.lock().map_err(|e| anyhow!("Lock WS_ADDR error: {}", e))? {
            let json = format!("{{\"wsPort\": \"{}\"}}", ws_addr.port());
            return Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::from(json))?);
        } else {
            bail!("Not found");
        }
    }
    // websocket
    if req.uri().path().ends_with("/ws") {
        if hyper_tungstenite::is_upgrade_request(&req) {
            let (resp, websocket) =
                hyper_tungstenite::upgrade(req, None).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            tokio::spawn(async move {
                if let Err(e) = on_ws_client(websocket, core, event_rx).await {
                    log::debug!("Serving websocket error: {}", e);
                }
            });
            Ok(resp)
        } else {
            bail!("/ws only serve websocket");
        }
    } else {
        let resp = static_.clone().serve(req).await?;
        Ok(resp)
    }
}

/// Serve http
async fn serve_http(http_addr: SocketAddr, web_dir: String, core: Arc<Mutex<PiSugarCore>>, event_rx: EventRx) {
    let static_ = hyper_staticfile::Static::new(web_dir);

    let make_service = make_service_fn(move |_| {
        let static_ = static_.clone();
        let core = core.clone();
        let event_rx = event_rx.clone();
        async {
            Ok::<_, anyhow::Error>(service_fn(move |req| {
                handle_http_req(req, static_.clone(), core.clone(), event_rx.clone()).map_err(|e| {
                    log::error!("Handle http req error: {}", e);
                    e
                })
            }))
        }
    });

    let server = Server::bind(&http_addr).serve(make_service);

    if let Err(e) = server.await {
        log::error!("Http web server error: {}", e);
    }
}

/// Init logging
fn init_logging(debug: bool, syslog: bool) {
    if syslog {
        // logging
        let pid = unsafe { libc::getpid() };
        let formatter = Formatter3164 {
            facility: Facility::LOG_USER,
            hostname: None,
            process: env!("CARGO_PKG_NAME").into(),
            pid: pid as u32,
        };
        let logger = syslog::unix(formatter).expect("Could not connect to syslog");
        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|_| match debug {
                true => log::set_max_level(LevelFilter::Debug),
                false => log::set_max_level(LevelFilter::Info),
            })
            .expect("Failed to init syslog");
    } else if debug {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Config file in json format, e.g. /etc/pisugar-server/config.json"),
        )
        .arg(
            Arg::new("tcp")
                .short('t')
                .long("tcp")
                .value_name("ADDR")
                .help("Tcp listen address, e.g. 0.0.0.0:8423"),
        )
        .arg(
            Arg::new("uds")
                .short('u')
                .long("uds")
                .value_name("FILE")
                .default_value("/tmp/pisugar-server.sock")
                .help("Unix domain socket file, e.g. /tmp/pisugar-server.sock"),
        )
        .arg(
            Arg::new("uds-mode")
                .long("uds-mode")
                .default_value("666")
                .help("Unix domain socket file mode, e.g. 666"),
        )
        .arg(
            Arg::new("ws")
                .short('w')
                .long("ws")
                .value_name("ADDR")
                .help("Websocket listen address, e.g. 0.0.0.0:8422"),
        )
        .arg(
            Arg::new("web")
                .requires_all(&["http"])
                .long("web")
                .value_name("DIR")
                .default_value("/usr/share/pisugar-server/web")
                .help("Web content directory, e.g. web"),
        )
        .arg(
            Arg::new("http")
                .long("http")
                .value_name("ADDR")
                .default_value("0.0.0.0:8421")
                .help("Http server listen address, e.g. 0.0.0.0:8421"),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(ArgAction::SetTrue)
                .help("Debug output"),
        )
        .arg(
            Arg::new("syslog")
                .short('s')
                .long("syslog")
                .action(ArgAction::SetTrue)
                .help("Log to syslog"),
        )
        .arg(Arg::new("led").long("led").default_value("4").help("2-led or 4-led"))
        .arg(
            Arg::new("model")
                .long("model")
                .required(true)
                .value_parser(clap::value_parser!(Model)),
        )
        .get_matches();

    // init logging
    let debug = matches.get_flag("debug");
    let syslog = matches.get_flag("syslog");
    init_logging(debug, syslog);

    // model
    let model = matches.get_one::<Model>("model").unwrap();
    log::debug!("Running with model: {}", model);

    // core
    let core;
    loop {
        let c = matches
            .get_one::<String>("config")
            .map(|c| PiSugarCore::new_with_path(c, true, model.clone()))
            .unwrap_or_else(|| {
                let config = PiSugarConfig::default();
                PiSugarCore::new(config, model.clone())
            });
        match c {
            Ok(c) => {
                core = Arc::new(Mutex::new(c));
                break;
            }
            Err(e) => log::error!("PiSugar init failed: {}", e),
        }
        sleep(Duration::from_secs(3));
    }

    // event watch
    let (event_tx, event_rx) = tokio::sync::watch::channel("".to_string());

    // CTRL+C signal handling
    let uds = matches.get_one::<String>("uds").cloned();
    let web_dir = matches.get_one::<String>("web").cloned();
    ctrlc::set_handler(move || {
        clean_up(uds.clone(), web_dir.clone());
    })
    .expect("Failed to setup ctrl+c");

    // tcp
    if let Some(tcp_addr) = matches.get_one::<String>("tcp").cloned() {
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        tokio::spawn(async move {
            loop {
                match TcpListener::bind(&tcp_addr).await {
                    Ok(tcp_listener) => {
                        log::info!("TCP listening...");
                        while let Ok((stream, addr)) = tcp_listener.accept().await {
                            log::info!("TCP from {}", addr);
                            let core = core_cloned.clone();
                            if let Err(e) = handle_tcp_stream(core, stream, event_rx_cloned.clone()).await {
                                log::error!("Handle tcp error: {}", e);
                            }
                        }
                        log::info!("TCP stopped");
                    }
                    Err(e) => {
                        log::warn!("TCP bind error: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }

    // ws
    if let Some(ws_addr) = matches.get_one::<String>("ws").cloned() {
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        tokio::spawn(async move {
            loop {
                match tokio::net::TcpListener::bind(&ws_addr).await {
                    Ok(ws_listener) => {
                        log::info!("WS listening...");
                        while let Ok((stream, addr)) = ws_listener.accept().await {
                            log::info!("WS from {}", addr);
                            let core = core_cloned.clone();
                            if let Err(e) = handle_ws_connection(core, stream, event_rx_cloned.clone()).await {
                                log::warn!("Handle ws error: {}", e);
                            }
                        }
                        log::info!("WS stopped");
                    }
                    Err(e) => {
                        log::warn!("WS bind error: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }

    // uds
    if let Some(uds_addr) = matches.get_one::<String>("uds").cloned() {
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        let uds_mode = matches
            .get_one::<String>("uds-mode")
            .and_then(|s| u32::from_str_radix(s, 8).ok())
            .unwrap_or(0o666);
        tokio::spawn(async move {
            loop {
                match tokio::net::UnixListener::bind(&uds_addr) {
                    Ok(uds_listener) => {
                        log::info!("UDS listening...");
                        let perm = fs::Permissions::from_mode(uds_mode);
                        if let Err(e) = fs::set_permissions(&uds_addr, perm) {
                            log::warn!("Set uds file permission {} error: {}", uds_mode, e);
                        }
                        while let Ok((stream, addr)) = uds_listener.accept().await {
                            log::info!("UDS from {:?}", addr);
                            let core = core_cloned.clone();
                            if let Err(e) = handle_uds_stream(core, stream, event_rx_cloned.clone()).await {
                                log::error!("Handle uds error: {}", e);
                            }
                        }
                        log::info!("UDS stopped");
                    }
                    Err(e) => {
                        log::warn!("UDS bind error: {}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }

    // http web/ws
    if let (Some(http_addr), Some(web_dir)) = (
        matches.get_one::<String>("http").cloned(),
        matches.get_one::<String>("web").cloned(),
    ) {
        let core_cloned = core.clone();
        let event_rx = event_rx.clone();
        let _web_dir_cloned = web_dir.clone();
        tokio::spawn(async move {
            loop {
                log::info!("Http web server listening...");
                serve_http(
                    http_addr.parse().unwrap(),
                    web_dir.clone(),
                    core_cloned.clone(),
                    event_rx.clone(),
                )
                .await;
                log::info!("Http web server stopped");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        // Write a _ws.json file
        if let Some(ws_addr) = matches.get_one::<String>("ws") {
            let ws_sock_addr: SocketAddr = ws_addr.parse().unwrap();
            *WS_ADDR.lock().unwrap() = Some(ws_sock_addr);
        }
    }

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(I2C_READ_INTERVAL);
    let mut notify_at = tokio::time::Instant::now();
    let mut battery_high_at = tokio::time::Instant::now(); // last battery high timestamp
    loop {
        interval.tick().await;
        log::debug!("Polling");
        let mut core = core_cloned.lock().expect("unexpected lock failed");
        poll_pisugar_status(&mut core, &event_tx).await;

        // auto shutdown at battery low
        let mut battery_high = true;
        let level = core.level().unwrap_or(100.0);
        let auto_shutdown_level = core.config().auto_shutdown_level.unwrap_or(0.0);

        // check battery level
        if auto_shutdown_level > 0.0 && auto_shutdown_level > (level as f64) {
            battery_high = false;
        }

        // skip if battery high
        if battery_high {
            battery_high_at = tokio::time::Instant::now();
            continue;
        }

        // battery low
        log::debug!("Battery low: {}", level);
        let auto_shutdown_delay = core.config().auto_shutdown_delay.unwrap_or(0.0);
        let now = tokio::time::Instant::now();
        let battery_low_secs = now.duration_since(battery_high_at).as_secs() as f64;
        let shutdown_remain_secs = auto_shutdown_delay - battery_low_secs;

        // notify battery low
        let should_notify = if shutdown_remain_secs > 0.0 {
            if shutdown_remain_secs < 10.0 {
                notify_at + Duration::from_secs(1) < now // every 1s
            } else if shutdown_remain_secs < 30.0 {
                notify_at + Duration::from_secs(3) < now // every 3s
            } else if shutdown_remain_secs < 60.0 {
                notify_at + Duration::from_secs(5) < now // every 5s
            } else {
                false
            }
        } else {
            false
        };
        if should_notify {
            let message = format!("Low battery, will power off after {} seconds", shutdown_remain_secs);
            log::warn!("{}", message);
            notify_shutdown_soon(message.as_str());
            notify_at = now;
        }

        // shutdown
        if shutdown_remain_secs <= 0.0 {
            let shell = core
                .config()
                .soft_poweroff_shell
                .clone()
                .unwrap_or_else(|| "shutdown --poweroff 0".to_string());
            let _ = execute_shell(&shell);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
