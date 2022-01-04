use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::env;
use std::fs::remove_file;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Instant, SystemTime};

use chrono::prelude::*;
use clap::{App, Arg};
use digest_auth::{AuthContext, AuthorizationHeader, Charset, Qop, WwwAuthenticateHeader};
use env_logger::Env;
use futures::prelude::*;
use futures::SinkExt;
use futures_channel::mpsc::unbounded;
use hyper::client::Client;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response};
use hyper::{Request, Server};
use hyper_websocket_lite::{server_upgrade, AsyncClient};
use lazy_static::lazy_static;
use log::LevelFilter;
use rand::RngCore;
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::time::Duration;
use tokio_util::codec::{BytesCodec, Framed};
use websocket_codec::{Message, Opcode};

use pisugar_core::{
    execute_shell, notify_shutdown_soon, sys_write_time, Error, Model, PiSugarConfig, PiSugarCore, RTCRawTime,
    I2C_READ_INTERVAL, TIME_HOST,
};

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
            let _ = tx.send(format!("{}", tap_type));
        }
        Err(e) => {
            log::debug!("Poll error: {}", e);
        }
        _ => {}
    }
}

/// Handle request
fn handle_request(core: Arc<Mutex<PiSugarCore>>, req: &str) -> String {
    let parts: Vec<String> = req.split(" ").map(|s| s.to_string()).collect();
    let err = "Invalid request.\n".to_string();

    log::debug!("Request: {}", req);

    let core_cloned = core.clone();
    if let Ok(mut core) = core.lock() {
        if parts.len() > 0 {
            match parts[0].as_str() {
                "get" => {
                    if parts.len() > 1 {
                        let resp = match parts[1].as_str() {
                            "version" => Ok(env!("CARGO_PKG_VERSION").to_string()),
                            "model" => Ok(core.model()),
                            "battery" => core.level().map(|l| l.to_string()),
                            "battery_v" => core.voltage_avg().map(|v| v.to_string()),
                            "battery_i" => core.intensity_avg().map(|i| i.to_string()),
                            "battery_led_amount" => core.led_amount().map(|n| n.to_string()),
                            "battery_power_plugged" => core.power_plugged().map(|p| p.to_string()),
                            "battery_allow_charging" => core.allow_charging().map(|a| a.to_string()),
                            "battery_charging_range" => core
                                .charging_range()
                                .map(|r| r.map_or("".to_string(), |r| format!("{},{}", r.0, r.1))),
                            "battery_charging" => core.charging().map(|c| c.to_string()),
                            "battery_input_protect_enabled" => core.input_protected().map(|c| c.to_string()),
                            "battery_output_enabled" => core.output_enabled().map(|o| o.to_string()),
                            "full_charge_duration" => Ok(core
                                .config()
                                .full_charge_duration
                                .map_or("".to_string(), |d| d.to_string())),
                            "system_time" => Ok(Local::now().to_rfc3339()),
                            "rtc_time" => core.read_time().map(|t| t.to_rfc3339()),
                            "rtc_time_list" => core.read_raw_time().map(|r| r.to_string()),
                            "rtc_alarm_flag" => core.read_alarm_flag().map(|f| f.to_string()),
                            "rtc_alarm_time" => {
                                let t = core
                                    .read_alarm_time()
                                    .and_then(|r| r.try_into().map_err(|_| Error::Other("Invalid".to_string())));
                                t.map(|t: DateTime<Utc>| t.with_timezone(Local::now().offset()).to_rfc3339())
                            }
                            "rtc_alarm_time_list" => core.read_alarm_time().map(|r| r.to_string()),
                            "rtc_alarm_enabled" => core.read_alarm_enabled().map(|e| e.to_string()),
                            "rtc_adjust_ppm" => Ok(core.config().rtc_adj_ppm.unwrap_or_default().to_string()),
                            "alarm_repeat" => Ok(core.config().auto_wake_repeat.to_string()),
                            "safe_shutdown_level" => Ok(core.config().auto_shutdown_level.to_string()),
                            "safe_shutdown_delay" => Ok(core.config().auto_shutdown_delay.to_string()),
                            "button_enable" => {
                                if parts.len() > 2 {
                                    let enable = match parts[2].as_str() {
                                        "single" => core.config().single_tap_enable,
                                        "double" => core.config().double_tap_enable,
                                        "long" => core.config().long_tap_enable,
                                        _ => {
                                            log::error!("{} {}: unknown tap type", parts[0], parts[1]);
                                            return err;
                                        }
                                    };
                                    Ok(format!("{} {}", parts[2], enable))
                                } else {
                                    return err;
                                }
                            }
                            "button_shell" => {
                                if parts.len() > 2 {
                                    let shell = match parts[2].as_str() {
                                        "single" => core.config().single_tap_shell.as_str(),
                                        "double" => core.config().double_tap_shell.as_str(),
                                        "long" => core.config().long_tap_shell.as_str(),
                                        _ => {
                                            log::error!("{} {}: unknown tap type", parts[0], parts[1]);
                                            return err;
                                        }
                                    };
                                    Ok(format!("{} {}", parts[2], shell))
                                } else {
                                    return err;
                                }
                            }
                            "auto_power_on" => Ok(core.config().auto_power_on.unwrap_or(false).to_string()),
                            "auth_username" => {
                                let username = if let Some(digest_auth) = &core.config().digest_auth {
                                    digest_auth.0.to_string()
                                } else {
                                    "".to_string()
                                };
                                Ok(username)
                            }
                            "anti_mistouch" => Ok(core.config().anti_mistouch.unwrap_or(true).to_string()),
                            "soft_poweroff" => Ok(core.config().soft_poweroff.unwrap_or(false).to_string()),
                            "temperature" => core.get_temperature().map(|x| x.to_string()),
                            "input_protect" => core.input_protected().map(|x| x.to_string()),
                            _ => return err,
                        };

                        return if resp.is_ok() {
                            format!("{}: {}\n", parts[1], resp.unwrap())
                        } else {
                            log::error!("{}", resp.err().unwrap());
                            err
                        };
                    };
                }
                "set_battery_charging_range" => {
                    let mut charging_range = None;
                    if parts.len() > 1 {
                        let range: Vec<String> = parts[1].split(',').map(|s| s.to_string()).collect();
                        if range.len() == 2 {
                            if let (Ok(begin), Ok(end)) = (range[0].parse::<f32>(), range[1].parse::<f32>()) {
                                charging_range = Some((begin, end));
                            }
                        }
                    }
                    return match core.set_charging_range(charging_range) {
                        Ok(_) => format!("{}: done\n", parts[0]),
                        Err(e) => {
                            log::error!("{}", e);
                            err
                        }
                    };
                }
                "set_battery_input_protect" => {
                    if parts.len() > 1 {
                        if let Ok(enable) = parts[1].parse::<bool>() {
                            if core.toggle_input_protected(enable).is_ok() {
                                return format!("{}: done\n", parts[0]);
                            }
                        }
                    }
                    return err;
                }
                "set_battery_output" => {
                    if parts.len() > 1 {
                        if let Ok(enable) = parts[1].parse::<bool>() {
                            if core.toggle_output_enabled(enable).is_ok() {
                                return format!("{}: done\n", parts[0]);
                            }
                        }
                    }
                    return err;
                }
                "set_full_charge_duration" => {
                    if parts.len() > 1 {
                        if let Ok(d) = parts[1].parse::<u64>() {
                            core.config_mut().full_charge_duration = Some(d);
                            if core.save_config().is_ok() {
                                return format!("{}: done\n", parts[0]);
                            }
                        }
                    }
                    return err;
                }
                "set_allow_charging" => {
                    if parts.len() > 1 {
                        if let Ok(enable) = parts[1].parse::<bool>() {
                            if core.toggle_allow_charging(enable).is_ok() {
                                return format!("{}: done\n", parts[0]);
                            }
                        }
                    }
                    return err;
                }
                "rtc_clear_flag" => {
                    return match core.clear_alarm_flag() {
                        Ok(_) => format!("{}: done\n", parts[0]),
                        Err(e) => {
                            log::error!("{}", e);
                            err
                        }
                    };
                }
                "rtc_pi2rtc" => {
                    let now = Local::now();
                    return match core.write_time(now) {
                        Ok(_) => format!("{}: done\n", parts[0]),
                        Err(e) => {
                            log::error!("{}", e);
                            err
                        }
                    };
                }
                "rtc_rtc2pi" => {
                    return match core.read_time() {
                        Ok(t) => {
                            sys_write_time(t);
                            format!("{}: done\n", parts[0])
                        }
                        Err(e) => {
                            log::error!("{}", e);
                            err
                        }
                    };
                }
                "rtc_web" => {
                    tokio::spawn(async move {
                        if let Ok(resp) = Client::new().get(TIME_HOST.parse().unwrap()).await {
                            if let Some(date) = resp.headers().get("date") {
                                if let Ok(Ok(dt)) = date.to_str().map(|s| DateTime::parse_from_rfc2822(s)) {
                                    sys_write_time(dt.into());
                                    if let Ok(core) = core_cloned.lock() {
                                        let _ = core.write_time(dt.into());
                                    }
                                } else {
                                    log::warn!("Parse date error");
                                }
                            } else {
                                log::warn!("Failed to get date from {}", TIME_HOST);
                            }
                        }
                    });
                    return format!("{}: done\n", parts[0]);
                }
                "rtc_alarm_set" => {
                    // rtc_alarm_set <iso8601 ignore ymd> weekday_repeat
                    if parts.len() >= 3 {
                        if let Ok(datetime) = parts[1].parse::<DateTime<FixedOffset>>() {
                            let datetime: DateTime<Local> = datetime.into();
                            let sd3078_time: RTCRawTime = datetime.into();
                            if let Ok(weekday_repeat) = parts[2].parse::<u8>() {
                                match core.write_alarm(sd3078_time, weekday_repeat) {
                                    Ok(_) => {
                                        core.config_mut().auto_wake_repeat = weekday_repeat;
                                        core.config_mut().auto_wake_time = Some(datetime);
                                        if let Err(e) = core.save_config() {
                                            log::warn!("{}", e);
                                        }
                                        return format!("{}: done\n", parts[0]);
                                    }
                                    Err(e) => log::error!("{}", e),
                                }
                            }
                        }
                    }
                    return err;
                }
                "rtc_alarm_disable" => {
                    return match core.disable_alarm() {
                        Ok(_) => {
                            core.config_mut().auto_wake_time = None;
                            if let Err(e) = core.save_config() {
                                log::warn!("{}", e);
                            }
                            format!("{}: done\n", parts[0])
                        }
                        Err(_) => err,
                    };
                }
                "rtc_adjust_ppm" => {
                    if parts.len() >= 1 {
                        if let Ok(ppm) = parts[1].parse::<f64>() {
                            let ppm = if ppm > 500.0 { 500.0 } else { ppm };
                            let ppm = if ppm < -500.0 { -500.0 } else { ppm };
                            return match core.write_rtc_adjust_ppm(ppm) {
                                Ok(()) => {
                                    core.config_mut().rtc_adj_ppm = Some(ppm);
                                    if let Err(e) = core.save_config() {
                                        log::warn!("{}", e);
                                    }
                                    format!("{}: done\n", parts[0])
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                    err
                                }
                            };
                        }
                    }
                }
                "set_safe_shutdown_level" => {
                    if parts.len() >= 1 {
                        if let Ok(level) = parts[1].parse::<f64>() {
                            // level between 0-30
                            let level = if level < 0.0 { 0.0 } else { level };
                            let level = if level > 30.0 { 30.0 } else { level };
                            core.config_mut().auto_shutdown_level = level;
                            if let Err(e) = core.save_config() {
                                log::error!("{}", e);
                            }
                            return format!("{}: done\n", parts[0]);
                        }
                    }
                    return err;
                }
                "set_safe_shutdown_delay" => {
                    if parts.len() >= 1 {
                        if let Ok(delay) = parts[1].parse::<f64>() {
                            // delay between 0-30
                            let delay = if delay < 0.0 { 0.0 } else { delay };
                            let delay = if delay > 120.0 { 120.0 } else { delay };
                            core.config_mut().auto_shutdown_delay = delay;
                            if let Err(e) = core.save_config() {
                                log::error!("{}", e);
                            }
                            return format!("{}: done\n", parts[0]);
                        }
                    }
                    return err;
                }
                "rtc_test_wake" => {
                    return match core.test_wake() {
                        Ok(_) => format!("{}: wakeup after 1 min 30 sec\n", parts[0]),
                        Err(e) => {
                            log::error!("{}", e);
                            err
                        }
                    };
                }
                "set_button_enable" => {
                    if parts.len() > 2 {
                        let enable = parts[2].as_str().ne("0");
                        match parts[1].as_str() {
                            "single" => core.config_mut().single_tap_enable = enable,
                            "double" => core.config_mut().double_tap_enable = enable,
                            "long" => core.config_mut().long_tap_enable = enable,
                            _ => {
                                return err;
                            }
                        }
                        if let Err(e) = core.save_config() {
                            log::error!("{}", e);
                        }
                        return format!("{}: done\n", parts[0]);
                    }
                    return err;
                }
                "set_button_shell" => {
                    if parts.len() > 2 {
                        let cmd = parts[2..].join(" ");
                        match parts[1].as_str() {
                            "single" => core.config_mut().single_tap_shell = cmd,
                            "double" => core.config_mut().double_tap_shell = cmd,
                            "long" => core.config_mut().long_tap_shell = cmd,
                            _ => {
                                return err;
                            }
                        }
                        if let Err(e) = core.save_config() {
                            log::error!("{}", e);
                        }
                        return format!("{}: done\n", parts[0]);
                    }
                    return err;
                }
                "set_auto_power_on" => {
                    if parts.len() > 1 {
                        if let Ok(auto_power_on) = parts[1].parse::<bool>() {
                            if let Err(e) = core.toggle_auto_power_on(auto_power_on) {
                                log::error!("{}", e);
                            }
                            return format!("{}: done\n", parts[0]);
                        }
                    }
                    return err;
                }
                "set_auth" => {
                    if parts.len() > 2 {
                        let username = parts[1].as_str();
                        let password = parts[2].as_str();
                        core.config_mut().digest_auth = Some((username.to_string(), password.to_string()));
                    } else {
                        core.config_mut().digest_auth = None;
                    }
                    if let Err(e) = core.save_config() {
                        log::error!("{}", e);
                        return err;
                    }
                    return format!("{}: done\n", parts[0]);
                }
                "force_shutdown" => {
                    match core.force_shutdown() {
                        Ok(_) => {
                            return format!("{}: done\n", parts[0]);
                        }
                        Err(e) => {
                            log::error!("{}", e);
                        }
                    }
                    return err;
                }
                "set_anti_mistouch" => {
                    if parts.len() > 1 {
                        if let Ok(anti_mistouch) = parts[1].parse::<bool>() {
                            match core.toggle_anti_mistouch(anti_mistouch) {
                                Ok(_) => {
                                    return format!("{}: done\n", parts[0]);
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                }
                            }
                        }
                    }
                    return err;
                }
                "set_soft_poweroff" => {
                    if parts.len() > 1 {
                        if let Ok(soft_poweroff) = parts[1].parse::<bool>() {
                            match core.toggle_soft_poweroff(soft_poweroff) {
                                Ok(_) => {
                                    return format!("{}: done\n", parts[0]);
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                }
                            }
                        }
                        return err;
                    }
                }
                "set_input_protect" => {
                    if parts.len() > 1 {
                        if let Ok(protect) = parts[1].parse::<bool>() {
                            match core.toggle_input_protected(protect) {
                                Ok(_) => {
                                    return format!("{}: done\n", parts[0]);
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                }
                            }
                        }
                        return err;
                    }
                }
                _ => return err,
            }
        };
    }

    err
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
            let reqs = reqs.trim_end_matches("\n");
            for req in reqs.split("\n") {
                log::debug!("Req: {}", req);
                let req = req.replace("\r", "");
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
    // tokio::spawn(event_rx.map(|event| Ok(Some(event))).forward(tx));
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
                let req = msg.replace("\n", "");
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

async fn on_ws_client(stream_mut: AsyncClient, core: Arc<Mutex<PiSugarCore>>, mut event_rx: EventRx) {
    let (mut tx, mut rx) = unbounded();
    let (mut sink, mut s) = stream_mut.split();

    // req
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = s.next().await {
            let resp_msg = match msg.opcode() {
                Opcode::Text => {
                    let req = msg.as_text().unwrap();
                    let resp = handle_request(core.clone(), req);
                    Some(Message::text(resp))
                }
                Opcode::Binary => Some(Message::close(None)),
                Opcode::Ping => Some(Message::pong(msg.into_data())),
                Opcode::Close => Some(Message::close(None)),
                Opcode::Pong => None,
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
        sink.send(rsp).await.expect("Channel failed");
    }
}

lazy_static! {
    static ref SECURITY_CTX: Mutex<HashMap<String, (u32, SystemTime)>> = Mutex::new(HashMap::default());
}
const SECURITY_TIMEOUT_SECONDS: u64 = 30 * 60;

fn build_realm(req: &Request<Body>, user: &str) -> String {
    let host = req
        .headers()
        .get(&hyper::header::HOST)
        .map(|v| v.to_str().unwrap_or(""))
        .map(|v| v.to_string())
        .unwrap_or("localhost".to_string());
    let port = req.uri().port_u16();
    let realm = if let Some(port) = port {
        format!("{}@{}:{}", user, host, port)
    } else {
        format!("{}@{}", user, host)
    };
    realm
}

fn build_www_header(req: &Request<Body>, user: &str) -> String {
    let now = SystemTime::now();
    let realm = build_realm(req, user);

    let mut nonce = [0; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let nonce = base64::encode(nonce);

    let mut opaque = [0; 12];
    rand::thread_rng().fill_bytes(&mut opaque);
    let opaque = base64::encode(opaque);

    let nc = 0;
    SECURITY_CTX
        .lock()
        .unwrap()
        .retain(|_, v| v.1 + Duration::from_secs(SECURITY_TIMEOUT_SECONDS) > now);
    SECURITY_CTX.lock().unwrap().insert(opaque.clone(), (nc, now));

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
        nc,
    };
    header.to_string()
}

fn rebuild_www_header(req: &Request<Body>, auth_header: &AuthorizationHeader) -> Option<WwwAuthenticateHeader> {
    if auth_header.opaque.is_none() {
        return None;
    }
    let opaque = auth_header.opaque.clone().unwrap();

    let ctx = SECURITY_CTX.lock().unwrap();
    if !ctx.contains_key(&opaque) {
        return None;
    }

    let realm = build_realm(req, &auth_header.username);

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
    Some(www_header)
}

/// Handle http request, /ws to websocket
async fn handle_http_req(
    req: Request<Body>,
    static_: hyper_staticfile::Static,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: EventRx,
) -> Result<Response<Body>, io::Error> {
    // check for http auth
    if let Ok(config) = core.lock() {
        if let Some(account) = &config.config().digest_auth {
            let mut auth_context = AuthContext::new(account.0.as_str(), account.1.as_str(), req.uri().to_string());
            let mut auth_ok = false;
            for (name, value) in req.headers() {
                if name.eq(&hyper::header::AUTHORIZATION) {
                    let value = value.to_str();
                    if value.is_err() {
                        continue;
                    }

                    let auth_header = match AuthorizationHeader::parse(value.unwrap()) {
                        Err(e) => {
                            log::warn!("invalid authentication header {}", e);
                            continue;
                        }
                        Ok(h) => h,
                    };
                    auth_context.set_custom_cnonce(auth_header.cnonce.clone().unwrap_or("".to_string()));

                    let www_header = rebuild_www_header(&req, &auth_header);
                    if www_header.is_none() {
                        log::warn!("rebuild www authentication header error");
                        continue;
                    }
                    let mut www_header = www_header.unwrap();
                    let auth_header2 = AuthorizationHeader::from_prompt(&mut www_header, &auth_context)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                    auth_ok = auth_header2.response == auth_header.response;
                }
            }
            if !auth_ok {
                let www_header = build_www_header(&req, &account.0);
                let resp = Response::builder()
                    .status(hyper::StatusCode::UNAUTHORIZED)
                    .header(hyper::header::WWW_AUTHENTICATE, www_header.to_string()) // fix chrome digest auth
                    .body(Body::empty())
                    .unwrap();
                return Ok(resp);
            }
        }
    }

    if req.uri().path().contains(WS_JSON) {
        if let Some(ws_addr) = *WS_ADDR.lock().unwrap() {
            let json = format!("{{\"wsPort\": \"{}\"}}", ws_addr.port());
            return Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::from(json))
                .unwrap());
        } else {
            return Err(io::Error::new(io::ErrorKind::NotFound, ""));
        }
    }
    if req.uri().path().ends_with("/ws") {
        server_upgrade(req, |c| on_ws_client(c, core, event_rx))
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
    } else {
        static_.clone().serve(req).await
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
            Ok::<_, io::Error>(service_fn(move |req| {
                handle_http_req(req, static_.clone(), core.clone(), event_rx.clone())
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
            pid,
        };
        let logger = syslog::unix(formatter).expect("Could not connect to syslog");
        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|_| match debug {
                true => log::set_max_level(LevelFilter::Debug),
                false => log::set_max_level(LevelFilter::Info),
            })
            .expect("Failed to init syslog");
    } else {
        if debug {
            env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        } else {
            env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let models = vec![
        Model::PiSugar_3.to_string(),
        Model::PiSugar_2_Pro.to_string(),
        Model::PiSugar_2_2LEDs.to_string(),
        Model::PiSugar_2_4LEDs.to_string(),
    ];

    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Config file in json format, e.g. /etc/pisugar-server/config.json"),
        )
        .arg(
            Arg::with_name("tcp")
                .short("t")
                .long("tcp")
                .value_name("ADDR")
                .help("Tcp listen address, e.g. 0.0.0.0:8423"),
        )
        .arg(
            Arg::with_name("uds")
                .short("u")
                .long("uds")
                .value_name("FILE")
                .default_value("/tmp/pisugar-server.sock")
                .help("Unix domain socket file, e.g. /tmp/pisugar-server.sock"),
        )
        .arg(
            Arg::with_name("ws")
                .short("w")
                .long("ws")
                .value_name("ADDR")
                .help("Websocket listen address, e.g. 0.0.0.0:8422"),
        )
        .arg(
            Arg::with_name("web")
                .requires_all(&["http"])
                .long("web")
                .value_name("DIR")
                .default_value("/usr/share/pisugar-server/web")
                .help("Web content directory, e.g. web"),
        )
        .arg(
            Arg::with_name("http")
                .long("http")
                .value_name("ADDR")
                .default_value("0.0.0.0:8421")
                .help("Http server listen address, e.g. 0.0.0.0:8421"),
        )
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .takes_value(false)
                .help("Debug output"),
        )
        .arg(
            Arg::with_name("syslog")
                .short("s")
                .long("syslog")
                .takes_value(false)
                .help("Log to syslog"),
        )
        .arg(
            Arg::with_name("led")
                .long("led")
                .takes_value(true)
                .default_value("4")
                .help("2-led or 4-led"),
        )
        .arg(
            Arg::with_name("model")
                .long("model")
                .takes_value(true)
                .required(true)
                .help(&format!("{:?}", models))
                .validator(move |x| {
                    if models.contains(&x) {
                        Ok(())
                    } else {
                        Err("Invalid model".to_string())
                    }
                }),
        )
        .get_matches();

    // init logging
    let debug = matches.is_present("debug");
    let syslog = matches.is_present("syslog");
    init_logging(debug, syslog);

    // model
    let m = matches.value_of("model").unwrap();
    let model = Model::try_from(m).expect(format!("Unknown PiSugar model: {}", m).as_str());
    log::debug!("Running with model: {}({})", model, m);

    // core
    let core;
    loop {
        let c = if matches.is_present("config") {
            PiSugarCore::new_with_path(matches.value_of("config").unwrap(), true, model)
        } else {
            let config = PiSugarConfig::default();
            PiSugarCore::new(config, model)
        };
        match c {
            Ok(_) => {
                core = Arc::new(Mutex::new(c.unwrap()));
                break;
            }
            Err(e) => log::error!("PiSugar init failed: {}", e),
        }
        sleep(Duration::from_secs(3));
    }

    // event watch
    let (event_tx, event_rx) = tokio::sync::watch::channel("".to_string());

    // CTRL+C signal handling
    let uds = matches.value_of("uds").and_then(|x| Some(x.to_string()));
    let web_dir = matches.value_of("web").and_then(|x| Some(x.to_string()));
    ctrlc::set_handler(move || {
        clean_up(uds.clone(), web_dir.clone());
    })
    .expect("Failed to setup ctrl+c");

    // tcp
    if matches.is_present("tcp") {
        let tcp_addr = matches.value_of("tcp").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        match TcpListener::bind(tcp_addr).await {
            Ok(tcp_listener) => {
                tokio::spawn(async move {
                    log::info!("TCP listening...");
                    while let Ok((stream, addr)) = tcp_listener.accept().await {
                        log::info!("TCP from {}", addr);
                        let core = core_cloned.clone();
                        let _ = handle_tcp_stream(core, stream, event_rx_cloned.clone()).await;
                    }
                    log::info!("TCP stopped");
                });
            }
            Err(e) => {
                log::warn!("TCP bind error: {}", e);
            }
        }
    }

    // ws
    if matches.is_present("ws") {
        let ws_addr = matches.value_of("ws").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        match tokio::net::TcpListener::bind(ws_addr).await {
            Ok(ws_listener) => {
                tokio::spawn(async move {
                    log::info!("WS listening...");
                    while let Ok((stream, addr)) = ws_listener.accept().await {
                        log::info!("WS from {}", addr);
                        let core = core_cloned.clone();
                        let _ = handle_ws_connection(core, stream, event_rx_cloned.clone()).await;
                    }
                    log::info!("WS stopped");
                });
            }
            Err(e) => {
                log::warn!("WS bind error: {}", e);
            }
        }
    }

    // uds
    if matches.is_present("uds") {
        let uds_addr = matches.value_of("uds").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        match tokio::net::UnixListener::bind(uds_addr) {
            Ok(uds_listener) => {
                tokio::spawn(async move {
                    log::info!("UDS listening...");
                    while let Ok((stream, addr)) = uds_listener.accept().await {
                        log::info!("UDS from {:?}", addr);
                        let core = core_cloned.clone();
                        let _ = handle_uds_stream(core, stream, event_rx_cloned.clone()).await;
                    }
                    log::info!("UDS stopped");
                });
            }
            Err(e) => {
                log::warn!("UDS bind error: {}", e);
            }
        }
    }

    // http web/ws
    if matches.is_present("http") && matches.is_present("web") {
        let core_cloned = core.clone();
        let event_rx = event_rx.clone();
        let web_dir = matches.value_of("web").unwrap().to_string();
        let http_addr = matches.value_of("http").unwrap().parse().unwrap();
        let _web_dir_cloned = web_dir.clone();
        tokio::spawn(async move {
            log::info!("Http web server listening...");
            let _ = serve_http(http_addr, web_dir, core_cloned, event_rx).await;
            log::info!("Http web server stopped");
        });

        // Write a _ws.json file
        if matches.is_present("ws") {
            let ws_addr = matches.value_of("ws").unwrap();
            let ws_sock_addr: SocketAddr = ws_addr.parse().unwrap();
            *WS_ADDR.lock().unwrap() = Some(ws_sock_addr);
            // let content = format!("{{\"wsPort\": \"{}\"}}", ws_sock_addr.port());
            // let filename = PathBuf::from(web_dir_cloned).join(WS_JSON);
            // let mut file = OpenOptions::default().create(true).write(true).open(filename).await?;
            // file.set_len(0).await?;
            // file.write_all(content.as_bytes()).await?;
        }
    }

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(I2C_READ_INTERVAL);
    let mut notify_at = tokio::time::Instant::now();
    let mut battery_high_at = tokio::time::Instant::now();
    loop {
        interval.tick().await;
        log::debug!("Polling");
        let mut core = core_cloned.lock().expect("unexpected lock failed");
        poll_pisugar_status(&mut core, &event_tx).await;

        // auto shutdown
        if let Ok(level) = core.level() {
            if (level as f64) < (core.config().auto_shutdown_level) {
                log::warn!("Battery low: {}", level);

                let now = tokio::time::Instant::now();
                let seconds = now.duration_since(battery_high_at).as_millis() as f64;
                let remains = core.config().auto_shutdown_delay - seconds;
                let remains = if remains < 0.0 { 0.0 } else { remains };

                let should_notify = if remains <= 0.0 {
                    false
                } else if remains < 30.0 {
                    notify_at + Duration::from_secs(1) < now
                } else if remains < 60.0 {
                    notify_at + Duration::from_secs(5) < now
                } else if remains < 120.0 {
                    notify_at + Duration::from_secs(10) < now
                } else {
                    false
                };

                if should_notify {
                    let message = format!("Low battery, will power off after {} seconds", remains);
                    notify_shutdown_soon(message.as_str());
                    notify_at = now;
                }

                if remains <= 0.0 {
                    let _ = execute_shell("shutdown --poweroff 0");
                }
            } else {
                battery_high_at = tokio::time::Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use hyper::Client;

    use super::TIME_HOST;

    #[tokio::test]
    async fn test_web_date() {
        let resp = Client::new().get(TIME_HOST.parse().unwrap()).await.unwrap();
        if let Some(date) = resp.headers().get("date") {
            if let Ok(Ok(dt)) = date.to_str().map(|s| DateTime::parse_from_rfc2822(s)) {
                println!("{}", dt);
            }
        }
    }
}
