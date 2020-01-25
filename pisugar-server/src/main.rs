use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use std::time::Instant;

use actix::prelude::*;
use actix_web::{
    get, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use actix_web_actors::ws;

use pisugar_core::{
    bat_read_gpio, bat_read_intensity, bat_read_voltage, gpio_detect_tap, PiSugarStatus, TapType,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const I2C_READ_INTERVAL: Duration = Duration::from_millis(100);

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

//#[get("/model")]
//async fn model() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery")]
//async fn battery() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_v")]
//async fn battery_v() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_i")]
//async fn battery_i() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_charging")]
//async fn battery_charging() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_time")]
//async fn rtc_time() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_time_list")]
//async fn rtc_time_list() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_alarm_flag")]
//async fn rtc_alarm_flag() -> impl Responder {
//    unimplemented!()
//}

/// start websocket, to push events
async fn start_ws(
    r: HttpRequest,
    stream: web::Payload,
    data: web::Data<Addr<PiSugarMonitor>>,
) -> Result<HttpResponse, Error> {
    log::debug!("WS connected {:?}", r);

    let (addr, res) = ws::start_with_addr(MyWebSocket::new(), &r, stream)?;
    data.get_ref().do_send(RegisterWSClient { addr });

    Ok(res)
}

/// Client must send ping every 10s, otherwise will be dropped
struct MyWebSocket {
    last_received: Instant,
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.ping(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // process websocket messages
        log::info!("WS: {:?}", msg);
        self.last_received = Instant::now();
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => ctx.text(text),
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(_)) => ctx.stop(),
            _ => ctx.stop(),
        }
    }
}

impl Handler<TapEvent> for MyWebSocket {
    type Result = ();

    fn handle(&mut self, msg: TapEvent, ctx: &mut Self::Context) {
        ctx.text(msg.tap_type);
    }
}

impl MyWebSocket {
    fn new() -> Self {
        Self {
            last_received: Instant::now(),
        }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn ping(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.last_received) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}

/// WS client connected
#[derive(Message)]
#[rtype(result = "()")]
struct RegisterWSClient {
    addr: Addr<MyWebSocket>,
}

/// PiSugar tap event
#[derive(Message)]
#[rtype(result = "()")]
struct TapEvent {
    tap_type: String,
}

impl From<TapType> for TapEvent {
    fn from(t: TapType) -> Self {
        match t {
            TapType::Single => Self {
                tap_type: String::from("single"),
            },
            TapType::Double => Self {
                tap_type: String::from("double"),
            },
            TapType::Long => Self {
                tap_type: String::from("long"),
            },
        }
    }
}

/// PiSugar config
#[derive(Default)]
struct PiSugarConfig {
    auto_shutdown_level: f64,
    single_tap_enable: bool,
    single_tap_shell: String,
    double_tap_enable: bool,
    double_tap_shell: String,
    long_tap_enable: bool,
    long_tap_shell: String,
}

fn execute_shell(shell: &str) {
    let args = ["-c", shell];
    let child = Command::new("sh").args(&args).spawn().expect("");
}

/// PiSugar monitor
struct PiSugarMonitor {
    config: Arc<RwLock<PiSugarConfig>>,
    status: Arc<RwLock<PiSugarStatus>>,
    listeners: Vec<Addr<MyWebSocket>>,
}

impl PiSugarMonitor {
    pub fn new(config: Arc<RwLock<PiSugarConfig>>, status: Arc<RwLock<PiSugarStatus>>) -> Self {
        Self {
            config,
            status,
            listeners: vec![],
        }
    }
}

impl Actor for PiSugarMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("PiSugar Power Manager started");

        let mut gpio_detect_history = String::with_capacity(32);

        ctx.run_interval(I2C_READ_INTERVAL, move |act, ctx| {
            log::debug!("polling PiSugar state");

            let mut status = act.status.write().expect("lock status failed");
            let config = act.config.read().expect("lock config failed");

            let now = Instant::now();

            // battery
            if let Ok(v) = bat_read_voltage() {
                log::debug!("voltage: {}", v);
                status.update_voltage(v, now);
            }
            if let Ok(i) = bat_read_intensity() {
                log::debug!("intensity: {}", i);
                status.update_intensity(i, now);
            }

            // auto shutdown
            if status.level() < config.auto_shutdown_level {
                log::debug!("low battery, will power off");
                loop {
                    let mut proc = Command::new("poweroff").spawn().unwrap();
                    let exit_status = proc.wait().unwrap();
                }
            }

            // rtc

            // GPIO tap detect
            if let Ok(pressed) = bat_read_gpio() {
                log::debug!("gpio button state: {}", pressed);

                if gpio_detect_history.len() == gpio_detect_history.capacity() {
                    gpio_detect_history.remove(0);
                }
                if pressed != 0 {
                    gpio_detect_history.push('1');
                } else {
                    gpio_detect_history.push('0');
                }

                if let Some(tap_type) = gpio_detect_tap(&mut gpio_detect_history) {
                    log::debug!("tap detected: {}", tap_type);
                    act.listeners
                        .iter()
                        .for_each(|l| l.do_send(tap_type.into()))
                }
            }
        });
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::info!("PiSugar Power Manager stopped");
    }
}

impl Handler<RegisterWSClient> for PiSugarMonitor {
    type Result = ();

    fn handle(&mut self, msg: RegisterWSClient, _: &mut Context<Self>) {
        self.listeners.push(msg.addr);
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let config = Arc::new(RwLock::new(PiSugarConfig::default()));
    let status = Arc::new(RwLock::new(PiSugarStatus::new()));

    let sys = System::new("PiSugar Power Manager");
    let monitor = PiSugarMonitor::new(config.clone(), status.clone()).start();

    HttpServer::new(move || {
        App::new()
            .data(monitor.clone())
            .wrap(middleware::Logger::default())
            .service(index)
            .service(web::resource("/ws/").to(start_ws))
    })
    .bind("[::0]:8080")?
    .run()
    .await
}
