use std::fs::remove_file;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::*;
use chrono::prelude::*;
use clap::{App, Arg};
use futures::prelude::*;
use futures::SinkExt;
use futures_channel::mpsc::unbounded;
use hyper::service::{make_service_fn, service_fn};
use hyper::Client;
use hyper::Server;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio_util::codec::{BytesCodec, Framed};

use pisugar_core::{sys_write_time, PiSugarConfig, PiSugarCore, I2C_READ_INTERVAL, TIME_HOST};

type EventTx = tokio::sync::watch::Sender<String>;
type EventRx = tokio::sync::watch::Receiver<String>;

/// Poll pisugar status
fn poll_pisugar_status(core: &mut PiSugarCore, tx: &EventTx) {
    log::debug!("Polling state");

    let now = Instant::now();
    let status = &mut core.status;
    let config = &mut core.config;

    if let Ok(Some(tap_type)) = status.poll(config, now) {
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
                            "model" => core.model().to_string(),
                            "battery" => core.level().to_string(),
                            "battery_v" => core.voltage().to_string(),
                            "battery_i" => core.intensity().to_string(),
                            "battery_charging" => core.charging().to_string(),
                            "rtc_time" => core.read_time().to_rfc2822(),
                            "rtc_time_list" => format!("{}", core.read_raw_time()),
                            "rtc_alarm_flag" => match core.read_alarm_flag() {
                                Ok(flag) => format!("{}", flag),
                                Err(e) => {
                                    log::error!("{}", e);
                                    return err;
                                }
                            },
                            "alarm_type" => format!("{}", core.config().auto_wake_type),
                            "alarm_repeat" => format!("{}", core.config().auto_wake_repeat),
                            "safe_shutdown_level" => {
                                format!("{}", core.config().auto_shutdown_level)
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
                                    format!("{} {}", parts[2], enable)
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
                    let t = core.read_time();
                    sys_write_time(t);
                    return format!("{}: done\n", parts[0]);
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
                                if let Ok(_) = core.set_alarm(bcd_time.into(), weekday_repeat) {
                                    return format!("{}: done\n", parts[0]);
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
                            core.config_mut().auto_shutdown_level = level;
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
            let req = String::from_utf8_lossy(buf.as_ref()).replace("\n", "");
            let resp = handle_request(core.clone(), req.as_str());
            tx_cloned
                .send(resp)
                .await
                .expect("Unexpected channel failed");
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

    let (tx, rx) = unbounded::<String>();
    let (sink, mut stream) = ws_stream.split();

    // handle request
    let mut tx_cloned = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok(msg) = msg.to_text() {
                let req = msg.replace("\n", "");
                let resp = handle_request(core.clone(), req.as_str());
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
fn clean_up() {
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

/// Serve web
async fn serve_http(http_addr: SocketAddr, web_dir: String) {
    let static_ = hyper_staticfile::Static::new(web_dir);

    let make_service = make_service_fn(move |_| {
        let static_ = static_.clone();
        future::ok::<_, hyper::Error>(service_fn(move |req| static_.clone().serve(req)))
    });

    let server = Server::bind(&http_addr).serve(make_service);

    if let Err(e) = server.await {
        log::error!("Http web server error: {}", e);
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let matches = App::new("PiSugar Power Manager")
        .version("1.0")
        .author("PiSugar")
        .about("PiSugar power management module.")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Config file in json format, e.g. /etc/pisugar.json"),
        )
        .arg(
            Arg::with_name("tcp")
                .short("t")
                .long("tcp")
                .default_value("0.0.0.0:8080")
                .help("Tcp listen address, e.g. 0.0.0.0:8080"),
        )
        .arg(
            Arg::with_name("uds")
                .short("u")
                .long("uds")
                .default_value("/temp/pisugar.sock")
                .help("Unix domain socket file, e.g. /temp/pisugar.sock"),
        )
        .arg(
            Arg::with_name("ws")
                .short("w")
                .long("ws")
                .default_value("0.0.0.0:8081")
                .help("Websocket listen address, e.g. 127.0.0.1:8081"),
        )
        .arg(
            Arg::with_name("web")
                .requires_all(&["http"])
                .long("web")
                .default_value("web")
                .help("Web content directory, e.g. web"),
        )
        .arg(
            Arg::with_name("http")
                .long("http")
                .default_value("127.0.0.1:80")
                .help("Http server listen address, e.g. 127.0.0.1:80"),
        )
        .get_matches();

    // core
    let core = if matches.is_present("config") {
        PiSugarCore::new_with_path(Path::new(matches.value_of("config").unwrap()), true).unwrap()
    } else {
        let config = PiSugarConfig::default();
        PiSugarCore::new(config)
    };
    let core = Arc::new(Mutex::new(core));

    // event watch
    let (event_tx, event_rx) = tokio::sync::watch::channel("".to_string());

    // CTRL+C signal handling
    let _ = ctrlc::set_handler(|| {
        clean_up();
    });

    // tcp
    if matches.is_present("tcp") {
        let tcp_addr = matches.value_of("tcp").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        let mut tcp_listener: TcpListener = TcpListener::bind(tcp_addr).await?;
        tokio::spawn(async move {
            log::info!("TCP listening...");
            while let Some(Ok(stream)) = tcp_listener.incoming().next().await {
                let core = core_cloned.clone();
                let _ = handle_tcp_stream(core, stream, event_rx_cloned.clone()).await;
            }
            log::info!("TCP stopped");
        });
    }

    // ws
    if matches.is_present("ws") {
        let ws_addr = matches.value_of("ws").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx.clone();
        let mut ws_listener = tokio::net::TcpListener::bind(ws_addr).await?;
        tokio::spawn(async move {
            log::info!("WS listening...");
            while let Some(Ok(stream)) = ws_listener.incoming().next().await {
                let core = core_cloned.clone();
                let _ = handle_ws_connection(core, stream, event_rx_cloned.clone()).await;
            }
            log::info!("WS stopped");
        });
    }

    // uds
    if matches.is_present("uds") {
        let uds_addr = matches.value_of("uds").unwrap();
        let core_cloned = core.clone();
        let event_rx_cloned = event_rx;
        let mut uds_listener = tokio::net::UnixListener::bind(uds_addr)?;
        tokio::spawn(async move {
            log::info!("UDS listening...");
            while let Some(Ok(stream)) = uds_listener.incoming().next().await {
                let core = core_cloned.clone();
                let _ = handle_uds_stream(core, stream, event_rx_cloned.clone()).await;
            }
            log::info!("UDS stopped");
        });
    }

    // http web
    if matches.is_present("http") && matches.is_present("web") {
        let web_dir = matches.value_of("web").unwrap().to_string();
        let http_addr = matches.value_of("http").unwrap().parse().unwrap();
        tokio::spawn(async move {
            log::info!("Http web server listening...");
            let _ = serve_http(http_addr, web_dir).await;
            log::info!("Http web server stopped");
        });
    }

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(I2C_READ_INTERVAL);
    loop {
        interval.tick().await;
        let mut core = core_cloned.lock().expect("unexpected lock failed");
        poll_pisugar_status(&mut core, &event_tx);
    }
}
