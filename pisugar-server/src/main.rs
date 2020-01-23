use std::time::Duration;
use std::time::Instant;

use actix::prelude::*;
use actix_web::{App, Error, get, HttpRequest, HttpResponse, HttpServer, middleware, Responder, web};
use actix_web_actors::ws;
use std::sync::{Mutex, Arc};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

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
async fn start_ws(r: HttpRequest, stream: web::Payload, data: web::Data<Addr<PiSugarMonitor>>) -> Result<HttpResponse, Error> {
    log::debug!("WS connected {:?}", r);

    let (addr, res) = ws::start_with_addr(MyWebSocket::new(), &r, stream)?;
    data.get_ref().do_send(RegisterWSClient { addr });

    Ok(res)
}

/// Client must send ping every 10s, otherwise will be dropped
struct MyWebSocket {
    last_received: Instant
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
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
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
        Self { last_received: Instant::now() }
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

/// PiSugar monitor
struct PiSugarMonitor {
    listeners: Vec<Addr<MyWebSocket>>,
    status: Arc<Mutex<PiSugarStatus>>,
}

impl PiSugarMonitor {
    pub fn new(status: Arc<Mutex<PiSugarStatus>>) -> Self {
        Self {
            listeners: vec![],
            status
        }
    }
}

impl Actor for PiSugarMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("PiSugar Power Manager started");
        ctx.run_interval(Duration::from_millis(100), |act, ctx| {
            log::debug!("polling PiSugar state");

            for l in &act.listeners {
                l.do_send(TapEvent { tap_type: String::from("Event:") });
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

#[derive(Default)]
pub struct PiSugarStatus {
    pub model: String,
    pub voltage: f64,
    pub intensity: f64,
    pub charging: f64,
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let sys = System::new("PiSugar Power Manager");
    let status = Arc::new(Mutex::new(PiSugarStatus::default()));
    let monitor = PiSugarMonitor::new(status.clone()).start();

    HttpServer::new(move || {
        App::new().data(monitor.clone())
            .wrap(middleware::Logger::default())
            .service(index)
            .service(web::resource("/ws/").to(start_ws))
    })
        .bind("[::0]:8080")?
        .run()
        .await
}
