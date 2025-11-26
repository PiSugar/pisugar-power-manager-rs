use std::env;
use std::fs::remove_file;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Instant;

use clap::ArgAction;
use clap::Parser;
use env_logger::Env;
use log::LevelFilter;
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::sync::Mutex;
use tokio::time::Duration;

use pisugar_core::{execute_shell, notify_shutdown_soon, Model, PiSugarConfig, PiSugarCore, I2C_READ_INTERVAL};
use tokio::time::sleep;

mod cmds;
mod http;
mod jwt;
mod stream;
mod tcp;
mod uds;
mod ws;

lazy_static::lazy_static! {
    static ref UDS: StdMutex<Option<String>> = StdMutex::new(None);
}

/// Poll pisugar status
async fn poll_pisugar_status(core: &mut PiSugarCore, tx: &tokio::sync::watch::Sender<String>) {
    log::debug!("Polling state");
    let now = Instant::now();
    match core.poll(now).await {
        Ok(Some(tap_type)) => {
            let _ = tx.send(format!("{}", tap_type));
        }
        Err(e) => {
            log::warn!("Poll error: {}, retry after 1s", e);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        _ => {}
    }
}

/// Clean up before exit
#[ctor::dtor]
fn clean_up() {
    if let Some(uds) = UDS.lock().unwrap().clone() {
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
    exit(0)
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

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Config file in json format, e.g. /etc/pisugar-server/config.json
    #[arg(short, long)]
    config: Option<String>,

    /// Tcp listen address, e.g. 0.0.0.0:8423
    #[arg(short, long)]
    tcp: Option<String>,

    /// Unix domain socket file, e.g. /tmp/pisugar-server.sock
    #[arg(short, long)]
    uds: Option<String>,

    /// Unix domain socket file mode, e.g. 666
    #[arg(long, default_value = "666")]
    uds_mode: String,

    /// Standalone Websocket listen address, e.g. 0.0.0.0:8422
    #[arg(short, long)]
    ws: Option<String>,

    /// Web content directory, e.g. web
    #[arg(long, default_value = "/usr/share/pisugar-server/web", requires = "http")]
    web: String,

    /// Http server listen address, e.g. 0.0.0.0:8421
    #[arg(long)]
    http: Option<String>,

    /// Debug output
    #[arg(short, long, action = ArgAction::SetTrue)]
    debug: bool,

    /// Log to syslog
    #[arg(short, long, action = ArgAction::SetTrue)]
    syslog: bool,

    /// PiSugar Model
    #[arg(long)]
    model: Model,

    /// Strict stream handling with '\n' as the flag
    #[arg(long, action = ArgAction::SetTrue)]
    strict: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // init logging
    init_logging(args.debug, args.syslog);

    // model
    log::info!("Running with model: {}", args.model);

    // core
    let core;
    loop {
        let c = args
            .config
            .clone()
            .map(|c| PiSugarCore::new_with_path(&c, true, args.model))
            .unwrap_or_else(|| {
                let config = PiSugarConfig::default();
                PiSugarCore::new(config, args.model)
            });
        match c {
            Ok(c) => {
                core = Arc::new(Mutex::new(c));
                break;
            }
            Err(e) => log::error!("PiSugar init failed: {}", e),
        }
        sleep(Duration::from_secs(3)).await;
    }

    // event watch
    let (event_tx, event_rx) = tokio::sync::watch::channel("".to_string());

    // CTRL+C signal handling
    let uds = args.uds.clone();
    {
        let mut uds_lock = UDS.lock().unwrap();
        *uds_lock = uds.clone();
    }

    // tcp
    if let Some(tcp_addr) = args.tcp.clone() {
        tcp::start_tcp_server(core.clone(), event_rx.clone(), tcp_addr, args.strict).await;
    }

    // ws
    if let Some(ws_addr) = args.ws.clone() {
        ws::start_ws_server(core.clone(), event_rx.clone(), ws_addr).await;
    }

    // uds
    if let Some(uds_addr) = args.uds.clone() {
        let uds_mode = u32::from_str_radix(&args.uds_mode, 8).unwrap_or(0o666);
        uds::start_uds_server(uds_addr, core.clone(), event_rx.clone(), uds_mode, args.strict).await;
    }

    // http web/ws
    if let Some(http_addr) = args.http.clone() {
        let jwt_secret = jwt::read_or_create_jwt_secret("/var/run/pisugar_jwt_secret")?;
        http::start_http_server(
            core.clone(),
            event_rx.clone(),
            http_addr,
            args.web.clone(),
            jwt_secret,
            args.debug,
        )
        .await;
    }

    // polling
    let core_cloned = core.clone();
    let mut interval = tokio::time::interval(I2C_READ_INTERVAL);
    let mut notify_at = tokio::time::Instant::now();
    let mut battery_high_at = tokio::time::Instant::now(); // last battery high timestamp
    loop {
        interval.tick().await;
        log::debug!("Polling");
        let mut core = core_cloned.lock().await;
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
