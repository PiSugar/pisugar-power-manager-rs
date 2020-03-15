use std::collections::LinkedList;
use std::fs::remove_file;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{exit, Command};
use std::result::Result;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use bytes::*;
use chrono::prelude::*;
use futures::prelude::*;
use futures::SinkExt;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt};
use hyper::{Client, Uri};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::prelude::*;
use tokio::sync::mpsc::unbounded_channel;
use tokio_tungstenite::{accept_async, tungstenite::Error};
use tokio_util::codec::{BytesCodec, Framed};

use pisugar_core::{
    bat_read_gpio_tap, bat_read_intensity, bat_read_voltage, bcd_to_datetime, datetime_to_bcd,
    execute_shell, gpio_detect_tap, rtc_clean_alarm_flag, rtc_disable_alarm, rtc_read_alarm_flag,
    rtc_read_time, rtc_set_alarm, rtc_set_test_wake, rtc_write_time, sys_write_time,
    Error as _Error, PiSugarConfig, PiSugarCore, PiSugarStatus, TapType, MODEL_V2,
};

const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
const CLIENT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const I2C_READ_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

type EventTx = tokio::sync::watch::Sender<String>;
type EventRx = tokio::sync::watch::Receiver<String>;

#[derive(Serialize)]
struct PiSugarStatusJson {
    model: String,
    voltage: f64,
    intensity: f64,
    level: f64,
    charging: bool,
    rtc_time: DateTime<Local>,
    rtc_time_list: Vec<DateTime<Local>>,
    rtc_alarm_flag: bool,
    rtc_alarm_type: String,
    rtc_alarm_time: DateTime<Local>,
    rtc_alarm_repeat: bool,
    safe_shutdown_level: f64,
    button_enable: bool,
    button_shell: String,
}

impl Default for PiSugarStatusJson {
    fn default() -> Self {
        Self {
            model: "".to_string(),
            voltage: 0.0,
            intensity: 0.0,
            level: 0.0,
            charging: false,
            rtc_time: Local::now(),
            rtc_time_list: vec![],
            rtc_alarm_flag: false,
            rtc_alarm_type: "".to_string(),
            rtc_alarm_time: Local::now(),
            rtc_alarm_repeat: false,
            safe_shutdown_level: 0.0,
            button_enable: false,
            button_shell: "".to_string(),
        }
    }
}

/// Poll pisugar status
fn poll_pisugar_status(core: &mut PiSugarCore, gpio_detect_history: &mut String, tx: &EventTx) {
    log::debug!("Polling state");

    let now = Instant::now();
    let status = &mut core.status;
    let config = &mut core.config;

    if let Ok(Some(tap_type)) = status.poll(config, now) {
        tx.broadcast(format!("{}", tap_type));
    }
}

/// Handle request
fn handle_request(core: &mut PiSugarCore, req: &str) -> String {
    let now = Instant::now();
    let parts: Vec<String> = req.split(" ").map(|s| s.to_string()).collect();

    let status = &mut core.status;
    let config = &mut core.config;

    let mut resp = String::new();
    let err = "Invalid request.\n".to_string();
    if parts.len() > 0 {
        match parts[0].as_str() {
            "get" => {
                if parts.len() > 1 {
                    resp = match parts[1].as_str() {
                        "model" => status.mode().to_string(),
                        "battery" => status.level().to_string(),
                        "battery_v" => status.voltage().to_string(),
                        "battery_i" => status.intensity().to_string(),
                        "battery_charging" => status.is_charging(now).to_string(),
                        "rtc_time" => status.rtc_time().to_rfc2822(),
                        "rtc_time_list" => format!("{:?}", datetime_to_bcd(status.rtc_time())),
                        "rtc_alarm_flag" => match rtc_read_alarm_flag() {
                            Ok(flag) => format!("{}", flag),
                            Err(e) => {
                                log::error!("{}", e);
                                return err;
                            }
                        },
                        "alarm_type" => format!("{}", config.auto_wake_type),
                        "alarm_repeat" => format!("{}", config.auto_wake_repeat),
                        "safe_shutdown_level" => format!("{}", config.auto_shutdown_level),
                        "button_enable" => {
                            if parts.len() > 2 {
                                let enable = match parts[2].as_str() {
                                    "single" => config.single_tap_enable,
                                    "double" => config.double_tap_enable,
                                    "long" => config.long_tap_enable,
                                    _ => {
                                        log::error!("{} {}: unknown tap type", parts[0], parts[1]);
                                        return err;
                                    }
                                };
                                format!("{} {}", parts[2], enable)
                            } else {
                                return err;
                            }
                        }
                        "button_shell" => {
                            if parts.len() > 2 {
                                let shell = match parts[2].as_str() {
                                    "single" => config.single_tap_shell.as_str(),
                                    "double" => config.double_tap_shell.as_str(),
                                    "long" => config.long_tap_shell.as_str(),
                                    _ => {
                                        log::error!("{} {}: unknown tap type", parts[0], parts[1]);
                                        return err;
                                    }
                                };
                                format!("{} {}", parts[2], shell)
                            } else {
                                return err;
                            }
                        }
                        _ => return err,
                    };

                    return format!("{}: {}\n", parts[1], resp);
                };
            }
            "rtc_clear_flag" => {
                return match rtc_clean_alarm_flag() {
                    Ok(flag) => format!("{}: done\n", parts[0]),
                    Err(e) => err,
                };
            }
            "rtc_pi2rtc" => {
                let now = Local::now();
                let bcd_time = datetime_to_bcd(now);
                return match rtc_write_time(&bcd_time) {
                    Ok(_) => format!("{}: done\n", parts[0]),
                    Err(e) => err,
                };
            }
            "rtc_rtc2pi" => {
                return match rtc_read_time() {
                    Ok(bcd_time) => {
                        let time = bcd_to_datetime(&bcd_time);
                        sys_write_time(time);
                        format!("{}: done\n", parts[0])
                    }
                    Err(e) => err,
                };
            }
            "rtc_web" => {
                tokio::spawn(async move {
                    if let Ok(resp) = Client::new()
                        .get("http://cdn.pisugar.com".parse().unwrap())
                        .await
                    {
                        if let Some(date) = resp.headers().get("Date") {
                            if let Err(e) =
                                date.to_str().map_err(|e| format!("{}", e)).and_then(|s| {
                                    DateTime::parse_from_rfc2822(s)
                                        .map_err(|e| "Not a rfc2822 datetime string".to_string())
                                        .and_then(|dt| {
                                            let bcd_time = datetime_to_bcd(dt.into());
                                            sys_write_time(dt.into());
                                            rtc_write_time(&bcd_time).map_err(|e| format!("{}", e))
                                        })
                                })
                            {
                                log::error!("{}", e);
                            }
                        }
                    }
                });
                return format!("{}: done\n", parts[0]);
            }
            "rtc_alarm_set" => {
                if parts.len() >= 3 {
                    let mut bcd_time = [0_u8; 7];
                    if let Ok(weekday_repeat) = parts[2].parse::<u8>() {
                        let times: Vec<String> =
                            parts[1].split(",").map(|s| s.to_string()).collect();
                        if times.len() == 7 {
                            for i in 0..7 {
                                if let Ok(v) = times[i].parse::<u8>() {
                                    bcd_time[i] = v;
                                } else {
                                    return err;
                                }
                            }
                            if let Ok(_) = rtc_set_alarm(&bcd_time, weekday_repeat) {
                                return format!("{}: done\n", parts[0]);
                            }
                        }
                    }
                }
                return err;
            }
            "rtc_alarm_disable" => {
                return match rtc_disable_alarm() {
                    Ok(_) => format!("{}: done\n", parts[0]),
                    Err(_) => err,
                };
            }
            "set_safe_shutdown_level" => {
                if parts.len() >= 1 {
                    if let Ok(level) = parts[1].parse::<f64>() {
                        config.auto_shutdown_level = level;
                        return format!("{}: done\n", parts[0]);
                    }
                }
                return err;
            }
            "rtc_test_wake" => {
                return match rtc_set_test_wake() {
                    Ok(_) => format!("{}: wakeup after 1 min 30 sec\n", parts[0]),
                    Err(e) => err,
                };
            }
            "set_button_enable" => {
                if parts.len() > 2 {
                    let enable = parts[2].as_str().ne("0");
                    match parts[1].as_str() {
                        "single" => config.single_tap_enable = enable,
                        "double" => config.double_tap_enable = enable,
                        "long" => config.long_tap_enable = enable,
                        _ => {
                            return err;
                        }
                    }
                    core.save_config();
                    return format!("{}: done\n", parts[0]);
                }
                return err;
            }
            "set_button_shell" => {
                if parts.len() > 2 {
                    let cmd = parts[2..].join(" ");
                    match parts[1].as_str() {
                        "single" => config.single_tap_shell = cmd,
                        "double" => config.double_tap_shell = cmd,
                        "long" => config.long_tap_shell = cmd,
                        _ => {
                            return err;
                        }
                    }
                    core.save_config();
                    return format!("{}: done\n", parts[0]);
                }
                return err;
            }
            _ => return err,
        }
    };

    err
}

async fn _handle_stream<T>(
    core: Arc<Mutex<PiSugarCore>>,
    stream: T,
    event_rx: EventRx,
) -> io::Result<()>
where
    T: 'static + AsyncRead + AsyncWrite + Send,
{
    let framed = Framed::new(stream, BytesCodec::new());
    let (sink, mut stream) = framed.split();
    let (tx, mut rx) = unbounded();

    // handle request
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(buf)) = stream.next().await {
            let req = String::from_utf8_lossy(buf.as_ref()).replace("\n", "");
            let resp = match core.lock() {
                Ok(mut core) => handle_request(&mut core, req.as_str()),
                Err(e) => "".to_string(),
            };
            tx_cloned.send(resp).await;
        }
    });

    // button event
    tokio::spawn(event_rx.map(Ok).forward(tx));

    // send back
    tokio::spawn(rx.map(|s| Ok(Bytes::from(s))).forward(sink));

    Ok(())
}

/// Handle tcp stream
async fn handle_tcp_stream(
    core: Arc<Mutex<PiSugarCore>>,
    stream: TcpStream,
    event_rx: EventRx,
) -> io::Result<()> {
    log::info!("Incoming tcp connection from: {}", stream.peer_addr()?);
    _handle_stream(core, stream, event_rx).await
}

/// Handle websocket request
async fn handle_ws_connection(
    core: Arc<Mutex<PiSugarCore>>,
    stream: TcpStream,
    event_rx: EventRx,
) -> io::Result<()> {
    log::info!("Incoming ws connection from: {}", stream.peer_addr()?);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .await?;
    log::info!("WS connection established");

    let (mut tx, mut rx) = unbounded::<String>();
    let (mut sink, mut stream) = ws_stream.split();

    // handle request
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok(msg) = msg.to_text() {
                let req = msg.replace("\n", "");
                let resp = match core.lock() {
                    Ok(mut core) => handle_request(&mut core, req.as_str()),
                    Err(e) => "".to_string(),
                };
                tx_cloned
                    .send(resp)
                    .await
                    .expect("Unexpected channel failed");
            }
        }
    });

    // button event
    tokio::spawn(event_rx.map(Ok).forward(tx));

    // send back
    tokio::spawn(rx.map(|s| Ok(s.into())).forward(sink));

    Ok(())
}

/// Handle uds
async fn handle_uds_stream(
    core: Arc<Mutex<PiSugarCore>>,
    stream: UnixStream,
    event_rx: EventRx,
) -> io::Result<()> {
    log::info!("Incoming uds stream: {:?}", stream.peer_addr()?);
    _handle_stream(core, stream, event_rx).await
}

/// Clean up before exit
extern "C" fn clean_up() {
    let uds_addr = "/tmp/pisugar.socket";
    let p: &Path = Path::new(uds_addr);
    if p.exists() {
        match remove_file(p) {
            Ok(_) => {}
            Err(e) => {
                log::error!("{}", e);
                exit(1);
            }
        }
    }
    exit(0)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    // config
    let tcp_addr = "127.0.0.1:8080";
    let ws_addr = "127.0.0.1:8082";
    let uds_addr = "/tmp/pisugar.socket";

    // core
    let config = PiSugarConfig::default();
    let core = Arc::new(Mutex::new(PiSugarCore::new(config)));

    // event watch
    let (event_tx, event_rx) = tokio::sync::watch::channel("".to_string());

    // CTRL+C signal handling
    ctrlc::set_handler(|| {
        clean_up();
    });

    // tcp
    let core_cloned = core.clone();
    let event_rx_cloned = event_rx.clone();
    let mut tcp_listener: TcpListener = TcpListener::bind(tcp_addr).await?;
    tokio::spawn(async move {
        log::info!("TCP listening...");
        while let Some(Ok(stream)) = tcp_listener.incoming().next().await {
            handle_tcp_stream(core_cloned.clone(), stream, event_rx_cloned.clone()).await;
        }
        log::info!("TCP stopped");
    });

    // ws
    let core_cloned = core.clone();
    let event_rx_cloned = event_rx.clone();
    let mut ws_listener = tokio::net::TcpListener::bind(ws_addr).await?;
    tokio::spawn(async move {
        log::info!("WS listening...");
        while let Ok(Some(stream)) = ws_listener.incoming().try_next().await {
            handle_ws_connection(core_cloned.clone(), stream, event_rx_cloned.clone()).await;
        }
        log::info!("WS stopped");
    });

    // uds
    let core_cloned = core.clone();
    let event_rx_cloned = event_rx;
    let mut uds_listener = tokio::net::UnixListener::bind(uds_addr)?;
    tokio::spawn(async move {
        log::info!("UDS listening...");
        while let Some(Ok(stream)) = uds_listener.incoming().next().await {
            handle_uds_stream(core_cloned.clone(), stream, event_rx_cloned.clone()).await;
        }
        log::info!("UDS stopped");
    });

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(Duration::from_millis(200));
    let mut gpio_detect_history = String::with_capacity(30);
    loop {
        interval.tick().await;
        let mut core = core_cloned.lock().expect("unexpected lock failed");
        poll_pisugar_status(&mut core, &mut gpio_detect_history, &event_tx);
    }

    clean_up();
    Ok(())
}
