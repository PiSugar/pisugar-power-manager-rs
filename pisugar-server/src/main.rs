use std::convert::TryInto;
use std::env;
use std::fs::remove_file;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::*;
use chrono::prelude::*;
use clap::{App, Arg};
use env_logger::Env;
use futures::prelude::*;
use futures::SinkExt;
use futures_channel::mpsc::unbounded;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Response};
use hyper::{Request, Server};
use hyper_websocket_lite::{server_upgrade, AsyncClient};
use log::LevelFilter;
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::time::Duration;
use tokio_util::codec::{BytesCodec, Framed};
use websocket_codec::{Message, Opcode};

use pisugar_core::{
    execute_shell, notify_shutdown_soon, sys_write_time, Error, PiSugarConfig, PiSugarCore,
    SD3078Time, I2C_READ_INTERVAL, TIME_HOST,
};

/// Websocket info
const WS_JSON: &str = "_ws.json";

/// Tap event tx
type EventTx = tokio::sync::watch::Sender<String>;

/// Tap event rx
type EventRx = tokio::sync::watch::Receiver<String>;

/// Poll pisugar status
fn poll_pisugar_status(core: &mut PiSugarCore, tx: &EventTx) {
    log::debug!("Polling state");
    let now = Instant::now();
    if let Ok(Some(tap_type)) = core.poll(now) {
        let _ = tx.broadcast(format!("{}", tap_type));
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
                            "model" => Ok(core.model()),
                            "battery" => core.level().map(|l| l.to_string()),
                            "battery_v" => core.voltage().map(|v| v.to_string()),
                            "battery_i" => core.intensity().map(|i| i.to_string()),
                            "battery_charging" => core.charging().map(|c| c.to_string()),
                            "rtc_time" => core.read_time().map(|t| t.to_rfc3339()),
                            "rtc_time_list" => core.read_raw_time().map(|r| r.to_string()),
                            "rtc_alarm_flag" => core.read_alarm_flag().map(|f| f.to_string()),
                            "rtc_alarm_time" => core
                                .read_alarm_time()
                                .and_then(|r| {
                                    r.try_into()
                                        .map_err(|_| Error::Other("Invalid".to_string()))
                                })
                                .map(|t: DateTime<Local>| t.to_rfc3339()),
                            "rtc_alarm_time_list" => core.read_alarm_time().map(|r| r.to_string()),
                            "rtc_alarm_enabled" => core.read_alarm_enabled().map(|e| e.to_string()),
                            "alarm_repeat" => Ok(core.config().auto_wake_repeat.to_string()),
                            "safe_shutdown_level" => {
                                Ok(core.config().auto_shutdown_level.to_string())
                            }
                            "safe_shutdown_delay" => {
                                Ok(core.config().auto_shutdown_delay.to_string())
                            }
                            "button_enable" => {
                                if parts.len() > 2 {
                                    let enable = match parts[2].as_str() {
                                        "single" => core.config().single_tap_enable,
                                        "double" => core.config().double_tap_enable,
                                        "long" => core.config().long_tap_enable,
                                        _ => {
                                            log::error!(
                                                "{} {}: unknown tap type",
                                                parts[0],
                                                parts[1]
                                            );
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
                                            log::error!(
                                                "{} {}: unknown tap type",
                                                parts[0],
                                                parts[1]
                                            );
                                            return err;
                                        }
                                    };
                                    Ok(format!("{} {}", parts[2], shell))
                                } else {
                                    return err;
                                }
                            }
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
                    if let Ok(t) = core.read_time() {
                        sys_write_time(t);
                        format!("{}: done\n", parts[0]);
                    } else {
                    }
                }
                "rtc_web" => {
                    tokio::spawn(async move {
                        if let Ok(resp) = Client::new().get(TIME_HOST.parse().unwrap()).await {
                            if let Some(date) = resp.headers().get("Date") {
                                if let Ok(s) = date.to_str() {
                                    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
                                        if let Ok(core) = core_cloned.lock() {
                                            sys_write_time(dt.into());
                                            let _ = core.write_time(dt.into());
                                        }
                                    }
                                }
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
                            let sd3078_time: SD3078Time = datetime.into();
                            if let Ok(weekday_repeat) = parts[2].parse::<u8>() {
                                match core.set_alarm(sd3078_time, weekday_repeat) {
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
                        Ok(_) => format!("{}: done\n", parts[0]),
                        Err(_) => err,
                    };
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
                _ => return err,
            }
        };
    }

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
    let (tx, rx) = unbounded();

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
        tokio::time::delay_for(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::debug!("Stream close");
    });

    // button event
    tokio::spawn(event_rx.map(|event| Ok(Some(event))).forward(tx));

    // send back
    tokio::spawn(
        rx.map(|s| match s {
            Some(s) => {
                log::debug!("Sink send: {}", s);
                Ok(Bytes::from(s))
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

    let (tx, rx) = unbounded();
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
        tokio::time::delay_for(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::debug!("WS stream close")
    });

    // button event
    tokio::spawn(event_rx.map(|e| Ok(Some(e))).forward(tx));

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
async fn handle_uds_stream(
    core: Arc<Mutex<PiSugarCore>>,
    stream: UnixStream,
    event_rx: EventRx,
) -> io::Result<()> {
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

async fn on_ws_client(stream_mut: AsyncClient, core: Arc<Mutex<PiSugarCore>>, event_rx: EventRx) {
    let (tx, mut rx) = unbounded();
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
        tokio::time::delay_for(Duration::from_millis(100)).await;
        tx_cloned.send(None).await.expect("Channel failed");
        log::info!("Websocket closed");
    });

    // button event
    tokio::spawn(event_rx.map(|e| Ok(Some(Message::text(e)))).forward(tx));

    // send back
    while let Some(Some(rsp)) = rx.next().await {
        sink.send(rsp).await.expect("Channel failed");
    }
}

async fn handle_http_req(
    req: Request<Body>,
    static_: hyper_staticfile::Static,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: EventRx,
) -> Result<Response<Body>, io::Error> {
    if req.uri().path().ends_with("/ws") {
        server_upgrade(req, |c| on_ws_client(c, core, event_rx))
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
    } else {
        static_.clone().serve(req).await
    }
}

/// Serve http
async fn serve_http(
    http_addr: SocketAddr,
    web_dir: String,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: EventRx,
) {
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
            env_logger::from_env(Env::default().default_filter_or("debug")).init();
        } else {
            env_logger::from_env(Env::default().default_filter_or("info")).init();
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Config file in json format, e.g. /etc/pisugar-server.json"),
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
                .help("Web content directory, e.g. web"),
        )
        .arg(
            Arg::with_name("http")
                .long("http")
                .value_name("ADDR")
                .default_value("0.0.0.0:8080")
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
        .get_matches();

    // init logging
    let debug = matches.is_present("debug");
    let syslog = matches.is_present("syslog");
    init_logging(debug, syslog);

    // led
    let led_amount = matches.value_of("led").unwrap_or("4").parse().unwrap_or(4);

    // core
    let core = if matches.is_present("config") {
        PiSugarCore::new_with_path(matches.value_of("config").unwrap(), true, led_amount).unwrap()
    } else {
        let config = PiSugarConfig::default();
        PiSugarCore::new(config, led_amount).unwrap()
    };
    let core = Arc::new(Mutex::new(core));

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
            Ok(mut tcp_listener) => {
                tokio::spawn(async move {
                    log::info!("TCP listening...");
                    while let Some(Ok(stream)) = tcp_listener.incoming().next().await {
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
            Ok(mut ws_listener) => {
                tokio::spawn(async move {
                    log::info!("WS listening...");
                    while let Some(Ok(stream)) = ws_listener.incoming().next().await {
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
            Ok(mut uds_listener) => {
                tokio::spawn(async move {
                    log::info!("UDS listening...");
                    while let Some(Ok(stream)) = uds_listener.incoming().next().await {
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
        let web_dir_cloned = web_dir.clone();
        tokio::spawn(async move {
            log::info!("Http web server listening...");
            let _ = serve_http(http_addr, web_dir, core_cloned, event_rx).await;
            log::info!("Http web server stopped");
        });

        // Write a _ws.json file
        if matches.is_present("ws") {
            let ws_addr = matches.value_of("ws").unwrap();
            let ws_sock_addr: SocketAddr = ws_addr.parse().unwrap();
            let content = format!("{{\"wsPort\": \"{}\"}}", ws_sock_addr.port());
            let filename = PathBuf::from(web_dir_cloned).join("_ws.json");
            let mut file = OpenOptions::default()
                .create(true)
                .write(true)
                .open(filename)
                .await?;
            file.set_len(0).await?;
            file.write_all(content.as_bytes()).await?;
        }
    }

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(I2C_READ_INTERVAL);
    let mut notify_at = tokio::time::Instant::now();
    let mut shutdown_at = tokio::time::Instant::now();
    loop {
        interval.tick().await;
        log::debug!("Polling");
        let mut core = core_cloned.lock().expect("unexpected lock failed");
        poll_pisugar_status(&mut core, &event_tx);

        // auto shutdown
        if let Ok(level) = core.level() {
            if level as f64 <= core.config().auto_shutdown_level {
                let now = tokio::time::Instant::now();
                let seconds = now.duration_since(shutdown_at).as_millis() as f64;
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
                    let _ = execute_shell("/sbin/shutdown --poweroff 0");
                }
            } else {
                shutdown_at = tokio::time::Instant::now();
            }
        }
    }
}
